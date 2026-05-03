// commands/analysis.rs — Analysis and quality metrics Tauri command handlers

use std::sync::Arc;
use tauri::State;
use crate::PhotoxState;

#[tauri::command]
pub fn get_analysis_results(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    let mut ctx = state.context.lock().expect("context lock poisoned");

    // If no cached metrics exist, return empty — AnalyzeFrames hasn't run yet.
    if ctx.analysis_results.is_empty() {
        return serde_json::json!({
            "frames": [],
            "session_stats": {},
            "applied_thresholds": null,
            "outlier_paths": [],
        });
    }

    // Reclassify all frames on the fly using cached metrics + current thresholds.
    // This allows threshold changes to take effect without rerunning AnalyzeFrames.
    use crate::analysis::session_stats::{
        classify_frame, compute_session_stats_iterative, AnalysisThresholds,
    };

    let thresholds: AnalysisThresholds = ctx.analysis_thresholds.clone();

    let result_refs: Vec<&crate::analysis::AnalysisResult> = ctx.file_list.iter()
        .filter_map(|p| ctx.analysis_results.get(p))
        .collect();

    let (session_stats, outlier_paths) = compute_session_stats_iterative(&result_refs);

    // Store updated stats and outliers back into ctx so blink overlay etc. stays consistent
    ctx.last_session_stats = Some(session_stats.clone());
    ctx.outlier_frame_paths = outlier_paths.clone();

    // Reclassify each frame and update stored results
    let paths: Vec<String> = ctx.file_list.clone();
    for path in &paths {
        if let Some(result) = ctx.analysis_results.get(path).cloned() {
            let (flag, triggered) = classify_frame(&result, &session_stats, &thresholds);
            if let Some(r) = ctx.analysis_results.get_mut(path) {
                r.flag = Some(flag);
                r.triggered_by = triggered;
            }
        }
    }

    let frames: Vec<serde_json::Value> = ctx.file_list.iter().enumerate().map(|(i, path)| {
        let short = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let label = extract_frame_label(short);

        if let Some(r) = ctx.analysis_results.get(path) {
            let flag = r.flag.as_ref().map(|f| f.as_str().to_string()).unwrap_or_default();
            serde_json::json!({
                "index":             i,
                "filename":          path,
                "label":             label,
                "short_name":        short,
                "background_median": r.background_median,
                "snr_estimate":      r.snr_estimate,
                "fwhm":              r.fwhm,
                "eccentricity":      r.eccentricity,
                "star_count":        r.star_count,
                "flag":              flag,
                "triggered":         r.triggered_by,
            })
        } else {
            serde_json::json!({
                "index":      i,
                "filename":   path,
                "label":      label,
                "short_name": short,
                "flag":       "",
                "triggered":  [],
            })
        }
    }).collect();

    let applied = serde_json::json!({
        "background_median": { "value": thresholds.background_median.reject, "direction": "high" },
        "snr_estimate":      { "value": thresholds.snr_estimate.reject,      "direction": "low"  },
        "fwhm":              { "value": thresholds.fwhm.reject,              "direction": "high" },
        "star_count":        { "value": thresholds.star_count.reject,        "direction": "low"  },
        "eccentricity":      { "value": thresholds.eccentricity.reject,      "direction": "high" },
    });

    let outlier_path_strs: Vec<&str> = outlier_paths.iter()
        .map(|s| s.as_str())
        .collect();

    serde_json::json!({
        "frames": frames,
        "session_stats": {
            "background_median": { "mean": session_stats.background_median.mean, "stddev": session_stats.background_median.stddev },
            "snr_estimate":      { "mean": session_stats.snr_estimate.mean,      "stddev": session_stats.snr_estimate.stddev },
            "fwhm":              { "mean": session_stats.fwhm.mean,              "stddev": session_stats.fwhm.stddev },
            "eccentricity":      { "mean": session_stats.eccentricity.mean,      "stddev": session_stats.eccentricity.stddev },
            "star_count":        { "mean": session_stats.star_count.mean,        "stddev": session_stats.star_count.stddev },
        },
        "applied_thresholds": applied,
        "outlier_paths": outlier_path_strs,
    })
}

