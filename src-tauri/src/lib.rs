// lib.rs — Tauri application entry point and command handlers
// Spec §4.2

mod analysis;
mod context;
mod db;
mod logging;
mod plugin;
mod plugins;
mod utils;

use context::AppContext;
use plugin::registry::PluginRegistry;
use plugin::{ArgMap, PluginOutput};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};
use tracing::info;

mod pcode;

/// Global registry reference for use by RunMacro and the pcode interpreter
pub static GLOBAL_REGISTRY: once_cell::sync::OnceCell<Arc<PluginRegistry>> = once_cell::sync::OnceCell::new();


// ── Application state ─────────────────────────────────────────────────────────

pub struct PhotoxState {
    pub registry: Arc<PluginRegistry>,
    pub context:  Mutex<AppContext>,
    pub db:       Mutex<rusqlite::Connection>,
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
fn dispatch_command(
    request: DispatchRequest,
    state:   State<PhotoxState>,
) -> DispatchResponse {
    let mut ctx = state.context.lock().expect("context lock poisoned");
    match state.registry.dispatch(&mut ctx, &request.command, &request.args) {
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
            DispatchResponse { success: true, output: msg, error: None, data }
        }
        Err(e) => {
            DispatchResponse { success: false, output: None, error: Some(e.message), data: None }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ScriptResponse {
    pub results:         Vec<ScriptResult>,
    pub session_changed: bool,
    pub display_changed: bool,
}

#[derive(Debug, Serialize)]
pub struct ScriptResult {
    pub line_number: usize,
    pub command:     String,
    pub success:     bool,
    pub message:     Option<String>,
    pub data:        Option<serde_json::Value>,
    pub trace_line:  Option<String>,
}

const SESSION_COMMANDS: &[&str] = &[
    "readfit", "readtiff", "readxisf", "readall",
    "readallfitfiles", "readalltifffiles", "readallxisffiles", "readallfiles",
    "selectdirectory", "clearsession", "movefile", "runmacro",
];
const DISPLAY_COMMANDS: &[&str] = &[
    "autostretch", "linearstretch", "histogramequalization", "runmacro",
];

/// Execute a pcode script string — used by the macro editor and Quick Launch
#[tauri::command]
fn run_script(
    script: String,
    state:  State<PhotoxState>,
) -> ScriptResponse {
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

    ScriptResponse {
        results: results.iter().map(|r| ScriptResult {
            line_number: r.line_number,
            command:     r.command.clone(),
            success:     r.success,
            message:     r.message.clone(),
            data:        r.data.clone(),
            trace_line:  r.trace_line.clone(),
        }).collect(),
        session_changed,
        display_changed,
    }
}

// ── Tauri command: list registered plugins ────────────────────────────────────

#[tauri::command]
fn list_plugins(state: State<PhotoxState>) -> Vec<serde_json::Value> {
    state.registry.list_with_details()
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
            "display_width": b.display_width,
            "bit_depth": format!("{:?}", b.bit_depth),
            "color_space": format!("{:?}", b.color_space),
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
fn get_autostretch_frame(
    shadow_clip: Option<f32>,
    target_background: Option<f32>,
    state: State<PhotoxState>,
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

    let db_conn = db::open_db(app_data_dir).expect("Failed to open database");

    let state = PhotoxState {
        registry,
        context: Mutex::new(AppContext::new()),
        db:      Mutex::new(db_conn),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            debug_buffer_info,
            delete_macro,
            dispatch_command,
            get_all_preferences,
            get_analysis_results,
            get_autostretch_frame,
            get_blink_cache_status,
            get_blink_frame,
            get_current_frame,
            get_frame_flags,
            get_full_frame,
            get_histogram,
            get_keywords,
            get_macros_dir,
            get_pixel,
            get_quick_launch_buttons,
            get_recent_directories,
            get_session,
            get_star_positions,
            get_variable,
            list_log_files,
            list_macros,
            list_plugins,
            load_file,
            read_log_file,
            record_directory_visit,
            rename_macro,
            run_script,
            save_quick_launch_buttons,
            set_preference,
            start_background_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}


#[tauri::command]
fn get_analysis_results(state: State<PhotoxState>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");

    let frames: Vec<serde_json::Value> = ctx.file_list.iter().enumerate().map(|(i, path)| {
        let flag = ctx.analysis_results.get(path)
            .and_then(|r| r.flag.as_ref())
            .map(|f| f.as_str().to_string())
            .or_else(|| ctx.image_buffers.get(path)
                .and_then(|b| b.keywords.get("PXFLAG"))
                .map(|kw| kw.value.clone()))
            .unwrap_or_default();

        let short = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let label = extract_frame_label(short);

        if let Some(r) = ctx.analysis_results.get(path) {
            serde_json::json!({
                "index":               i,
                "filename":            path,
                "label":               label,
                "short_name":          short,
                "background_median":   r.background_median,
                "background_stddev":   r.background_stddev,
                "background_gradient": r.background_gradient,
                "snr_estimate":        r.snr_estimate,
                "fwhm":                r.fwhm,
                "eccentricity":        r.eccentricity,
                "star_count":          r.star_count,
                "flag":                flag,
                "triggered":           r.triggered_by,
            })
        } else {
            serde_json::json!({
                "index":      i,
                "filename":   path,
                "label":      label,
                "short_name": short,
                "flag":       flag,
                "triggered":  [],
            })
        }
    }).collect();

    use crate::analysis::session_stats::compute_session_stats;
    let result_refs: Vec<&crate::analysis::AnalysisResult> = ctx.file_list.iter()
        .filter_map(|p| ctx.analysis_results.get(p))
        .collect();
    let stats = compute_session_stats(&result_refs);

    serde_json::json!({
        "frames": frames,
        "session_stats": {
            "background_median":   { "mean": stats.background_median.mean,   "stddev": stats.background_median.stddev },
            "background_stddev":   { "mean": stats.background_stddev.mean,   "stddev": stats.background_stddev.stddev },
            "background_gradient": { "mean": stats.background_gradient.mean, "stddev": stats.background_gradient.stddev },
            "snr_estimate":        { "mean": stats.snr_estimate.mean,        "stddev": stats.snr_estimate.stddev },
            "fwhm":                { "mean": stats.fwhm.mean,                "stddev": stats.fwhm.stddev },
            "eccentricity":        { "mean": stats.eccentricity.mean,        "stddev": stats.eccentricity.stddev },
            "star_count":          { "mean": stats.star_count.mean,          "stddev": stats.star_count.stddev },
        }
    })
}

fn extract_frame_label(filename: &str) -> String {
    let stem = filename.rsplit('.').nth(1).unwrap_or(filename);
    let digits: String = stem.chars().rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars().rev().collect();
    if !digits.is_empty() && digits.len() <= 6 {
        return digits.trim_start_matches('0').to_string()
            .parse::<u32>().unwrap_or(0).to_string();
    }
    let chars: Vec<char> = stem.chars().collect();
    chars[chars.len().saturating_sub(8)..].iter().collect()
}

#[tauri::command]
fn get_frame_flags(state: State<PhotoxState>) -> Vec<String> {
    let ctx = state.context.lock().expect("context lock poisoned");
    ctx.file_list.iter().map(|path| {
        // Check analysis_results first (in-memory, most current)
        if let Some(result) = ctx.analysis_results.get(path) {
            if let Some(flag) = &result.flag {
                return flag.as_str().to_string();
            }
        }
        // Fall back to PXFLAG keyword in image buffer
        if let Some(buf) = ctx.image_buffers.get(path) {
            if let Some(kw) = buf.keywords.get("PXFLAG") {
                return kw.value.clone();
            }
        }
        String::new()
    }).collect()
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
    let num_threads = (num_cpus::get()).saturating_sub(1).max(1);
    tauri::async_runtime::spawn(async move {
        let state_arc = app.state::<PhotoxState>();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .expect("Failed to build thread pool");

        // ── Build per-frame stretched JPEGs for blink cache ───────────────────
        // Each frame is stretched independently using Auto-STF, then downsampled
        // to blink resolution. This is the only cache we maintain; normal display
        // renders raw pixels on the fly via get_current_frame.

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

        // Build full-resolution stretched JPEGs first, then downsample for blink
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

                // Prerendered RGB (e.g. heatmap): downsample directly, no stretch
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

                // Normal path: extract per-channel display data
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

                // Apply Auto-STF per channel
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

                // Interleave channels to RGB
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

        info!("Background cache: display-resolution stretched JPEGs complete");

        // ── Blink caches — downsample from stretched display JPEGs ────────────
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
            info!("Background cache: {} complete", res_name);
        }

        // Mark complete
        let mut ctx = state_arc.context.lock().expect("context lock poisoned");
        ctx.blink_cache_status = crate::context::BlinkCacheStatus::Ready;
        info!("Background blink cache complete");
    });

    Ok(())
}

#[tauri::command]
fn get_pixel(x: u32, y: u32, state: State<PhotoxState>) -> Result<serde_json::Value, String> {
    let ctx = state.context.lock().expect("context lock poisoned");

    let path = ctx.file_list.get(ctx.current_frame)
        .ok_or_else(|| "No image loaded".to_string())?;

    let buffer = ctx.image_buffers.get(path)
        .ok_or_else(|| "Buffer not found".to_string())?;

    let pixels = buffer.pixels.as_ref()
        .ok_or_else(|| "No pixel data".to_string())?;

    let w = buffer.width as u32;
    let h = buffer.height as u32;
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
        "raw": raw,
        "val": val,
        "channels": ch,
    }))
}

#[tauri::command]
fn get_full_frame(state: State<PhotoxState>) -> Result<String, String> {
    let path = {
        let ctx = state.context.lock().expect("context lock poisoned");
        ctx.file_list.get(ctx.current_frame)
            .cloned()
            .ok_or_else(|| "No image loaded".to_string())?
    };

    // Return from cache if already built
    {
        let ctx = state.context.lock().expect("context lock poisoned");
        if let Some(jpeg_bytes) = ctx.full_res_cache.get(&path) {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(jpeg_bytes);
            return Ok(format!("data:image/jpeg;base64,{}", b64));
        }
    }

    // Build full-res JPEG from raw buffer
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

    // Store in cache
    {
        let mut ctx = state.context.lock().expect("context lock poisoned");
        ctx.full_res_cache.insert(path, jpeg_bytes.clone());
    }

    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

#[tauri::command]
fn get_keywords(state: State<PhotoxState>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    let path = match ctx.file_list.get(ctx.current_frame) {
        Some(p) => p,
        None => return serde_json::json!({}),
    };
    let buffer = match ctx.image_buffers.get(path) {
        Some(b) => b,
        None => return serde_json::json!({}),
    };

    let mut map = serde_json::Map::new();
    for kw in buffer.keywords.values() {
        map.insert(kw.name.clone(), serde_json::json!({
            "name": kw.name,
            "value": kw.value,
            "comment": kw.comment,
        }));
    }
    serde_json::Value::Object(map)
}


#[tauri::command]
fn get_histogram(state: State<PhotoxState>) -> Result<serde_json::Value, String> {
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
        "bins":        stats.bins,
        "bins_g":      stats.bins_g,
        "bins_b":      stats.bins_b,
        "median":      stats.median,
        "median_g":    stats.median_g,
        "median_b":    stats.median_b,
        "mean":        stats.mean,
        "std_dev":     stats.std_dev,
        "std_dev_g":   stats.std_dev_g,
        "std_dev_b":   stats.std_dev_b,
        "clipping_pct": stats.clipping_pct,
    }))
}


