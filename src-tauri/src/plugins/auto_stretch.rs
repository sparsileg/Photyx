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
use crate::context::{AppContext, PixelData};

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
            PluginError::new("NO_IMAGE", "No image loaded. Use ReadAllFITFiles first.")
        })?;

        let buffer = ctx.image_buffers.get(&path).ok_or_else(|| {
            PluginError::new("NO_IMAGE", "Current image buffer not found.")
        })?;

        let pixels = buffer.pixels.as_ref().ok_or_else(|| {
            PluginError::new("NO_PIXELS", "Image has no pixel data.")
        })?;

        let src_w = buffer.width as usize;
        let src_h = buffer.height as usize;

        // ── Step 1: Downsample to display resolution ──────────────────────────
        // All subsequent work (stats + stretch + encode) operates on this small
        // buffer (~180k pixels for 3008×3008), not the 9M pixel raw buffer.
        const MAX_DISPLAY_W: usize = 1200;
        let (disp_w, disp_h, step) = if src_w > MAX_DISPLAY_W {
            let step = (src_w + MAX_DISPLAY_W - 1) / MAX_DISPLAY_W; // ceiling div
            let disp_w = src_w / step;
            let disp_h = src_h / step;
            (disp_w, disp_h, step)
        } else {
            (src_w, src_h, 1)
        };

        let pixel_count = disp_w * disp_h;
        let mut display: Vec<f32> = Vec::with_capacity(pixel_count);

        match pixels {
            PixelData::U16(v) => {
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        let mut sum = 0u32;
                        let mut count = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                sum += v[sy * src_w + sx] as u32;
                                count += 1;
                            }
                        }
                        display.push(sum as f32 / (count as f32 * 65535.0));
                    }
                }
            }
            PixelData::F32(v) => {
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        let mut sum = 0.0f32;
                        let mut count = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                let val = v[sy * src_w + sx];
                                if val.is_finite() { // exclude NaN/Inf (bad pixels)
                                    sum += val;
                                    count += 1;
                                }
                            }
                        }
                        display.push(if count > 0 { sum / count as f32 } else { 0.0 });
                    }
                }
            }
            PixelData::U8(v) => {
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        let mut sum = 0u32;
                        let mut count = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                sum += v[sy * src_w + sx] as u32;
                                count += 1;
                            }
                        }
                        display.push(sum as f32 / (count as f32 * 255.0));
                    }
                }
            }
        }

        // ── Step 2: Compute Auto-STF parameters on the display buffer ─────────
        // At ~180k pixels, sorting is trivial. No sampling needed.
        let (c0, m) = compute_stf_params(&display, shadow_clip, target_bg);
        ctx.last_stf_params = Some((c0, m));

        // ── Step 3: Apply MTF stretch in-place ────────────────────────────────
        let c0_range = 1.0 - c0;
        for p in display.iter_mut() {
            let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
            *p = mtf(m, clipped);
        }

        // ── Step 4: Encode to JPEG and store in display cache ─────────────────
        let mut rgb = Vec::with_capacity(pixel_count * 3);
        for &p in &display {
            let val = (p.clamp(0.0, 1.0) * 255.0) as u8;
            rgb.push(val);
            rgb.push(val);
            rgb.push(val);
        }

        let img = RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
            .ok_or_else(|| PluginError::new("ENCODE_ERROR", "Failed to create display image"))?;

        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Jpeg)
            .map_err(|e| PluginError::new("ENCODE_ERROR", &e.to_string()))?;

        let jpeg_bytes = buf.into_inner();
        let byte_count = jpeg_bytes.len();
        ctx.display_cache.insert(path.clone(), jpeg_bytes);
        ctx.full_res_cache.remove(&path); // invalidate so get_full_frame re-encodes with new params

        // Record the actual display width so Viewer knows when to request full-res
        if let Some(buf) = ctx.image_buffers.get_mut(&path) {
            buf.display_width = disp_w as u32;
        }

        info!("AutoStretch: {} → {}×{} display, {} JPEG bytes", path, disp_w, disp_h, byte_count);
        Ok(PluginOutput::Message(format!("AutoStretch applied ({}×{} display)", disp_w, disp_h)))
    }
}

/// Compute PixInsight-compatible Auto-STF parameters.
/// Returns (c0, m) — shadow clip point and MTF midpoint.
fn compute_stf_params(pixels: &[f32], shadow_clip: f32, target_bg: f32) -> (f32, f32) {
    // Filter out non-finite values before sorting
    let mut valid: Vec<f32> = pixels.iter().cloned().filter(|p| p.is_finite()).collect();
    if valid.is_empty() {
        return (0.0, 0.5);
    }
    valid.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

    let n = valid.len();
    let median = valid[n / 2];

    // MAD — median absolute deviation
    let mut deviations: Vec<f32> = valid.iter().map(|&p| (p - median).abs()).collect();
    deviations.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let mad = deviations[deviations.len() / 2];

    // Shadow clipping point (PixInsight formula: median + shadowclip * 1.4826 * MAD)
    // With default shadowclip=-2.8 this clips ~2.8 sigma below median
    let c0 = (median + shadow_clip * 1.4826 * mad).clamp(0.0, 1.0);

    // Median of shadow-clipped data for midtone parameter
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
