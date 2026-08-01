// plugins/background_median.rs — BackgroundMedian plugin
// Spec §15.4, §7.8

// Thin plugin wrapper over analysis::background::compute_background_metrics.
// Populates background_median in the AnalysisResult for the current frame.

use crate::analysis::{
    self,
    background::{estimate_background, BackgroundEstimate},
    BackgroundConfig
};
use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotyxPlugin, PluginError, PluginOutput};
use serde_json::json;

// ── Shared pixel preparation ──────────────────────────────────────────────────

struct PreparedImage {
    luma:         Vec<f32>,
    path:         String,
    session_path: String,
}

fn prepare_current_image(ctx: &AppContext) -> Result<PreparedImage, PluginError> {
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

    let luma = analysis::extract_luminance(&normalized, width, height, channels);

    // Issue 117: analysis_results must be keyed by the session path
    // (matching execute_all's snap.path and file_list entries), not
    // img.filename — image_reader sets filename to the basename only,
    // which never matches a file_list entry and left ghost, unremovable
    // analysis_results entries. `path` remains the basename, used only
    // for display in each plugin variant's response/message.
    let session_path = ctx.file_list.get(ctx.current_frame).cloned().ok_or_else(|| {
        PluginError::new("NO_IMAGE", "No current frame in session.")
    })?;

    Ok(PreparedImage {
        luma,
        path: img.filename.clone(),
        session_path,
    })
}

/// Parse optional sigma-clip and gradient grid overrides from args.
fn parse_config(args: &ArgMap) -> Result<BackgroundConfig, PluginError> {
    let mut config = BackgroundConfig::default();

    if let Some(s) = args.get("sigma") {
        config.sigma_clip.sigma = s.parse::<f32>().map_err(|_| {
            PluginError::invalid_arg("sigma", "must be a positive float (e.g. sigma=3.0)")
        })?;
    }

    if let Some(s) = args.get("iterations") {
        config.sigma_clip.iterations = s.parse::<u32>().map_err(|_| {
            PluginError::invalid_arg("iterations", "must be a positive integer (e.g. iterations=5)")
        })?;
    }

    Ok(config)
}

/// Run background estimation and store background_median in AppContext.
fn run_and_store(
    ctx:  &mut AppContext,
    args: &ArgMap,
) -> Result<(BackgroundEstimate, String), PluginError> {
    let config  = parse_config(args)?;
    let prepped = prepare_current_image(ctx)?;

    let metrics = estimate_background(&prepped.luma, &config.sigma_clip);

    {
        let result = ctx.analysis_result_for(&prepped.session_path);
        result.background_median = Some(metrics.median);
    }

    Ok((metrics, prepped.path))
}

// ── BackgroundMedian plugin ───────────────────────────────────────────────────

pub struct BackgroundMedianPlugin;

impl PhotyxPlugin for BackgroundMedianPlugin {
    fn name(&self)        -> &str { "BackgroundMedian" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes the sigma-clipped background median for the current frame."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "sigma".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Sigma-clipping threshold in std dev units (default: 3.0)".to_string(),
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
        let (metrics, path) = run_and_store(ctx, args)?;

        let median_adu = (metrics.median * 65535.0).round() as u32;

        ctx.variables.insert("BACKGROUNDMEDIAN".to_string(), metrics.median.to_string());

        Ok(PluginOutput::Data(json!({
            "plugin":                "BackgroundMedian",
            "filename":              path,
            "background_median":     metrics.median,
            "background_median_adu": median_adu,
            "message": format!(
                "Background median: {:.4} ({} ADU)",
                metrics.median, median_adu
            ),
        })))
    }
}


// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
