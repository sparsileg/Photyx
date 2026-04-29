// commands/display.rs — Image display and pixel data Tauri command handlers

use std::sync::Arc;
use tauri::{Manager, State};
use crate::PhotoxState;

#[tauri::command]
pub fn get_current_frame(state: State<Arc<PhotoxState>>) -> Result<String, String> {
    let ctx = state.context.lock().expect("context lock poisoned");

    let path = ctx.file_list.get(ctx.current_frame)
        .ok_or_else(|| "No image loaded".to_string())?;

    let buffer = ctx.image_buffers.get(path)
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

    const MAX_DISPLAY_W: usize = 1200;
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
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90)
                .encode_image(&img)
                .map_err(|e| e.to_string())?;
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
            return Ok(format!("data:image/jpeg;base64,{}", b64));
        }
    }

    // Normal path: render raw pixels without stretch
    let mut rgb = Vec::with_capacity(pixel_count * 3);

    match pixels {
        PixelData::U16(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    let mut sum = 0u32; let mut count = 0u32;
                    for dy in 0..step {
                        let sy = oy * step + dy;
                        if sy >= src_h { continue; }
                        for dx in 0..step {
                            let sx = ox * step + dx;
                            if sx >= src_w { continue; }
                            let idx = if is_rgb { (sy * src_w + sx) * channels } else { sy * src_w + sx };
                            sum += v[idx] as u32;
                            count += 1;
                        }
                    }
                    let val = (sum as f32 / (count as f32 * 65535.0) * 255.0) as u8;
                    if is_rgb {
                        let mut sr = 0u32; let mut sg = 0u32; let mut sb = 0u32; let mut sc = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                let idx = (sy * src_w + sx) * 3;
                                sr += v[idx] as u32;
                                sg += v[idx + 1] as u32;
                                sb += v[idx + 2] as u32;
                                sc += 1;
                            }
                        }
                        let scale = sc as f32 * 65535.0;
                        rgb.push((sr as f32 / scale * 255.0) as u8);
                        rgb.push((sg as f32 / scale * 255.0) as u8);
                        rgb.push((sb as f32 / scale * 255.0) as u8);
                    } else {
                        rgb.push(val); rgb.push(val); rgb.push(val);
                    }
                }
            }
        }
        PixelData::F32(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    if is_rgb {
                        let mut sr = 0.0f32; let mut sg = 0.0f32; let mut sb = 0.0f32; let mut sc = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                let idx = (sy * src_w + sx) * 3;
                                if v[idx].is_finite() { sr += v[idx]; sg += v[idx+1]; sb += v[idx+2]; sc += 1; }
                            }
                        }
                        let sc = sc as f32;
                        rgb.push((sr / sc * 255.0).clamp(0.0, 255.0) as u8);
                        rgb.push((sg / sc * 255.0).clamp(0.0, 255.0) as u8);
                        rgb.push((sb / sc * 255.0).clamp(0.0, 255.0) as u8);
                    } else {
                        let mut sum = 0.0f32; let mut count = 0u32;
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
                        let val = (sum / count as f32 * 255.0).clamp(0.0, 255.0) as u8;
                        rgb.push(val); rgb.push(val); rgb.push(val);
                    }
                }
            }
        }
        PixelData::U8(v) => {
            for oy in 0..disp_h {
                for ox in 0..disp_w {
                    if is_rgb {
                        let mut sr = 0u32; let mut sg = 0u32; let mut sb = 0u32; let mut sc = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                let idx = (sy * src_w + sx) * 3;
                                sr += v[idx] as u32; sg += v[idx+1] as u32; sb += v[idx+2] as u32; sc += 1;
                            }
                        }
                        let scale = sc as f32 * 255.0;
                        rgb.push((sr as f32 / scale * 255.0) as u8);
                        rgb.push((sg as f32 / scale * 255.0) as u8);
                        rgb.push((sb as f32 / scale * 255.0) as u8);
                    } else {
                        let mut sum = 0u32; let mut count = 0u32;
                        for dy in 0..step {
                            let sy = oy * step + dy;
                            if sy >= src_h { continue; }
                            for dx in 0..step {
                                let sx = ox * step + dx;
                                if sx >= src_w { continue; }
                                sum += v[sy * src_w + sx] as u32; count += 1;
                            }
                        }
                        let val = (sum as f32 / (count as f32 * 255.0) * 255.0) as u8;
                        rgb.push(val); rgb.push(val); rgb.push(val);
                    }
                }
            }
        }
    }

    let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
        .ok_or_else(|| "Failed to create display image".to_string())?;
    let mut buf = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85)
        .encode_image(&img)
        .map_err(|e| e.to_string())?;
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

