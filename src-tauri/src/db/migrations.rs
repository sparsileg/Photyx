// src-tauri/src/db/migrations.rs
// Schema migration runner using PRAGMA user_version.
// To add a new migration: push a new closure onto the `migrations` vec
// and bump CURRENT_SCHEMA_VERSION by one. Never edit existing entries.

use rusqlite::{Connection, Result};
use crate::db::schema;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    let version = get_version(conn)?;
    tracing::info!("DB schema version on open: {}", version);

    let migrations: Vec<fn(&Connection) -> Result<()>> = vec![
        migrate_v1,  // version 0 → 1: create all tables
    ];

    for (i, migration) in migrations.iter().enumerate() {
        let target = (i + 1) as u32;
        if version < target {
            tracing::info!("Applying DB migration to version {}", target);
            migration(conn)?;
            set_version(conn, target)?;
            tracing::info!("DB migration to version {} complete", target);
        }
    }

    Ok(())
}

fn get_version(conn: &Connection) -> Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn set_version(conn: &Connection, version: u32) -> Result<()> {
    conn.execute_batch(&format!("PRAGMA user_version = {}", version))
}

fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(&format!(
        "{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
        schema::CREATE_PREFERENCES,
        schema::CREATE_QUICK_LAUNCH_BUTTONS,
        schema::CREATE_RECENT_DIRECTORIES,
        schema::CREATE_THRESHOLD_PROFILES,
        schema::CREATE_ALGORITHM_SETS,
        schema::CREATE_FRAME_ANALYSIS_RESULTS,
        schema::CREATE_FRAME_ANALYSIS_RESULTS_IDX_PATH,
        schema::CREATE_FRAME_ANALYSIS_RESULTS_IDX_VERSION,
        schema::CREATE_MACROS,
        schema::CREATE_MACRO_VERSIONS,
        schema::CREATE_MACRO_VERSIONS_IDX,
        schema::CREATE_SESSION_HISTORY,
        schema::CREATE_CONSOLE_HISTORY,
        schema::CREATE_CRASH_RECOVERY,
        schema::CREATE_CRASH_RECOVERY_SEED,
    ))
}
