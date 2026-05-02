// plugins/analyze_frames.rs — AnalyzeFrames plugin
// Spec §15, §7.8
//
// Two-pass operation:
//   Pass 1 — run all seven metrics on every loaded frame (or current frame if scope=current)
//   Pass 2 — compute session stats → PXFLAG (PASS/REJECT) → write keyword
//
// scope=current — runs all seven metrics on the current frame and prints raw results.
//                 No session stats, no PXFLAG written.

use crate::analysis::{
    self,
    background::compute_background_metrics,
    eccentricity::compute_eccentricity,
    fwhm::compute_fwhm,
    metrics::snr_estimate,
    profiles,
    session_stats::{
        classify_frame, compute_session_stats,
    },
    stars::detect_stars,
    AnalysisResult, BackgroundConfig, StarDetectionConfig,
};
use crate::context::{AppContext, ImageBuffer, KeywordEntry};
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use rayon::prelude::*;
use serde_json::json;
use tracing::info;

pub struct AnalyzeFrames;

impl PhotonPlugin for AnalyzeFrames {
    fn name(&self)        -> &str { "AnalyzeFrames" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes seven quality metrics for loaded frames, classifies each as \
         PASS or REJECT, and writes PXFLAG keyword to each file. \
         Use scope=current to inspect a single frame without writing keywords."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "scope".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "all (default) — all loaded frames; current — current frame only (diagnostic, no keywords written)".to_string(),
                default:     Some("all".to_string()),
            },
            ParamSpec {
                name:        "threshold".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Star detection threshold in units of background std dev (default: 5.0)".to_string(),
                default:     Some("5.0".to_string()),
            },
            ParamSpec {
                name:        "saturation".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Saturation threshold for star rejection (default: 0.98)".to_string(),
                default:     Some("0.98".to_string()),
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

    let result      = compute_metrics_for_image(img, det_config)?;
    let path        = result.filename.clone();
    let plate_scale = derive_plate_scale(&img.keywords);

    let _ = img;

    *ctx.analysis_result_for(&path) = result.clone();

    let fwhm_str = match (result.fwhm, plate_scale) {
        (Some(px), Some(ps)) => format!("{:.2}px / {:.2}\"", px, px * ps),
        (Some(px), None)     => format!("{:.2}px", px),
        _                    => "n/a".to_string(),
    };

    let message = format!(
        "Background median:   {}\n\
         Background std dev:  {}\n\
         Background gradient: {}\n\
         SNR estimate:        {}\n\
         FWHM:                {}\n\
         Eccentricity:        {}\n\
         Star count:          {}",
        fmt_opt_adu(result.background_median),
        fmt_opt_adu(result.background_stddev),
        fmt_opt_adu(result.background_gradient),
        result.snr_estimate.map(|v| format!("{:.1}", v)).unwrap_or_else(|| "n/a".to_string()),
        fwhm_str,
        result.eccentricity.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "n/a".to_string()),
        result.star_count.map(|v| v.to_string()).unwrap_or_else(|| "n/a".to_string()),
    );

