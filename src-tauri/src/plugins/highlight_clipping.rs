// plugins/highlight_clipping.rs — HighlightClipping and SnrEstimate plugins
// Spec §7.8, §15.4
//
// Both plugins live in the same file since they share pixel-preparation logic
// and are logically paired as the two non-star-shape metrics.

use crate::analysis::{
    self,
    metrics::{highlight_clipping, snr_estimate, CLIP_THRESHOLD},
    stars::detect_stars,
    BackgroundConfig, StarDetectionConfig,
};
use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use serde_json::json;

// ── Shared image preparation ──────────────────────────────────────────────────

struct PreparedLuma {
    luma:   Vec<f32>,
    width:  usize,
    height: usize,
    path:   String,
}

fn prepare_luma(ctx: &AppContext) -> Result<PreparedLuma, PluginError> {
    let img = ctx.current_image().ok_or_else(|| {
        PluginError::new("NO_IMAGE", "No image loaded.")
    })?;
    let pixels = img.pixels.as_ref().ok_or_else(|| {
        PluginError::new("NO_PIXELS", "Image buffer contains no pixel data.")
    })?;
    let normalized = analysis::to_f32_normalized(pixels);
    let channels   = img.channels as usize;
    let width      = img.width  as usize;
    let height     = img.height as usize;
    let luma       = analysis::extract_luminance(&normalized, width, height, channels);
    Ok(PreparedLuma { luma, width, height, path: img.filename.clone() })
}

// ── HighlightClipping plugin ──────────────────────────────────────────────────

pub struct HighlightClippingPlugin;

impl PhotonPlugin for HighlightClippingPlugin {
    fn name(&self)        -> &str { "HighlightClipping" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes the fraction of pixels at or above the highlight clipping \
         threshold (0.995) for the current frame. Returns value as a percentage."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![] // No parameters — threshold is fixed per spec
    }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let prepped  = prepare_luma(ctx)?;
        let fraction = highlight_clipping(&prepped.luma);
        let pct      = fraction * 100.0;

        {
            let ar = ctx.analysis_result_for(&prepped.path);
            ar.highlight_clipping = Some(fraction);
        }

        Ok(PluginOutput::Data(json!({
            "plugin":              "HighlightClipping",
            "filename":            prepped.path,
            "highlight_clipping":  fraction,
            "highlight_clipping_pct": pct,
            "clip_threshold":      CLIP_THRESHOLD,
            "message": format!("Highlight clipping: {:.4}%", pct),
        })))
    }
}

// ── SnrEstimate plugin ────────────────────────────────────────────────────────

pub struct SnrEstimatePlugin;

impl PhotonPlugin for SnrEstimatePlugin {
    fn name(&self)        -> &str { "SnrEstimate" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Estimates the signal-to-noise ratio of the current frame. \
         Signal is the median of background-subtracted star pixels; \
         noise is the sigma-clipped background standard deviation. \
         Result is relative — use for comparing frames within a session."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "threshold".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Star detection threshold in units of background std dev (default: 5.0)".to_string(),
                default:     Some("5.0".to_string()),
            },
            ParamSpec {
                name:        "peak_radius".to_string(),
                param_type:  ParamType::Integer,
                required:    false,
                description: "Radius in pixels for local maximum test (default: 3)".to_string(),
                default:     Some("3".to_string()),
            },
            ParamSpec {
                name:        "saturation".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Saturation threshold for star rejection (default: 0.98)".to_string(),
                default:     Some("0.98".to_string()),
            },
            ParamSpec {
                name:        "sigma".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Sigma-clipping threshold for background estimation (default: 3.0)".to_string(),
                default:     Some("3.0".to_string()),
            },
            ParamSpec {
                name:        "iterations".to_string(),
                param_type:  ParamType::Integer,
                required:    false,
                description: "Maximum sigma-clipping iterations (default: 5)".to_string(),
                default:     Some("5".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        // ── Parse configs ─────────────────────────────────────────────────────
        let mut det_config = StarDetectionConfig::default();
        let mut bg_config  = BackgroundConfig::default();

        if let Some(s) = args.get("threshold") {
            det_config.detection_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("threshold", "must be a positive float")
            })?;
        }
        if let Some(s) = args.get("peak_radius") {
            det_config.peak_radius = s.parse::<u32>().map_err(|_| {
                PluginError::invalid_arg("peak_radius", "must be a positive integer")
            })?;
        }
        if let Some(s) = args.get("saturation") {
            det_config.saturation_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("saturation", "must be a float between 0.0 and 1.0")
            })?;
        }
        if let Some(s) = args.get("sigma") {
            det_config.sigma_clip.sigma    = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("sigma", "must be a positive float")
            })?;
            bg_config.sigma_clip.sigma = det_config.sigma_clip.sigma;
        }
        if let Some(s) = args.get("iterations") {
            det_config.sigma_clip.iterations    = s.parse::<u32>().map_err(|_| {
                PluginError::invalid_arg("iterations", "must be a positive integer")
            })?;
            bg_config.sigma_clip.iterations = det_config.sigma_clip.iterations;
        }

        // ── Prepare image ─────────────────────────────────────────────────────
        let prepped = prepare_luma(ctx)?;

        // ── Detect stars ──────────────────────────────────────────────────────
        let stars = detect_stars(
            &prepped.luma,
            prepped.width,
            prepped.height,
            &det_config,
        );

        if stars.is_empty() {
            return Err(PluginError::new(
                "NO_STARS",
                "No stars detected. Try lowering the threshold parameter.",
            ));
        }

        // ── Compute SNR ───────────────────────────────────────────────────────
        let result = snr_estimate(
            &prepped.luma,
            prepped.width,
            prepped.height,
            &stars,
            &bg_config.sigma_clip,
        ).ok_or_else(|| {
            PluginError::new("SNR_FAILED", "Could not estimate SNR — star regions may be empty.")
        })?;

        // ── Store result ──────────────────────────────────────────────────────
        {
            let ar = ctx.analysis_result_for(&prepped.path);
            ar.snr_estimate = Some(result.snr);
        }

        Ok(PluginOutput::Data(json!({
            "plugin":        "SnrEstimate",
            "filename":      prepped.path,
            "snr":           result.snr,
            "signal_median": result.signal_median,
            "noise":         result.noise,
            "star_pixels":   result.star_pixels,
            "message": format!(
                "SNR estimate: {:.1} (signal: {:.4}, noise: {:.4}, {} star pixels)",
                result.snr, result.signal_median, result.noise, result.star_pixels
            ),
        })))
    }
}


// ----------------------------------------------------------------------