#[tauri::command]
fn get_star_positions(state: State<PhotoxState>) -> serde_json::Value {
    use crate::analysis::{self, stars::detect_stars, fwhm::star_fwhm, StarDetectionConfig};

    let ctx = state.context.lock().expect("context lock poisoned");

    let img = match ctx.current_image() {
        Some(i) => i,
        None => return serde_json::json!({ "stars": [] }),
    };

    let pixels = match img.pixels.as_ref() {
        Some(p) => p,
        None => return serde_json::json!({ "stars": [] }),
    };

    let channels = img.channels as usize;
    let width    = img.width as usize;
    let height   = img.height as usize;

    let luma   = analysis::to_luminance(pixels, channels);
    let config = StarDetectionConfig::default();
    let stars  = detect_stars(&luma, width, height, &config);

    let positions: Vec<serde_json::Value> = stars.iter()
        .filter_map(|s| {
            let fwhm = star_fwhm(s)?;
            if fwhm < 0.5 || fwhm > 50.0 { return None; }
            Some(serde_json::json!({
                "cx":   s.cx,
                "cy":   s.cy,
                "fwhm": fwhm,
                "r":    fwhm / 2.0,
            }))
        })
        .collect();

    serde_json::json!({ "stars": positions })
}

// ── Tauri command: get macros directory ──────────────────────────────────────

