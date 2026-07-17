// commands/analysis.rs — Analysis and quality metrics Tauri command handlers

use std::sync::Arc;
use tauri::State;
use crate::PhotoxState;

#[tauri::command]
pub fn get_analysis_results(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    let mut ctx = state.context.lock().expect("context lock poisoned");

    let session_path = ctx.common_parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let is_imported  = ctx.is_imported_session;

    // If no cached metrics exist, return empty — AnalyzeFrames hasn't run yet.
    if ctx.analysis_results.is_empty() {
        return serde_json::json!({
            "frames":             [],
            "session_stats":      {},
            "applied_thresholds": null,
            "outlier_paths":      [],
            "session_path":       session_path,
            "is_imported":        is_imported,
        });
    }

    use crate::analysis::session_stats::{
        categorize_rejection, classify_frame, compute_session_stats_iterative, AnalysisThresholds,
    };

    // Prefer the thresholds the last AnalyzeFrames run (or JSON import)
    // actually used, falling back to the active profile only if nothing
    // has been analyzed yet. This is what keeps a `profile=`-pinned run's
    // classifications stable across Refresh instead of silently drifting
    // to whatever the active profile happens to be — see Issue 78. An
    // explicit active-profile change (Edit > Analysis Parameters OK/Apply)
    // updates last_analysis_thresholds itself, so that path still takes
    // effect on the next Refresh as documented in §6.8.
    let thresholds: AnalysisThresholds = ctx.last_analysis_thresholds.clone()
        .unwrap_or_else(|| ctx.analysis_thresholds.clone());

    // For imported sessions, classifications are already authoritative — skip reclassification.
    // For live sessions, reclassify on the fly so threshold changes take effect immediately.
    if !is_imported {
        let result_refs: Vec<&crate::analysis::AnalysisResult> = ctx.file_list.iter()
            .filter_map(|p| ctx.analysis_results.get(p))
            .collect();

        let (session_stats, outlier_paths) = compute_session_stats_iterative(&result_refs);
        ctx.last_session_stats  = Some(session_stats.clone());
        ctx.outlier_frame_paths = outlier_paths.clone();

        let paths: Vec<String> = ctx.file_list.clone();
        for path in &paths {
            if let Some(result) = ctx.analysis_results.get(path).cloned() {
                let (flag, triggered) = classify_frame(&result, &session_stats, &thresholds);
                let category = match flag {
                    crate::analysis::PxFlag::Reject => categorize_rejection(&triggered),
                    crate::analysis::PxFlag::Pass   => None,
                };
                if let Some(r) = ctx.analysis_results.get_mut(path) {
                    r.flag               = Some(flag);
                    r.triggered_by       = triggered;
                    r.rejection_category = category;
                }
            }
        }
    }

    let session_stats = ctx.last_session_stats.clone().unwrap_or_default();
    let outlier_paths = ctx.outlier_frame_paths.clone();

    let frames: Vec<serde_json::Value> = ctx.file_list.iter().enumerate().map(|(i, path)| {
        let short = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let label = extract_frame_label(short);

        if let Some(r) = ctx.analysis_results.get(path) {
            let flag = r.flag.as_ref().map(|f| f.as_str().to_string()).unwrap_or_default();
            serde_json::json!({
                "index":              i,
                "filename":           path,
                "label":              label,
                "short_name":         short,
                "background_median":  r.background_median,
                "fwhm":               r.fwhm,
                "eccentricity":       r.eccentricity,
                "star_count":         r.star_count,
                "flag":               flag,
                "triggered":          r.triggered_by,
                "rejection_category": r.rejection_category,
                "is_reference":       r.is_reference,
            })
        } else {
            serde_json::json!({
                "index":              i,
                "filename":           path,
                "label":              label,
                "short_name":         short,
                "flag":               "",
                "triggered":          [],
                "rejection_category": null,
            })
        }
    }).collect();

    let applied = serde_json::json!({
        "background_median": { "value": thresholds.background_median.reject, "direction": "high" },
        "fwhm":              { "value": thresholds.fwhm.reject,              "direction": "high" },
        "star_count":        { "value": thresholds.star_count.reject,        "direction": "low"  },
        "eccentricity":      { "value": thresholds.eccentricity.reject,      "direction": "high" },
    });

    let outlier_path_strs: Vec<&str> = outlier_paths.iter().map(|s| s.as_str()).collect();

    serde_json::json!({
        "frames": frames,
        "session_stats": {
            "background_median": { "mean": session_stats.background_median.mean, "stddev": session_stats.background_median.stddev },
            "fwhm":              { "mean": session_stats.fwhm.mean,              "stddev": session_stats.fwhm.stddev },
            "eccentricity":      { "mean": session_stats.eccentricity.mean,      "stddev": session_stats.eccentricity.stddev },
            "star_count":        { "mean": session_stats.star_count.mean,        "stddev": session_stats.star_count.stddev },
        },
        "applied_thresholds": applied,
        "outlier_paths":      outlier_path_strs,
        "session_path":       session_path,
        "is_imported":        is_imported,
    })
}

