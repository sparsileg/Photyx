// plugins/analyze_frames.rs — AnalyzeFrames plugin

// Spec §15, §7.8
//
// Two-pass operation:
//   Pass 1 — compute four quality metrics for every loaded frame (or current frame if scope=current)
//   Pass 2 — compute session stats → classify PASS/REJECT (in-memory only, not written to files)
//
// scope=current — computes the four metrics for the current frame and prints raw results.
//                 No session stats, no classification.

use crate::analysis::{
    self,
    background::compute_background_metrics,
    eccentricity::compute_eccentricity,
    fwhm::compute_fwhm,
    profiles,
    session_stats::{
        classify_frame, compute_session_stats_iterative,
    },
    stars::detect_stars,
    AnalysisResult, BackgroundConfig, StarDetectionConfig,
};
use crate::context::{AppContext, ImageBuffer, KeywordEntry};
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotyxPlugin, PluginError, PluginOutput};
use rayon::prelude::*;
use serde_json::json;
use tracing::info;

pub struct AnalyzeFrames;

fn load_thresholds_by_name(
    name: &str,
) -> Result<crate::analysis::session_stats::AnalysisThresholds, PluginError> {
    let db = crate::GLOBAL_DB
        .get()
        .ok_or_else(|| PluginError::new("DB_UNAVAILABLE", "Global DB not initialised."))?
        .lock()
        .expect("global db lock poisoned");

    let result = db.query_row(
        "SELECT bg_median_reject_sigma,
                fwhm_reject_sigma, star_count_reject_sigma, eccentricity_reject_abs
         FROM threshold_profiles WHERE name = ?1 COLLATE NOCASE",
        rusqlite::params![name],
        |row| {
            Ok(crate::analysis::session_stats::AnalysisThresholds {
                background_median: crate::analysis::session_stats::MetricThresholds {
                    reject: row.get::<_, f64>(0)? as f32,
                },
                fwhm: crate::analysis::session_stats::MetricThresholds {
                    reject: row.get::<_, f64>(1)? as f32,
                },
                star_count: crate::analysis::session_stats::MetricThresholds {
                    reject: row.get::<_, f64>(2)? as f32,
                },
                eccentricity: crate::analysis::session_stats::MetricThresholds {
                    reject: row.get::<_, f64>(3)? as f32,
                },
            })
        },
    );

    result.map_err(|_| PluginError::new(
        "PROFILE_NOT_FOUND",
        &format!("Threshold profile '{}' not found.", name),
    ))
}

impl PhotyxPlugin for AnalyzeFrames {
    fn name(&self)        -> &str { "AnalyzeFrames" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes four quality metrics (background median, FWHM, eccentricity, star count) \
         for loaded frames, classifies each as PASS or REJECT against session statistics, \
         and records the result in the session — not written to the files on disk. \
         Use scope=current to inspect a single frame without classification."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "profile".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Threshold profile name to use for this run (e.g. profile=Session). If omitted, uses the active profile set in Edit > Analysis Parameters.".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "scope".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "all (default) or current. all runs both passes across every loaded frame and classifies against session statistics. current inspects only the current frame and prints raw metrics, with no session stats and no classification.".to_string(),
                default:     Some("all".to_string()),
            },
            ParamSpec {
                name:        "threshold".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Star detection threshold (sigma above background). If omitted, uses the built-in star detection default.".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "saturation".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Saturation threshold (0.0–1.0) above which a pixel is excluded from star detection. If omitted, uses the built-in star detection default.".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let scope = args.get("scope").map(|s| s.as_str()).unwrap_or("all");

        let mut det_config = StarDetectionConfig::default();
        if let Some(s) = args.get("threshold") {
            det_config.detection_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("threshold", "must be a positive float")
            })?;
        }
        if let Some(s) = args.get("saturation") {
            det_config.saturation_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("saturation", "must be a float between 0.0 and 1.0")
            })?;
        }

        // Optional profile= argument — look up thresholds by name from DB
        if let Some(profile_name) = args.get("profile") {
            let thresholds = load_thresholds_by_name(profile_name)?;
            let saved = ctx.analysis_thresholds.clone();
            ctx.analysis_thresholds = thresholds;

            // Issue 120: restore the saved thresholds unconditionally,
            // including if execute_current/execute_all panics. A plain
            // match+restore already covered the Ok/Err cases; only a panic
            // unwinding straight through this block could skip the restore
            // and leave the temporary profile active in ctx indefinitely.
            // catch_unwind here converts that panic into a normal
            // PluginError instead of relying on the registry's dispatch-
            // level catch_unwind, which restores nothing plugin-specific.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                match scope.to_lowercase().as_str() {
                    "current" => execute_current(ctx, &det_config),
                    _         => execute_all(ctx, &det_config),
                }
            }));
            ctx.analysis_thresholds = saved;

            return match result {
                Ok(r) => r,
                Err(payload) => {
                    let msg = payload.downcast_ref::<&str>()
                        .map(|s| s.to_string())
                        .or_else(|| payload.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "AnalyzeFrames panicked during profile=-scoped execution".to_string());
                    Err(PluginError::new("PANIC", &msg))
                }
            };
        }

        match scope.to_lowercase().as_str() {
            "current" => execute_current(ctx, &det_config),
            _         => execute_all(ctx, &det_config),
        }
    }
}

