// plugins/auto_stretch.rs — AutoStretch built-in native plugin
// Spec §12.2 — Auto-STF (PixInsight-compatible algorithm)

use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, PixelData, BitDepth};

pub struct AutoStretch;

impl PhotonPlugin for AutoStretch {
    fn name(&self) -> &str { "AutoStretch" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Applies automatic screen transfer function stretch" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "method".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Stretch method: asinh (default)".to_string(),
                default:     Some("asinh".to_string()),
            },
            ParamSpec {
                name:        "shadowclip".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Shadow clipping point (default 0.0)".to_string(),
                default:     Some("0.0".to_string()),
            },
            ParamSpec {
                name:        "targetbackground".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Target background value 0.0-1.0 (default 0.25)".to_string(),
                default:     Some("0.25".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let target_bg = args.get("targetbackground")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.25);

        let shadow_clip = args.get("shadowclip")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);

        let path = ctx.file_list.get(ctx.current_frame).cloned().ok_or_else(|| {
            PluginError::new("NO_IMAGE", "No image loaded. Use ReadAllFITFiles first.")
        })?;

        let buffer = ctx.image_buffers.get_mut(&path).ok_or_else(|| {
            PluginError::new("NO_IMAGE", "Current image buffer not found.")
        })?;

        let pixels = buffer.pixels.as_ref().ok_or_else(|| {
            PluginError::new("NO_PIXELS", "Image has no pixel data.")
        })?;

        // Normalize all pixels to f32 0.0-1.0
        let normalized: Vec<f32> = match pixels {
            PixelData::U8(v)  => v.iter().map(|&p| p as f32 / 255.0).collect(),
            PixelData::U16(v) => v.iter().map(|&p| p as f32 / 65535.0).collect(),
            PixelData::F32(v) => v.clone(),
        };

        // Apply Auto-STF
        let stretched = auto_stf(&normalized, shadow_clip as f32, target_bg as f32);

        buffer.pixels = Some(PixelData::F32(stretched));
        buffer.bit_depth = BitDepth::F32;

        info!("AutoStretch applied to: {}", path);
        Ok(PluginOutput::Message("AutoStretch applied.".to_string()))
    }
}

/// PixInsight-compatible Auto-STF algorithm
fn auto_stf(pixels: &[f32], shadow_clip: f32, target_bg: f32) -> Vec<f32> {
    // Sample up to 50000 pixels for statistics — much faster on large images
    let sample: Vec<f32> = if pixels.len() > 50_000 {
        let step = pixels.len() / 50_000;
        pixels.iter().step_by(step).cloned().collect()
    } else {
        pixels.to_vec()
    };

    // Compute median from sample
    let mut sorted = sample.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let median = sorted[n / 2];

    // Compute MAD from sample
    let mut deviations: Vec<f32> = sample.iter().map(|&p| (p - median).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mad = deviations[deviations.len() / 2];

    // Shadow clipping point c0
    let c0 = (median - shadow_clip * 1.4826 * mad).max(0.0).min(1.0);

    // Compute median of the shadow-clipped data for midtone parameter
    let clipped_median = {
        let clipped: Vec<f32> = pixels.iter()
            .filter(|&&p| p > c0)
            .map(|&p| (p - c0) / (1.0 - c0))
            .collect();
        if clipped.is_empty() {
            0.5
        } else {
            let mut cs = clipped.clone();
            cs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            cs[cs.len() / 2]
        }
    };

    // Midtone transfer function parameter
    let m = if clipped_median < f32::EPSILON {
        0.5
    } else {
        mtf(target_bg, clipped_median)
    };

    // Apply stretch
    pixels.iter().map(|&p| {
        let clipped = ((p - c0) / (1.0 - c0)).max(0.0).min(1.0);
        mtf(m, clipped)
    }).collect()
}

/// Midtone Transfer Function
fn mtf(m: f32, x: f32) -> f32 {
    if x <= 0.0 { return 0.0; }
    if x >= 1.0 { return 1.0; }
    if (m - 0.5).abs() < f32::EPSILON { return x; }
    (m - 1.0) * x / ((2.0 * m - 1.0) * x - m)
}
