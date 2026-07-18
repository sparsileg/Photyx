// src-tauri/src/db/migrations.rs
// Schema migration runner using PRAGMA user_version.
// To add a new migration: push a new closure onto the `migrations` vec
// and bump CURRENT_SCHEMA_VERSION by one. Never edit existing entries.

use rusqlite::{Connection, Result};
use crate::db::schema;

pub const CURRENT_SCHEMA_VERSION: u32 = 7;
pub fn run_migrations(conn: &Connection) -> Result<()> {
    let version = get_version(conn)?;
    tracing::info!("DB schema version on open: {}", version);
    let migrations: Vec<fn(&Connection) -> Result<()>> = vec![
        migrate_v1,  // version 0 → 1: create all tables
        migrate_v2,  // version 1 → 2: rename snr_reject_sigma → signal_weight_reject_sigma
        migrate_v3,  // version 2 → 3: drop active_directory from crash_recovery
        migrate_v4,  // version 3 → 4: drop bg_stddev_reject_sigma and bg_gradient_reject_sigma
        migrate_v5,  // version 4 → 5: drop unused tables (session_history, algorithm_sets,
                     //                frame_analysis_results, console_history) and the dead
                     //                signal_weight_reject_sigma column (Issue 89)
        migrate_v6,  // version 5 → 6: drop crash_recovery — feature removed, never used
                     //                in production (Issue 107)
        migrate_v7,  // version 6 → 7: create feature_flags (Issue 130)
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

// Creates feature_flags (Issue 130) — a dynamically toggleable UI-facing
// switch table, distinct from preferences/threshold_profiles in that the
// backend has no seed rows and no knowledge of which keys are valid; the
// frontend's FEATURE_FLAGS registry owns that. A fresh install and an
// existing database both just get the empty table via IF NOT EXISTS.
fn migrate_v7(conn: &Connection) -> Result<()> {
    conn.execute_batch(schema::CREATE_FEATURE_FLAGS)
}

// Drops crash_recovery — the feature was live at one point (this table has
// a real reader/writer, unlike the dead tables migrate_v5 dropped) but was
// never actually reachable in practice: check_crash_recovery gated on an
// open session_history row, and open_session (the only writer of such a
// row) had no callers, so the restore offer could never fire. Confirmed
// dead, not fixed — the feature is being removed rather than reconnected
// (Issue 107). IF EXISTS handles both an existing database and a
// from-here-on fresh install identically.
fn migrate_v6(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS crash_recovery;"
    )
}

// Drops four tables that were created but never given a runtime reader or
// writer (Issue 89), and the signal_weight_reject_sigma column, dead since
// the Signal Weight metric was removed from classification. Child tables
// (referencing others via foreign key) are dropped before their parents.
// IF EXISTS on the table drops handles both an existing database (which has
// all four, created by the historical migrate_v1) and a from-here-on fresh
// install — the fixed migrate_v1 fidelity above means signal_weight_reject_sigma
// will always exist by this point in either case, so its DROP COLUMN does not
// need the same IF EXISTS guard.
fn migrate_v5(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS frame_analysis_results;
         DROP TABLE IF EXISTS algorithm_sets;
         DROP TABLE IF EXISTS session_history;
         DROP TABLE IF EXISTS console_history;
         ALTER TABLE threshold_profiles DROP COLUMN signal_weight_reject_sigma;"
    )
}

fn migrate_v4(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE threshold_profiles DROP COLUMN bg_stddev_reject_sigma;
         ALTER TABLE threshold_profiles DROP COLUMN bg_gradient_reject_sigma;"
    )
}

fn migrate_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE crash_recovery DROP COLUMN active_directory;"
    )
}

fn migrate_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE threshold_profiles
         RENAME COLUMN snr_reject_sigma TO signal_weight_reject_sigma;"
    )
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
