// plugins/auto_stretch.rs — AutoStretch built-in native plugin
// Spec §12.2 — Auto-STF (PixInsight-compatible algorithm)
//
// Design: operates entirely on a display-resolution copy of the image.
// The raw image_buffer is never modified. The result is returned as JPEG
// bytes directly to the caller — no caching. The get_autostretch_frame
// Tauri command calls compute_autostretch_jpeg() and returns a base64
// data URL to the frontend for immediate display.

use tracing::info;
use image::{RgbImage, ImageFormat};
use std::io::Cursor;

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, ColorSpace, PixelData};

// ── AutoStretch defaults ──────────────────────────────────────────────────
const DEFAULT_SHADOW_CLIP:       f32 = -2.8;
const DEFAULT_TARGET_BACKGROUND: f32 = 0.15;

pub struct AutoStretch;

impl PhotonPlugin for AutoStretch {
    fn name(&self) -> &str { "AutoStretch" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Applies automatic screen transfer function stretch" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "shadowclip".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: format!("Shadow clipping factor (default: {})", DEFAULT_SHADOW_CLIP),
                default:     Some(DEFAULT_SHADOW_CLIP.to_string()),
            },
            ParamSpec {
                name:        "targetbackground".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: format!("Target background value 0.0-1.0 (default: {})", DEFAULT_TARGET_BACKGROUND),
                default:     Some(DEFAULT_TARGET_BACKGROUND.to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let target_bg = args.get("targetbackground")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(ctx.autostretch_target_bg);

        let shadow_clip = args.get("shadowclip")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(ctx.autostretch_shadow_clip);

        let jpeg_bytes = compute_autostretch_jpeg(ctx, shadow_clip, target_bg)
            .map_err(|e| PluginError::new("AUTOSTRETCH_ERROR", &e))?;

        let byte_count = jpeg_bytes.len();

        info!("AutoStretch: {} JPEG bytes", byte_count);

        Ok(PluginOutput::Data(serde_json::json!({
            "message":       format!("AutoStretch applied ({} bytes)", byte_count),
            "client_action": "refresh_autostretch",
        })))
    }
}

// ── Public computation function ───────────────────────────────────────────────
// Called by both the plugin execute() and the get_autostretch_frame Tauri command.
// Returns raw JPEG bytes — no caching, no side effects on AppContext.

/// Compute an auto-stretched JPEG from the current session frame.
pub fn compute_autostretch_jpeg(
    ctx: &AppContext,
    shadow_clip: f32,
    target_bg: f32,
) -> Result<Vec<u8>, String> {
    let path = ctx.file_list.get(ctx.current_frame)
        .ok_or_else(|| "No image loaded".to_string())?;

    let buffer = ctx.image_buffers.get(path)
        .ok_or_else(|| "Image buffer not found".to_string())?;

    compute_autostretch_jpeg_from_buffer(buffer, shadow_clip, target_bg)
}

/// Compute an auto-stretched JPEG from any ImageBuffer directly.
pub fn compute_autostretch_jpeg_from_buffer(
    buffer: &crate::context::ImageBuffer,
    shadow_clip: f32,
    target_bg: f32,
) -> Result<Vec<u8>, String> {
    let pixels = buffer.pixels.as_ref()
        .ok_or_else(|| "No pixel data".to_string())?;

    let src_w    = buffer.width as usize;
    let src_h    = buffer.height as usize;
    let channels = buffer.channels as usize;
    let is_rgb   = channels == 3 && buffer.color_space == ColorSpace::RGB;

    // ── Downsample to display resolution ─────────────────────────────────────
    const MAX_DISPLAY_W: usize = 1200;
    let (disp_w, disp_h, step) = if src_w > MAX_DISPLAY_W {
        let step = (src_w + MAX_DISPLAY_W - 1) / MAX_DISPLAY_W;
        (src_w / step, src_h / step, step)
    } else {
        (src_w, src_h, 1)
    };

    let pixel_count = disp_w * disp_h;
    let num_display_channels = if is_rgb { 3 } else { 1 };

    let mut display_channels: Vec<Vec<f32>> = (0..num_display_channels)
        .map(|_| Vec::with_capacity(pixel_count))
        .collect();

    match pixels {
        PixelData::U16(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    for ch in 0..num_display_channels {
                        let mut sum = 0u32; let mut count = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                let idx = (sy * src_w + sx) * channels + ch;
                                sum += v[idx] as u32;
                                count += 1;
                            }
                        }
                        display_channels[ch].push(sum as f32 / (count as f32 * 65535.0));
                    }
                }
            }
        }
        PixelData::F32(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    for ch in 0..num_display_channels {
                        let mut sum = 0.0f32; let mut count = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                let idx = (sy * src_w + sx) * channels + ch;
                                let val = v[idx];
                                if val.is_finite() { sum += val; count += 1; }
                            }
                        }
                        display_channels[ch].push(if count > 0 { sum / count as f32 } else { 0.0 });
                    }
                }
            }
        }
        PixelData::U8(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    for ch in 0..num_display_channels {
                        let mut sum = 0u32; let mut count = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                let idx = (sy * src_w + sx) * channels + ch;
                                sum += v[idx] as u32;
                                count += 1;
                            }
                        }
                        display_channels[ch].push(sum as f32 / (count as f32 * 255.0));
                    }
                }
            }
        }
    }

    // ── Compute Auto-STF parameters per channel ───────────────────────────────
    let stf_params: Vec<(f32, f32)> = display_channels.iter()
        .map(|ch| compute_stf_params(ch, shadow_clip, target_bg))
        .collect();

    // ── Apply MTF stretch per channel ─────────────────────────────────────────
    for (ch_data, &(c0, m)) in display_channels.iter_mut().zip(stf_params.iter()) {
        let c0_range = 1.0 - c0;
        for p in ch_data.iter_mut() {
            let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
            *p = mtf(m, clipped);
        }
    }

    // ── Interleave channels to RGB ────────────────────────────────────────────
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    if is_rgb {
        for i in 0..pixel_count {
            rgb.push((display_channels[0][i].clamp(0.0, 1.0) * 255.0) as u8);
            rgb.push((display_channels[1][i].clamp(0.0, 1.0) * 255.0) as u8);
            rgb.push((display_channels[2][i].clamp(0.0, 1.0) * 255.0) as u8);
        }
    } else {
        for &p in &display_channels[0] {
            let val = (p.clamp(0.0, 1.0) * 255.0) as u8;
            rgb.push(val); rgb.push(val); rgb.push(val);
        }
    }

    // ── Encode to JPEG ────────────────────────────────────────────────────────
    let img = RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
        .ok_or_else(|| "Failed to create display image".to_string())?;
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;

    Ok(buf.into_inner())
}