// ── scope=current ─────────────────────────────────────────────────────────────

fn execute_current(
    ctx:        &mut AppContext,
    det_config: &StarDetectionConfig,
) -> Result<PluginOutput, PluginError> {
    let img = ctx.current_image().ok_or_else(|| {
        PluginError::new("NO_IMAGE", "No image loaded.")
    })?;

    // Issue 117: analysis_results must be keyed by the session path
    // (matching execute_all's snap.path and file_list entries), not
    // img.filename — image_reader sets filename to the basename only,
    // which never matches a file_list entry and left ghost, unremovable
    // analysis_results entries. display_name (basename) is kept for the
    // response/console message below; AnalysisResult.filename itself now
    // also stores session_path, matching execute_all's convention.
    let display_name = img.filename.clone();
    let session_path  = ctx.file_list.get(ctx.current_frame).cloned().ok_or_else(|| {
        PluginError::new("NO_IMAGE", "No current frame in session.")
    })?;

    let result      = compute_metrics_for_image(img, det_config, &session_path)?;
    let plate_scale = derive_plate_scale(&img.keywords);

    let _ = img;

    *ctx.analysis_result_for(&session_path) = result.clone();

    let fwhm_str = match (result.fwhm, plate_scale) {
        (Some(px), Some(ps)) => format!("{:.2}px / {:.2}\"", px, px * ps),
        (Some(px), None)     => format!("{:.2}px", px),
        _                    => "n/a".to_string(),
    };

    let message = format!(
        "Background median: {}\n\
         FWHM:              {}\n\
         Eccentricity:      {}\n\
         Star count:        {}",
        fmt_opt_adu(result.background_median),
        fwhm_str,
        result.eccentricity.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "n/a".to_string()),
        result.star_count.map(|v| v.to_string()).unwrap_or_else(|| "n/a".to_string()),
    );

    Ok(PluginOutput::Data(json!({
        "plugin":           "AnalyzeFrames",
        "scope":            "current",
        "filename":         display_name,
        "background_median": result.background_median,
        "fwhm_pixels":      result.fwhm,
        "eccentricity":     result.eccentricity,
        "star_count":       result.star_count,
        "message":          message,
    })))
}

// ── scope=all ─────────────────────────────────────────────────────────────────

