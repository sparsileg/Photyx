// commands/feature_flags.rs — Feature flag Tauri command handlers (Issue 130).
// Feature flags are dynamically toggleable UI-facing switches, distinct
// from preferences: the backend has no seed rows and no fixed list of
// valid keys — the frontend's FEATURE_FLAGS registry (settings/constants.ts)
// owns that. A key absent from feature_flags simply means "not yet
// toggled from its registry default."

use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use crate::PhotoxState;
use crate::db::now_unix;

#[tauri::command]
pub fn get_feature_flags(state: State<Arc<PhotoxState>>) -> Result<HashMap<String, bool>, String> {
    let db = state.db.lock().expect("db lock poisoned");
    let mut stmt = db.prepare("SELECT key, enabled FROM feature_flags")
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0))
    }).map_err(|e| e.to_string())?;

    let mut flags = HashMap::new();
    for row in rows {
        let (key, enabled) = row.map_err(|e| e.to_string())?;
        flags.insert(key, enabled);
    }
    Ok(flags)
}

#[tauri::command]
pub fn set_feature_flag(key: String, enabled: bool, state: State<Arc<PhotoxState>>) -> Result<(), String> {
    let db = state.db.lock().expect("db lock poisoned");
    db.execute(
        "INSERT INTO feature_flags (key, enabled, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
        rusqlite::params![key, enabled as i64, now_unix()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ----------------------------------------------------------------------