/// Write PXFLAG keywords to all image buffers and flush to disk via WriteCurrent.
/// Updates last_analysis_thresholds to match the thresholds used for the committed results.
#[tauri::command]
pub fn commit_analysis_results(state: State<Arc<PhotoxState>>) -> Result<String, String> {
    use crate::context::KeywordEntry;
    use crate::plugin::ArgMap;

    {
        let mut ctx = state.context.lock().expect("context lock poisoned");

        if ctx.analysis_results.is_empty() {
            return Err("No analysis results to commit. Run AnalyzeFrames first.".to_string());
        }

        let thresholds = ctx.analysis_thresholds.clone();
        let paths: Vec<String> = ctx.file_list.clone();
        let mut pass_count   = 0u32;
        let mut reject_count = 0u32;

        // Write PXFLAG to image buffers in memory
        for path in &paths {
            if let Some(result) = ctx.analysis_results.get(path) {
                if let Some(flag) = &result.flag {
                    let flag_str = flag.as_str().to_string();
                    match flag {
                        crate::analysis::PxFlag::Pass   => pass_count   += 1,
                        crate::analysis::PxFlag::Reject => reject_count += 1,
                    }
                    if let Some(buf) = ctx.image_buffers.get_mut(path) {
                        buf.keywords.insert(
                            "PXFLAG".to_string(),
                            KeywordEntry::new("PXFLAG", &flag_str, Some("Photyx frame quality flag")),
                        );
                    }
                }
            }
        }

        // Update last_analysis_thresholds to reflect what was committed
        ctx.last_analysis_thresholds = Some(thresholds);

        tracing::info!(
            "CommitResults: {} PASS, {} REJECT — writing to disk",
            pass_count, reject_count
        );
    }

    // Flush to disk via WriteCurrent — drop ctx lock first so registry can reacquire it
    let args = ArgMap::new();
    state.registry
        .dispatch(&mut state.context.lock().expect("context lock poisoned"), "WriteCurrent", &args)
        .map(|output| {
            match output {
                crate::plugin::PluginOutput::Message(m) => m,
                crate::plugin::PluginOutput::Data(d) => d
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("Committed.")
                    .to_string(),
                _ => "Committed.".to_string(),
            }
        })
        .map_err(|e| e.message)
}

pub fn extract_frame_label(filename: &str) -> String {
    let stem = filename.rsplit('.').nth(1).unwrap_or(filename);
    let digits: String = stem.chars().rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars().rev().collect();
    if !digits.is_empty() && digits.len() <= 6 {
        return digits.trim_start_matches('0').to_string()
            .parse::<u32>().unwrap_or(0).to_string();
    }
    let chars: Vec<char> = stem.chars().collect();
    chars[chars.len().saturating_sub(8)..].iter().collect()
}

#[tauri::command]
pub fn get_frame_flags(state: State<Arc<PhotoxState>>) -> Vec<String> {
    let ctx = state.context.lock().expect("context lock poisoned");
    ctx.file_list.iter().map(|path| {
        if let Some(result) = ctx.analysis_results.get(path) {
            if let Some(flag) = &result.flag {
                return flag.as_str().to_string();
            }
        }
        if let Some(buf) = ctx.image_buffers.get(path) {
            if let Some(kw) = buf.keywords.get("PXFLAG") {
                return kw.value.clone();
            }
        }
        String::new()
    }).collect()
}

#[tauri::command]
pub fn get_star_positions(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    use crate::analysis::{self, stars::detect_stars, fwhm::star_fwhm, StarDetectionConfig};

    let ctx = state.context.lock().expect("context lock poisoned");

    let img = match ctx.current_image() {
        Some(i) => i,
        None => return serde_json::json!({ "stars": [] }),
    };

    let pixels = match img.pixels.as_ref() {
        Some(p) => p,
        None => return serde_json::json!({ "stars": [] }),
    };

    let channels = img.channels as usize;
    let width    = img.width as usize;
    let height   = img.height as usize;

    let luma   = analysis::to_luminance(pixels, channels);
    let config = StarDetectionConfig::default();
    let stars  = detect_stars(&luma, width, height, &config);

    let positions: Vec<serde_json::Value> = stars.iter()
        .filter_map(|s| {
            let fwhm = star_fwhm(s)?;
            if fwhm < 0.5 || fwhm > 50.0 { return None; }
            Some(serde_json::json!({
                "cx":   s.cx,
                "cy":   s.cy,
                "fwhm": fwhm,
                "r":    fwhm / 2.0,
            }))
        })
        .collect();

    serde_json::json!({ "stars": positions })
}

// ----------------------------------------------------------------------
