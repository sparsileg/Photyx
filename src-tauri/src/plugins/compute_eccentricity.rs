// plugins/compute_eccentricity.rs — ComputeEccentricity plugin
// Spec §7.8, §15.4

use crate::analysis::{
    self,
    eccentricity::compute_eccentricity,
    stars::detect_stars,
    StarDetectionConfig,
};
use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use serde_json::json;

pub struct ComputeEccentricity;

impl PhotonPlugin for ComputeEccentricity {
    fn name(&self)        -> &str { "ComputeEccentricity" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes the median eccentricity of detected stars in the current frame. \
         Eccentricity ranges from 0.0 (perfectly circular) to 1.0 (fully elongated). \
         High values indicate tracking errors, poor seeing, or optical issues."
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
                description: "Saturation threshold — stars at or above this value are rejected (default: 0.98)".to_string(),
                default:     Some("0.98".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        // ── Parse detection config ────────────────────────────────────────────
        let mut det_config = StarDetectionConfig::default();
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

        // ── Prepare image ─────────────────────────────────────────────────────
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
        let path       = img.filename.clone();

        let luma = analysis::extract_luminance(&normalized, width, height, channels);

        // ── Detect stars ──────────────────────────────────────────────────────
        let stars = detect_stars(&luma, width, height, &det_config);

        if stars.is_empty() {
            return Err(PluginError::new(
                "NO_STARS",
                "No stars detected. Try lowering the threshold parameter.",
            ));
        }

        // ── Compute eccentricity ──────────────────────────────────────────────
        let result = compute_eccentricity(&stars).ok_or_else(|| {
            PluginError::new("ECC_FAILED", "Could not measure eccentricity — star patches may be too small.")
        })?;

        // ── Store result ──────────────────────────────────────────────────────
        {
            let ar = ctx.analysis_result_for(&path);
            ar.eccentricity = Some(result.eccentricity);
            ar.star_count   = Some(result.star_count as u32);
        }

        // ── Build response ────────────────────────────────────────────────────
        let message = format!(
            "Eccentricity: {:.3} ({} stars)",
            result.eccentricity, result.star_count
        );

        Ok(PluginOutput::Data(json!({
            "plugin":         "ComputeEccentricity",
            "filename":       path,
            "eccentricity":   result.eccentricity,
            "star_count":     result.star_count,
            "rejected_count": result.rejected_count,
            "message":        message,
        })))
    }
}


// ----------------------------------------------------------------------
