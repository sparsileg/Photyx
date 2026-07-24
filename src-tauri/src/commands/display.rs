// commands/display.rs   Image display and pixel data Tauri command handlers

use std::sync::Arc;
use tauri::State;
use crate::PhotoxState;
use image::codecs::jpeg::JpegEncoder;
use crate::settings::defaults::{DETAIL_JPEG_QUALITY, DISPLAY_MAX_WIDTH_PX};

#[tauri::command]
pub async fn get_current_frame(state: State<'_, Arc<PhotoxState>>) -> Result<String, String> {
    // Issue 177 (related): previously a plain sync fn doing a real disk
    // read (ensure_pixels_resident), a box-filter downsample, a JPEG
    // encode, and a base64 encode all synchronously in the command
    // handler itself — no async, no thread hop, no yield point at all.
    // Whatever thread services this command ran that entire pipeline
    // uninterrupted. Wrapped in spawn_blocking, matching dispatch_command's
    // existing pattern, so the runtime can service other commands/polls
    // while this runs.
    let state: Arc<PhotoxState> = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let mut ctx = state.context.lock().expect("context lock poisoned");

        let path = ctx.file_list.get(ctx.current_frame)
            .cloned()
            .ok_or_else(|| "No image loaded".to_string())?;

        // Issue 173: the entry may be metadata-only — read pixels from disk
        // into the viewing LRU if not resident.
        ctx.ensure_pixels_resident(&path)?;

        let buffer = ctx.image_buffers.get(&path)
            .ok_or_else(|| "Image buffer not found".to_string())?;

        let pixels = buffer.pixels.as_ref()
            .ok_or_else(|| "No pixel data".to_string())?;

        let src_w    = buffer.width as usize;
        let src_h    = buffer.height as usize;
        let channels = buffer.channels as usize;
        let is_rgb   = channels == 3 && buffer.color_space == crate::context::ColorSpace::RGB;
        let is_prerendered = buffer.keywords.get("PXTYPE")
            .map(|kw| kw.value == "HEATMAP")
            .unwrap_or(false);

        tracing::debug!(
            "get_current_frame: src={}x{} channels={} color_space={:?} is_rgb={}",
            src_w, src_h, channels, buffer.color_space, is_rgb
        );

        const MAX_DISPLAY_W: usize = DISPLAY_MAX_WIDTH_PX as usize;
        let step = if src_w > MAX_DISPLAY_W { (src_w + MAX_DISPLAY_W - 1) / MAX_DISPLAY_W } else { 1 };
        let disp_w = src_w / step;
        let disp_h = src_h / step;
        let pixel_count = disp_w * disp_h;

        use crate::context::PixelData;

        // Prerendered RGB (e.g. heatmap): downsample directly, no stretch
        if is_prerendered && is_rgb {
            if let PixelData::U8(v) = pixels {
                let mut rgb = Vec::with_capacity(pixel_count * 3);
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        let sy = oy * step;
                        let sx = ox * step;
                        let idx = (sy * src_w + sx) * 3;
                        rgb.push(v[idx]);
                        rgb.push(v[idx + 1]);
                        rgb.push(v[idx + 2]);
                    }
                }
                let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
                    .ok_or_else(|| "Failed to create display image".to_string())?;
                let mut buf = std::io::Cursor::new(Vec::new());
                JpegEncoder::new_with_quality(&mut buf, DETAIL_JPEG_QUALITY)
                    .encode_image(&img)
                    .map_err(|e| e.to_string())?;
                use base64::Engine as _;
                let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
                return Ok(format!("data:image/jpeg;base64,{}", b64));
            }
        }

        // Normal path: render raw pixels without stretch, via the shared
        // box-filter core (Issue 86). This also eliminates the redundant mono
        // pass the old U16 branch used to compute and discard on every RGB frame.
        let render_channels = if is_rgb { 3 } else { 1 };
        let (planes, disp_w, disp_h) = crate::render::downsample_to_planes(
            pixels, src_w, src_h, render_channels, MAX_DISPLAY_W,
        );
        let pixel_count = disp_w * disp_h;
        let rgb = crate::render::planes_to_rgb8(&planes, pixel_count);

        let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
            .ok_or_else(|| "Failed to create display image".to_string())?;
        let mut buf = std::io::Cursor::new(Vec::new());
        JpegEncoder::new_with_quality(&mut buf, DETAIL_JPEG_QUALITY)
            .encode_image(&img)
            .map_err(|e| e.to_string())?;
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
        Ok(format!("data:image/jpeg;base64,{}", b64))
    }).await.map_err(|e| format!("spawn_blocking panicked: {:?}", e))?
}


