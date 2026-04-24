// plugins/auto_stretch.rs — AutoStretch built-in native plugin
// Spec §12.2 — Auto-STF (PixInsight-compatible algorithm)
//
// Design: operates entirely on a display-resolution copy of the image.
// The raw image_buffer is never modified. The result is stored as JPEG
// bytes in AppContext::display_cache, keyed by file path.

use tracing::info;
use image::{RgbImage, ImageFormat};
use std::io::Cursor;

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, ColorSpace, PixelData};

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
                description: "Shadow clipping factor (PixInsight default: -2.8)".to_string(),
                default:     Some("-2.8".to_string()),
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
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.25);

        let shadow_clip = args.get("shadowclip")
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(-2.8);

        let path = ctx.file_list.get(ctx.current_frame).cloned().ok_or_else(|| {
            PluginError::new("NO_IMAGE", "No image loaded. Use ReadAll first.")
        })?;

        let buffer = ctx.image_buffers.get(&path).ok_or_else(|| {
            PluginError::new("NO_IMAGE", "Current image buffer not found.")
        })?;

        let pixels = buffer.pixels.as_ref().ok_or_else(|| {
            PluginError::new("NO_PIXELS", "Image has no pixel data.")
        })?;

        let src_w    = buffer.width as usize;
        let src_h    = buffer.height as usize;
        let channels = buffer.channels as usize;
        let is_rgb   = channels == 3 && buffer.color_space == ColorSpace::RGB;

        // ── Step 1: Downsample to display resolution ──────────────────────────
        const MAX_DISPLAY_W: usize = 1200;
        let (disp_w, disp_h, step) = if src_w > MAX_DISPLAY_W {
            let step = (src_w + MAX_DISPLAY_W - 1) / MAX_DISPLAY_W;
            (src_w / step, src_h / step, step)
        } else {
            (src_w, src_h, 1)
        };

        let pixel_count = disp_w * disp_h;

        // Build one display channel per image channel
        // For mono/bayer: 1 channel. For RGB: 3 channels.
        let num_display_channels = if is_rgb { 3 } else { 1 };
        let mut display_channels: Vec<Vec<f32>> = (0..num_display_channels)
            .map(|_| Vec::with_capacity(pixel_count))
            .collect();

        match pixels {
            PixelData::U16(v) => {
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        for ch in 0..num_display_channels {
                            let mut sum = 0u32;
                            let mut count = 0u32;
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
                            let mut sum = 0.0f32;
                            let mut count = 0u32;
                            for dy in 0..step {
                                let sy = oy * step + dy;
                                if sy >= src_h { continue; }
                                for dx in 0..step {
                                    let sx = ox * step + dx;
                                    if sx >= src_w { continue; }
                                    let idx = (sy * src_w + sx) * channels + ch;
                                    let val = v[idx];
                                    if val.is_finite() {
                                        sum += val;
                                        count += 1;
                                    }
                                }
                            }
                            display_channels[ch].push(
                                if count > 0 { sum / count as f32 } else { 0.0 }
                            );
                        }
                    }
                }
            }
            PixelData::U8(v) => {
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        for ch in 0..num_display_channels {
                            let mut sum = 0u32;
                            let mut count = 0u32;
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

        // ── Step 2: Compute Auto-STF parameters per channel ───────────────────
        // For RGB: independent STF per channel (PixInsight behaviour)
        // For mono: single STF
        let stf_params: Vec<(f32, f32)> = display_channels.iter()
            .map(|ch| compute_stf_params(ch, shadow_clip, target_bg))
            .collect();

        // Store first channel's params for get_full_frame reuse
        ctx.last_stf_params = Some(stf_params[0]);

        // ── Step 3: Apply MTF stretch per channel ─────────────────────────────
        for (ch_data, &(c0, m)) in display_channels.iter_mut().zip(stf_params.iter()) {
            let c0_range = 1.0 - c0;
            for p in ch_data.iter_mut() {
                let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
                *p = mtf(m, clipped);
            }
        }

        // ── Step 4: Encode to JPEG and store in display cache ─────────────────
        let mut rgb = Vec::with_capacity(pixel_count * 3);

        if is_rgb {
            // Interleave R, G, B channels into RGB bytes
            for i in 0..pixel_count {
                let r = (display_channels[0][i].clamp(0.0, 1.0) * 255.0) as u8;
                let g = (display_channels[1][i].clamp(0.0, 1.0) * 255.0) as u8;
                let b = (display_channels[2][i].clamp(0.0, 1.0) * 255.0) as u8;
                rgb.push(r);
                rgb.push(g);
                rgb.push(b);
            }
        } else {
            // Mono — replicate single channel to RGB
            for &p in &display_channels[0] {
                let val = (p.clamp(0.0, 1.0) * 255.0) as u8;
                rgb.push(val);
                rgb.push(val);
                rgb.push(val);
            }
        }

        let img = RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
            .ok_or_else(|| PluginError::new("ENCODE_ERROR", "Failed to create display image"))?;

        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Jpeg)
            .map_err(|e| PluginError::new("ENCODE_ERROR", &e.to_string()))?;

        let jpeg_bytes = buf.into_inner();
        let byte_count = jpeg_bytes.len();
        ctx.display_cache.insert(path.clone(), jpeg_bytes);
        ctx.full_res_cache.remove(&path);

        if let Some(buf) = ctx.image_buffers.get_mut(&path) {
            buf.display_width = disp_w as u32;
        }

        info!("AutoStretch: {} → {}×{} display ({} ch), {} JPEG bytes",
            path, disp_w, disp_h, num_display_channels, byte_count);
        Ok(PluginOutput::Message(format!(
            "AutoStretch applied ({}×{} display, {} channel{})",
            disp_w, disp_h, num_display_channels,
            if num_display_channels == 1 { "" } else { "s" }
        )))
    }
}

/// Compute PixInsight-compatible Auto-STF parameters.
/// Returns (c0, m) — shadow clip point and MTF midpoint.
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
