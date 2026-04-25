// plugins/highlight_clipping.rs — SnrEstimate plugin
// Note: HighlightClipping metric has been removed (values too low to be useful).
// This file is retained for the SnrEstimate plugin.

use crate::analysis::{
    self,
    metrics::snr_estimate,
    stars::detect_stars,
    BackgroundConfig, StarDetectionConfig,
};
use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use serde_json::json;

// ── SnrEstimate plugin ────────────────────────────────────────────────────────

pub struct SnrEstimatePlugin;

impl PhotonPlugin for SnrEstimatePlugin {
    fn name(&self)        -> &str { "SnrEstimate" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Estimates the signal-to-noise ratio of the current frame. \
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
            det_config.sigma_clip.iterations = s.parse::<u32>().map_err(|_| {
                PluginError::invalid_arg("iterations", "must be a positive integer")
            })?;
            bg_config.sigma_clip.iterations = det_config.sigma_clip.iterations;
        }

        let img = ctx.current_image().ok_or_else(|| {
            PluginError::new("NO_IMAGE", "No image loaded.")
        })?;
        let pixels = img.pixels.as_ref().ok_or_else(|| {
            PluginError::new("NO_PIXELS", "Image buffer contains no pixel data.")
        })?;

        let channels = img.channels as usize;
        let width    = img.width  as usize;
        let height   = img.height as usize;
        let path     = img.filename.clone();

        let luma  = analysis::to_luminance(pixels, channels);
        let stars = detect_stars(&luma, width, height, &det_config);

        if stars.is_empty() {
            return Err(PluginError::new(
                "NO_STARS",
                "No stars detected. Try lowering the threshold parameter.",
            ));
        }

        let result = snr_estimate(
            &luma, width, height, &stars, &bg_config.sigma_clip,
        ).ok_or_else(|| {
            PluginError::new("SNR_FAILED", "Could not estimate SNR — star regions may be empty.")
        })?;

        {
            let ar = ctx.analysis_result_for(&path);
            ar.snr_estimate = Some(result.snr);
        }

        Ok(PluginOutput::Data(json!({
            "plugin":        "SnrEstimate",
            "filename":      path,
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
