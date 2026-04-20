// plugins/cache_frames.rs — CacheFrames built-in plugin
// Pre-renders all loaded images to blink-resolution JPEGs.
// Stores results in AppContext::blink_cache, keyed by file path.
// Raw image_buffers are never modified.

use tracing::info;
use image::{RgbImage, ImageFormat};
use std::io::Cursor;

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, PixelData};

pub struct CacheFrames;

impl PhotonPlugin for CacheFrames {
    fn name(&self) -> &str { "CacheFrames" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Pre-renders all loaded images to blink-resolution JPEGs" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "resolution".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Blink resolution: 12 (12.5%) or 25 (25%). Default: 25".to_string(),
                default:     Some("25".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let resolution = args.get("resolution")
            .map(|s| s.as_str())
            .unwrap_or("25");

        let max_w: usize = match resolution {
            "12" => 376,
            "25" => 752,
            _    => 752,
        };

        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded. Use ReadAllFITFiles first."));
        }

        ctx.blink_cache.clear();

        let file_list = ctx.file_list.clone();
        let total = file_list.len();
        let mut cached = 0;

        for path in &file_list {
            let buffer = match ctx.image_buffers.get(path) {
                Some(b) => b,
                None => { continue; }
            };

            let pixels = match buffer.pixels.as_ref() {
                Some(p) => p,
                None => { continue; }
            };

            let src_w = buffer.width as usize;
            let src_h = buffer.height as usize;

            // Downsample to blink resolution
            let (disp_w, disp_h, step) = if src_w > max_w {
                let step = (src_w + max_w - 1) / max_w;
                (src_w / step, src_h / step, step)
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
                                    if val.is_finite() {
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

            // Compute STF parameters and stretch
            let (c0, m) = compute_stf_params(&display);
            let c0_range = 1.0 - c0;
            for p in display.iter_mut() {
                let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
                *p = mtf(m, clipped);
            }

            // Encode to JPEG
            let mut rgb = Vec::with_capacity(pixel_count * 3);
            for &p in &display {
                let val = (p.clamp(0.0, 1.0) * 255.0) as u8;
                rgb.push(val); rgb.push(val); rgb.push(val);
            }

            let img = match RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb) {
                Some(i) => i,
                None => { continue; }
            };

            let mut buf = Cursor::new(Vec::new());
            if img.write_to(&mut buf, ImageFormat::Jpeg).is_err() { continue; }

            ctx.blink_cache.insert(path.clone(), buf.into_inner());
            cached += 1;

            info!("CacheFrames: cached {} ({}×{})", path, disp_w, disp_h);
        }

        Ok(PluginOutput::Message(format!("Cached {}/{} frames at {}% resolution", cached, total, resolution)))
    }
}

/// Compute Auto-STF parameters with default settings
fn compute_stf_params(pixels: &[f32]) -> (f32, f32) {
    let mut valid: Vec<f32> = pixels.iter().cloned().filter(|p| p.is_finite()).collect();
    if valid.is_empty() { return (0.0, 0.5); }
    valid.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

    let n = valid.len();
    let median = valid[n / 2];

    let mut deviations: Vec<f32> = valid.iter().map(|&p| (p - median).abs()).collect();
    deviations.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let mad = deviations[deviations.len() / 2];

    let c0 = (median + (-2.8) * 1.4826 * mad).clamp(0.0, 1.0);

    let c0_range = 1.0 - c0;
    let mut clipped: Vec<f32> = valid.iter()
        .filter(|&&p| p > c0)
        .map(|&p| (p - c0) / c0_range)
        .collect();
    if clipped.is_empty() { return (c0, 0.5); }
    clipped.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let clipped_median = clipped[clipped.len() / 2];

    let m = if clipped_median < f32::EPSILON { 0.5 } else { mtf(0.25, clipped_median) };
    (c0, m)
}

#[inline(always)]
fn mtf(m: f32, x: f32) -> f32 {
    if x <= 0.0 { return 0.0; }
    if x >= 1.0 { return 1.0; }
    if (m - 0.5).abs() < f32::EPSILON { return x; }
    (m - 1.0) * x / ((2.0 * m - 1.0) * x - m)
}
