// commands/analysis.rs — Analysis and quality metrics Tauri command handlers

use std::sync::Arc;
use tauri::State;
use crate::PhotoxState;

#[tauri::command]
pub fn get_analysis_results(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");

    let frames: Vec<serde_json::Value> = ctx.file_list.iter().enumerate().map(|(i, path)| {
        let flag = ctx.analysis_results.get(path)
            .and_then(|r| r.flag.as_ref())
            .map(|f| f.as_str().to_string())
            .or_else(|| ctx.image_buffers.get(path)
                     .and_then(|b| b.keywords.get("PXFLAG"))
                     .map(|kw| kw.value.clone()))
            .unwrap_or_default();

        let short = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let label = extract_frame_label(short);

        if let Some(r) = ctx.analysis_results.get(path) {
            serde_json::json!({
                "index":               i,
                "filename":            path,
                "label":               label,
                "short_name":          short,
                "background_median":   r.background_median,
                "background_stddev":   r.background_stddev,
                "background_gradient": r.background_gradient,
                "snr_estimate":        r.snr_estimate,
                "fwhm":                r.fwhm,
                "eccentricity":        r.eccentricity,
                "star_count":          r.star_count,
                "flag":                flag,
                "triggered":           r.triggered_by,
            })
        } else {
            serde_json::json!({
                "index":      i,
                "filename":   path,
                "label":      label,
                "short_name": short,
                "flag":       flag,
                "triggered":  [],
            })
        }
    }).collect();

    use crate::analysis::session_stats::compute_session_stats;
    let result_refs: Vec<&crate::analysis::AnalysisResult> = ctx.file_list.iter()
        .filter_map(|p| ctx.analysis_results.get(p))
        .collect();
    let stats = compute_session_stats(&result_refs);

    serde_json::json!({
        "frames": frames,
        "session_stats": {
            "background_median":   { "mean": stats.background_median.mean,   "stddev": stats.background_median.stddev },
            "background_stddev":   { "mean": stats.background_stddev.mean,   "stddev": stats.background_stddev.stddev },
            "background_gradient": { "mean": stats.background_gradient.mean, "stddev": stats.background_gradient.stddev },
            "snr_estimate":        { "mean": stats.snr_estimate.mean,        "stddev": stats.snr_estimate.stddev },
            "fwhm":                { "mean": stats.fwhm.mean,                "stddev": stats.fwhm.stddev },
            "eccentricity":        { "mean": stats.eccentricity.mean,        "stddev": stats.eccentricity.stddev },
            "star_count":          { "mean": stats.star_count.mean,          "stddev": stats.star_count.stddev },
        }
    })
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