#[tauri::command]
fn get_macros_dir() -> String {
    crate::utils::get_macros_dir()
        .to_str()
        .unwrap_or("")
        .replace('\\', "/")
}

// ── Tauri command: list macros ────────────────────────────────────────────────

#[tauri::command]
fn list_macros(state: State<PhotoxState>) -> Result<Vec<serde_json::Value>, String> {
    // Resolve macros directory — ctx.log_dir pattern; future: ctx.macros_dir
    let macros_path = {
        let _ctx = state.context.lock().expect("context lock poisoned");
        crate::utils::get_macros_dir()
    };

    // Create directory if it doesn't exist
    if !macros_path.exists() {
        std::fs::create_dir_all(&macros_path)
            .map_err(|e| format!("Failed to create Macros directory: {}", e))?;
    }

    let mut entries: Vec<serde_json::Value> = std::fs::read_dir(&macros_path)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path  = entry.path();
            if path.extension()?.to_str()? != "phs" { return None; }
            let filename  = path.file_name()?.to_str()?.to_string();
            let stem      = path.file_stem()?.to_str()?.to_string();
            let full_path = path.to_str()?.to_string();
            let tooltip   = extract_macro_tooltip(&full_path);
            let lines     = std::fs::read_to_string(&full_path)
                .map(|s| s.lines().count())
                .unwrap_or(0);
            Some(serde_json::json!({
                "name":     stem,
                "filename": filename,
                "path":     full_path,
                "lines":    lines,
                "tooltip":  tooltip,
            }))
        })
        .collect();

    // Sort alphabetically by name
    entries.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        na.cmp(nb)
    });

    Ok(entries)
}