fn execute_all(
    ctx:        &mut AppContext,
    det_config: &StarDetectionConfig,
) -> Result<PluginOutput, PluginError> {
    if ctx.file_list.is_empty() {
        return Err(PluginError::new("NO_FILES", "No files loaded."));
    }

    let thresholds = ctx.analysis_thresholds.clone();
    ctx.analysis_results.clear();

    let total = ctx.file_list.len();
    info!("AnalyzeFrames: Pass 1 — computing metrics for {} frames", total);

    crate::set_progress("Analyzing", 0, total as u32);

    let det_config_ref = det_config;
    let completed  = std::sync::atomic::AtomicU32::new(0);
    let chunk_len  = crate::plugins::pixel_chunking::chunk_size(ctx);
    let file_list  = ctx.file_list.clone();

    let mut par_results: Vec<AnalysisResult> = Vec::with_capacity(total);

    // Issue 175: LoadKind::Luma, not Raw — the reader thread now performs
    // the debayer/luminance conversion that used to happen inline below
    // (analysis::to_luminance(&snap.pixels, channels)), so what arrives
    // here is already luma. The full ordered request list is known up
    // front (one per file_list entry, same order), letting the reader
    // start decoding ahead of the per-chunk classify loop below.
    let requests: Vec<crate::plugins::pixel_chunking::LoadRequest> = file_list.iter()
        .map(|path| crate::plugins::pixel_chunking::LoadRequest {
            path: path.clone(),
            kind: crate::plugins::pixel_chunking::LoadKind::Luma,
        })
        .collect();
    let reader_capacity = crate::plugins::pixel_chunking::prefetch_capacity_chunked(ctx);
    let mut reader = crate::plugins::pixel_chunking::PixelReaderHandle::spawn_disk_reader(
        requests, reader_capacity,
    );

    for path_chunk in file_list.chunks(chunk_len) {
        // Sequential: drain this chunk's worth of outcomes from the
        // background reader (Issue 175) rather than reading synchronously
        // here. The reader has been decoding + converting to luma ahead of
        // this loop on its own thread since spawn_disk_reader above.
        //
        // Issue 174 (unchanged in spirit, Issue 175 adds a completeness
        // check on top): a source file that is missing or unreadable is a
        // HARD ERROR for AnalyzeFrames — unlike StackFrames/CacheFrames,
        // which exclude-and-continue. Session statistics (sigma clipping,
        // PASS/REJECT thresholds) are computed across the whole frame set,
        // so a silently or partially dropped frame would corrupt the
        // classification of every other frame. Abort at the first failure,
        // naming the file and distinguishing missing from unreadable.
        //
        // Issue 175 addition: the background reader's recv() contract is
        // "None means channel closed, NOT necessarily every request
        // fulfilled" (see PixelReaderHandle::recv doc comment) — a reader
        // that closed early for any reason (including an internal panic
        // recv couldn't have converted, though that path is itself covered
        // by PixelReaderHandle's own catch_unwind) must not be silently
        // read as "this chunk is done". received is checked against
        // path_chunk.len() below and a shortfall is a hard error, exactly
        // like a Missing/Unreadable outcome — completeness here is not
        // inferred from None, it's verified by count.
        let mut snapshots: Vec<crate::plugins::pixel_chunking::LumaSnapshot> =
            Vec::with_capacity(path_chunk.len());
        let mut received = 0usize;
        for _ in 0..path_chunk.len() {
            let outcome = match reader.recv() {
                Some(o) => o,
                None    => break, // channel closed early — shortfall caught below
            };
            received += 1;
            match outcome {
                crate::plugins::pixel_chunking::LoadOutcome::Loaded(
                    crate::plugins::pixel_chunking::LoadedFrame::Luma(snap)
                ) => snapshots.push(snap),
                crate::plugins::pixel_chunking::LoadOutcome::Loaded(_) => {
                    // Unreachable in practice: this reader was spawned with
                    // LoadKind::Luma requests only. Treated as unreadable
                    // rather than left to panic if that ever changes.
                    return Err(PluginError::new(
                        "INTERNAL_ERROR",
                        "AnalyzeFrames: reader returned a non-Luma LoadedFrame for a Luma request",
                    ));
                }
                crate::plugins::pixel_chunking::LoadOutcome::Missing { path } => {
                    return Err(PluginError::new(
                        "SOURCE_FILE_MISSING",
                        &format!("Source file missing during analysis: {}", path),
                    ));
                }
                crate::plugins::pixel_chunking::LoadOutcome::Unreadable { path, error } => {
                    return Err(PluginError::new(
                        "SOURCE_FILE_UNREADABLE",
                        &format!("Source file unreadable during analysis: {} ({})", path, error),
                    ));
                }
            }
        }
        if received < path_chunk.len() {
            return Err(PluginError::new(
                "SOURCE_READ_INCOMPLETE",
                &format!(
                    "Background reader closed before delivering all frames in this batch \
                     ({} of {} received) — aborting analysis rather than computing session \
                     statistics on a partial set.",
                    received, path_chunk.len(),
                ),
            ));
        }

        let chunk_results: Vec<AnalysisResult> = snapshots
            .par_iter()
            .map(|snap| {
                let width  = snap.width;
                let height = snap.height;

                // luma arrives pre-converted from the reader thread (Issue
                // 175) — no analysis::to_luminance call needed here anymore.
                let bg_config = BackgroundConfig::default();
                let bg        = compute_background_metrics(&snap.luma, width, height, &bg_config);
                let stars     = detect_stars(&snap.luma, width, height, det_config_ref);
                // Keywords travel with the snapshot (Issue 174/175), so no
                // separate image_buffers lookup is needed for plate scale.
                let plate_scale   = derive_plate_scale(&snap.keywords);
                let fwhm_result   = compute_fwhm(&stars, plate_scale);
                let ecc_result    = compute_eccentricity(&stars);

                let result = AnalysisResult {
                    filename:          snap.path.clone(),
                    background_median: Some(bg.median),
                    fwhm:              fwhm_result.as_ref().map(|r| r.fwhm_pixels),
                    eccentricity:      ecc_result.as_ref().map(|r| r.eccentricity),
                    star_count:        fwhm_result.as_ref().map(|r| r.star_count as u32)
                        .or_else(|| ecc_result.as_ref().map(|r| r.star_count as u32))
                        .or_else(|| Some(stars.len() as u32)),
                    flag: None,
                    triggered_by: vec![],
                    rejection_category: None,
                    is_reference: false,
                };

                info!("AnalyzeFrames: {} — done", short_name(&snap.path));
                let n = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                crate::set_progress("Analyzing", n, total as u32);
                result
            })
            .collect();

        par_results.extend(chunk_results);
        // This chunk's decoded luma buffers (`snapshots`) drop here, before
        // the next chunk is drained.
    }

    crate::set_progress("", 0, 0);

    // Issue 174: every frame in file_list is either analyzed or the run
    // aborts (see the hard-error handling in the chunk loop above), so
    // par_results is already the complete, final results list — there is no
    // skipped-frame set to reconcile. The old Issue 159 per-chunk skip
    // tracking is gone with the silent-skip behavior it reported.
    let mut results: Vec<AnalysisResult> = par_results;

    if results.is_empty() {
        return Err(PluginError::new(
            "NO_RESULTS",
            "Could not compute metrics for any frame.",
        ));
    }

    //    Pass 2: iterative sigma clipping   session stats   classify   write PXFLAG
    info!("AnalyzeFrames: Pass 2   classifying {} frames (iterative sigma clipping)", results.len());

    let result_refs: Vec<&AnalysisResult> = results.iter().collect();
    let (session_stats, outlier_paths) = compute_session_stats_iterative(&result_refs);

    let outlier_count = outlier_paths.len();
    if outlier_count > 0 {
        info!("AnalyzeFrames: {} frame(s) excluded from session stats as extreme outliers", outlier_count);
    }
    ctx.outlier_frame_paths = outlier_paths;

    let mut pass_count   = 0u32;
    let mut reject_count = 0u32;
    let mut frame_summaries: Vec<serde_json::Value> = Vec::new();

    for result in &mut results {
        let (flag, triggered) = classify_frame(result, &session_stats, &thresholds);
        result.flag = Some(flag.clone());
        result.triggered_by = triggered.clone();


        match flag {
            crate::analysis::PxFlag::Pass   => pass_count   += 1,
            crate::analysis::PxFlag::Reject => reject_count += 1,
        }

        frame_summaries.push(json!({
            "filename":       short_name(&result.filename),
            "flag":           result.flag.as_ref().map(|f| f.as_str()).unwrap_or("?"),
            "triggered":      triggered,
            "fwhm":           result.fwhm,
            "ecc":            result.eccentricity,
            "stars":          result.star_count,
            "is_reference":   result.is_reference,
        }));

        ctx.analysis_results.insert(result.filename.clone(), result.clone());
    }

    // ── Reference frame selection ─────────────────────────────────────────
    // Shared quality formula with StackFrames (Issue 95) — one definition
    // of "best frame" for both. Restricted to PASS frames so a REJECT
    // frame isn't crowned reference while any PASS frame exists; falls
    // back to the whole set only if the entire session failed
    // classification (rare, but a reference is still useful in that
    // case — see Issue 95 discussion). star_count remains the tiebreak,
    // higher wins.
    let ref_path: Option<String> = {
        let score_candidates = |only_pass: bool| {
            results.iter()
                .filter(|r| !only_pass || r.flag == Some(crate::analysis::PxFlag::Pass))
                .map(|r| (r.filename.clone(), crate::analysis::frame_quality_score(r.fwhm, r.eccentricity), r.star_count.unwrap_or(0)))
                .max_by(|(_, sa, ca), (_, sb, cb)| {
                    sa.partial_cmp(sb)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| ca.cmp(cb)) // higher star_count wins tiebreak
                })
                .map(|(path, _, _)| path)
        };

        score_candidates(true).or_else(|| score_candidates(false))
    };

    for result in &mut results {
        result.is_reference = ref_path.as_deref() == Some(&result.filename);
    }

    // Write is_reference back into ctx.analysis_results now that it's been set
    for result in &results {
        if let Some(r) = ctx.analysis_results.get_mut(&result.filename) {
            r.is_reference = result.is_reference;
        }
    }

    ctx.last_analysis_thresholds = Some(thresholds);
    ctx.last_session_stats = Some(session_stats);

    let message = format!(
        "AnalyzeFrames complete: {} frames — {} PASS, {} REJECT",
        results.len(),
        pass_count,
        reject_count,
    );

    info!("{}", message);

    // Issue 174: skipped_count/skipped are retained as constant-empty for
    // response-shape compatibility (any frontend or pcode reading them still
    // gets valid 0/[]). Frames are never skipped now — a missing or
    // unreadable source aborts the run — so these can be removed in a later
    // cleanup once no consumer depends on them.
    Ok(PluginOutput::Data(json!({
        "plugin":        "AnalyzeFrames",
        "scope":         "all",
        "frame_count":   results.len(),
        "pass_count":    pass_count,
        "reject_count":  reject_count,
        "skipped_count": 0,
        "skipped":       Vec::<String>::new(),
        "frames":        frame_summaries,
        "message":       message,
    })))
}

