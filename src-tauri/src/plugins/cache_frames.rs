// plugins/cache_frames.rs — CacheFrames built-in plugin
// Pre-renders all loaded images to blink-resolution JPEGs.
// Stores results in AppContext::blink_cache, keyed by file path.
// Raw image_buffers are never modified.
// Uses Rayon for parallel processing across frames.

use tracing::info;
use image::RgbImage;
use std::io::Cursor;
use rayon::prelude::*;

use crate::plugin::{PhotyxPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, PixelData};

pub struct CacheFrames;

impl PhotyxPlugin for CacheFrames {
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
        let resolution = args.get("resolution").map(|s| s.as_str()).unwrap_or("both");

        let resolutions: &[(&str, usize)] = match resolution {
            "12"   => &[("12", 376)],
            "25"   => &[("25", 752)],
            _      => &[("12", 376), ("25", 752)], // "both" — default
        };

        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded. Use AddFiles first."));
        }

        // Clear target caches up front — chunks are processed and inserted
        // incrementally below, so caches can't be bulk-replaced at the end
        // the way the old single-pass version did.
        for &(res_name, _) in resolutions {
            match res_name {
                "12" => ctx.blink_cache_12.clear(),
                _    => ctx.blink_cache_25.clear(),
            }
        }

        let total     = ctx.file_list.len();
        let chunk_len = crate::plugins::pixel_chunking::chunk_size(ctx);
        let file_list = ctx.file_list.clone();
        let mut cached_counts: std::collections::HashMap<&str, usize> =
            resolutions.iter().map(|&(name, _)| (name, 0)).collect();

        for path_chunk in file_list.chunks(chunk_len) {
            // Sequential: clone this chunk's pixel data once, reused across
            // every requested resolution below — avoids reloading pixels
            // twice per chunk when resolution=both (the default), while
            // still bounding peak memory to one chunk instead of the
            // whole session.
            let frames = crate::plugins::pixel_chunking::snapshot_pixel_chunk(ctx, path_chunk);

            for &(res_name, max_w) in resolutions {
            let results: Vec<(String, Vec<u8>)> = frames.par_iter().filter_map(|frame| {
                let src_w = frame.width;
                let src_h = frame.height;

                // Box filter downsampling — averages step×step block per output pixel
                // Preserves fine detail (thin clouds, gradients) better than point sampling
                let step = if src_w > max_w {
                    (src_w + max_w - 1) / max_w
                } else {
                    1
                };

                let disp_w = src_w / step;
                let disp_h = src_h / step;
                let pixel_count = disp_w * disp_h;

                let mut display: Vec<f32> = Vec::with_capacity(pixel_count);

                match &frame.pixels {
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
                                        if val.is_finite() { sum += val; count += 1; }
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
                let (c0, m) = compute_stf_params_pub(&display);
                let c0_range = (1.0 - c0).max(f32::EPSILON);
                for p in display.iter_mut() {
                    let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
                    *p = mtf_pub(m, clipped);
                }

                // Encode to JPEG at quality 75 — sufficient for blink
                let mut rgb = Vec::with_capacity(pixel_count * 3);
                for &p in &display {
                    let val = (p.clamp(0.0, 1.0) * 255.0) as u8;
                    rgb.push(val); rgb.push(val); rgb.push(val);
                }

                let img = RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)?;
                let mut buf = Cursor::new(Vec::new());

                // Use JPEG with quality 75
                use image::codecs::jpeg::JpegEncoder;
                let mut encoder = JpegEncoder::new_with_quality(&mut buf, 75);
                encoder.encode_image(&img).ok()?;

                info!("CacheFrames: cached {} ({}×{})", frame.path, disp_w, disp_h);
                Some((frame.path.clone(), buf.into_inner()))
            }).collect();

                // ── Store this chunk's results ─────────────────────────────────
                let n = results.len();
                match res_name {
                    "12" => { for (path, jpeg) in results { ctx.blink_cache_12.insert(path, jpeg); } }
                    _    => { for (path, jpeg) in results { ctx.blink_cache_25.insert(path, jpeg); } }
                }
                *cached_counts.get_mut(res_name).unwrap() += n;
            } // end resolution loop
            // This chunk's cloned pixel buffers (`frames`) drop here,
            // before the next chunk is loaded.
        } // end chunk loop

        for &(res_name, _) in resolutions {
            info!("CacheFrames: {}% resolution — {}/{} frames cached", res_name, cached_counts[&res_name], total);
        }

        ctx.blink_cache_status = crate::context::BlinkCacheStatus::Ready;
        Ok(PluginOutput::Message(format!("Cached {}/{} frames at both resolutions", total, total)))
    }
}

pub fn compute_stf_params_pub(pixels: &[f32]) -> (f32, f32) {
    let mut valid: Vec<f32> = pixels.iter().cloned().filter(|p| p.is_finite()).collect();
    if valid.is_empty() { return (0.0, 0.5); }
    valid.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

    let n = valid.len();
    let median = valid[n / 2];

    let mut deviations: Vec<f32> = valid.iter().map(|&p| (p - median).abs()).collect();
    deviations.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let mad = deviations[deviations.len() / 2];

    let c0 = (median + (-2.8_f32) * 1.4826 * mad).clamp(0.0, 1.0);

    let c0_range = (1.0 - c0).max(f32::EPSILON);
    let mut clipped: Vec<f32> = valid.iter()
        .filter(|&&p| p > c0)
        .map(|&p| (p - c0) / c0_range)
        .collect();
    if clipped.is_empty() { return (c0, 0.5); }
    clipped.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let clipped_median = clipped[clipped.len() / 2];

    let m = if clipped_median < f32::EPSILON { 0.5 } else { mtf_pub(0.25, clipped_median) };
    (c0, m)
}

#[inline(always)]
pub fn mtf_pub(m: f32, x: f32) -> f32 {
    if x <= 0.0 { return 0.0; }
    if x >= 1.0 { return 1.0; }
    if (m - 0.5).abs() < f32::EPSILON { return x; }
    (m - 1.0) * x / ((2.0 * m - 1.0) * x - m)
}


// ----------------------------------------------------------------------
