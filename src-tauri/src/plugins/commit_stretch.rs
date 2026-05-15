// plugins/commit_stretch.rs — CommitStretch built-in plugin
// Applies Auto-STF stretch permanently to ctx.stack_result pixel buffer.
// Parameters: shadow_clip (float, optional), target_bg (float, optional)

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, ColorSpace, PixelData};
use crate::plugins::auto_stretch::{compute_stf_params, mtf};
use tracing::info;

pub struct CommitStretch;

impl PhotonPlugin for CommitStretch {
    fn name(&self) -> &str { "CommitStretch" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str {
        "Permanently applies Auto-STF stretch to the stack result pixel buffer."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "shadow_clip".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Shadow clipping factor (default: context value)".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "target_bg".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Target background value 0.0–1.0 (default: context value)".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let shadow_clip = args.get("shadow_clip")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(ctx.autostretch_shadow_clip);

        let target_bg = args.get("target_bg")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(ctx.autostretch_target_bg);

        let buffer = ctx.stack_result.as_mut()
            .ok_or_else(|| PluginError::new("NO_STACK", "No stack result available."))?;

        let channels  = buffer.channels as usize;
        let is_rgb    = channels == 3 && buffer.color_space == ColorSpace::RGB;
        let n_ch      = if is_rgb { 3 } else { 1 };
        let n_pixels  = buffer.width as usize * buffer.height as usize;

        let pixels = match &buffer.pixels {
            Some(PixelData::F32(v)) => v.clone(),
            Some(PixelData::U16(v)) => v.iter().map(|&p| p as f32 / 65535.0).collect(),
            Some(PixelData::U8(v))  => v.iter().map(|&p| p as f32 / 255.0).collect(),
            None => return Err(PluginError::new("NO_PIXELS", "Stack result has no pixel data.")),
        };

        // Extract per-channel sample vectors for STF parameter computation
        let channel_samples: Vec<Vec<f32>> = (0..n_ch)
            .map(|ch| {
                (0..n_pixels)
                    .map(|i| pixels[i * channels + ch])
                    .filter(|v| v.is_finite())
                    .collect()
            })
            .collect();

        // Compute STF params per channel
        let stf_params: Vec<(f32, f32)> = channel_samples.iter()
            .map(|ch| compute_stf_params(ch, shadow_clip, target_bg))
            .collect();

        info!(
            "CommitStretch: shadow_clip={:.2} target_bg={:.2} params={:?}",
            shadow_clip, target_bg, stf_params
        );

        // Apply stretch in place
        let mut stretched = pixels.clone();
        for i in 0..n_pixels {
            for ch in 0..n_ch {
                let idx       = i * channels + ch;
                let (c0, m)   = stf_params[ch];
                let c0_range  = 1.0 - c0;
                let clipped   = ((stretched[idx] - c0) / c0_range).clamp(0.0, 1.0);
                stretched[idx] = mtf(m, clipped);
            }
        }

        buffer.pixels = Some(PixelData::F32(stretched));

        Ok(PluginOutput::Message(format!(
            "CommitStretch: stretch applied (shadow_clip={:.2}, target_bg={:.2})",
            shadow_clip, target_bg
        )))
    }
}
