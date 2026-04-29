use std::sync::Arc;
// commands/macros.rs — Macro database Tauri command handlers

use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::db;
use crate::PhotoxState;

// ── Return types ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct MacroRow {
    pub id:           i64,
    pub name:         String,
    pub display_name: String,
    pub script:       String,
    pub tags:         Option<String>,
    pub run_count:    i64,
    pub last_run_at:  Option<i64>,
    pub created_at:   i64,
    pub updated_at:   i64,
}

#[derive(Serialize)]
pub struct MacroVersionRow {
    pub id:       i64,
    pub macro_id: i64,
    pub script:   String,
    pub saved_at: i64,
}

// ── Name derivation ───────────────────────────────────────────────────────────

/// Derives a RunMacro-compatible `name` from a `display_name`.
/// Spaces → hyphens; strips characters that are not alphanumeric, hyphens, or underscores.
pub fn derive_name(display_name: &str) -> String {
    display_name
        .chars()
        .map(|c| if c == ' ' { '-' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Returns all macros ordered by display_name (case-insensitive).
#[tauri::command]
pub fn get_macros(state: State<Arc<PhotoxState>>) -> Result<Vec<MacroRow>, String> {
    let db = state.db.lock().expect("db lock poisoned");
    let mut stmt = db
        .prepare(
            "SELECT id, name, display_name, script, tags, run_count, last_run_at,
                    created_at, updated_at
             FROM macros
             ORDER BY display_name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(MacroRow {
                id:           row.get(0)?,
                name:         row.get(1)?,
                display_name: row.get(2)?,
                script:       row.get(3)?,
                tags:         row.get(4)?,
                run_count:    row.get(5)?,
                last_run_at:  row.get(6)?,
                created_at:   row.get(7)?,
                updated_at:   row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

/// Inserts or updates a macro by name.
/// On update, saves the previous script to macro_versions before overwriting.
/// Returns the macro id.
#[tauri::command]
pub fn save_macro(
    state:        State<Arc<PhotoxState>>,
    name:         String,
    display_name: String,
    script:       String,
) -> Result<i64, String> {
    let db  = state.db.lock().expect("db lock poisoned");
    let now = db::now_unix();

    let existing: Option<(i64, String)> = db
        .query_row(
            "SELECT id, script FROM macros WHERE name = ?1",
            params![name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    if let Some((existing_id, old_script)) = existing {
        // Preserve the previous version before overwriting.
        db.execute(
            "INSERT INTO macro_versions (macro_id, script, saved_at) VALUES (?1, ?2, ?3)",
            params![existing_id, old_script, now],
        )
        .map_err(|e| e.to_string())?;

        db.execute(
            "UPDATE macros
             SET display_name = ?1, script = ?2, updated_at = ?3
             WHERE id = ?4",
            params![display_name, script, now, existing_id],
        )
        .map_err(|e| e.to_string())?;

        Ok(existing_id)
    } else {
        db.execute(
            "INSERT INTO macros (name, display_name, script, run_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, ?4, ?5)",
            params![name, display_name, script, now, now],
        )
        .map_err(|e| e.to_string())?;

        Ok(db.last_insert_rowid())
    }
}

/// Deletes a macro by id. ON DELETE CASCADE removes version history automatically.
#[tauri::command]
pub fn delete_macro(state: State<Arc<PhotoxState>>, id: i64) -> Result<(), String> {
    let db = state.db.lock().expect("db lock poisoned");
    db.execute("DELETE FROM macros WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Renames a macro: re-derives `name` from `new_display_name`, updates both columns.
/// Returns the new derived name so the frontend can update its state.
#[tauri::command]
pub fn rename_macro(
    state:            State<Arc<PhotoxState>>,
    id:               i64,
    new_display_name: String,
) -> Result<String, String> {
    let new_name = derive_name(&new_display_name);
    if new_name.is_empty() {
        return Err("Display name produces an empty derived name.".to_string());
    }

    let db  = state.db.lock().expect("db lock poisoned");
    let now = db::now_unix();

    db.execute(
        "UPDATE macros SET name = ?1, display_name = ?2, updated_at = ?3 WHERE id = ?4",
        params![new_name, new_display_name, now, id],
    )
    .map_err(|e| e.to_string())?;

    Ok(new_name)
}

/// Returns version history for a macro, newest first.
#[tauri::command]
pub fn get_macro_versions(
    state:    State<Arc<PhotoxState>>,
    macro_id: i64,
) -> Result<Vec<MacroVersionRow>, String> {
    let db = state.db.lock().expect("db lock poisoned");
    let mut stmt = db
        .prepare(
            "SELECT id, macro_id, script, saved_at
             FROM macro_versions
             WHERE macro_id = ?1
             ORDER BY saved_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![macro_id], |row| {
            Ok(MacroVersionRow {
                id:       row.get(0)?,
                macro_id: row.get(1)?,
                script:   row.get(2)?,
                saved_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

/// Restores a macro to a previous version.
/// Saves the current script to macro_versions before overwriting.
#[tauri::command]
pub fn restore_macro_version(
    state:      State<Arc<PhotoxState>>,
    version_id: i64,
) -> Result<(), String> {
    let db  = state.db.lock().expect("db lock poisoned");
    let now = db::now_unix();

    let (macro_id, version_script): (i64, String) = db
        .query_row(
            "SELECT macro_id, script FROM macro_versions WHERE id = ?1",
            params![version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|e| format!("Version not found: {}", e))?;

    let current_script: String = db
        .query_row(
            "SELECT script FROM macros WHERE id = ?1",
            params![macro_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Macro not found: {}", e))?;

    // Preserve the current script as a version before restoring.
    db.execute(
        "INSERT INTO macro_versions (macro_id, script, saved_at) VALUES (?1, ?2, ?3)",
        params![macro_id, current_script, now],
    )
    .map_err(|e| e.to_string())?;

    db.execute(
        "UPDATE macros SET script = ?1, updated_at = ?2 WHERE id = ?3",
        params![version_script, now, macro_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Increments run_count and updates last_run_at for a macro.
/// Called after a successful RunMacro execution.
#[tauri::command]
pub fn increment_macro_run_count(state: State<Arc<PhotoxState>>, id: i64) -> Result<(), String> {
    let db  = state.db.lock().expect("db lock poisoned");
    let now = db::now_unix();
    db.execute(
        "UPDATE macros SET run_count = run_count + 1, last_run_at = ?1 WHERE id = ?2",
        params![now, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// One-time migration: rewrites Quick Launch button scripts from
/// `RunMacro path="..."` (filesystem) to `RunMacro name="..."` (DB).
/// Called from lib.rs run() after DB is opened. Safe to call on every
/// launch — only rewrites rows that still have the old path= format.
pub fn migrate_quick_launch_macro_refs(db: &rusqlite::Connection) -> Result<(), String> {
    let mut stmt = db
        .prepare("SELECT id, script FROM quick_launch_buttons")
        .map_err(|e| e.to_string())?;

    let rows: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    let now = db::now_unix();

    for (id, script) in rows {
        if let Some(path_val) = extract_path_arg(&script) {
            // Derive the macro name from the file stem.
            let stem = std::path::Path::new(&path_val)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&path_val)
                .to_string();

            // Only rewrite if a macro with that name exists in the DB.
            let exists: bool = db
                .query_row(
                    "SELECT COUNT(*) FROM macros WHERE name = ?1",
                    params![stem],
                    |row| row.get::<_, i64>(0),
                )
                .map(|n| n > 0)
                .unwrap_or(false);

            if exists {
                let new_script = format!("RunMacro name=\"{}\"", stem);
                db.execute(
                    "UPDATE quick_launch_buttons
                     SET script = ?1, updated_at = ?2
                     WHERE id = ?3",
                    params![new_script, now, id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }

    Ok(())
}

/// Extracts the path value from a `RunMacro path="..."` script string.
fn extract_path_arg(script: &str) -> Option<String> {
    let lower = script.to_lowercase();
    if !lower.starts_with("runmacro") { return None; }
    if !lower.contains("path=")       { return None; }

    let after_path = script.split_once("path=")?.1;
    if after_path.starts_with('"') {
        let inner = after_path.strip_prefix('"')?;
        let end   = inner.find('"')?;
        Some(inner[..end].to_string())
    } else {
        let end = after_path.find(char::is_whitespace).unwrap_or(after_path.len());
        Some(after_path[..end].to_string())
    }
}

// ----------------------------------------------------------------------
