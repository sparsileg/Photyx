// plugins/star_count.rs — CountStars plugin
// Spec §7.8, §15.4
//
// Thin wrapper over analysis::stars::detect_stars.
// Stores star_count in AnalysisResult for the current frame.
// The detected star list is not stored in AppContext — FWHM and eccentricity
// plugins will re-run detection when they are implemented. If this proves
// expensive, a star candidate cache can be added to AppContext in a later pass.

use crate::analysis::{stars::detect_stars, StarDetectionConfig};
use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotyxPlugin, PluginError, PluginOutput};
use crate::plugins::pixel_chunking::{self, LoadKind, LoadedFrame, LoadOutcome};
use serde_json::json;

pub struct CountStarsPlugin;

impl PhotyxPlugin for CountStarsPlugin {
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

        // ── Resolve current frame's session path ─────────────────────────────
        let session_path = ctx.file_list.get(ctx.current_frame).cloned().ok_or_else(|| {
            PluginError::new("NO_IMAGE", "No current frame in session.")
        })?;

        // ── Load luma via the shared pixel_chunking pipeline ─────────────────
        // Uses the exact same decode → debayer (if Bayer) → luminance path as
        // AnalyzeFrames (pixel_chunking::load_request with LoadKind::Luma),
        // instead of a locally re-implemented normalize/extract-luminance
        // pass over ctx.current_image().pixels. That local re-implementation
        // never debayered Bayer sources, which let standalone CountStars
        // diverge from AnalyzeFrames' batch star counts on OSC sessions.
        let snap = match pixel_chunking::load_request(&session_path, LoadKind::Luma) {
            LoadOutcome::Loaded(LoadedFrame::Luma(snap)) => snap,
            LoadOutcome::Loaded(_) => {
                return Err(PluginError::new(
                    "INTERNAL_ERROR",
                    "CountStars: loader returned a non-Luma LoadedFrame for a Luma request",
                ));
            }
            LoadOutcome::Missing { path } => {
                return Err(PluginError::new(
                    "SOURCE_FILE_MISSING",
                    &format!("Source file missing: {}", path),
                ));
            }
            LoadOutcome::Unreadable { path, error } => {
                return Err(PluginError::new(
                    "SOURCE_FILE_UNREADABLE",
                    &format!("Source file unreadable: {} ({})", path, error),
                ));
            }
        };

        let path = short_name(&snap.path).to_string();

        // ── Detect stars ──────────────────────────────────────────────────────
        let stars = detect_stars(&snap.luma, snap.width, snap.height, &config);
        let count = stars.len() as u32;

        // ── Store result ──────────────────────────────────────────────────────
        {
            let result = ctx.analysis_result_for(&session_path);
            result.star_count = Some(count);
        }
        // Uppercase key so $starcount and $STARCOUNT resolve identically —
        // Issue 118.
        ctx.variables.insert("STARCOUNT".to_string(), count.to_string());

        Ok(PluginOutput::Data(json!({
            "plugin":     "CountStars",
            "filename":   path,
            "star_count": count,
            "message":    format!("Detected {} star{}", count, if count == 1 { "" } else { "s" }),
        })))
    }
}

fn short_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}


// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
