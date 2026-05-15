// plugins/debayer_image.rs — DebayerImage built-in plugin
// Spec §15.4 (DebayerImage)
//
// Debayers the current stack result (if Bayer) or the current session frame.
// Uses bilinear interpolation. Reads BAYERPAT keyword for pattern; defaults to RGGB.

use crate::analysis::debayer::{debayer_bilinear, BayerPattern};
use crate::context::{AppContext, ColorSpace, PixelData};
use crate::plugin::{ArgMap, ParamSpec, PhotonPlugin, PluginError, PluginOutput};
use tracing::info;

pub struct DebayerImage;

impl PhotonPlugin for DebayerImage {
    fn name(&self) -> &str { "DebayerImage" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str {
        "Debayers a Bayer CFA image using bilinear interpolation. \
         Works on the stack result if present, otherwise the current frame."
    }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        // Prefer stack result; fall back to current session frame
        if ctx.stack_result.is_some() {
            debayer_stack(ctx)
        } else {
            debayer_current_frame(ctx)
        }
    }
}

/// Debayer ctx.stack_result in place.
pub fn debayer_stack(ctx: &mut AppContext) -> Result<PluginOutput, PluginError> {
    let buffer = ctx.stack_result.as_ref()
        .ok_or_else(|| PluginError::new("NO_STACK", "No stack result available."))?;

    if buffer.color_space != ColorSpace::Bayer && buffer.color_space != ColorSpace::Mono {
        return Ok(PluginOutput::Message("Stack result is already RGB — no debayering needed.".into()));
    }

    let pattern = buffer.keywords.get("BAYERPAT")
        .or_else(|| buffer.keywords.get("BAYER_PATTERN"))
        .map(|kw| BayerPattern::from_str(&kw.value))
        .unwrap_or(BayerPattern::RGGB);

    let width  = buffer.width  as usize;
    let height = buffer.height as usize;

    let mono: Vec<f32> = match &buffer.pixels {
        Some(PixelData::F32(v)) => v.clone(),
        Some(PixelData::U16(v)) => v.iter().map(|&p| p as f32 / 65535.0).collect(),
        Some(PixelData::U8(v))  => v.iter().map(|&p| p as f32 / 255.0).collect(),
        None => return Err(PluginError::new("NO_PIXELS", "Stack result has no pixel data.")),
    };

    info!("DebayerImage: debayering stack result {}×{} pattern={:?}", width, height, pattern);

    let rgb = debayer_bilinear(&mono, width, height, pattern);

    // Update the stack result buffer in place
    let buffer = ctx.stack_result.as_mut().unwrap();
    buffer.pixels      = Some(PixelData::F32(rgb));
    buffer.channels    = 3;
    buffer.color_space = ColorSpace::RGB;

    Ok(PluginOutput::Message(format!(
        "DebayerImage: stack result debayered ({:?}, bilinear) → RGB", pattern
    )))
}

/// Debayer the current session frame in place.
fn debayer_current_frame(ctx: &mut AppContext) -> Result<PluginOutput, PluginError> {
    let path = ctx.file_list.get(ctx.current_frame)
        .cloned()
        .ok_or_else(|| PluginError::new("NO_FRAME", "No current frame."))?;

    let buffer = ctx.image_buffers.get(&path)
        .ok_or_else(|| PluginError::new("NO_BUFFER", "Current frame not loaded."))?;

    if buffer.color_space == ColorSpace::RGB {
        return Ok(PluginOutput::Message("Frame is already RGB — no debayering needed.".into()));
    }

    let pattern = buffer.keywords.get("BAYERPAT")
        .or_else(|| buffer.keywords.get("BAYER_PATTERN"))
        .or_else(|| buffer.keywords.get("BAYERPAT"))
        .map(|kw| BayerPattern::from_str(&kw.value))
        .unwrap_or(BayerPattern::RGGB);

    let width  = buffer.width  as usize;
    let height = buffer.height as usize;

    let mono: Vec<f32> = match &buffer.pixels {
        Some(PixelData::F32(v)) => v.clone(),
        Some(PixelData::U16(v)) => v.iter().map(|&p| p as f32 / 65535.0).collect(),
        Some(PixelData::U8(v))  => v.iter().map(|&p| p as f32 / 255.0).collect(),
        None => return Err(PluginError::new("NO_PIXELS", "Frame has no pixel data.")),
    };

    info!("DebayerImage: debayering frame {}×{} pattern={:?}", width, height, pattern);

    let rgb = debayer_bilinear(&mono, width, height, pattern);

    let buffer = ctx.image_buffers.get_mut(&path).unwrap();
    buffer.pixels      = Some(PixelData::F32(rgb));
    buffer.channels    = 3;
    buffer.color_space = ColorSpace::RGB;

    // Invalidate display caches for this frame
    ctx.display_cache.remove(&path);
    ctx.full_res_cache.remove(&path);

    Ok(PluginOutput::Message(format!(
        "DebayerImage: frame debayered ({:?}, bilinear) → RGB", pattern
    )))
}
