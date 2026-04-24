// plugins/compute_fwhm.rs — ComputeFWHM plugin
// Spec §7.8, §15.4

use crate::analysis::{
    self,
    fwhm::compute_fwhm,
    profiles,
    stars::detect_stars,
    StarDetectionConfig,
};
use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use serde_json::json;

pub struct ComputeFWHM;

impl PhotonPlugin for ComputeFWHM {
    fn name(&self)        -> &str { "ComputeFWHM" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes the median Full Width at Half Maximum (FWHM) of detected stars \
         in the current frame. Reports result in pixels and arcseconds (when \
         FOCALLEN, INSTRUME, and XBINNING keywords are present)."
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

        // ── Plate scale from keywords ─────────────────────────────────────────
        let plate_scale = derive_plate_scale(&img.keywords);

        let luma = analysis::extract_luminance(&normalized, width, height, channels);

        // ── Detect stars ──────────────────────────────────────────────────────
        let stars = detect_stars(&luma, width, height, &det_config);

        if stars.is_empty() {
            return Err(PluginError::new(
                "NO_STARS",
                "No stars detected. Try lowering the threshold parameter.",
            ));
        }

        // ── Compute FWHM ──────────────────────────────────────────────────────
        let result = compute_fwhm(&stars, plate_scale).ok_or_else(|| {
            PluginError::new("FWHM_FAILED", "Could not measure FWHM — star profiles may be too small or flat.")
        })?;

        // ── Store result ──────────────────────────────────────────────────────
        {
            let ar = ctx.analysis_result_for(&path);
            ar.fwhm       = Some(result.fwhm_pixels);
            ar.star_count = Some(result.star_count as u32);
        }

        // ── Build response ────────────────────────────────────────────────────
        let message = match result.fwhm_arcsec {
            Some(arcsec) => format!(
                "FWHM: {:.2}px / {:.2}\" ({} stars)",
                result.fwhm_pixels, arcsec, result.star_count
            ),
            None => format!(
                "FWHM: {:.2}px ({} stars) — no plate scale available",
                result.fwhm_pixels, result.star_count
            ),
        };

        let mut data = json!({
            "plugin":          "ComputeFWHM",
            "filename":        path,
            "fwhm_pixels":     result.fwhm_pixels,
            "star_count":      result.star_count,
            "rejected_count":  result.rejected_count,
            "message":         message,
        });

        if let Some(arcsec) = result.fwhm_arcsec {
            data["fwhm_arcsec"]   = json!(arcsec);
            data["plate_scale"]   = json!(plate_scale);
        }

        Ok(PluginOutput::Data(data))
    }
}

// ── Plate scale derivation ────────────────────────────────────────────────────

fn derive_plate_scale(
    keywords: &std::collections::HashMap<String, crate::context::KeywordEntry>,
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


// ----------------------------------------------------------------------
