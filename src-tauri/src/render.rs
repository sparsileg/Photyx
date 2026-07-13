// render.rs — shared box-filter downsample core for display/cache paths (Issue 86)
//
// get_current_frame, load_file, start_background_cache (commands/display.rs),
// and CacheFrames (plugins/cache_frames.rs) each downsample raw pixel buffers
// before display or caching. They differ only in whether the result gets
// stretched, how many channels they want, and what JPEG quality they encode
// at — not in how the box filter itself works. This module is the one
// shared implementation of that arithmetic.

use crate::context::PixelData;

/// Box-filter downsample a raw pixel buffer to one f32 plane per channel,
/// each value normalized to [0.0, 1.0]. `channels` is 1 (mono) or 3 (RGB) —
/// pass 1 to force a mono plane even for an RGB source (CacheFrames' existing
/// behavior, preserved as-is here) or the buffer's actual channel count to
/// preserve color.
///
/// Returns (planes, disp_w, disp_h). Averages a step×step block of source
/// pixels per output pixel, which preserves fine detail (thin clouds,
/// gradients) better than point sampling.
///
/// F32 finiteness gating is per-channel and independent (a non-finite G or B
/// sample doesn't exclude R from that pixel's average, and vice versa) —
/// this matches start_background_cache's pre-consolidation behavior. Two of
/// the four original call sites (get_current_frame, load_file) instead
/// gated all three channels on channel 0's finiteness alone; that quirk is
/// not preserved here since it only affects the rare case of a finite R
/// with a non-finite G/B in the same source pixel, and the per-channel
/// behavior is the more defensible of the two. Flagged rather than carried
/// forward silently.
pub fn downsample_to_planes(
    pixels:   &PixelData,
    src_w:    usize,
    src_h:    usize,
    channels: usize,
    max_w:    usize,
) -> (Vec<Vec<f32>>, usize, usize) {
    let step = if src_w > max_w { (src_w + max_w - 1) / max_w } else { 1 };
    let disp_w = src_w / step;
    let disp_h = src_h / step;
    let pixel_count = disp_w * disp_h;

    let mut planes: Vec<Vec<f32>> = (0..channels).map(|_| Vec::with_capacity(pixel_count)).collect();

    match pixels {
        PixelData::U16(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    for ch in 0..channels {
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
                        planes[ch].push(sum as f32 / (count as f32 * 65535.0));
                    }
                }
            }
        }
        PixelData::F32(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    for ch in 0..channels {
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
                                if val.is_finite() { sum += val; count += 1; }
                            }
                        }
                        planes[ch].push(if count > 0 { sum / count as f32 } else { 0.0 });
                    }
                }
            }
        }
        PixelData::U8(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    for ch in 0..channels {
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
                        planes[ch].push(sum as f32 / (count as f32 * 255.0));
                    }
                }
            }
        }
    }

    (planes, disp_w, disp_h)
}

/// Pack 1 or 3 f32 planes (each already [0.0, 1.0]) into an interleaved RGB
/// u8 buffer suitable for image::RgbImage — a single mono plane is
/// replicated across all three output channels.
pub fn planes_to_rgb8(planes: &[Vec<f32>], pixel_count: usize) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(pixel_count * 3);
    if planes.len() == 3 {
        for i in 0..pixel_count {
            rgb.push((planes[0][i].clamp(0.0, 1.0) * 255.0) as u8);
            rgb.push((planes[1][i].clamp(0.0, 1.0) * 255.0) as u8);
            rgb.push((planes[2][i].clamp(0.0, 1.0) * 255.0) as u8);
        }
    } else {
        for &p in &planes[0] {
            let val = (p.clamp(0.0, 1.0) * 255.0) as u8;
            rgb.push(val); rgb.push(val); rgb.push(val);
        }
    }
    rgb
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
