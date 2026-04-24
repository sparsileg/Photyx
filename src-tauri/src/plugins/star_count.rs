// plugins/star_count.rs — CountStars plugin
// Spec §7.8, §15.4
//
// Thin wrapper over analysis::stars::detect_stars.
// Stores star_count in AnalysisResult for the current frame.
// The detected star list is not stored in AppContext — FWHM and eccentricity
// plugins will re-run detection when they are implemented. If this proves
// expensive, a star candidate cache can be added to AppContext in a later pass.

use crate::analysis::{self, stars::detect_stars, StarDetectionConfig};
use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use serde_json::json;

pub struct CountStarsPlugin;

impl PhotonPlugin for CountStarsPlugin {
    fn name(&self)        -> &str { "CountStars" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Counts the number of detected stars in the current frame using \
         peak-finding on a sigma-clipped background-subtracted image."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "threshold".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Detection threshold in units of background std dev (default: 5.0)".to_string(),
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
                name:        "flood_threshold".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Flood-fill lower bound in units of background std dev (default: 2.0)".to_string(),
                default:     Some("2.0".to_string()),
            },
            ParamSpec {
                name:        "saturation".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Peak value at or above which a star is considered saturated and rejected (default: 0.98)".to_string(),
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
                description: "Maximum sigma-clipping iterations for background estimation (default: 5)".to_string(),
                default:     Some("5".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        // ── Parse config ──────────────────────────────────────────────────────
        let mut config = StarDetectionConfig::default();

        if let Some(s) = args.get("threshold") {
            config.detection_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("threshold", "must be a positive float (e.g. threshold=5.0)")
            })?;
        }
        if let Some(s) = args.get("peak_radius") {
            config.peak_radius = s.parse::<u32>().map_err(|_| {
                PluginError::invalid_arg("peak_radius", "must be a positive integer (e.g. peak_radius=3)")
            })?;
        }
        if let Some(s) = args.get("flood_threshold") {
            config.flood_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("flood_threshold", "must be a positive float (e.g. flood_threshold=2.0)")
            })?;
        }
        if let Some(s) = args.get("saturation") {
            config.saturation_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("saturation", "must be a float between 0.0 and 1.0")
            })?;
        }
        if let Some(s) = args.get("sigma") {
            config.sigma_clip.sigma = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("sigma", "must be a positive float (e.g. sigma=3.0)")
            })?;
        }
        if let Some(s) = args.get("iterations") {
            config.sigma_clip.iterations = s.parse::<u32>().map_err(|_| {
                PluginError::invalid_arg("iterations", "must be a positive integer")
            })?;
        }

        // ── Prepare image ─────────────────────────────────────────────────────
        let img = ctx.current_image().ok_or_else(|| {
            PluginError::new("NO_IMAGE", "No image loaded. Load files before running analysis.")
        })?;

        let pixels = img.pixels.as_ref().ok_or_else(|| {
            PluginError::new("NO_PIXELS", "Image buffer contains no pixel data.")
        })?;

        let normalized = analysis::to_f32_normalized(pixels);
        let channels   = img.channels as usize;
        let width      = img.width  as usize;
        let height     = img.height as usize;
        let path       = img.filename.clone();

        let luma = analysis::extract_luminance(&normalized, width, height, channels);

        // ── Detect stars ──────────────────────────────────────────────────────
        let stars = detect_stars(&luma, width, height, &config);
        let count = stars.len() as u32;

        // ── Store result ──────────────────────────────────────────────────────
        {
            let result = ctx.analysis_result_for(&path);
            result.star_count = Some(count);
        }

        Ok(PluginOutput::Data(json!({
            "plugin":     "CountStars",
            "filename":   path,
            "star_count": count,
            "message":    format!("Detected {} star{}", count, if count == 1 { "" } else { "s" }),
        })))
    }
}


// ----------------------------------------------------------------------