/// Reads the first contiguous block of # comment lines from a .phs file
/// and returns them as a tooltip string with # stripped.
fn extract_macro_tooltip(path: &str) -> String {
    let Ok(contents) = std::fs::read_to_string(path) else { return String::new() };
    let lines: Vec<&str> = contents.lines()
        .take_while(|l| l.trim().starts_with('#') || l.trim().is_empty())
        .filter(|l| l.trim().starts_with('#'))
        .map(|l| l.trim().trim_start_matches('#').trim())
        .collect();
    lines.join("\n")
}

// ── Tauri command: rename macro ───────────────────────────────────────────────

#[tauri::command]
fn rename_macro(old_path: String, new_name: String) -> Result<String, String> {
    let old = std::path::PathBuf::from(&old_path);
    let dir = old.parent()
        .ok_or_else(|| "Cannot determine macro directory".to_string())?;
    let safe = new_name.replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != ' ', "").trim().to_string();
    if safe.is_empty() {
        return Err("Invalid macro name".to_string());
    }
    let new_path = dir.join(format!("{}.phs", safe));
    if new_path.exists() {
        return Err(format!("A macro named '{}' already exists", safe));
    }
    std::fs::rename(&old, &new_path)
        .map_err(|e| format!("Rename failed: {}", e))?;
    Ok(new_path.to_str().unwrap_or("").replace('\\', "/"))
}

// ── Tauri command: delete macro ───────────────────────────────────────────────

#[tauri::command]
fn delete_macro(path: String) -> Result<(), String> {
    std::fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete macro: {}", e))
}

// ── Tauri command: list log files ─────────────────────────────────────────────

