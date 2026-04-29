use std::sync::Arc;
// commands/preferences.rs — Preferences and Quick Launch Tauri command handlers

use tauri::State;
use crate::PhotoxState;
use crate::db;

#[tauri::command]
pub fn get_all_preferences(state: State<Arc<PhotoxState>>) -> Result<std::collections::HashMap<String, String>, String> {
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
pub fn set_preference(key: String, value: String, state: State<Arc<PhotoxState>>) -> Result<(), String> {
    let db = state.db.lock().expect("db lock poisoned");
    db.execute(
        "INSERT INTO preferences (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![key, value, db::now_unix()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_quick_launch_buttons(state: State<Arc<PhotoxState>>) -> Result<Vec<serde_json::Value>, String> {
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
pub fn save_quick_launch_buttons(
    buttons: Vec<serde_json::Value>,
    state: State<Arc<PhotoxState>>,
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

#[tauri::command]
pub fn get_recent_directories(state: State<Arc<PhotoxState>>) -> Result<Vec<String>, String> {
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
pub fn record_directory_visit(path: String, state: State<Arc<PhotoxState>>) -> Result<(), String> {
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
