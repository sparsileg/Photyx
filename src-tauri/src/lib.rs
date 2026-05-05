// lib.rs — Tauri application entry point and command handlers
// Spec §4.2

mod analysis;
mod commands;
mod context;
mod db;
mod logging;
mod plugin;
mod plugins;
mod settings;
mod utils;

use commands::session::start_crash_recovery_timer;
use context::AppContext;
use plugin::registry::PluginRegistry;
use plugin::{ArgMap, PluginOutput};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::State;
use tracing::info;

mod pcode;

/// Global registry reference for use by RunMacro and the pcode interpreter
pub static GLOBAL_REGISTRY: once_cell::sync::OnceCell<Arc<PluginRegistry>> =
    once_cell::sync::OnceCell::new();

/// Global DB reference for use by RunMacro (plugins cannot access PhotoxState directly)
pub static GLOBAL_DB: once_cell::sync::OnceCell<std::sync::Mutex<rusqlite::Connection>> =
    once_cell::sync::OnceCell::new();

// ── Application state ─────────────────────────────────────────────────────────

pub struct PhotoxState {
    pub registry: Arc<PluginRegistry>,
    pub context:  Mutex<AppContext>,
    pub db:       Mutex<rusqlite::Connection>,
    pub settings: Mutex<settings::AppSettings>,
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
    pub data:    Option<serde_json::Value>,
}

#[tauri::command]
async fn dispatch_command(
    request: DispatchRequest,
    state:   State<'_, Arc<PhotoxState>>,
) -> Result<DispatchResponse, String> {
    let state: Arc<PhotoxState> = Arc::clone(&state);
    let command = request.command.clone();
    let args = request.args.clone();
    let result = tokio::task::spawn_blocking(move || {
        tracing::info!("spawn_blocking: starting {}", command);
        let mut ctx = state.context.lock().expect("context lock poisoned");
        state.registry.dispatch(&mut ctx, &command, &args)
    }).await.map_err(|e| format!("spawn_blocking panicked: {:?}", e))?;
    match result {
        Ok(output) => {
            let (msg, data) = match output {
                PluginOutput::Success        => (None, None),
                PluginOutput::Message(m)     => (Some(m), None),
                PluginOutput::Value(v)       => (Some(v), None),
                PluginOutput::Values(vs)     => (Some(vs.join("\n")), None),
                PluginOutput::Data(d)        => (
                    Some(
                        d.get("message")
                            .and_then(|m| m.as_str())
                            .unwrap_or("Done")
                            .to_string()
                    ),
                    Some(d),
                ),
            };
            Ok(DispatchResponse { success: true, output: msg, error: None, data })
        }
        Err(e) => {
            Ok(DispatchResponse { success: false, output: None, error: Some(e.message), data: None })
        }
    }
}

// ── Tauri command: execute a pcode script ─────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ScriptResponse {
    pub results:         Vec<ScriptResult>,
    pub session_changed: bool,
    pub display_changed: bool,
    pub client_actions:  Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ScriptResult {
    pub line_number:    usize,
    pub command:        String,
    pub success:        bool,
    pub message:        Option<String>,
    pub data:           Option<serde_json::Value>,
    pub trace_line:     Option<String>,
    pub client_actions: Vec<String>,
}

// Commands that modify the session file list or active directory.
const SESSION_COMMANDS: &[&str] = &[
    "readfit", "readtiff", "readxisf", "readall",
    "selectdirectory", "clearsession", "movefile", "runmacro",
];

// Commands that alter the pixel data currently displayed in the viewer.
const DISPLAY_COMMANDS: &[&str] = &[
    "autostretch", "linearstretch", "histogramequalization",
];

#[tauri::command]
fn run_script(script: String, state: State<Arc<PhotoxState>>) -> ScriptResponse {
    let mut ctx = state.context.lock().expect("context lock poisoned");
    let results = pcode::execute_script(&script, &mut ctx, &state.registry, true);

    let mut session_changed = false;
    let mut display_changed = false;

    for r in &results {
        if r.success {
            let cmd = r.command.to_lowercase();
            if SESSION_COMMANDS.contains(&cmd.as_str()) { session_changed = true; }
            if DISPLAY_COMMANDS.contains(&cmd.as_str()) { display_changed = true; }
        }
    }

    let client_actions: Vec<String> = results.iter()
        .filter(|r| r.success)
        .flat_map(|r| r.client_actions.iter().cloned())
        .collect();

    ScriptResponse {
        results: results.iter().map(|r| ScriptResult {
            line_number:    r.line_number,
            command:        r.command.clone(),
            success:        r.success,
            message:        r.message.clone(),
            data:           r.data.clone(),
            trace_line:     r.trace_line.clone(),
            client_actions: r.client_actions.clone(),
        }).collect(),
        session_changed,
        display_changed,
        client_actions,
    }
}

// ── Tauri command: list registered plugins ────────────────────────────────────

