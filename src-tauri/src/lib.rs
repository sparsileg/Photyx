// lib.rs — Tauri application entry point and command handlers
// Spec §4.2

mod plugin;
mod context;
mod plugins;
mod logging;

use std::sync::{Arc, Mutex};
use tauri::{Manager, State};
use serde::{Deserialize, Serialize};
use tracing::info;

use plugin::registry::PluginRegistry;
use plugin::{ArgMap, PluginOutput};
use context::AppContext;

// ── Application state ─────────────────────────────────────────────────────────

pub struct PhotoxState {
    pub registry: Arc<PluginRegistry>,
    pub context:  Mutex<AppContext>,
}

// ── Tauri command: dispatch a pcode command ───────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DispatchRequest {
    pub command: String,
    pub args:    ArgMap,
}

#[derive(Debug, Serialize)]
pub struct DispatchResponse {
    pub success: bool,
    pub output:  Option<String>,
    pub error:   Option<String>,
}

#[tauri::command]
fn dispatch_command(
    request: DispatchRequest,
    state:   State<PhotoxState>,
) -> DispatchResponse {
    let mut ctx = state.context.lock().expect("context lock poisoned");
    match state.registry.dispatch(&mut ctx, &request.command, &request.args) {
        Ok(output) => {
            let msg = match output {
                PluginOutput::Success        => None,
                PluginOutput::Message(m)     => Some(m),
                PluginOutput::Value(v)       => Some(v),
                PluginOutput::Values(vs)     => Some(vs.join("\n")),
            };
            DispatchResponse { success: true, output: msg, error: None }
        }
        Err(e) => {
            DispatchResponse { success: false, output: None, error: Some(e.message) }
        }
    }
}

// ── Tauri command: list registered plugins ────────────────────────────────────

#[tauri::command]
fn list_plugins(state: State<PhotoxState>) -> Vec<String> {
    state.registry.list()
}

// ── Tauri command: get session state ─────────────────────────────────────────

#[tauri::command]
fn get_session(state: State<PhotoxState>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    serde_json::json!({
        "activeDirectory": ctx.active_directory,
        "fileList": ctx.file_list,
        "currentFrame": ctx.current_frame,
    })
}

// ── Tauri command: get current frame as PNG data URL ─────────────────────────

#[tauri::command]
fn debug_buffer_info(state: State<PhotoxState>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    let path = ctx.file_list.get(ctx.current_frame).cloned();
    let buffer_info = path.as_ref().and_then(|p| ctx.image_buffers.get(p)).map(|b| {
        serde_json::json!({
            "filename": b.filename,
            "width": b.width,
            "height": b.height,
            "bit_depth": format!("{:?}", b.bit_depth),
            "channels": b.channels,
            "has_pixels": b.pixels.is_some(),
            "pixel_type": b.pixels.as_ref().map(|p| match p {
                crate::context::PixelData::U8(_)  => "U8",
                crate::context::PixelData::U16(_) => "U16",
                crate::context::PixelData::F32(_) => "F32",
            }),
        })
    });
    serde_json::json!({
        "current_frame": ctx.current_frame,
        "file_count": ctx.file_list.len(),
        "buffer": buffer_info,
    })
}

#[tauri::command]
fn get_current_frame(state: State<PhotoxState>) -> Result<String, String> {
    let ctx = state.context.lock().expect("context lock poisoned");

    let path = ctx.file_list.get(ctx.current_frame)
        .ok_or_else(|| "No image loaded".to_string())?;

    let jpeg_bytes = ctx.display_cache.get(path)
        .ok_or_else(|| "No display cache entry for current frame. Run AutoStretch first.".to_string())?;

    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg_bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}


// ── Logging init ──────────────────────────────────────────────────────────────

fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    logging::init_logging()
}

