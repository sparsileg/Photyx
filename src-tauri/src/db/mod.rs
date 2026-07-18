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
/// See constants.rs for related protected array.
fn seed_defaults(conn: &Connection) -> Result<()> {
    let now = now_unix();

    // Protected built-in threshold profiles — seeded on every launch via OR IGNORE
    let profiles: &[(&str, &str, f64, f64, f64, f64)] = &[
        // name       description                   bg_med  fwhm  stars  ecc
        ("Default", "Default rejection thresholds", 2.5,   2.5,  1.5,  0.85),
    ];
    for (name, desc, bg_med, fwhm, stars, ecc) in profiles {
        conn.execute(
            "INSERT OR IGNORE INTO threshold_profiles
                (name, description,
                 bg_median_reject_sigma,
                 fwhm_reject_sigma, star_count_reject_sigma,
                 eccentricity_reject_abs, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            rusqlite::params![name, desc, bg_med, fwhm, stars, ecc, now],
        )?;
    }

    Ok(())
}

pub fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
