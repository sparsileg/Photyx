// plugins/auto_crop.rs — AutoCrop built-in plugin
// Crops ctx.stack_result to the well-covered region using ctx.stack_coverage.
// Parameters: x, y, w, h (all required — computed by get_autocrop_preview)

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, ImageBuffer, PixelData};
use tracing::info;

pub struct AutoCrop;

impl PhotonPlugin for AutoCrop {
    fn name(&self) -> &str { "AutoCrop" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str {
        "Crops the stack result to the well-covered region, removing border stacking artifacts."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "x".to_string(),
                param_type:  ParamType::Integer,
                required:    true,
                description: "Left edge of crop rectangle in pixels".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "y".to_string(),
                param_type:  ParamType::Integer,
                required:    true,
                description: "Top edge of crop rectangle in pixels".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "w".to_string(),
                param_type:  ParamType::Integer,
                required:    true,
                description: "Width of crop rectangle in pixels".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "h".to_string(),
                param_type:  ParamType::Integer,
                required:    true,
                description: "Height of crop rectangle in pixels".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let x = args.get("x").and_then(|v| v.parse::<usize>().ok())
            .ok_or_else(|| PluginError::new("MISSING_ARG", "Missing argument: x"))?;
        let y = args.get("y").and_then(|v| v.parse::<usize>().ok())
            .ok_or_else(|| PluginError::new("MISSING_ARG", "Missing argument: y"))?;
        let w = args.get("w").and_then(|v| v.parse::<usize>().ok())
            .ok_or_else(|| PluginError::new("MISSING_ARG", "Missing argument: w"))?;
        let h = args.get("h").and_then(|v| v.parse::<usize>().ok())
            .ok_or_else(|| PluginError::new("MISSING_ARG", "Missing argument: h"))?;

        apply_crop(ctx, x, y, w, h)
    }
}

/// Apply a crop to ctx.stack_result in place.
pub fn apply_crop(
    ctx: &mut AppContext,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
) -> Result<PluginOutput, PluginError> {
    let buffer = ctx.stack_result.as_ref()
        .ok_or_else(|| PluginError::new("NO_STACK", "No stack result available."))?;

    let src_w    = buffer.width  as usize;
    let src_h    = buffer.height as usize;
    let channels = buffer.channels as usize;

    // Validate crop bounds
    if x + w > src_w || y + h > src_h || w == 0 || h == 0 {
        return Err(PluginError::new("INVALID_CROP",
            &format!("Crop rectangle ({},{},{},{}) exceeds image bounds ({}×{})",
                x, y, w, h, src_w, src_h)));
    }

    let pixels = match &buffer.pixels {
        Some(PixelData::F32(v)) => v.clone(),
        _ => return Err(PluginError::new("PIXEL_FORMAT", "Stack result must be F32.")),
    };

    // Extract cropped region row by row
    let mut cropped = Vec::with_capacity(w * h * channels);
    for row in y..y + h {
        let row_start = (row * src_w + x) * channels;
        let row_end   = row_start + w * channels;
        cropped.extend_from_slice(&pixels[row_start..row_end]);
    }

    // Crop the coverage buffer too if present
    if let Some(coverage) = ctx.stack_coverage.as_ref() {
        let mut cropped_cov = Vec::with_capacity(w * h);
        for row in y..y + h {
            let row_start = row * src_w + x;
            cropped_cov.extend_from_slice(&coverage[row_start..row_start + w]);
        }
        ctx.stack_coverage = Some(cropped_cov);
    }

    let old_buf = ctx.stack_result.as_ref().unwrap();
    let new_buf = ImageBuffer {
        filename:      old_buf.filename.clone(),
        width:         w as u32,
        height:        h as u32,
        display_width: w as u32,
        bit_depth:     old_buf.bit_depth.clone(),
        color_space:   old_buf.color_space.clone(),
        channels:      old_buf.channels,
        keywords:      old_buf.keywords.clone(),
        pixels:        Some(PixelData::F32(cropped)),
    };

    ctx.stack_result = Some(new_buf);

    info!("AutoCrop: cropped stack result to {}×{} at ({},{})", w, h, x, y);

    Ok(PluginOutput::Message(format!(
        "AutoCrop: stack result cropped to {}×{}", w, h
    )))
}

// ── Crop boundary detection ───────────────────────────────────────────────────
// Scans inward from each edge of the coverage buffer until a row/column
// reaches the coverage threshold. Returns (x, y, w, h).

pub fn detect_crop_rect(
    coverage: &[u32],
    width:    usize,
    height:   usize,
    threshold_fraction: f32,
) -> (usize, usize, usize, usize) {
    // Find the median coverage count across all pixels
    let mut counts: Vec<u32> = coverage.iter().cloned().filter(|&c| c > 0).collect();
    if counts.is_empty() {
        return (0, 0, width, height);
    }
    counts.sort_unstable();
    let median_count = counts[counts.len() / 2];
    let threshold    = (median_count as f32 * threshold_fraction).ceil() as u32;

    // Row coverage: fraction of pixels in each row meeting the threshold
    let row_ok = |row: usize| -> bool {
        let start = row * width;
        let valid = coverage[start..start + width].iter().filter(|&&c| c >= threshold).count();
        valid as f32 / width as f32 >= 0.80
    };

    // Column coverage: fraction of pixels in each column meeting the threshold
    let col_ok = |col: usize| -> bool {
        let valid = (0..height).filter(|&row| coverage[row * width + col] >= threshold).count();
        valid as f32 / height as f32 >= 0.80
    };

    let top    = (0..height).find(|&r| row_ok(r)).unwrap_or(0);
    let bottom = (0..height).rev().find(|&r| row_ok(r)).unwrap_or(height - 1);
    let left   = (0..width).find(|&c| col_ok(c)).unwrap_or(0);
    let right  = (0..width).rev().find(|&c| col_ok(c)).unwrap_or(width - 1);

    let x = left;
    let y = top;
    let w = if right >= left { right - left + 1 } else { width };
    let h = if bottom >= top { bottom - top + 1 } else { height };

    (x, y, w, h)
}