#[tauri::command]
pub fn get_autostretch_frame(
    shadow_clip: Option<f32>,
    target_background: Option<f32>,
    state: State<Arc<PhotoxState>>,
) -> Result<String, String> {
    use crate::plugins::auto_stretch::compute_autostretch_jpeg;
    let ctx = state.context.lock().expect("context lock poisoned");
    let jpeg_bytes = compute_autostretch_jpeg(
        &ctx,
        shadow_clip.unwrap_or(-2.8),
        target_background.unwrap_or(0.15),
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

#[tauri::command]
pub fn start_background_cache(
    state: State<Arc<PhotoxState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    {
        let mut ctx = state.context.lock().expect("context lock poisoned");
        if ctx.file_list.is_empty() { return Ok(()); }
        ctx.blink_cache_status = crate::context::BlinkCacheStatus::Building;
    }

    let app = app.clone();
    let num_threads = (num_cpus::get()).saturating_sub(1).max(1);
    tauri::async_runtime::spawn(async move {
        let state_arc = app.state::<Arc<PhotoxState>>();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .expect("Failed to build thread pool");

        let snapshots = {
            let ctx = state_arc.context.lock().expect("context lock poisoned");
            let snaps: Vec<_> = ctx.file_list.iter().filter_map(|path| {
                let buf = ctx.image_buffers.get(path)?;
                let pixels = buf.pixels.as_ref()?;
                let is_prerendered = buf.keywords.get("PXTYPE")
                    .map(|kw| kw.value == "HEATMAP")
                    .unwrap_or(false);
                let channels = buf.channels as usize;
                use crate::context::PixelData;
                let snap = match pixels {
                    PixelData::U8(v)  => PixelData::U8(v.clone()),
                    PixelData::U16(v) => PixelData::U16(v.clone()),
                    PixelData::F32(v) => PixelData::F32(v.clone()),
                };
                Some((path.clone(), buf.width as usize, buf.height as usize, channels, is_prerendered, snap))
            }).collect();
            snaps
        };

        const MAX_DISPLAY_W: usize = 1200;

        let display_results: Vec<(String, Vec<u8>)> = pool.install(|| {
            use rayon::prelude::*;
            snapshots.par_iter().filter_map(|(path, src_w, src_h, channels, is_prerendered, pixels)| {
                let step = if *src_w > MAX_DISPLAY_W { (src_w + MAX_DISPLAY_W - 1) / MAX_DISPLAY_W } else { 1 };
                let disp_w = src_w / step;
                let disp_h = src_h / step;
                let pixel_count = disp_w * disp_h;
                let is_rgb = *channels == 3;

                use crate::context::PixelData;

                if *is_prerendered && is_rgb {
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
                        let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)?;
                        let mut buf = std::io::Cursor::new(Vec::new());
                        use image::codecs::jpeg::JpegEncoder;
                        JpegEncoder::new_with_quality(&mut buf, 90).encode_image(&img).ok()?;
                        return Some((path.clone(), buf.into_inner()));
                    }
                }

                let num_ch = if is_rgb { 3 } else { 1 };
                let mut display_channels: Vec<Vec<f32>> = (0..num_ch)
                    .map(|_| Vec::with_capacity(pixel_count))
                    .collect();

                match pixels {
                    PixelData::U16(v) => {
                        for oy in 0..disp_h {
                            for ox in 0..disp_w {
                                for ch in 0..num_ch {
                                    let mut sum = 0u32; let mut count = 0u32;
                                    for dy in 0..step {
                                        let sy = oy * step + dy;
                                        if sy >= *src_h { continue; }
                                        for dx in 0..step {
                                            let sx = ox * step + dx;
                                            if sx >= *src_w { continue; }
                                            let idx = (sy * src_w + sx) * num_ch + ch;
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
                                for ch in 0..num_ch {
                                    let mut sum = 0.0f32; let mut count = 0u32;
                                    for dy in 0..step {
                                        let sy = oy * step + dy;
                                        if sy >= *src_h { continue; }
                                        for dx in 0..step {
                                            let sx = ox * step + dx;
                                            if sx >= *src_w { continue; }
                                            let idx = (sy * src_w + sx) * num_ch + ch;
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
                                for ch in 0..num_ch {
                                    let mut sum = 0u32; let mut count = 0u32;
                                    for dy in 0..step {
                                        let sy = oy * step + dy;
                                        if sy >= *src_h { continue; }
                                        for dx in 0..step {
                                            let sx = ox * step + dx;
                                            if sx >= *src_w { continue; }
                                            let idx = (sy * src_w + sx) * num_ch + ch;
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

                let stf_params: Vec<(f32, f32)> = display_channels.iter()
                    .map(|ch| crate::plugins::cache_frames::compute_stf_params_pub(ch))
                    .collect();

                for (ch_data, &(c0, m)) in display_channels.iter_mut().zip(stf_params.iter()) {
                    let c0_range = (1.0 - c0).max(f32::EPSILON);
                    for p in ch_data.iter_mut() {
                        let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
                        *p = crate::plugins::cache_frames::mtf_pub(m, clipped);
                    }
                }

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

                let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)?;
                let mut buf = std::io::Cursor::new(Vec::new());
                use image::codecs::jpeg::JpegEncoder;
                JpegEncoder::new_with_quality(&mut buf, 85).encode_image(&img).ok()?;
                Some((path.clone(), buf.into_inner()))
            }).collect()
        });

        tracing::info!("Background cache: display-resolution stretched JPEGs complete");

        for &(res_name, target_w) in &[("12", 376u32), ("25", 752u32)] {
            let results: Vec<(String, Vec<u8>)> = pool.install(|| {
                use rayon::prelude::*;
                display_results.par_iter().filter_map(|(path, jpeg_bytes)| {
                    let img = image::load_from_memory(jpeg_bytes).ok()?;
                    let src_w = img.width();
                    let src_h = img.height();
                    let target_h = (src_h as f32 * target_w as f32 / src_w as f32).round() as u32;
                    let resized = img.resize(target_w, target_h, image::imageops::FilterType::Triangle);
                    let mut buf = std::io::Cursor::new(Vec::new());
                    use image::codecs::jpeg::JpegEncoder;
                    JpegEncoder::new_with_quality(&mut buf, 75).encode_image(&resized).ok()?;
                    Some((path.clone(), buf.into_inner()))
                }).collect()
            });

            {
                let mut ctx = state_arc.context.lock().expect("context lock poisoned");
                match res_name {
                    "12" => { ctx.blink_cache_12.clear(); for (p, j) in results { ctx.blink_cache_12.insert(p, j); } }
                    _    => { ctx.blink_cache_25.clear(); for (p, j) in results { ctx.blink_cache_25.insert(p, j); } }
                }
            }
            tracing::info!("Background cache: {} complete", res_name);
        }

        let mut ctx = state_arc.context.lock().expect("context lock poisoned");
        ctx.blink_cache_status = crate::context::BlinkCacheStatus::Ready;
        tracing::info!("Background blink cache complete");
    });

    Ok(())
}

#[tauri::command]
pub fn get_pixel(x: u32, y: u32, state: State<Arc<PhotoxState>>) -> Result<serde_json::Value, String> {
    let ctx = state.context.lock().expect("context lock poisoned");

    let path = ctx.file_list.get(ctx.current_frame)
        .ok_or_else(|| "No image loaded".to_string())?;

    let buffer = ctx.image_buffers.get(path)
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
        let ctx = state.context.lock().expect("context lock poisoned");
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
        JpegEncoder::new_with_quality(&mut buf, 90)
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
    let ctx = state.context.lock().expect("context lock poisoned");

    let path = ctx.file_list.get(ctx.current_frame)
        .ok_or_else(|| "No image loaded".to_string())?;

    let buffer = ctx.image_buffers.get(path)
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
        let mut ctx = state.context.lock().expect("context lock poisoned");
        if ctx.file_list.len() == 1 && ctx.file_list[0] == path {
            // Same file — just update
        } else if ctx.file_list.len() == 1 {
            let old = ctx.file_list[0].clone();
            ctx.file_list.clear();
            ctx.image_buffers.remove(&old);
        }
        if !ctx.file_list.contains(&path) {
            ctx.file_list.push(path.clone());
            ctx.image_buffers.insert(path.clone(), buffer.clone());
        }
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

    const MAX_DISPLAY_W: usize = 1200;
    let step = if width > MAX_DISPLAY_W { (width + MAX_DISPLAY_W - 1) / MAX_DISPLAY_W } else { 1 };
    let disp_w = width / step;
    let disp_h = height / step;
    let pixel_count = disp_w * disp_h;

    let mut rgb = Vec::with_capacity(pixel_count * 3);

    if is_prerendered && channels == 3 {
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
    } else {
        match &pixels {
            PixelData::U16(v) => {
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        if channels == 3 {
                            let mut sr = 0u32; let mut sg = 0u32; let mut sb = 0u32; let mut sc = 0u32;
                            for dy in 0..step {
                                let sy = oy * step + dy;
                                if sy >= height { continue; }
                                for dx in 0..step {
                                    let sx = ox * step + dx;
                                    if sx >= width { continue; }
                                    let idx = (sy * width + sx) * 3;
                                    sr += v[idx] as u32;
                                    sg += v[idx + 1] as u32;
                                    sb += v[idx + 2] as u32;
                                    sc += 1;
                                }
                            }
                            let scale = sc as f32 * 65535.0;
                            rgb.push((sr as f32 / scale * 255.0) as u8);
                            rgb.push((sg as f32 / scale * 255.0) as u8);
                            rgb.push((sb as f32 / scale * 255.0) as u8);
                        } else {
                            let mut sum = 0u32; let mut count = 0u32;
                            for dy in 0..step {
                                let sy = oy * step + dy;
                                if sy >= height { continue; }
                                for dx in 0..step {
                                    let sx = ox * step + dx;
                                    if sx >= width { continue; }
                                    sum += v[sy * width + sx] as u32;
                                    count += 1;
                                }
                            }
                            let val = (sum as f32 / (count as f32 * 65535.0) * 255.0) as u8;
                            rgb.push(val); rgb.push(val); rgb.push(val);
                        }
                    }
                }
            }
            PixelData::F32(v) => {
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        if channels == 3 {
                            let mut sr = 0.0f32; let mut sg = 0.0f32; let mut sb = 0.0f32; let mut sc = 0u32;
                            for dy in 0..step {
                                let sy = oy * step + dy;
                                if sy >= height { continue; }
                                for dx in 0..step {
                                    let sx = ox * step + dx;
                                    if sx >= width { continue; }
                                    let idx = (sy * width + sx) * 3;
                                    if v[idx].is_finite() {
                                        sr += v[idx];
                                        sg += v[idx + 1];
                                        sb += v[idx + 2];
                                        sc += 1;
                                    }
                                }
                            }
                            let sc = sc as f32;
                            rgb.push((sr / sc * 255.0).clamp(0.0, 255.0) as u8);
                            rgb.push((sg / sc * 255.0).clamp(0.0, 255.0) as u8);
                            rgb.push((sb / sc * 255.0).clamp(0.0, 255.0) as u8);
                        } else {
                            let mut sum = 0.0f32; let mut count = 0u32;
                            for dy in 0..step {
                                let sy = oy * step + dy;
                                if sy >= height { continue; }
                                for dx in 0..step {
                                    let sx = ox * step + dx;
                                    if sx >= width { continue; }
                                    let val = v[sy * width + sx];
                                    if val.is_finite() { sum += val; count += 1; }
                                }
                            }
                            let val = (sum / count as f32 * 255.0).clamp(0.0, 255.0) as u8;
                            rgb.push(val); rgb.push(val); rgb.push(val);
                        }
                    }
                }
            }
            PixelData::U8(v) => {
                for oy in 0..disp_h {
                    for ox in 0..disp_w {
                        if channels == 3 {
                            let mut sr = 0u32; let mut sg = 0u32; let mut sb = 0u32; let mut sc = 0u32;
                            for dy in 0..step {
                                let sy = oy * step + dy;
                                if sy >= height { continue; }
                                for dx in 0..step {
                                    let sx = ox * step + dx;
                                    if sx >= width { continue; }
                                    let idx = (sy * width + sx) * 3;
                                    sr += v[idx] as u32;
                                    sg += v[idx + 1] as u32;
                                    sb += v[idx + 2] as u32;
                                    sc += 1;
                                }
                            }
                            let scale = sc as f32 * 255.0;
                            rgb.push((sr as f32 / scale * 255.0) as u8);
                            rgb.push((sg as f32 / scale * 255.0) as u8);
                            rgb.push((sb as f32 / scale * 255.0) as u8);
                        } else {
                            let mut sum = 0u32; let mut count = 0u32;
                            for dy in 0..step {
                                let sy = oy * step + dy;
                                if sy >= height { continue; }
                                for dx in 0..step {
                                    let sx = ox * step + dx;
                                    if sx >= width { continue; }
                                    sum += v[sy * width + sx] as u32;
                                    count += 1;
                                }
                            }
                            let val = (sum as f32 / (count as f32 * 255.0) * 255.0) as u8;
                            rgb.push(val); rgb.push(val); rgb.push(val);
                        }
                    }
                }
            }
        }
    }

    let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)
        .ok_or_else(|| "Failed to create preview image".to_string())?;
    let mut buf = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90)
        .encode_image(&img)
        .map_err(|e| e.to_string())?;
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

// ----------------------------------------------------------------------
