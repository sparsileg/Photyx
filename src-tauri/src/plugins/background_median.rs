// plugins/background_median.rs — BackgroundMedian, BackgroundStdDev, BackgroundGradient plugins
// Spec §15.4, §7.8
//
// Three thin plugin wrappers over analysis::background::compute_background_metrics.
// All three share identical pixel-preparation logic; only which field they report differs.
// Running any one of them populates all three fields in the AnalysisResult for that frame,
// so there is no redundant computation if the user runs them in sequence.

use crate::analysis::{
    self,
    background::{compute_background_metrics, BackgroundMetrics},
    BackgroundConfig
};
use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use serde_json::json;

// ── Shared pixel preparation ──────────────────────────────────────────────────

struct PreparedImage {
    luma:   Vec<f32>,
    width:  usize,
    height: usize,
    path:   String,
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

    Ok(PreparedImage {
        luma,
        width,
        height,
        path: img.filename.clone(),
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

    if let Some(s) = args.get("grid") {
        config.gradient_grid_size = s.parse::<u32>().map_err(|_| {
            PluginError::invalid_arg("grid", "must be a positive integer (e.g. grid=4)")
        })?;
    }

    Ok(config)
}

/// Run background metrics and store all three results in AppContext.
/// Returns the BackgroundMetrics for the caller to select which value to surface.
fn run_and_store(
    ctx:    &mut AppContext,
    args:   &ArgMap,
) -> Result<(BackgroundMetrics, String), PluginError> {
    let config  = parse_config(args)?;
    let prepped = prepare_current_image(ctx)?;

    let metrics = compute_background_metrics(
        &prepped.luma,
        prepped.width,
        prepped.height,
        &config,
    );

    // Store all three results — no wasted work if user runs all three plugins
    {
        let result = ctx.analysis_result_for(&prepped.path);
        result.background_median   = Some(metrics.median);
        result.background_stddev   = Some(metrics.stddev);
        result.background_gradient = Some(metrics.gradient);
    }

    Ok((metrics, prepped.path))
}

// ── BackgroundMedian plugin ───────────────────────────────────────────────────

pub struct BackgroundMedianPlugin;

impl PhotonPlugin for BackgroundMedianPlugin {
    fn name(&self)        -> &str { "BackgroundMedian" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes the sigma-clipped background median for the current frame. \
         Also computes and stores background std dev and gradient as a side effect."
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
            ParamSpec {
                name:        "grid".to_string(),
                param_type:  ParamType::Integer,
                required:    false,
                description: "Grid divisions per axis for gradient estimation (default: 4)".to_string(),
                default:     Some("4".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let (metrics, path) = run_and_store(ctx, args)?;

        let median_adu = (metrics.median * 65535.0).round() as u32;

        Ok(PluginOutput::Data(json!({
            "plugin":   "BackgroundMedian",
            "filename": path,
            "background_median":   metrics.median,
            "background_median_adu": median_adu,
            "background_stddev":   metrics.stddev,
            "background_gradient": metrics.gradient,
            "message": format!(
                "Background median: {:.4} ({} ADU)",
                metrics.median, median_adu
            ),
        })))
    }
}

// ── BackgroundStdDev plugin ───────────────────────────────────────────────────

pub struct BackgroundStdDevPlugin;

impl PhotonPlugin for BackgroundStdDevPlugin {
    fn name(&self)        -> &str { "BackgroundStdDev" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes the sigma-clipped background standard deviation for the current frame. \
         Also computes and stores background median and gradient as a side effect."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        // Same parameters as BackgroundMedian
        BackgroundMedianPlugin.parameters()
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let (metrics, path) = run_and_store(ctx, args)?;

        let stddev_adu = (metrics.stddev * 65535.0).round() as u32;

        Ok(PluginOutput::Data(json!({
            "plugin":   "BackgroundStdDev",
            "filename": path,
            "background_median":   metrics.median,
            "background_stddev":   metrics.stddev,
            "background_stddev_adu": stddev_adu,
            "background_gradient": metrics.gradient,
            "message": format!(
                "Background std dev: {:.4} ({} ADU)",
                metrics.stddev, stddev_adu
            ),
        })))
    }
}

// ── BackgroundGradient plugin ─────────────────────────────────────────────────

pub struct BackgroundGradientPlugin;

impl PhotonPlugin for BackgroundGradientPlugin {
    fn name(&self)        -> &str { "BackgroundGradient" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Computes the background gradient (max − min cell background median across a grid) \
         for the current frame. Also computes and stores background median and std dev."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        BackgroundMedianPlugin.parameters()
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let (metrics, path) = run_and_store(ctx, args)?;

        let gradient_adu = (metrics.gradient * 65535.0).round() as u32;

        Ok(PluginOutput::Data(json!({
            "plugin":   "BackgroundGradient",
            "filename": path,
            "background_median":   metrics.median,
            "background_stddev":   metrics.stddev,
            "background_gradient": metrics.gradient,
            "background_gradient_adu": gradient_adu,
            "message": format!(
                "Background gradient: {:.4} ({} ADU)",
                metrics.gradient, gradient_adu
            ),
        })))
    }
}


// ----------------------------------------------------------------------
