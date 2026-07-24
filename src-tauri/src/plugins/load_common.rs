// plugins/load_common.rs — shared post-glob load machinery (Issue 91)
//
// AddFiles and ReadImages both load images into the session but were built
// at different times, so only AddFiles got the memory-limit gate. This
// module is the first piece of consolidating the two onto shared logic —
// phase 1 covers just the memory gate; progress reporting and the basename
// re-sort/current_frame reset follow in later phases.

use crate::context::AppContext;

// check_memory_limit retired under Issue 173: the load path no longer keeps
// raw pixels resident, so the "will the whole session fit in the buffer
// pool" projection it enforced has no meaning. Resident memory now scales
// with metadata + blink caches + the bounded viewing LRU, none of which
// approach the old raw-residency costs.

/// Re-sort the whole session by filename (not full path) and reset
/// current_frame to 0. Shared by AddFiles and ReadImages so both load
/// paths — and any mix of the two — leave the session in identical order.
/// Filenames are DTG-first, so this keeps capture order intact for
/// StackFrames' rotational grouping (Technical Reference §7.1), and a
/// rejected-then-re-added frame slots back into its original chronological
/// position instead of landing at the end.
///
/// Callers should only invoke this when at least one new path was actually
/// attempted (i.e. skip it on an all-duplicates no-op load) — matching the
/// existing behavior where a load that adds nothing leaves current_frame
/// untouched rather than resetting it on every call.
pub(crate) fn finalize_session_order(ctx: &mut AppContext) {
    ctx.file_list.sort_by(|a, b| {
        let a_name = a.rsplit(['/', '\\']).next().unwrap_or(a.as_str());
        let b_name = b.rsplit(['/', '\\']).next().unwrap_or(b.as_str());
        a_name.cmp(b_name)
    });
    ctx.current_frame = 0;
}

/// Build the 12.5% and 25% blink thumbnails for a freshly read frame
/// (Issue 173). Returns (blink_12_jpeg, blink_25_jpeg).
///
/// Logic lifted from the retired start_background_cache per-frame body
/// (commands/display.rs): STF-stretched display-resolution render, then
/// thumbnail resizes. One deliberate change: thumbnails are resized
/// directly from the in-memory stretched image instead of encoding a
/// display-res JPEG and decoding it back for resizing — the intermediate
/// display JPEG is no longer kept (Issue 173 design discussion), so the
/// lossy encode/decode round trip would be pure waste.
pub(crate) fn build_blink_jpegs(
    buffer: &crate::context::ImageBuffer,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    use crate::context::PixelData;
    use crate::settings::defaults::{
        DISPLAY_MAX_WIDTH_PX, THUMBNAIL_JPEG_QUALITY, BLINK_WIDTH_12, BLINK_WIDTH_25,
    };
    use image::codecs::jpeg::JpegEncoder;

    let pixels = buffer.pixels.as_ref()
        .ok_or_else(|| "No pixel data".to_string())?;

    let src_w    = buffer.width as usize;
    let src_h    = buffer.height as usize;
    let channels = buffer.channels as usize;
    let is_rgb   = channels == 3;
    let is_prerendered = buffer.keywords.get("PXTYPE")
        .map(|kw| kw.value == "HEATMAP")
        .unwrap_or(false);

    const MAX_DISPLAY_W: usize = DISPLAY_MAX_WIDTH_PX as usize;

    // Stretched (or prerendered) display-resolution RGB — same two paths
    // as the retired background build.
    let (rgb, disp_w, disp_h) = if is_prerendered && is_rgb {
        if let PixelData::U8(v) = pixels {
            let step = if src_w > MAX_DISPLAY_W { (src_w + MAX_DISPLAY_W - 1) / MAX_DISPLAY_W } else { 1 };
            let disp_w = src_w / step;
            let disp_h = src_h / step;
            let mut rgb = Vec::with_capacity(disp_w * disp_h * 3);
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    let idx = ((oy * step) * src_w + (ox * step)) * 3;
                    rgb.push(v[idx]);
                    rgb.push(v[idx + 1]);
                    rgb.push(v[idx + 2]);
                }
            }
            (rgb, disp_w, disp_h)
        } else {
            return Err("Prerendered frame is not U8".to_string());
        }
    } else {
        let render_channels = if is_rgb { 3 } else { 1 };
        let (mut planes, disp_w, disp_h) = crate::render::downsample_to_planes(
            pixels, src_w, src_h, render_channels, MAX_DISPLAY_W,
        );

        let stf_params: Vec<(f32, f32)> = planes.iter()
            .map(|ch| crate::plugins::cache_frames::compute_stf_params_pub(ch))
            .collect();

        for (ch_data, &(c0, m)) in planes.iter_mut().zip(stf_params.iter()) {
            let c0_range = (1.0 - c0).max(f32::EPSILON);
            for p in ch_data.iter_mut() {
                let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
                *p = crate::plugins::cache_frames::mtf_pub(m, clipped);
            }
        }

        let pixel_count = disp_w * disp_h;
        (crate::render::planes_to_rgb8(&planes, pixel_count), disp_w, disp_h)
    };

    let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
        .ok_or_else(|| "Failed to create display image".to_string())?;
    let img = image::DynamicImage::ImageRgb8(img);

    let mut out: Vec<Vec<u8>> = Vec::with_capacity(2);
    for &target_w in &[BLINK_WIDTH_12, BLINK_WIDTH_25] {
        let target_h = (disp_h as f32 * target_w as f32 / disp_w as f32).round() as u32;
        let resized = img.resize(target_w, target_h, image::imageops::FilterType::Triangle);
        let mut buf = std::io::Cursor::new(Vec::new());
        JpegEncoder::new_with_quality(&mut buf, THUMBNAIL_JPEG_QUALITY)
            .encode_image(&resized)
            .map_err(|e| e.to_string())?;
        out.push(buf.into_inner());
    }
    let blink_25 = out.pop().unwrap();
    let blink_12 = out.pop().unwrap();
    Ok((blink_12, blink_25))
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