// ── Metric computation for a single image ────────────────────────────────────

fn compute_metrics_for_image(
    img:          &ImageBuffer,
    det_config:   &StarDetectionConfig,
    session_path: &str,
) -> Result<AnalysisResult, PluginError> {
    let pixels = img.pixels.as_ref().ok_or_else(|| {
        PluginError::new("NO_PIXELS", "Image buffer contains no pixel data.")
    })?;

    let channels  = img.channels as usize;
    let width     = img.width  as usize;
    let height    = img.height as usize;

    let luma      = analysis::to_luminance(pixels, channels);
    let bg_config = BackgroundConfig::default();
    let bg        = compute_background_metrics(&luma, width, height, &bg_config);
    let stars     = detect_stars(&luma, width, height, det_config);
    let plate_scale = derive_plate_scale(&img.keywords);
    let fwhm_result = compute_fwhm(&stars, plate_scale);
    let ecc_result  = compute_eccentricity(&stars);

    Ok(AnalysisResult {
        // Issue 117: session path, not img.filename (basename) — matches
        // execute_all's AnalysisResult.filename = snap.path convention.
        filename:          session_path.to_string(),
        background_median: Some(bg.median),
        fwhm:              fwhm_result.as_ref().map(|r| r.fwhm_pixels),
        eccentricity:      ecc_result.as_ref().map(|r| r.eccentricity),
        star_count:        fwhm_result.as_ref().map(|r| r.star_count as u32)
            .or_else(|| ecc_result.as_ref().map(|r| r.star_count as u32))
            .or_else(|| Some(stars.len() as u32)),
        flag: None,
        triggered_by: vec![],
        rejection_category: None,
        is_reference: false,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn derive_plate_scale(
    keywords: &std::collections::HashMap<String, KeywordEntry>,
) -> Option<f32> {
    let focal_length = keywords.get("FOCALLEN")
        .and_then(|kw| kw.value.trim().parse::<f32>().ok())?;
    let instrume = keywords.get("INSTRUME").map(|kw| kw.value.as_str())?;
    let pixel_size = profiles::pixel_size_um(instrume)?;
    let binning = keywords.get("XBINNING")
        .and_then(|kw| kw.value.trim().parse::<u32>().ok())
        .unwrap_or(1);
    Some(profiles::plate_scale(focal_length, pixel_size, binning))
}

fn fmt_opt_adu(val: Option<f32>) -> String {
    match val {
        Some(v) => format!("{:.4} ({} ADU)", v, (v * 65535.0).round() as u32),
        None    => "n/a".to_string(),
    }
}

fn short_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 174 — a source file that does not exist on disk aborts the
    /// whole analysis run rather than being silently skipped, because
    /// session statistics (sigma clipping, PASS/REJECT thresholds) are
    /// computed across the entire frame set and a partial set would corrupt
    /// every frame's classification. This is the unit-level encoding of the
    /// regression-gate clause "a deliberately-removed source file mid-run
    /// produces a clear error, never a silently smaller result set."
    ///
    /// No temp file is needed: the chunk loader's existence check fires
    /// before any decode is attempted, so a nonexistent path exercises the
    /// SOURCE_FILE_MISSING abort directly. (The successful-analysis path is
    /// covered end-to-end by the numerical regression gate on real data.)
    #[test]
    fn test_analyze_frames_missing_source_aborts() {
        let mut ctx = AppContext::new();

        ctx.file_list = vec!["/nonexistent/path/frame_174.fits".to_string()];

        let det_config = StarDetectionConfig::default();
        let result = execute_all(&mut ctx, &det_config);

        assert!(result.is_err(), "expected a hard error for a missing source file");
        assert_eq!(result.unwrap_err().code, "SOURCE_FILE_MISSING");
    }
}


// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