// ── JSON import payload types ─────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub struct AnalysisJsonPayload {
    pub thresholds:    ThresholdPayload,
    pub session_stats: SessionStatsPayload,
    pub outliers:      Vec<String>,
    pub frames:        Vec<FramePayload>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ThresholdPayload {
    pub bg_median_reject_sigma:      f64,
    pub fwhm_reject_sigma:           f64,
    pub star_count_reject_sigma:     f64,
    pub eccentricity_reject_abs:     f64,
}

#[derive(Debug, serde::Deserialize)]
pub struct MetricStatsPayload {
    pub mean:   f32,
    pub stddev: f32,
}

#[derive(Debug, serde::Deserialize)]
pub struct SessionStatsPayload {
    pub background_median: MetricStatsPayload,
    pub fwhm:              MetricStatsPayload,
    pub eccentricity:      MetricStatsPayload,
    pub star_count:        MetricStatsPayload,
}

#[derive(Debug, serde::Deserialize)]
pub struct FramePayload {
    pub filename:           String,
    pub fwhm:               Option<f32>,
    pub eccentricity:       Option<f32>,
    pub star_count:         Option<u32>,
    pub background_median:  Option<f32>,
    pub flag:               String,
    pub triggered_by:       Vec<String>,
    pub rejection_category: Option<String>,
    #[serde(default)]
    pub is_reference:       bool,
}

// ── load_analysis_json command ────────────────────────────────────────────────