// ── Application entry point ───────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _log_guard = init_logging();
    info!("Photyx starting up");

    let registry = Arc::new(PluginRegistry::new());

    // Phase 1: built-in native plugins
    registry.register(Arc::new(plugins::select_directory::SelectDirectory));
    registry.register(Arc::new(plugins::read_fits::ReadAllFITFiles));
    // Phase 2: processing plugins
    registry.register(Arc::new(plugins::auto_stretch::AutoStretch));
    registry.register(Arc::new(plugins::set_frame::SetFrame));
    registry.register(Arc::new(plugins::clear_session::ClearSession));
    registry.register(Arc::new(plugins::cache_frames::CacheFrames));

    let state = PhotoxState {
        registry,
        context: Mutex::new(AppContext::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            dispatch_command,
            list_plugins,
            get_session,
            get_current_frame,
            get_blink_frame,
            get_blink_cache_status,
            start_background_cache,
            debug_buffer_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


#[tauri::command]
fn get_blink_frame(index: usize, resolution: String, state: State<PhotoxState>) -> Result<String, String> {
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
fn get_blink_cache_status(state: State<PhotoxState>) -> String {
    let ctx = state.context.lock().expect("context lock poisoned");
    match ctx.blink_cache_status {
        crate::context::BlinkCacheStatus::Idle    => "idle".to_string(),
        crate::context::BlinkCacheStatus::Building => "building".to_string(),
        crate::context::BlinkCacheStatus::Ready   => "ready".to_string(),
    }
}

#[tauri::command]
fn start_background_cache(state: State<PhotoxState>, app: tauri::AppHandle) -> Result<(), String> {
    {
        let mut ctx = state.context.lock().expect("context lock poisoned");
        if ctx.file_list.is_empty() { return Ok(()); }
        ctx.blink_cache_status = crate::context::BlinkCacheStatus::Building;
    }

    let app = app.clone();
    // Build a dedicated thread pool using num_cpus - 1 threads
    let num_threads = (num_cpus::get()).saturating_sub(1).max(1);
    tauri::async_runtime::spawn(async move {
        let state_arc = app.state::<PhotoxState>();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .expect("Failed to build thread pool");

        for &(res_name, max_w) in &[("12", 376usize), ("25", 752usize)] {
            // Collect snapshot
            let (_file_list, snapshots) = {
                let ctx = state_arc.context.lock().expect("context lock poisoned");
                let snapshots: Vec<_> = ctx.file_list.iter().filter_map(|path| {
                    let buf = ctx.image_buffers.get(path)?;
                    let pixels = buf.pixels.as_ref()?;
                    use crate::context::PixelData;
                    let snap = match pixels {
                        PixelData::U8(v)  => PixelData::U8(v.clone()),
                        PixelData::U16(v) => PixelData::U16(v.clone()),
                        PixelData::F32(v) => PixelData::F32(v.clone()),
                    };
                    Some((path.clone(), buf.width as usize, buf.height as usize, snap))
                }).collect();
                (ctx.file_list.clone(), snapshots)
            };

            // Process in parallel using dedicated pool
            let results: Vec<(String, Vec<u8>)> = pool.install(|| {
                use rayon::prelude::*;
                snapshots.par_iter().filter_map(|(path, src_w, src_h, pixels)| {
                    let step = if *src_w > max_w { (src_w + max_w - 1) / max_w } else { 1 };
                    let disp_w = src_w / step;
                    let disp_h = src_h / step;
                    let pixel_count = disp_w * disp_h;
                    let mut display: Vec<f32> = Vec::with_capacity(pixel_count);

                    use crate::context::PixelData;
                    match pixels {
                        PixelData::U16(v) => {
                            for oy in 0..disp_h {
                                for ox in 0..disp_w {
                                    let mut sum = 0u32; let mut count = 0u32;
                                    for dy in 0..step {
                                        let sy = oy * step + dy;
                                        if sy >= *src_h { continue; }
                                        for dx in 0..step {
                                            let sx = ox * step + dx;
                                            if sx >= *src_w { continue; }
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
                                    let mut sum = 0.0f32; let mut count = 0u32;
                                    for dy in 0..step {
                                        let sy = oy * step + dy;
                                        if sy >= *src_h { continue; }
                                        for dx in 0..step {
                                            let sx = ox * step + dx;
                                            if sx >= *src_w { continue; }
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
                                    let mut sum = 0u32; let mut count = 0u32;
                                    for dy in 0..step {
                                        let sy = oy * step + dy;
                                        if sy >= *src_h { continue; }
                                        for dx in 0..step {
                                            let sx = ox * step + dx;
                                            if sx >= *src_w { continue; }
                                            sum += v[sy * src_w + sx] as u32;
                                            count += 1;
                                        }
                                    }
                                    display.push(sum as f32 / (count as f32 * 255.0));
                                }
                            }
                        }
                    }

                    let (c0, m) = crate::plugins::cache_frames::compute_stf_params_pub(&display);
                    let c0_range = (1.0 - c0).max(f32::EPSILON);
                    let mut stretched = display;
                    for p in stretched.iter_mut() {
                        let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
                        *p = crate::plugins::cache_frames::mtf_pub(m, clipped);
                    }

                    let mut rgb = Vec::with_capacity(pixel_count * 3);
                    for &p in &stretched {
                        let val = (p.clamp(0.0, 1.0) * 255.0) as u8;
                        rgb.push(val); rgb.push(val); rgb.push(val);
                    }

                    let img = image::RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)?;
                    let mut buf = std::io::Cursor::new(Vec::new());
                    use image::codecs::jpeg::JpegEncoder;
                    JpegEncoder::new_with_quality(&mut buf, 75).encode_image(&img).ok()?;
                    Some((path.clone(), buf.into_inner()))
                }).collect()
            });

            // Store results
            {
                let mut ctx = state_arc.context.lock().expect("context lock poisoned");
                match res_name {
                    "12" => { ctx.blink_cache_12.clear(); for (p, j) in results { ctx.blink_cache_12.insert(p, j); } }
                    _    => { ctx.blink_cache_25.clear(); for (p, j) in results { ctx.blink_cache_25.insert(p, j); } }
                }
            }
            info!("Background cache: {}% complete", res_name);
        }

        // Mark complete
        let mut ctx = state_arc.context.lock().expect("context lock poisoned");
        ctx.blink_cache_status = crate::context::BlinkCacheStatus::Ready;
        info!("Background blink cache complete");
    });

    Ok(())
}

// ----------------------------------------------------------------------