#[tauri::command]
fn list_plugins(state: State<Arc<PhotoxState>>) -> Vec<serde_json::Value> {
    state.registry.list_with_details()
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

    plugins::scripting::register_all(&registry);
    registry.register(Arc::new(plugins::analyze_frames::AnalyzeFrames));
    registry.register(Arc::new(plugins::auto_stretch::AutoStretch));
    registry.register(Arc::new(plugins::background_median::BackgroundGradientPlugin));
    registry.register(Arc::new(plugins::background_median::BackgroundMedianPlugin));
    registry.register(Arc::new(plugins::background_median::BackgroundStdDevPlugin));
    registry.register(Arc::new(plugins::cache_frames::CacheFrames));
    registry.register(Arc::new(plugins::clear_session::ClearSession));
    registry.register(Arc::new(plugins::compute_eccentricity::ComputeEccentricity));
    registry.register(Arc::new(plugins::compute_fwhm::ComputeFWHM));
    registry.register(Arc::new(plugins::contour_heatmap::ContourHeatmap));
    registry.register(Arc::new(plugins::get_histogram::GetHistogram));
    registry.register(Arc::new(plugins::highlight_clipping::SnrEstimatePlugin));
    registry.register(Arc::new(plugins::keywords::AddKeyword));
    registry.register(Arc::new(plugins::keywords::CopyKeyword));
    registry.register(Arc::new(plugins::keywords::DeleteKeyword));
    registry.register(Arc::new(plugins::keywords::ModifyKeyword));
    registry.register(Arc::new(plugins::list_keywords::ListKeywords));
    registry.register(Arc::new(plugins::read_all_files::ReadAll));
    registry.register(Arc::new(plugins::read_fits::ReadFIT));
    registry.register(Arc::new(plugins::read_tiff::ReadTIFF));
    registry.register(Arc::new(plugins::read_xisf::ReadXISF));
    registry.register(Arc::new(plugins::run_macro::RunMacro));
    registry.register(Arc::new(plugins::select_directory::SelectDirectory));
    registry.register(Arc::new(plugins::set_frame::SetFrame));
    registry.register(Arc::new(plugins::star_count::CountStarsPlugin));
    registry.register(Arc::new(plugins::write_current_files::WriteCurrent));
    registry.register(Arc::new(plugins::write_fits::WriteFIT));
    registry.register(Arc::new(plugins::write_frame::WriteFrame));
    registry.register(Arc::new(plugins::write_tiff::WriteTIFF));
    registry.register(Arc::new(plugins::write_xisf::WriteXISF));

    let _ = GLOBAL_REGISTRY.set(registry.clone());

    let app_data_dir = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Photyx");
    std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");

    let db_conn = db::open_db(app_data_dir.clone()).expect("Failed to open database");
    commands::macros::migrate_quick_launch_macro_refs(&db_conn)
        .unwrap_or_else(|e| tracing::warn!("Quick Launch macro migration failed: {}", e));
    let global_db_conn = db::open_db(app_data_dir).expect("Failed to open global DB connection");
    let _ = GLOBAL_DB.set(Mutex::new(global_db_conn));

    let mut app_settings = settings::AppSettings::new();
    app_settings.load_from_db(&db_conn);
    app_settings.load_threshold_profiles(&db_conn);

    let mut app_context = AppContext::new();
    app_context.sync_from_settings(&app_settings);

    // Load active threshold profile into AppContext
    if let Some(active_id) = app_settings.active_threshold_profile_id {
        if let Some(profile) = app_settings.threshold_profiles.iter().find(|p| p.id == active_id) {
            app_context.analysis_thresholds = crate::analysis::session_stats::AnalysisThresholds {
                background_median: crate::analysis::session_stats::MetricThresholds { reject: profile.bg_median_reject_sigma as f32 },
                snr_estimate:      crate::analysis::session_stats::MetricThresholds { reject: profile.snr_reject_sigma.abs() as f32 },
                fwhm:              crate::analysis::session_stats::MetricThresholds { reject: profile.fwhm_reject_sigma as f32 },
                star_count:        crate::analysis::session_stats::MetricThresholds { reject: profile.star_count_reject_sigma.abs() as f32 },
                eccentricity:      crate::analysis::session_stats::MetricThresholds { reject: profile.eccentricity_reject_abs as f32 },
            };
        }
    }

    let state = Arc::new(PhotoxState {
        registry,
        context:  Mutex::new(app_context),
        db:       Mutex::new(db_conn),
        settings: Mutex::new(app_settings),
    });

    tauri::Builder::default()
        .setup(|app| { start_crash_recovery_timer(app) })
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::analysis::commit_analysis_results,
            commands::analysis::get_analysis_results,
            commands::analysis::get_frame_flags,
            commands::analysis::get_star_positions,
            commands::analysis::load_analysis_json,
            commands::analysis::set_frame_flag,
            commands::backup::backup_database,
            commands::backup::restore_database,
            commands::display::get_autostretch_frame,
            commands::display::get_blink_cache_status,
            commands::display::get_blink_frame,
            commands::display::get_current_frame,
            commands::display::get_full_frame,
            commands::display::get_histogram,
            commands::display::get_pixel,
            commands::display::load_file,
            commands::display::start_background_cache,
            commands::logging::list_log_files,
            commands::logging::read_log_file,
            commands::macros::delete_macro,
            commands::macros::get_macro_versions,
            commands::macros::get_macros,
            commands::macros::increment_macro_run_count,
            commands::macros::rename_macro,
            commands::macros::restore_macro_version,
            commands::macros::save_macro,
            commands::preferences::get_all_preferences,
            commands::preferences::get_quick_launch_buttons,
            commands::preferences::get_recent_directories,
            commands::preferences::record_directory_visit,
            commands::preferences::save_quick_launch_buttons,
            commands::preferences::set_preference,
            commands::threshold_profiles::delete_threshold_profile,
            commands::threshold_profiles::get_active_threshold_profile_id,
            commands::threshold_profiles::get_threshold_profiles,
            commands::threshold_profiles::save_threshold_profile,
            commands::threshold_profiles::set_active_threshold_profile,
            commands::session::check_crash_recovery,
            commands::session::close_session,
            commands::session::debug_buffer_info,
            commands::session::get_keywords,
            commands::session::get_session,
            commands::session::get_variable,
            commands::session::open_session,
            commands::session::write_crash_recovery,
            dispatch_command,
            list_plugins,
            run_script,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ----------------------------------------------------------------------