#[tauri::command]
pub fn load_analysis_json(
    payload: AnalysisJsonPayload,
    state:   State<Arc<PhotoxState>>,
) -> Result<(), String> {
    use crate::analysis::{AnalysisResult, PxFlag};
    use crate::analysis::session_stats::{
        AnalysisThresholds, MetricThresholds, SessionStats, MetricStats,
    };

    let mut ctx = state.context.lock().expect("context lock poisoned");
    ctx.clear_session();

    for frame in &payload.frames {
        let full_path = frame.filename.replace('\\', "/");
        ctx.file_list.push(full_path.clone());

        let flag = match frame.flag.as_str() {
            "REJECT" => Some(PxFlag::Reject),
            _        => Some(PxFlag::Pass),
        };

        let result = AnalysisResult {
            filename:           full_path.clone(),
            background_median:  frame.background_median,
            fwhm:               frame.fwhm,
            eccentricity:       frame.eccentricity,
            star_count:         frame.star_count,
            flag,
            triggered_by:       frame.triggered_by.clone(),
            rejection_category: frame.rejection_category.clone(),
            is_reference:       frame.is_reference,
        };

        ctx.analysis_results.insert(full_path, result);
    }

    // Outlier entries are basenames, matching FramePayload.filename's
    // convention — see Issue 115. Not resolved against file_list; imported
    // sessions never load pixels or resolve paths on disk.
    ctx.outlier_frame_paths = payload.outliers.iter()
        .map(|f| f.replace('\\', "/"))
        .collect();

    // Restore session stats
    ctx.last_session_stats = Some(SessionStats {
        background_median: MetricStats { mean: payload.session_stats.background_median.mean, stddev: payload.session_stats.background_median.stddev },
        fwhm:              MetricStats { mean: payload.session_stats.fwhm.mean,              stddev: payload.session_stats.fwhm.stddev },
        eccentricity:      MetricStats { mean: payload.session_stats.eccentricity.mean,      stddev: payload.session_stats.eccentricity.stddev },
        star_count:        MetricStats { mean: payload.session_stats.star_count.mean,        stddev: payload.session_stats.star_count.stddev },
    });

    // Restore thresholds — apply as both last_analysis_thresholds and active thresholds
    let restored_thresholds = AnalysisThresholds {
        background_median: MetricThresholds { reject: payload.thresholds.bg_median_reject_sigma as f32 },
        fwhm:              MetricThresholds { reject: payload.thresholds.fwhm_reject_sigma as f32 },
        star_count:        MetricThresholds { reject: payload.thresholds.star_count_reject_sigma.abs() as f32 },
        eccentricity:      MetricThresholds { reject: payload.thresholds.eccentricity_reject_abs as f32 },
    };
    ctx.last_analysis_thresholds = Some(restored_thresholds.clone());
    ctx.analysis_thresholds      = restored_thresholds;

    ctx.is_imported_session = true;

    tracing::info!(
        "load_analysis_json: imported {} frames",
        ctx.file_list.len(),
    );

    Ok(())
}

// ── commit_analysis_results ───────────────────────────────────────────────────

/// Move REJECT files to a `rejected/` subfolder. If `append` is non-empty it
/// is added after the original filename (leading dot optional, e.g. ".session"
/// or "session" both produce `frame.fit.session`). If `append` is empty the
/// file is moved with its original name unchanged.
/// Does not write PXFLAG keywords or flush files to disk — the move itself
/// is the persistence action.
#[tauri::command]
pub fn commit_analysis_results(
    append: String,
    state:  State<Arc<PhotoxState>>,
) -> Result<String, String> {
    let mut ctx = state.context.lock().expect("context lock poisoned");
    do_commit(&mut ctx, &append).map_err(|e| e.to_string())
}

/// Moves a single file into a `rejected/` subfolder within its own parent
/// directory, optionally appending a suffix after the original filename
/// (e.g. suffix="reject" → "frame001.fit.reject"). A leading dot on the
/// suffix is optional and stripped if present. Returns the new path
/// (forward-slash normalized) on success. Shared by do_commit() (bulk,
/// user-supplied suffix) and RejectCurrentFrame (single frame, fixed
/// REJECT_FILE_SUFFIX constant).
pub fn move_to_rejected(old_path: &str, suffix: &str) -> Result<String, String> {
    let suffix = suffix.trim_start_matches('.');
    let p = std::path::Path::new(old_path);

    let parent = p.parent()
        .ok_or_else(|| format!("No parent dir: {}", old_path))?;
    let rejected_dir = parent.join("rejected");

    if !rejected_dir.exists() {
        std::fs::create_dir_all(&rejected_dir)
            .map_err(|e| format!("Cannot create rejected/: {}", e))?;
    }

    let filename = p.file_name()
        .ok_or_else(|| format!("No filename: {}", old_path))?;

    let new_filename = if suffix.is_empty() {
        filename.to_string_lossy().into_owned()
    } else {
        format!("{}.{}", filename.to_string_lossy(), suffix)
    };
    let new_path     = rejected_dir.join(&new_filename);
    let new_path_str = new_path.to_string_lossy().replace('\\', "/");

    std::fs::rename(old_path, &new_path)
        .map_err(|e| format!("{}: {}", filename.to_string_lossy(), e))?;

    tracing::info!("move_to_rejected: moved {} → {}", old_path, new_path_str);
    Ok(new_path_str)
}

