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