#[tauri::command]
fn list_log_files(
    state: State<PhotoxState>,
) -> Result<Vec<serde_json::Value>, String> {
    // Resolve log directory — ctx.log_dir overrides the default
    let log_dir = {
        let ctx = state.context.lock().expect("context lock poisoned");
        ctx.log_dir.clone()
    };

    let log_path = if let Some(dir) = log_dir {
        std::path::PathBuf::from(dir)
    } else {
        crate::utils::get_log_dir()
    };

    if !log_path.exists() {
        return Ok(vec![]);
    }

    let mut entries: Vec<serde_json::Value> = std::fs::read_dir(&log_path)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path  = entry.path();
            if path.extension()?.to_str()? != "log" { return None; }
            let meta     = std::fs::metadata(&path).ok()?;
            let modified = meta.modified().ok()?;
            let modified_secs = modified
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let size = meta.len();
            let filename = path.file_name()?.to_str()?.to_string();
            let full_path = path.to_str()?.to_string();
            Some(serde_json::json!({
                "filename":      filename,
                "path":          full_path,
                "size":          size,
                "modified_secs": modified_secs,
            }))
        })
        .collect();

    // Sort newest first
    entries.sort_by(|a, b| {
        let ta = a["modified_secs"].as_u64().unwrap_or(0);
        let tb = b["modified_secs"].as_u64().unwrap_or(0);
        tb.cmp(&ta)
    });

    Ok(entries)
}

// ── Tauri command: read and parse a log file ──────────────────────────────────

#[tauri::command]
fn read_log_file(path: String) -> Result<Vec<serde_json::Value>, String> {
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read log file: {}", e))?;

    let mut lines: Vec<serde_json::Value> = Vec::new();

    for raw in contents.lines() {
        // Expected format:
        // 2026-04-26T06:40:43.946544Z  INFO photyx_lib::module: message
        // 2026-04-26T06:42:14.158053Z DEBUG tao::module: message
        let parsed = parse_log_line(raw);
        lines.push(parsed);
    }

    Ok(lines)
}

fn parse_log_line(line: &str) -> serde_json::Value {
    // Split on first 'Z ' to get timestamp and remainder
    if let Some(z_pos) = line.find("Z ") {
        let timestamp = &line[..z_pos + 1]; // include the Z
        let rest = line[z_pos + 1..].trim();

        // rest is like " INFO photyx_lib::module: message"
        // or           "DEBUG tao::module: message"
        let level;
        let remainder;

        if rest.starts_with("ERROR") {
            level = "ERROR";
            remainder = rest[5..].trim();
        } else if rest.starts_with("WARN") {
            level = "WARN";
            remainder = rest[4..].trim();
        } else if rest.starts_with("INFO") {
            level = "INFO";
            remainder = rest[4..].trim();
        } else if rest.starts_with("DEBUG") {
            level = "DEBUG";
            remainder = rest[5..].trim();
        } else if rest.starts_with("TRACE") {
            level = "TRACE";
            remainder = rest[5..].trim();
        } else {
            level = "INFO";
            remainder = rest;
        }

        // remainder is "photyx_lib::module: message"
        // Split on first ': ' to separate module from message
        let (module, message) = if let Some(colon_pos) = remainder.find(": ") {
            (&remainder[..colon_pos], remainder[colon_pos + 2..].trim())
        } else {
            ("", remainder)
        };

        serde_json::json!({
            "timestamp": timestamp,
            "level":     level,
            "module":    module,
            "message":   message,
            "raw":       line,
        })
    } else {
        // Non-conforming line — return as raw continuation
        serde_json::json!({
            "timestamp": "",
            "level":     "RAW",
            "module":    "",
            "message":   line,
            "raw":       line,
        })
    }
}