/// Shared commit implementation called by both the Tauri command and the
/// CommitAnalysis pcode plugin.
pub fn do_commit(ctx: &mut crate::context::AppContext, append: &str) -> Result<String, String> {
    // ── Step 1: collect reject paths ──────────────────────────────────────────
    // Issue 117: a non-empty analysis_results is not sufficient evidence a
    // real (scope=all) analysis ran — AnalyzeFrames scope=current now
    // correctly keys its single-frame entry by session path (previously it
    // was keyed by basename and never matched file_list at all), so a
    // non-empty check alone would let a single-frame scope=current result
    // pass this guard and "commit" 0 real classifications silently. Require
    // every file_list path to have an entry instead — true for a genuine
    // scope=all run (which populates one entry per file) or an equivalent
    // per-frame scope=current loop covering the whole session, false for a
    // partial scope=current spot-check.
    let fully_analyzed = !ctx.file_list.is_empty()
        && ctx.file_list.iter().all(|p| ctx.analysis_results.contains_key(p));
    if !fully_analyzed {
        return Err("No analysis results to commit. Run AnalyzeFrames first.".to_string());
    }
    if ctx.is_imported_session {
        return Err("Cannot commit an imported session — no images are loaded.".to_string());
    }

    let mut pass_count   = 0u32;
    let mut reject_count = 0u32;
    let mut reject_paths: Vec<String> = Vec::new();

    for path in &ctx.file_list {
        if let Some(result) = ctx.analysis_results.get(path) {
            match result.flag {
                Some(crate::analysis::PxFlag::Pass) => pass_count += 1,
                Some(crate::analysis::PxFlag::Reject) => {
                    reject_count += 1;
                    reject_paths.push(path.clone());
                }
                None => {}
            }
        }
    }

    tracing::info!("CommitResults: {} PASS, {} REJECT", pass_count, reject_count);

    // ── Step 2: move REJECT files to rejected/ subfolder ─────────────────────
    let mut move_errors: Vec<String> = Vec::new();
    let mut moved_paths:  Vec<String> = Vec::new();

    for old_path in &reject_paths {
        match move_to_rejected(old_path, append) {
            Ok(_new_path) => moved_paths.push(old_path.clone()),
            Err(e)        => move_errors.push(e),
        }
    }
    let moved_count = moved_paths.len() as u32;

    // ── Step 3: remove only successfully-moved files from the session; a ──────
    //    REJECT frame whose move failed stays loaded, on disk under its
    //    original name, and reclassifies as unanalyzed on the next Refresh
    //    (Issue 123 — do not feed the failures into remove_rejected_files).
    ctx.remove_rejected_files(&moved_paths);

    // ── Build result message ──────────────────────────────────────────────────
    let mut msg = format!(
        "Committed: {} PASS, {} REJECT ({} moved to rejected/).",
        pass_count, reject_count, moved_count
    );
    if !move_errors.is_empty() {
        msg.push_str(&format!(
            " {} FAILED TO MOVE — these frames remain in the session and on disk under their original names:",
            move_errors.len()
        ));
        for e in &move_errors {
            msg.push_str(&format!("\n  - {}", e));
            tracing::warn!("CommitResults move error: {}", e);
        }
    }

    Ok(msg)
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
pub fn set_frame_flag(
    path:  String,
    flag:  String,
    state: State<Arc<PhotoxState>>,
) -> Result<(), String> {
    use crate::analysis::PxFlag;
    let mut ctx = state.context.lock().expect("context lock poisoned");
    if let Some(result) = ctx.analysis_results.get_mut(&path) {
        result.flag = match flag.as_str() {
            "PASS"   => Some(PxFlag::Pass),
            "REJECT" => Some(PxFlag::Reject),
            _        => return Err(format!("Unknown flag value: {}", flag)),
        };
    }
    Ok(())
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