// ── STF parameter computation ─────────────────────────────────────────────────

fn compute_stf_params(pixels: &[f32], shadow_clip: f32, target_bg: f32) -> (f32, f32) {
    let mut valid: Vec<f32> = pixels.iter().cloned().filter(|p| p.is_finite()).collect();
    if valid.is_empty() {
        return (0.0, 0.5);
    }
    valid.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

    let n = valid.len();
    let median = valid[n / 2];

    let mut deviations: Vec<f32> = valid.iter().map(|&p| (p - median).abs()).collect();
    deviations.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let mad = deviations[deviations.len() / 2];

    let c0 = (median + shadow_clip * 1.4826 * mad).clamp(0.0, 1.0);

    let clipped_median = {
        let c0_range = 1.0 - c0;
        let mut clipped: Vec<f32> = valid.iter()
            .filter(|&&p| p > c0)
            .map(|&p| (p - c0) / c0_range)
            .collect();
        if clipped.is_empty() {
            return (c0, 0.5);
        }
        clipped.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        clipped[clipped.len() / 2]
    };

    let m = if clipped_median < f32::EPSILON {
        0.5
    } else {
        mtf(target_bg, clipped_median)
    };

    (c0, m)
}

/// Midtone Transfer Function — PixInsight-compatible
#[inline(always)]
fn mtf(m: f32, x: f32) -> f32 {
    if x <= 0.0 { return 0.0; }
    if x >= 1.0 { return 1.0; }
    if (m - 0.5).abs() < f32::EPSILON { return x; }
    (m - 1.0) * x / ((2.0 * m - 1.0) * x - m)
}

// ----------------------------------------------------------------------
