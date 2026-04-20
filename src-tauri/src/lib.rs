// lib.rs — Tauri application entry point and command handlers
// Spec §4.2

mod plugin;
mod context;
mod plugins;
mod logging;

use std::sync::{Arc, Mutex};
use tauri::State;
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

    let buffer = ctx.image_buffers.get(path)
        .ok_or_else(|| "Image buffer not found".to_string())?;

    let pixels = buffer.pixels.as_ref()
        .ok_or_else(|| "No pixel data".to_string())?;

    use crate::context::PixelData;
    let width  = buffer.width as u32;
    let height = buffer.height as u32;

    // Downsample during pixel extraction — avoid full-res image allocation
    let (out_w, out_h, step) = if width > 1200 {
        let scale = width as f32 / 1200.0;
        let new_w = 1200u32;
        let new_h = (height as f32 / scale) as u32;
        (new_w, new_h, scale as usize)
    } else {
        (width, height, 1usize)
    };

    let rgb: Vec<u8> = match pixels {
        PixelData::F32(v) => {
            (0..out_h as usize).flat_map(|y| {
                (0..out_w as usize).flat_map(move |x| {
                    // Box filter — average step×step block
                    let mut sum = 0.0f32;
                    let mut count = 0usize;
                    for dy in 0..step {
                        for dx in 0..step {
                            let sy = y * step + dy;
                            let sx = x * step + dx;
                            if sy < height as usize && sx < width as usize {
                                sum += v[sy * width as usize + sx];
                                count += 1;
                            }
                        }
                    }
                    let p = (sum / count as f32).clamp(0.0, 1.0);
                    let val = (p * 255.0) as u8;
                    [val, val, val]
                }).collect::<Vec<u8>>()
            }).collect()
        }
        PixelData::U16(v) => {
            (0..out_h as usize).flat_map(|y| {
                (0..out_w as usize).flat_map(move |x| {
                    let mut sum = 0u32;
                    let mut count = 0usize;
                    for dy in 0..step {
                        for dx in 0..step {
                            let sy = y * step + dy;
                            let sx = x * step + dx;
                            if sy < height as usize && sx < width as usize {
                                sum += v[sy * width as usize + sx] as u32;
                                count += 1;
                            }
                        }
                    }
                    let val = ((sum / count as u32) >> 8) as u8;
                    [val, val, val]
                }).collect::<Vec<u8>>()
            }).collect()
        }
        PixelData::U8(v) => {
            (0..out_h as usize).flat_map(|y| {
                (0..out_w as usize).flat_map(move |x| {
                    let mut sum = 0u32;
                    let mut count = 0usize;
                    for dy in 0..step {
                        for dx in 0..step {
                            let sy = y * step + dy;
                            let sx = x * step + dx;
                            if sy < height as usize && sx < width as usize {
                                sum += v[sy * width as usize + sx] as u32;
                                count += 1;
                            }
                        }
                    }
                    let val = (sum / count as u32) as u8;
                    [val, val, val]
                }).collect::<Vec<u8>>()
            }).collect()
        }
    };

    use image::{RgbImage, ImageFormat};
    use base64::Engine as _;
    use std::io::Cursor;

    let display_img = RgbImage::from_raw(out_w, out_h, rgb)
        .ok_or_else(|| "Failed to create image".to_string())?;

    let mut buf = Cursor::new(Vec::new());
    display_img.write_to(&mut buf, ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());
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

    let state = PhotoxState {
        registry,
        context: Mutex::new(AppContext::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            dispatch_command,
            list_plugins,
            get_session,
            get_current_frame,
            debug_buffer_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
