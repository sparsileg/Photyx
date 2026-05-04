// commands/session.rs — Session state and crash recovery Tauri command handlers

use std::sync::Arc;
use tauri::{Manager, State};
use crate::PhotoxState;
use crate::db;

#[tauri::command]
pub fn get_session(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    serde_json::json!({
        "activeDirectory": ctx.active_directory,
        "fileList":        ctx.file_list,
        "currentFrame":    ctx.current_frame,
    })
}

#[tauri::command]
pub fn get_variable(name: String, state: State<Arc<PhotoxState>>) -> Option<String> {
    let ctx = state.context.lock().expect("context lock poisoned");
    ctx.variables.get(&name.to_uppercase())
        .or_else(|| ctx.variables.get(&name))
        .cloned()
}

#[tauri::command]
pub fn debug_buffer_info(state: State<Arc<PhotoxState>>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    let path = ctx.file_list.get(ctx.current_frame).cloned();
    let buffer_info = path.as_ref().and_then(|p| ctx.image_buffers.get(p)).map(|b| {
        serde_json::json!({
            "filename":      b.filename,
            "width":         b.width,
            "height":        b.height,
            "display_width": b.display_width,
            "bit_depth":     format!("{:?}", b.bit_depth),
            "color_space":   format!("{:?}", b.color_space),
            "channels":      b.channels,
            "has_pixels":    b.pixels.is_some(),
            "pixel_type":    b.pixels.as_ref().map(|p| match p {
                crate::context::PixelData::U8(_)  => "U8",
                crate::context::PixelData::U16(_) => "U16",
                crate::context::PixelData::F32(_) => "F32",
            }),
        })
    });
    serde_json::json!({
        "current_frame": ctx.current_frame,
        "file_count":    ctx.file_list.len(),
        "buffer":        buffer_info,
    })
}

#[tauri::command]
pub fn open_session(
    directory: String,
    file_count: usize,
    state: State<Arc<PhotoxState>>,
) -> Result<i64, String> {
    let db  = state.db.lock().expect("db lock poisoned");
    let now = db::now_unix();
    db.execute(
        "INSERT INTO session_history (directory, opened_at, file_count) VALUES (?1, ?2, ?3)",
        rusqlite::params![directory, now, file_count as i64],
    ).map_err(|e| e.to_string())?;
    let id = db.last_insert_rowid();
    let mut ctx = state.context.lock().expect("context lock poisoned");
    ctx.current_session_id = Some(id);
    Ok(id)
}

#[tauri::command]
pub fn close_session(state: State<Arc<PhotoxState>>) -> Result<(), String> {
    let db  = state.db.lock().expect("db lock poisoned");
    let now = db::now_unix();
    db.execute(
        "UPDATE session_history SET closed_at = ?1 WHERE closed_at IS NULL",
        rusqlite::params![now],
    ).map_err(|e| e.to_string())?;

    // Reset imported session flag so a fresh live session can begin
    let mut ctx = state.context.lock().expect("context lock poisoned");
    ctx.is_imported_session = false;

    Ok(())
}

pub fn do_write_crash_recovery(state: &PhotoxState) -> Result<(), String> {
    let ctx = state.context.lock().expect("context lock poisoned");
    let db  = state.db.lock().expect("db lock poisoned");
    let now = db::now_unix();
    let file_list = serde_json::to_string(&ctx.file_list).unwrap_or_default();
    let autostretch_enabled: i64 = 1;
    db.execute(
        "INSERT INTO crash_recovery
             (id, active_directory, file_list, current_frame_index, autostretch_enabled, written_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET
             active_directory    = excluded.active_directory,
             file_list           = excluded.file_list,
             current_frame_index = excluded.current_frame_index,
             autostretch_enabled = excluded.autostretch_enabled,
             written_at          = excluded.written_at",
        rusqlite::params![
            ctx.active_directory,
            file_list,
            ctx.current_frame as i64,
            autostretch_enabled,
            now,
        ],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn write_crash_recovery(state: State<Arc<PhotoxState>>) -> Result<(), String> {
    do_write_crash_recovery(&state)
}

#[tauri::command]
pub fn check_crash_recovery(state: State<Arc<PhotoxState>>) -> Result<Option<serde_json::Value>, String> {
    let db = state.db.lock().expect("db lock poisoned");

    let open_session_id: Option<i64> = db.query_row(
        "SELECT id FROM session_history WHERE closed_at IS NULL ORDER BY opened_at DESC LIMIT 1",
        [],
        |row| row.get(0),
    ).ok();

    let Some(_session_id) = open_session_id else { return Ok(None); };

    let now = db::now_unix();
    let _ = db.execute(
        "UPDATE session_history SET closed_at = ?1 WHERE closed_at IS NULL",
        rusqlite::params![now],
    );

    let result = db.query_row(
        "SELECT active_directory, file_list, current_frame_index, written_at
         FROM crash_recovery WHERE id = 1",
        [],
        |row| Ok(serde_json::json!({
            "active_directory":    row.get::<_, Option<String>>(0)?,
            "file_list":           row.get::<_, Option<String>>(1)?,
            "current_frame_index": row.get::<_, Option<i64>>(2)?,
            "written_at":          row.get::<_, i64>(3)?,
        })),
    ).ok();
    Ok(result)
}

pub fn start_crash_recovery_timer(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let app_handle = app.handle().clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            let state = app_handle.state::<Arc<PhotoxState>>();
            let has_session = {
                let ctx = state.context.lock().expect("context lock poisoned");
                ctx.current_session_id.is_some()
            };
            if has_session {
                if let Err(e) = do_write_crash_recovery(&state) {
                    tracing::warn!("Crash recovery write failed: {}", e);
                } else {
                    tracing::debug!("Crash recovery state written");
                }
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn get_keywords(state: State<Arc<PhotoxState>>) -> serde_json::Value {
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
            "name":    kw.name,
            "value":   kw.value,
            "comment": kw.comment,
        }));
    }
    serde_json::Value::Object(map)
}

// ----------------------------------------------------------------------
