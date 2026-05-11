// src-tauri/src/db/mod.rs
// Database initialisation. Call open_db() once in lib.rs run() and store
// the returned Connection in PhotoxState as Mutex<Connection>.

pub mod migrations;
pub mod schema;

use rusqlite::{Connection, Result};
use std::path::PathBuf;

pub fn open_db(app_data_dir: PathBuf) -> Result<Connection> {
    let db_path = app_data_dir.join("photyx.db");
    tracing::info!("Opening database at {:?}", db_path);

    let conn = Connection::open(&db_path)?;

    // Performance and correctness pragmas — must run before any queries
    conn.execute_batch("
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;
        PRAGMA synchronous=NORMAL;
    ")?;

    migrations::run_migrations(&conn)?;
    seed_defaults(&conn)?;

    tracing::info!("Database ready (schema v{})", migrations::CURRENT_SCHEMA_VERSION);
    Ok(conn)
}

/// Insert default rows that must always exist.
/// All inserts use OR IGNORE so re-runs on an existing DB are safe.
fn seed_defaults(conn: &Connection) -> Result<()> {
    let now = now_unix();

    // Default "Default" threshold profile
    conn.execute(
        "INSERT OR IGNORE INTO threshold_profiles
            (name, description,
             bg_median_reject_sigma, bg_stddev_reject_sigma, bg_gradient_reject_sigma,
             signal_weight_reject_sigma, fwhm_reject_sigma, star_count_reject_sigma,
             eccentricity_reject_abs, created_at, updated_at)
         VALUES (?1, ?2, 2.5, 2.5, 2.5, 2.5, 2.5, 1.5, 0.85, ?3, ?3)",
        rusqlite::params!["Default", "Default rejection thresholds", now],
    )?;

    // Algorithm set version 1
    conn.execute(
        "INSERT OR IGNORE INTO algorithm_sets
            (version, bg_algorithm_version, snr_algorithm_version,
             fwhm_algorithm_version, eccentricity_algorithm_version,
             star_count_algorithm_version, released_at, notes)
         VALUES (1, '1.0', '1.0', '1.0', '1.0', '1.0', ?1, ?2)",
        rusqlite::params![now, "Initial algorithm set — Phase 8 baselines"],
    )?;

    Ok(())
}

pub fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
