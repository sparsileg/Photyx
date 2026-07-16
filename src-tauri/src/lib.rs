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

use context::AppContext;
use plugin::registry::PluginRegistry;
use plugin::{ArgMap, PluginError, PluginOutput};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::State;
use tracing::info;

pub mod constants;
mod pcode;
mod render;

/// Global registry reference for use by RunMacro and the pcode interpreter
pub static GLOBAL_REGISTRY: once_cell::sync::OnceCell<Arc<PluginRegistry>> =
    once_cell::sync::OnceCell::new();

/// Global DB reference for use by RunMacro (plugins cannot access PhotoxState directly)
pub static GLOBAL_DB: once_cell::sync::OnceCell<std::sync::Mutex<rusqlite::Connection>> =
    once_cell::sync::OnceCell::new();

/// Global progress atomics — written by long-running plugins, polled by the frontend
pub static PROGRESS_CURRENT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);
pub static PROGRESS_LABEL: once_cell::sync::OnceCell<Mutex<String>> =
    once_cell::sync::OnceCell::new();
pub static PROGRESS_TOTAL: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

/// Convenience function for plugins to update progress label and counters atomically.
pub fn set_progress(label: &str, current: u32, total: u32) {
    if let Some(l) = PROGRESS_LABEL.get() {
        if let Ok(mut g) = l.lock() { *g = label.to_string(); }
    }
    PROGRESS_CURRENT.store(current, std::sync::atomic::Ordering::Relaxed);
    PROGRESS_TOTAL.store(total, std::sync::atomic::Ordering::Relaxed);
}

// ── Script result types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct ScriptResult {
    pub line_number:    usize,
    pub command:        String,
    pub success:        bool,
    pub message:        Option<String>,
    pub data:           Option<serde_json::Value>,
    pub trace_line:     Option<String>,
    pub client_actions: Vec<String>,
}

// ── Async job result ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct JobResult {
    pub results:         Vec<ScriptResult>,
    pub session_changed: bool,
    pub display_changed: bool,
    pub client_actions:  Vec<String>,
}

pub static JOB_RESULT: once_cell::sync::OnceCell<Mutex<Option<JobResult>>> =
    once_cell::sync::OnceCell::new();

/// Guards against a second run_script call overlapping with one already in
/// flight. Without this, two threads freely interleave writes to the global
/// progress atomics and the single-slot JOB_RESULT, causing flicker and lost
/// results (Issue 83). Released via JobGuard's Drop impl so it resets even if
/// the spawned thread panics somewhere execute_script itself doesn't catch
/// (i.e. outside a plugin's execute(), which is caught at the registry
/// dispatch site) — a flag stuck at `true` would otherwise permanently block
/// every future script, which is the exact failure mode this issue exists to
/// eliminate, not reintroduce under a different name.
pub static JOB_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// RAII guard that releases JOB_RUNNING when dropped, including during a panic unwind.
struct JobGuard;

impl Drop for JobGuard {
    fn drop(&mut self) {
        JOB_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

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

        // Assert and Print are handled here directly, not via the plugin
        // registry, so the interactive console path shares exactly the same
        // variable-resolution logic as the script/macro path
        // (pcode::mod::execute_line) — one implementation, two entry points.
        match command.to_lowercase().as_str() {
            "assert" => {
                let expression = args.get("expression").cloned().unwrap_or_default();
                match crate::pcode::expr::evaluate_condition(&expression, &ctx.variables) {
                    Ok(true)  => Ok(PluginOutput::Success),
                    Ok(false) => Err(PluginError::new("ASSERT_FAILED", &format!("Assertion failed: {}", expression))),
                    Err(e)    => Err(PluginError::new("EXPR_ERROR", &format!("Assert expression error: {}", e))),
                }
            }
            "print" => {
                let message = args.get("message").cloned().unwrap_or_default();
                // Issue 118: previously fell back to substitute_vars (literal
                // passthrough) on any evaluate_expr error, which silently
                // swallowed the undefined-variable error that function is
                // now expected to raise — matching Assert's error handling
                // just above instead.
                match crate::pcode::expr::evaluate_expr(&message, &ctx.variables) {
                    Ok(evaluated) => Ok(PluginOutput::Message(evaluated)),
                    Err(e) => Err(PluginError::new("EXPR_ERROR", &format!("Print expression error: {}", e))),
                }
            }
            _ => state.registry.dispatch(&mut ctx, &command, &args),
        }
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
    pub accepted: bool,
}

// Commands that modify the session file list or active directory.
const SESSION_COMMANDS: &[&str] = &[
    "addfiles", "clearsession", "commitanalysis", "filterbykeyword", "movefile", "readimages", "rejectcurrentframe", "runmacro", "setframe",
];

// Commands that alter the pixel data currently displayed in the viewer.
const DISPLAY_COMMANDS: &[&str] = &[
    "autostretch",
];

#[tauri::command]
fn run_script(script: String, state: State<Arc<PhotoxState>>) -> ScriptResponse {
    // Reject a second script while one is already running, rather than
    // letting both threads interleave writes to the global progress
    // atomics and the single-slot JOB_RESULT (Issue 83).
    if JOB_RUNNING
        .compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst)
        .is_err()
    {
        return ScriptResponse { accepted: false };
    }

