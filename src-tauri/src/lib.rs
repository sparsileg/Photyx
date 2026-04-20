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
// tracing_subscriber configured in logging.rs

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
