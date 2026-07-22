// src-tauri/src/db/migrations.rs
// Schema migration runner using PRAGMA user_version.
// To add a new migration: push a new closure onto the `migrations` vec
// and bump CURRENT_SCHEMA_VERSION by one. Never edit existing entries.
//
// The incremental migrate_v1 through migrate_v7 chain (Issue 89, Issue
// 107, Issue 130, and others) was squashed into a single migrate_v1
// (Issue 163), since no installs predating that history exist in the
// wild. migrate_v1 now creates the schema in its current, final shape
// directly rather than replaying historical intermediate states.

use rusqlite::{Connection, Result};
use crate::db::schema;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

pub fn run_migrations(conn: &Connection) -> Result<()> {
    let version = get_version(conn)?;
    tracing::info!("DB schema version on open: {}", version);
    let migrations: Vec<fn(&Connection) -> Result<()>> = vec![
        migrate_v1,  // version 0 → 1: create all tables (current end-state schema)
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
        "{}{}{}{}{}{}{}{}",
        schema::CREATE_PREFERENCES,
        schema::CREATE_QUICK_LAUNCH_BUTTONS,
        schema::CREATE_RECENT_DIRECTORIES,
        schema::CREATE_THRESHOLD_PROFILES,
        schema::CREATE_FEATURE_FLAGS,
        schema::CREATE_MACROS,
        schema::CREATE_MACRO_VERSIONS,
        schema::CREATE_MACRO_VERSIONS_IDX,
    ))
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