    // Clear progress atomics, label, and job result slot before starting
    PROGRESS_CURRENT.store(0, std::sync::atomic::Ordering::Relaxed);
    PROGRESS_TOTAL.store(0, std::sync::atomic::Ordering::Relaxed);
    if let Some(label) = PROGRESS_LABEL.get() {
        if let Ok(mut g) = label.lock() { g.clear(); }
    }
    if let Some(slot) = JOB_RESULT.get() {
        *slot.lock().expect("job result lock poisoned") = None;
    }

    let state = Arc::clone(&state);

    std::thread::spawn(move || {
        let _job_guard = JobGuard; // releases JOB_RUNNING on scope exit, including panics

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

        let job_result = JobResult {
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
        };

        if let Some(slot) = JOB_RESULT.get() {
            *slot.lock().expect("job result lock poisoned") = Some(job_result);
        }
    });

    ScriptResponse { accepted: true }
}

// ── Tauri command: get async job result ───────────────────────────────────────

#[tauri::command]
fn get_job_result() -> Option<JobResult> {
    JOB_RESULT.get().and_then(|slot| {
        slot.lock().expect("job result lock poisoned").take()
    })
}

// ── Tauri command: get progress ───────────────────────────────────────────────

#[tauri::command]
fn get_progress() -> (String, u32, u32) {
    let label = PROGRESS_LABEL.get()
        .and_then(|m| m.lock().ok())
        .map(|g| g.clone())
        .unwrap_or_default();
    (
        label,
        PROGRESS_CURRENT.load(std::sync::atomic::Ordering::Relaxed),
        PROGRESS_TOTAL.load(std::sync::atomic::Ordering::Relaxed),
    )
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

/// Pin glibc's mmap threshold to a static 1 MB (Linux only).
///
/// By default, glibc dynamically raises its mmap threshold to the size of
/// the largest recently freed mmap'd block (capped at 32 MB). Photyx's
/// ~17 MB pixel buffers fall under that raised cap, so after the first few
/// frees, all subsequent pixel-buffer allocations are served from the brk
/// heap instead of mmap. Heap memory can only be returned to the OS from
/// the top down, and interleaved small allocations across threads pin the
/// heap top — leaving gigabytes of freed-but-resident memory after
/// ClearSession (multi-GB residuals confirmed empirically, July 2026).
///
/// Setting the threshold explicitly disables the dynamic adjustment:
/// every allocation ≥ 1 MB gets its own mmap and is returned to the OS
/// immediately on free. Small allocations (star vectors, JPEG buffers,
/// keyword maps) stay on the fast heap path. Windows uses a different
/// allocator and has not exhibited this behavior.
///
/// Must run before any large allocations — first statement in run().
#[cfg(target_os = "linux")]
fn pin_mmap_threshold() {
    const MMAP_THRESHOLD_BYTES: libc::c_int = 1_048_576; // 1 MB
    let ok = unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, MMAP_THRESHOLD_BYTES) };
    if ok != 1 {
        eprintln!("Warning: mallopt(M_MMAP_THRESHOLD) failed — memory may not be returned to the OS promptly");
    }
}