/// Load a single file from disk into the session and return a base64 JPEG data URL for display.
/// Adds the file to ctx.file_list and ctx.image_buffers as a single-frame session.
/// Cleared automatically when the user loads a new batch with ReadAll/ReadFIT/etc.
#[tauri::command]
fn load_file(path: String, state: State<PhotoxState>) -> Result<String, String> {
    use crate::plugins::image_reader::read_image_file;
    use crate::context::PixelData;

    let buffer = read_image_file(&path)
        .map_err(|e| format!("Failed to load '{}': {}", path, e))?;

    // Inject into session so AutoStretch, ContourHeatmap, etc. can operate on it
    {
        let mut ctx = state.context.lock().expect("context lock poisoned");
        // Remove any previously single-loaded file if it's not part of a batch
        if ctx.file_list.len() == 1 && ctx.file_list[0] == path {
            // Same file — just update
        } else if ctx.file_list.len() == 1 {
            // Single file loaded previously — replace it
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

    // Prerendered RGB: downsample directly, no stretch
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
        // Raw render — no stretch
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

/// Get a pcode variable value from AppContext.
#[tauri::command]
fn get_variable(name: String, state: State<PhotoxState>) -> Option<String> {
    let ctx = state.context.lock().expect("context lock poisoned");
    ctx.variables.get(&name.to_uppercase())
        .or_else(|| ctx.variables.get(&name))
        .cloned()
}


// ── Tauri commands: preferences ───────────────────────────────────────────────

#[tauri::command]
fn get_all_preferences(state: State<PhotoxState>) -> Result<std::collections::HashMap<String, String>, String> {
    let db = state.db.lock().expect("db lock poisoned");
    let mut stmt = db.prepare("SELECT key, value FROM preferences")
        .map_err(|e| e.to_string())?;
    let map = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();
    Ok(map)
}

#[tauri::command]
fn set_preference(key: String, value: String, state: State<PhotoxState>) -> Result<(), String> {
    let db = state.db.lock().expect("db lock poisoned");
    db.execute(
        "INSERT INTO preferences (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, value, db::now_unix()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Tauri commands: quick launch ──────────────────────────────────────────────

#[tauri::command]
fn get_quick_launch_buttons(state: State<PhotoxState>) -> Result<Vec<serde_json::Value>, String> {
    let db = state.db.lock().expect("db lock poisoned");
    let mut stmt = db.prepare(
        "SELECT id, position, label, script FROM quick_launch_buttons ORDER BY position"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id":       row.get::<_, i64>(0)?,
            "position": row.get::<_, i64>(1)?,
            "label":    row.get::<_, String>(2)?,
            "script":   row.get::<_, String>(3)?,
        }))
    })
    .map_err(|e| e.to_string())?
    .filter_map(|r| r.ok())
    .collect();
    Ok(rows)
}

#[tauri::command]
fn save_quick_launch_buttons(
    buttons: Vec<serde_json::Value>,
    state: State<PhotoxState>,
) -> Result<(), String> {
    let db = state.db.lock().expect("db lock poisoned");
    db.execute("DELETE FROM quick_launch_buttons", [])
        .map_err(|e| e.to_string())?;
    let now = db::now_unix();
    for (i, btn) in buttons.iter().enumerate() {
        let label  = btn["label"].as_str().unwrap_or("").to_string();
        let script = btn["script"].as_str().unwrap_or("").to_string();
        db.execute(
            "INSERT INTO quick_launch_buttons (position, label, script, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![i as i64, label, script, now],
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Tauri commands: recent directories ───────────────────────────────────────

#[tauri::command]
fn get_recent_directories(state: State<PhotoxState>) -> Result<Vec<String>, String> {
    let db = state.db.lock().expect("db lock poisoned");
    let max: i64 = db.query_row(
        "SELECT CAST(value AS INTEGER) FROM preferences WHERE key = 'recent_directories_max'",
        [],
        |row| row.get(0),
    ).unwrap_or(10);
    let mut stmt = db.prepare(
        "SELECT path FROM recent_directories ORDER BY last_used DESC LIMIT ?1"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(rusqlite::params![max], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

#[tauri::command]
fn record_directory_visit(path: String, state: State<PhotoxState>) -> Result<(), String> {
    let db = state.db.lock().expect("db lock poisoned");
    let now = db::now_unix();
    db.execute(
        "INSERT INTO recent_directories (path, last_used, use_count)
         VALUES (?1, ?2, 1)
         ON CONFLICT(path) DO UPDATE SET
             last_used = excluded.last_used,
             use_count = use_count + 1",
        rusqlite::params![path, now],
    ).map_err(|e| e.to_string())?;
    // Trim to max
    let max: i64 = db.query_row(
        "SELECT CAST(value AS INTEGER) FROM preferences WHERE key = 'recent_directories_max'",
        [],
        |row| row.get(0),
    ).unwrap_or(10);
    db.execute(
        "DELETE FROM recent_directories WHERE id NOT IN (
             SELECT id FROM recent_directories ORDER BY last_used DESC LIMIT ?1
         )",
        rusqlite::params![max],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ----------------------------------------------------------------------