    Ok(PluginOutput::Data(json!({
        "plugin":               "AnalyzeFrames",
        "scope":                "current",
        "filename":             path,
        "background_median":    result.background_median,
        "background_stddev":    result.background_stddev,
        "background_gradient":  result.background_gradient,
        "snr_estimate":         result.snr_estimate,
        "fwhm_pixels":          result.fwhm,
        "eccentricity":         result.eccentricity,
        "star_count":           result.star_count,
        "message":              message,
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

    struct FrameSnapshot {
        path:     String,
        width:    u32,
        height:   u32,
        channels: u8,
        pixels:   crate::context::PixelData,
        keywords: std::collections::HashMap<String, KeywordEntry>,
    }

    let snapshots: Vec<FrameSnapshot> = ctx.file_list.iter().filter_map(|path| {
        let buf = ctx.image_buffers.get(path)?;
        let pixels = buf.pixels.as_ref()?.clone();
        Some(FrameSnapshot {
            path:     path.clone(),
            width:    buf.width,
            height:   buf.height,
            channels: buf.channels,
            pixels,
            keywords: buf.keywords.clone(),
        })
    }).collect();

    let total = snapshots.len();
    info!("AnalyzeFrames: Pass 1 — computing metrics for {} frames", total);

    let det_config_ref = det_config;

    let par_results: Vec<Result<AnalysisResult, (String, String)>> = snapshots
        .par_iter()
        .map(|snap| {
            let channels = snap.channels as usize;
            let width    = snap.width as usize;
            let height   = snap.height as usize;

            let luma      = analysis::to_luminance(&snap.pixels, channels);
            let bg_config = BackgroundConfig::default();
            let bg        = compute_background_metrics(&luma, width, height, &bg_config);
            let stars     = detect_stars(&luma, width, height, det_config_ref);
            let plate_scale = derive_plate_scale(&snap.keywords);
            let fwhm_result = compute_fwhm(&stars, plate_scale);
            let ecc_result  = compute_eccentricity(&stars);
            let snr_result  = snr_estimate(&luma, width, height, &stars, &bg_config.sigma_clip);

            let result = AnalysisResult {
                filename:            snap.path.clone(),
                background_median:   Some(bg.median),
                background_stddev:   Some(bg.stddev),
                background_gradient: Some(bg.gradient),
                snr_estimate:        snr_result.map(|r| r.snr),
                fwhm:                fwhm_result.as_ref().map(|r| r.fwhm_pixels),
                eccentricity:        ecc_result.as_ref().map(|r| r.eccentricity),
                star_count:          fwhm_result.as_ref().map(|r| r.star_count as u32)
                    .or_else(|| ecc_result.as_ref().map(|r| r.star_count as u32))
                    .or_else(|| Some(stars.len() as u32)),
                flag: None,
                triggered_by: vec![],
            };

            info!("AnalyzeFrames: {} — done", short_name(&snap.path));
            Ok(result)
        })
        .collect();

    let mut results: Vec<AnalysisResult> = Vec::with_capacity(total);
    let mut errors:  Vec<String>         = Vec::new();

    for r in par_results {
        match r {
            Ok(result) => results.push(result),
            Err((path, msg)) => errors.push(format!("{}: {}", path, msg)),
        }
    }

    if results.is_empty() {
        return Err(PluginError::new(
            "NO_RESULTS",
            "Could not compute metrics for any frame.",
        ));
    }

    // ── Pass 2: session stats → classify → write PXFLAG keyword ──────────────
    info!("AnalyzeFrames: Pass 2 — classifying {} frames", results.len());

    let result_refs: Vec<&AnalysisResult> = results.iter().collect();
    let session_stats = compute_session_stats(&result_refs);

    let mut pass_count   = 0u32;
    let mut reject_count = 0u32;
    let mut frame_summaries: Vec<serde_json::Value> = Vec::new();

    for result in &mut results {
        let (flag, triggered) = classify_frame(result, &session_stats, &thresholds);
        result.flag = Some(flag.clone());
        result.triggered_by = triggered.clone();

        // Write PXFLAG keyword to image buffer
        if let Some(buf) = ctx.image_buffers.get_mut(&result.filename) {
            buf.keywords.insert(
                "PXFLAG".to_string(),
                KeywordEntry::new("PXFLAG", flag.as_str(), Some("Photyx frame quality flag")),
            );
        }

        match flag {
            crate::analysis::PxFlag::Pass   => pass_count   += 1,
            crate::analysis::PxFlag::Reject => reject_count += 1,
        }

        frame_summaries.push(json!({
            "filename":    short_name(&result.filename),
            "flag":        result.flag.as_ref().map(|f| f.as_str()).unwrap_or("?"),
            "triggered":   triggered,
            "fwhm":        result.fwhm,
            "ecc":         result.eccentricity,
            "snr":         result.snr_estimate,
            "stars":       result.star_count,
        }));

        ctx.analysis_results.insert(result.filename.clone(), result.clone());
    }

    ctx.last_analysis_thresholds = Some(thresholds);

    let message = format!(
        "AnalyzeFrames complete: {} frames — {} PASS, {} REJECT{}",
        results.len(),
        pass_count,
        reject_count,
        if errors.is_empty() { String::new() } else { format!(" ({} errors)", errors.len()) }
    );

    info!("{}", message);

    Ok(PluginOutput::Data(json!({
        "plugin":       "AnalyzeFrames",
        "scope":        "all",
        "frame_count":  results.len(),
        "pass_count":   pass_count,
        "reject_count": reject_count,
        "errors":       errors,
        "frames":       frame_summaries,
        "message":      message,
    })))
}

// ── Metric computation for a single image ────────────────────────────────────

fn compute_metrics_for_image(
    img:        &ImageBuffer,
    det_config: &StarDetectionConfig,
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
    let snr_result  = snr_estimate(&luma, width, height, &stars, &bg_config.sigma_clip);

    Ok(AnalysisResult {
        filename:            img.filename.clone(),
        background_median:   Some(bg.median),
        background_stddev:   Some(bg.stddev),
        background_gradient: Some(bg.gradient),
        snr_estimate:        snr_result.map(|r| r.snr),
        fwhm:                fwhm_result.as_ref().map(|r| r.fwhm_pixels),
        eccentricity:        ecc_result.as_ref().map(|r| r.eccentricity),
        star_count:          fwhm_result.as_ref().map(|r| r.star_count as u32)
            .or_else(|| ecc_result.as_ref().map(|r| r.star_count as u32))
            .or_else(|| Some(stars.len() as u32)),
        flag: None,
        triggered_by: vec![],
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


// ----------------------------------------------------------------------