#[cfg(not(target_os = "linux"))]
fn pin_mmap_threshold() {}

/// Registers every built-in plugin against the given registry. Extracted
/// from run() (Issue 99) so a test can build the same real command surface
/// without duplicating this list by hand — a hand-duplicated copy would
/// itself become a second place that can drift.
fn register_all_plugins(registry: &PluginRegistry) {
    plugins::scripting::register_all(registry);
    registry.register(Arc::new(plugins::add_files::AddFiles));
    registry.register(Arc::new(plugins::analyze_frames::AnalyzeFrames));
    registry.register(Arc::new(plugins::auto_stretch::AutoStretch));
    registry.register(Arc::new(plugins::background_median::BackgroundGradientPlugin));
    registry.register(Arc::new(plugins::background_median::BackgroundMedianPlugin));
    registry.register(Arc::new(plugins::background_median::BackgroundStdDevPlugin));
    registry.register(Arc::new(plugins::cache_frames::CacheFrames));
    registry.register(Arc::new(plugins::clear_session::ClearSession));
    registry.register(Arc::new(plugins::clear_stack::ClearStack));
    registry.register(Arc::new(plugins::commit_analysis::CommitAnalysis));
    registry.register(Arc::new(plugins::commit_stretch::CommitStretch));
    registry.register(Arc::new(plugins::compute_eccentricity::ComputeEccentricity));
    registry.register(Arc::new(plugins::compute_fwhm::ComputeFWHM));
    registry.register(Arc::new(plugins::contour_heatmap::ContourHeatmap));
    registry.register(Arc::new(plugins::debayer_image::DebayerImage));
    registry.register(Arc::new(plugins::export_analysis_report::ExportAnalysisReport));
    registry.register(Arc::new(plugins::filter_by_keyword::FilterByKeyword));
    registry.register(Arc::new(plugins::get_histogram::GetHistogram));
    registry.register(Arc::new(plugins::keywords::AddKeyword));
    registry.register(Arc::new(plugins::keywords::CopyKeyword));
    registry.register(Arc::new(plugins::keywords::DeleteKeyword));
    registry.register(Arc::new(plugins::keywords::ModifyKeyword));
    registry.register(Arc::new(plugins::list_keywords::ListKeywords));
    registry.register(Arc::new(plugins::read_images::ReadImages));
    registry.register(Arc::new(plugins::reject_current_frame::RejectCurrentFrame));
    registry.register(Arc::new(plugins::run_macro::RunMacro));
    registry.register(Arc::new(plugins::set_frame::SetFrame));
    registry.register(Arc::new(plugins::stack_frames::StackFrames));
    registry.register(Arc::new(plugins::star_count::CountStarsPlugin));
    registry.register(Arc::new(plugins::write_current_files::WriteCurrent));
    registry.register(Arc::new(plugins::write_fits::WriteFIT));
    registry.register(Arc::new(plugins::write_frame::WriteFrame));
    registry.register(Arc::new(plugins::write_tiff::WriteTIFF));
    registry.register(Arc::new(plugins::write_xisf::WriteXISF));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    pin_mmap_threshold();
    let _log_guard = init_logging();
    info!("Photyx starting up");

    let registry = Arc::new(PluginRegistry::new());
    register_all_plugins(&registry);

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
                fwhm:              crate::analysis::session_stats::MetricThresholds { reject: profile.fwhm_reject_sigma as f32 },
                star_count:        crate::analysis::session_stats::MetricThresholds { reject: profile.star_count_reject_sigma as f32 },
                eccentricity:      crate::analysis::session_stats::MetricThresholds { reject: profile.eccentricity_reject_abs as f32 },
            };
        }
    }

    // Initialize JOB_RESULT and PROGRESS_LABEL slots
    let _ = JOB_RESULT.set(Mutex::new(None));
    let _ = PROGRESS_LABEL.set(Mutex::new(String::new()));

    let state = Arc::new(PhotoxState {
        registry,
        context:  Mutex::new(app_context),
        db:       Mutex::new(db_conn),
        settings: Mutex::new(app_settings),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::analysis::commit_analysis_results,
            commands::analysis::get_analysis_results,
            commands::analysis::get_star_positions,
            commands::analysis::load_analysis_json,
            commands::analysis::set_frame_flag,
            commands::backup::backup_database,
            commands::backup::restore_database,
            commands::display::get_autostretch_frame,
            commands::display::get_autostretch_stack_frame,
            commands::display::get_blink_cache_status,
            commands::display::get_blink_frame,
            commands::display::get_cpu_count,
            commands::display::get_current_frame,
            commands::display::get_full_frame,
            commands::display::get_histogram,
            commands::display::get_pixel,
            commands::display::get_stack_frame,
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
            commands::session::debug_buffer_info,
            commands::session::get_keywords,
            commands::session::get_session,
            commands::session::get_variable,
            commands::threshold_profiles::delete_threshold_profile,
            commands::threshold_profiles::get_active_threshold_profile_id,
            commands::threshold_profiles::get_threshold_profiles,
            commands::threshold_profiles::save_threshold_profile,
            commands::threshold_profiles::set_active_threshold_profile,
            dispatch_command,
            get_job_result,
            get_progress,
            list_plugins,
            run_script,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ----------------------------------------------------------------------

#[cfg(test)]
mod command_drift_tests {
    use super::*;
    use std::collections::HashSet;

    // Embedded at compile time — no filesystem read at test time, no path
    // fragility. Path is relative to this file (src-tauri/src/lib.rs).
    const PCODE_COMMANDS_JSON: &str = include_str!("../../src-svelte/lib/pcode_commands.json");

    /// Commands handled directly by the pcode interpreter or the frontend
    /// console via dedicated code paths (a string match, or a distinct
    /// AST/line type) rather than being looked up in the plugin registry or
    /// the backend CLIENT_COMMANDS array. There is no backend "source of
    /// truth" list to diff these against, so this set is maintained by
    /// hand — reviewed whenever a new language construct or console-only
    /// command is added.
    const INTERPRETER_AND_CONSOLE_ONLY: &[&str] = &[
        "assert", "print", "log", "set",
        "if", "else", "endif", "for", "endfor",
        "clear", "help",
    ];

    /// Retired commands (Technical Reference §4.2) — intentionally absent
    /// from pcode.ts, the plugin registry, and CLIENT_COMMANDS. Listed here
    /// so that if one is ever mistakenly reintroduced without a working
    /// implementation, this test still doesn't silently let it through.
    const RETIRED_COMMANDS: &[&str] = &[
        "selectdirectory", "getimageproperty", "getsessionproperty",
        "listfiles", "test", "cropimage",
        "readall", "readfit", "readtiff", "readxisf",
    ];

    #[test]
    fn pcode_commands_match_backend_surface() {
        let raw: Vec<String> = serde_json::from_str(PCODE_COMMANDS_JSON)
            .expect("pcode_commands.json must parse as a JSON array of strings");

        let excluded: HashSet<String> = INTERPRETER_AND_CONSOLE_ONLY.iter()
            .chain(RETIRED_COMMANDS.iter())
            .map(|s| s.to_string())
            .collect();

        let pcode_commands: HashSet<String> = raw.into_iter()
            .map(|s| s.to_lowercase())
            .filter(|s| !excluded.contains(s))
            .collect();

        let registry = PluginRegistry::new();
        register_all_plugins(&registry);

        let mut backend_surface: HashSet<String> = registry.list().into_iter().collect();
        backend_surface.extend(pcode::CLIENT_COMMANDS.iter().map(|s| s.to_string()));

        let missing_from_backend: Vec<&String> =
            pcode_commands.difference(&backend_surface).collect();
        let missing_from_pcode: Vec<&String> =
            backend_surface.difference(&pcode_commands).collect();

        assert!(
            missing_from_backend.is_empty() && missing_from_pcode.is_empty(),
            "pcode.ts / backend command surface mismatch!\n\
             In pcode.ts but not backend (registry + CLIENT_COMMANDS): {:?}\n\
             In backend but not pcode.ts: {:?}",
            missing_from_backend, missing_from_pcode,
        );
    }
}

// ---------------------------------------------------------------------
// ---------------------------------------------------------------------
// ---------------------------------------------------------------------