#[tauri::command]
pub fn get_autostretch_frame(
    shadow_clip: Option<f32>,
    target_bg: Option<f32>,
    state: State<Arc<PhotoxState>>,
) -> Result<String, String> {
    use crate::plugins::auto_stretch::compute_autostretch_jpeg;
    let mut ctx = state.context.lock().expect("context lock poisoned");
    // Issue 173: read pixels from disk into the viewing LRU if not resident.
    let path = ctx.file_list.get(ctx.current_frame)
        .cloned()
        .ok_or_else(|| "No image loaded".to_string())?;
    ctx.ensure_pixels_resident(&path)?;
    let jpeg_bytes = compute_autostretch_jpeg(
        &ctx,
        shadow_clip.unwrap_or(ctx.autostretch_shadow_clip),
        target_bg.unwrap_or(ctx.autostretch_target_bg),
    )?;
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

#[tauri::command]
pub fn get_blink_frame(
    index: usize,
    resolution: String,
    state: State<Arc<PhotoxState>>,
) -> Result<String, String> {
    let ctx = state.context.lock().expect("context lock poisoned");

    let path = ctx.file_list.get(index)
        .ok_or_else(|| format!("Frame index {} out of range", index))?;

    let cache = if resolution == "12" { &ctx.blink_cache_12 } else { &ctx.blink_cache_25 };

    let jpeg_bytes = cache.get(path)
        .ok_or_else(|| format!("Frame {} not in blink cache", index))?;

    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg_bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

#[tauri::command]
pub fn get_blink_cache_status(state: State<Arc<PhotoxState>>) -> String {
    let ctx = state.context.lock().expect("context lock poisoned");
    match ctx.blink_cache_status {
        crate::context::BlinkCacheStatus::Idle     => "idle".to_string(),
        crate::context::BlinkCacheStatus::Building => "building".to_string(),
        crate::context::BlinkCacheStatus::Ready    => "ready".to_string(),
    }
}

// start_background_cache retired under Issue 173: blink caches (both
// resolutions) are now built during the load pass itself
// (load_common::build_blink_jpegs, called by AddFiles/ReadImages), so
// there is no post-load background build. get_blink_cache_status and the
// BlinkCacheStatus enum survive — the loaders set Ready at load end.

#[tauri::command]
pub fn get_pixel(x: u32, y: u32, state: State<Arc<PhotoxState>>) -> Result<serde_json::Value, String> {
    let mut ctx = state.context.lock().expect("context lock poisoned");

    let path = ctx.file_list.get(ctx.current_frame)
        .cloned()
        .ok_or_else(|| "No image loaded".to_string())?;

    // Issue 173: read pixels from disk into the viewing LRU if not resident.
    ctx.ensure_pixels_resident(&path)?;

    let buffer = ctx.image_buffers.get(&path)
        .ok_or_else(|| "Buffer not found".to_string())?;

    let pixels = buffer.pixels.as_ref()
        .ok_or_else(|| "No pixel data".to_string())?;

    let w  = buffer.width as u32;
    let h  = buffer.height as u32;
    let ch = buffer.channels as u32;

    if x >= w || y >= h {
        return Err(format!("Pixel ({},{}) out of bounds ({}x{})", x, y, w, h));
    }

    use crate::context::PixelData;
    let base = (y * w + x) as usize;

    let (raw, val) = match pixels {
        PixelData::U8(v) => {
            if ch == 3 {
                let r = v[base * 3]     as f32 / 255.0;
                let g = v[base * 3 + 1] as f32 / 255.0;
                let b = v[base * 3 + 2] as f32 / 255.0;
                (format!("{:.4}/{:.4}/{:.4}", r, g, b),
                 format!("{}/{}/{}",
                         (r * 65535.0) as u32,
                         (g * 65535.0) as u32,
                         (b * 65535.0) as u32))
            } else {
                let p = v[base] as f32 / 255.0;
                (format!("{:.4}", p), format!("{}", (p * 65535.0) as u32))
            }
        }
        PixelData::U16(v) => {
            if ch == 3 {
                let r = v[base * 3]     as f32 / 65535.0;
                let g = v[base * 3 + 1] as f32 / 65535.0;
                let b = v[base * 3 + 2] as f32 / 65535.0;
                (format!("{:.4}/{:.4}/{:.4}", r, g, b),
                 format!("{}/{}/{}",
                         v[base * 3] as u32,
                         v[base * 3 + 1] as u32,
                         v[base * 3 + 2] as u32))
            } else {
                let p = v[base];
                (format!("{:.4}", p as f32 / 65535.0), format!("{}", p))
            }
        }
        PixelData::F32(v) => {
            if ch == 3 {
                let r = v[base * 3];
                let g = v[base * 3 + 1];
                let b = v[base * 3 + 2];
                (format!("{:.4}/{:.4}/{:.4}", r, g, b),
                 format!("{}/{}/{}",
                         (r.clamp(0.0,1.0) * 65535.0) as u32,
                         (g.clamp(0.0,1.0) * 65535.0) as u32,
                         (b.clamp(0.0,1.0) * 65535.0) as u32))
            } else {
                let p = v[base];
                (format!("{:.4}", p), format!("{}", (p.clamp(0.0,1.0) * 65535.0) as u32))
            }
        }
    };

    Ok(serde_json::json!({
        "raw":      raw,
        "val":      val,
        "channels": ch,
    }))
}

#[tauri::command]
pub fn get_full_frame(state: State<Arc<PhotoxState>>) -> Result<String, String> {
    let path = {
        let ctx = state.context.lock().expect("context lock poisoned");
        ctx.file_list.get(ctx.current_frame)
            .cloned()
            .ok_or_else(|| "No image loaded".to_string())?
    };

    {
        let ctx = state.context.lock().expect("context lock poisoned");
        if let Some(jpeg_bytes) = ctx.full_res_cache.get(&path) {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg_bytes);
            return Ok(format!("data:image/jpeg;base64,{}", b64));
        }
    }

    let jpeg_bytes = {
        let mut ctx = state.context.lock().expect("context lock poisoned");
        // Issue 173: read pixels from disk into the viewing LRU if not resident.
        ctx.ensure_pixels_resident(&path)?;
        let buffer = ctx.image_buffers.get(&path)
            .ok_or_else(|| "Buffer not found".to_string())?;
        let pixels = buffer.pixels.as_ref()
            .ok_or_else(|| "No pixel data".to_string())?;

        let w = buffer.width as usize;
        let h = buffer.height as usize;

        use crate::context::PixelData;
        let mut rgb = Vec::with_capacity(w * h * 3);

        let (c0, m) = ctx.last_stf_params.unwrap_or((0.0, 0.5));
        let c0_range = (1.0 - c0).max(f32::EPSILON);

        #[inline(always)]
        fn mtf(m: f32, x: f32) -> f32 {
            if x <= 0.0 { return 0.0; }
            if x >= 1.0 { return 1.0; }
            if (m - 0.5).abs() < f32::EPSILON { return x; }
            (m - 1.0) * x / ((2.0 * m - 1.0) * x - m)
        }

        #[inline(always)]
        fn stretch(p: f32, c0: f32, c0_range: f32, m: f32) -> u8 {
            let clipped = ((p - c0) / c0_range).clamp(0.0, 1.0);
            (mtf(m, clipped) * 255.0) as u8
        }

        let channels = buffer.channels as usize;
        let is_rgb = channels == 3 && buffer.color_space == crate::context::ColorSpace::RGB;

        match pixels {
            PixelData::U16(v) => {
                if is_rgb {
                    for chunk in v.chunks_exact(3) {
                        rgb.push(stretch(chunk[0] as f32 / 65535.0, c0, c0_range, m));
                        rgb.push(stretch(chunk[1] as f32 / 65535.0, c0, c0_range, m));
                        rgb.push(stretch(chunk[2] as f32 / 65535.0, c0, c0_range, m));
                    }
                } else {
                    for &p in v {
                        let val = stretch(p as f32 / 65535.0, c0, c0_range, m);
                        rgb.push(val); rgb.push(val); rgb.push(val);
                    }
                }
            }
            PixelData::F32(v) => {
                if is_rgb {
                    for chunk in v.chunks_exact(3) {
                        rgb.push(stretch(chunk[0].clamp(0.0, 1.0), c0, c0_range, m));
                        rgb.push(stretch(chunk[1].clamp(0.0, 1.0), c0, c0_range, m));
                        rgb.push(stretch(chunk[2].clamp(0.0, 1.0), c0, c0_range, m));
                    }
                } else {
                    for &p in v {
                        let val = stretch(p.clamp(0.0, 1.0), c0, c0_range, m);
                        rgb.push(val); rgb.push(val); rgb.push(val);
                    }
                }
            }
            PixelData::U8(v) => {
                if is_rgb {
                    for chunk in v.chunks_exact(3) {
                        rgb.push(stretch(chunk[0] as f32 / 255.0, c0, c0_range, m));
                        rgb.push(stretch(chunk[1] as f32 / 255.0, c0, c0_range, m));
                        rgb.push(stretch(chunk[2] as f32 / 255.0, c0, c0_range, m));
                    }
                } else {
                    for &p in v {
                        let val = stretch(p as f32 / 255.0, c0, c0_range, m);
                        rgb.push(val); rgb.push(val); rgb.push(val);
                    }
                }
            }
        }

        let img = image::RgbImage::from_raw(w as u32, h as u32, rgb)
            .ok_or_else(|| "Failed to create full-res image".to_string())?;
        let mut buf = std::io::Cursor::new(Vec::new());
        use image::codecs::jpeg::JpegEncoder;
        JpegEncoder::new_with_quality(&mut buf, DETAIL_JPEG_QUALITY)
            .encode_image(&img)
            .map_err(|e| e.to_string())?;
        buf.into_inner()
    };

    {
        let mut ctx = state.context.lock().expect("context lock poisoned");
        ctx.full_res_cache.insert(path, jpeg_bytes.clone());
    }

    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

#[tauri::command]
pub fn get_histogram(state: State<Arc<PhotoxState>>) -> Result<serde_json::Value, String> {
    let mut ctx = state.context.lock().expect("context lock poisoned");

    let path = ctx.file_list.get(ctx.current_frame)
        .cloned()
        .ok_or_else(|| "No image loaded".to_string())?;

    // Issue 173: read pixels from disk into the viewing LRU if not resident.
    ctx.ensure_pixels_resident(&path)?;

    let buffer = ctx.image_buffers.get(&path)
        .ok_or_else(|| "Image buffer not found".to_string())?;

    let pixels = buffer.pixels.as_ref()
        .ok_or_else(|| "No pixel data".to_string())?;

    let is_rgb = buffer.channels == 3 && buffer.color_space == crate::context::ColorSpace::RGB;
    let stats = crate::plugins::get_histogram::compute_stats(pixels, is_rgb);

    Ok(serde_json::json!({
        "bins":         stats.bins,
        "bins_g":       stats.bins_g,
        "bins_b":       stats.bins_b,
        "median":       stats.median,
        "median_g":     stats.median_g,
        "median_b":     stats.median_b,
        "mean":         stats.mean,
        "std_dev":      stats.std_dev,
        "std_dev_g":    stats.std_dev_g,
        "std_dev_b":    stats.std_dev_b,
        "clipping_pct": stats.clipping_pct,
    }))
}

#[tauri::command]
pub fn load_file(path: String, state: State<Arc<PhotoxState>>) -> Result<String, String> {
    use crate::plugins::image_reader::read_image_file;
    use crate::context::PixelData;

    let buffer = read_image_file(&path)
        .map_err(|e| format!("Failed to load '{}': {}", path, e))?;

    {
        // Issue 157: load_file no longer evicts existing files from the
        // session — File > Open Image now matches drag-and-drop/AddFiles
        // behavior (append, don't replace). Reloading an already-open
        // path refreshes its buffer with freshly-read pixel data rather
        // than silently keeping the stale one.
        let mut ctx = state.context.lock().expect("context lock poisoned");
        if !ctx.file_list.contains(&path) {
            ctx.file_list.push(path.clone());
        }
        // Issue 173: build blink thumbnails for this frame too, so a file
        // opened via File > Open Image participates in blink like any
        // batch-loaded frame. Failure is non-fatal (frame views fine, just
        // won't blink) — matching AddFiles/ReadImages severity.
        match crate::plugins::load_common::build_blink_jpegs(&buffer) {
            Ok((b12, b25)) => {
                ctx.blink_cache_12.insert(path.clone(), b12);
                ctx.blink_cache_25.insert(path.clone(), b25);
            }
            Err(e) => {
                tracing::warn!("load_file: blink cache failed for {}: {}", path, e);
            }
        }
        ctx.image_buffers.insert(path.clone(), buffer.clone());
        // Issue 173: this insert carries pixels — register it in the
        // viewing LRU so it participates in eviction accounting.
        ctx.touch_pixels_lru(&path);
        ctx.current_frame = ctx.file_list.iter().position(|p| p == &path).unwrap_or(0);
    }

    let width    = buffer.width as usize;
    let height   = buffer.height as usize;
    let channels = buffer.channels as usize;

    let is_prerendered = buffer.keywords.get("PXTYPE")
        .map(|kw| kw.value == "HEATMAP")
        .unwrap_or(false);

    let pixels = match &buffer.pixels {
        Some(p) => p,
        None => return Err("No pixel data in file".to_string()),
    };

    const MAX_DISPLAY_W: usize = DISPLAY_MAX_WIDTH_PX as usize;
    let step = if width > MAX_DISPLAY_W { (width + MAX_DISPLAY_W - 1) / MAX_DISPLAY_W } else { 1 };
    let disp_w = width / step;
    let disp_h = height / step;
    let pixel_count = disp_w * disp_h;

    let rgb = if is_prerendered && channels == 3 {
        let mut rgb = Vec::with_capacity(pixel_count * 3);
        if let PixelData::U8(v) = &pixels {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    let sy = oy * step;
                    let sx = ox * step;
                    let idx = (sy * width + sx) * 3;
                    rgb.push(v[idx]);
                    rgb.push(v[idx + 1]);
                    rgb.push(v[idx + 2]);
                }
            }
        }
        rgb
    } else {
        // Render raw pixels without stretch, via the shared box-filter
        // core (Issue 86) — this path didn't have the redundant-mono-pass
        // bug get_current_frame had, so this is pure deduplication here,
        // not a behavior fix.
        let render_channels = if channels == 3 { 3 } else { 1 };
        let (planes, _dw, _dh) = crate::render::downsample_to_planes(
            pixels, width, height, render_channels, MAX_DISPLAY_W,
        );
        crate::render::planes_to_rgb8(&planes, pixel_count)
    };

    let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
        .ok_or_else(|| "Failed to create preview image".to_string())?;
    let mut buf = std::io::Cursor::new(Vec::new());
    JpegEncoder::new_with_quality(&mut buf, DETAIL_JPEG_QUALITY)
        .encode_image(&img)
        .map_err(|e| e.to_string())?;
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

/// Returns the current stack result as an auto-stretched display-resolution
/// JPEG data URL plus the stack summary. Uses the same Auto-STF parameters
/// as the main viewer.
#[tauri::command]
pub fn get_autostretch_stack_frame(
    shadow_clip: Option<f32>,
    target_bg:   Option<f32>,
    state: State<Arc<PhotoxState>>,
) -> Result<serde_json::Value, String> {
    let ctx = state.context.lock().expect("context lock poisoned");
    let summary_json = ctx.stack_summary.as_ref().map(|s| serde_json::json!({
        "stacked_frames":         s.stacked_frames,
        "total_frames":           s.total_frames,
        "snr_improvement":        s.snr_improvement,
        "alignment_success_rate": s.alignment_success_rate,
        "background_uniformity":  s.background_uniformity.as_str(),
        "target":                 s.target,
        "filter":                 s.filter,
        "integration_seconds":    s.integration_seconds,
        "completed_at":           s.completed_at,
    }));
    let buffer = ctx.stack_result.as_ref()
        .ok_or_else(|| "No stack result available.".to_string())
        .map_err(|e| e)?;
    let shadow_clip = shadow_clip.unwrap_or(ctx.autostretch_shadow_clip);
    let target_bg   = target_bg.unwrap_or(ctx.autostretch_target_bg);

    let jpeg_bytes = crate::plugins::auto_stretch::compute_autostretch_jpeg_from_buffer(
        buffer, shadow_clip, target_bg,
    ).map_err(|e| e)?;

    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
    let data_url = format!("data:image/jpeg;base64,{}", b64);
    Ok(serde_json::json!({
        "image_url": data_url,
        "summary":   summary_json,
    }))
}

/// Returns the current stack result as a display-resolution JPEG data URL,
/// linearly auto-scaled to the buffer's actual min/max pixel range (as
/// opposed to get_autostretch_stack_frame's STF stretch). Used by
/// StackingWorkspace.svelte for a raw, unstretched preview of the stack
/// result.
#[tauri::command]
pub fn get_stack_frame(state: State<Arc<PhotoxState>>) -> Result<String, String> {
    let ctx = state.context.lock().expect("context lock poisoned");

    let buffer = ctx.stack_result.as_ref()
        .ok_or_else(|| "No stack result available. Run StackFrames first.".to_string())?;

    use crate::context::PixelData;
    let pixel_data = buffer.pixels.as_ref()
        .ok_or_else(|| "Stack result has no pixel data.".to_string())?;
    let pixels = match pixel_data {
        PixelData::F32(v) => v,
        _ => return Err("Stack result has unexpected pixel format.".to_string()),
    };

    let src_w   = buffer.width  as usize;
    let src_h   = buffer.height as usize;
    // Issue 113: this was previously hand-rolled with no channel awareness
    // at all, so an RGB stack's interleaved [R,G,B,...] data was misread as
    // mono — only the first third of the buffer was ever touched, each
    // R/G/B triplet displayed as three separate gray pixels. Now routed
    // through the shared, channel-aware box filter (render.rs, Issue 86)
    // like every other downsample path in this file.
    let is_rgb  = buffer.channels == 3 && buffer.color_space == crate::context::ColorSpace::RGB;
    let channels = if is_rgb { 3 } else { 1 };

    // Find the actual pixel range for auto-scaling — one range shared
    // across all channels (linked scale), so color balance isn't distorted
    // by one channel happening to have a wider dynamic range than another.
    // Unchanged from before: this already scanned the full raw buffer, not
    // just the misread subset the old box-filter loop below it touched.
    let max_val = pixels.iter()
        .filter(|v| v.is_finite())
        .cloned()
        .fold(f32::NEG_INFINITY, f32::max);
    let min_val = pixels.iter()
        .filter(|v| v.is_finite())
        .cloned()
        .fold(f32::INFINITY, f32::min);
    let range = (max_val - min_val).max(1e-6);

    const MAX_DISPLAY_W: usize = DISPLAY_MAX_WIDTH_PX as usize;
    let (mut planes, disp_w, disp_h) = crate::render::downsample_to_planes(
        pixel_data, src_w, src_h, channels, MAX_DISPLAY_W,
    );

    // downsample_to_planes averages raw F32 values without normalizing
    // (unlike its U16/U8 branches, which divide by 65535/255) — apply the
    // linear min/max scale here, on the already-correctly-channeled planes.
    for plane in planes.iter_mut() {
        for v in plane.iter_mut() {
            *v = (*v - min_val) / range;
        }
    }

    let pixel_count = disp_w * disp_h;
    let rgb = crate::render::planes_to_rgb8(&planes, pixel_count);

    let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
        .ok_or_else(|| "Failed to create stack preview image".to_string())?;
    let mut buf = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, DETAIL_JPEG_QUALITY)
        .encode_image(&img)
        .map_err(|e| e.to_string())?;
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Ok(format!("data:image/jpeg;base64,{}", b64))
}



#[tauri::command]
pub fn get_cpu_count() -> usize {
    num_cpus::get()
}

// ----------------------------------------------------------------------
