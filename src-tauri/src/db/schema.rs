// src-tauri/src/db/schema.rs
// All CREATE TABLE statements. Each is idempotent (IF NOT EXISTS).

pub const CREATE_PREFERENCES: &str = "
CREATE TABLE IF NOT EXISTS preferences (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);";

pub const CREATE_QUICK_LAUNCH_BUTTONS: &str = "
CREATE TABLE IF NOT EXISTS quick_launch_buttons (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    position    INTEGER NOT NULL,
    label       TEXT NOT NULL,
    script      TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);";

pub const CREATE_RECENT_DIRECTORIES: &str = "
CREATE TABLE IF NOT EXISTS recent_directories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,
    last_used   INTEGER NOT NULL,
    use_count   INTEGER NOT NULL DEFAULT 1
);";

pub const CREATE_THRESHOLD_PROFILES: &str = "
CREATE TABLE IF NOT EXISTS threshold_profiles (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    name                        TEXT NOT NULL UNIQUE,
    description                 TEXT,
    bg_median_reject_sigma      REAL NOT NULL DEFAULT 2.5,
    bg_stddev_reject_sigma      REAL NOT NULL DEFAULT 2.5,
    bg_gradient_reject_sigma    REAL NOT NULL DEFAULT 2.5,
    signal_weight_reject_sigma  REAL NOT NULL DEFAULT 2.5,
    fwhm_reject_sigma           REAL NOT NULL DEFAULT 2.5,
    star_count_reject_sigma     REAL NOT NULL DEFAULT 1.5,
    eccentricity_reject_abs     REAL NOT NULL DEFAULT 0.85,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);";

pub const CREATE_ALGORITHM_SETS: &str = "
CREATE TABLE IF NOT EXISTS algorithm_sets (
    version                         INTEGER PRIMARY KEY,
    bg_algorithm_version            TEXT NOT NULL,
    snr_algorithm_version           TEXT NOT NULL,
    fwhm_algorithm_version          TEXT NOT NULL,
    eccentricity_algorithm_version  TEXT NOT NULL,
    star_count_algorithm_version    TEXT NOT NULL,
    released_at                     INTEGER NOT NULL,
    notes                           TEXT
);";

pub const CREATE_FRAME_ANALYSIS_RESULTS: &str = "
CREATE TABLE IF NOT EXISTS frame_analysis_results (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path               TEXT NOT NULL,
    algorithm_set_version   INTEGER NOT NULL REFERENCES algorithm_sets(version),
    threshold_profile_id    INTEGER REFERENCES threshold_profiles(id),
    equipment_profile_name  TEXT,
    analyzed_at             INTEGER NOT NULL,
    bg_median               REAL,
    bg_stddev               REAL,
    bg_gradient             REAL,
    snr_estimate            REAL,
    fwhm_median_px          REAL,
    fwhm_median_arcsec      REAL,
    eccentricity            REAL,
    star_count              INTEGER,
    session_bg_median_mean  REAL,
    session_bg_median_sd    REAL,
    session_fwhm_mean       REAL,
    session_fwhm_sd         REAL,
    session_ecc_mean        REAL,
    session_ecc_sd          REAL,
    session_snr_mean        REAL,
    session_snr_sd          REAL,
    session_stars_mean      REAL,
    session_stars_sd        REAL,
    pxflag                  TEXT NOT NULL DEFAULT 'PASS',
    triggered_by            TEXT,
    user_override           INTEGER NOT NULL DEFAULT 0,
    UNIQUE(file_path, algorithm_set_version)
);";

pub const CREATE_FRAME_ANALYSIS_RESULTS_IDX_PATH: &str = "
CREATE INDEX IF NOT EXISTS idx_far_path ON frame_analysis_results(file_path);";

pub const CREATE_FRAME_ANALYSIS_RESULTS_IDX_VERSION: &str = "
CREATE INDEX IF NOT EXISTS idx_far_version ON frame_analysis_results(algorithm_set_version);";

pub const CREATE_MACROS: &str = "
CREATE TABLE IF NOT EXISTS macros (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL UNIQUE,
    display_name    TEXT,
    script          TEXT NOT NULL,
    tags            TEXT,
    run_count       INTEGER NOT NULL DEFAULT 0,
    last_run_at     INTEGER,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);";

pub const CREATE_MACRO_VERSIONS: &str = "
CREATE TABLE IF NOT EXISTS macro_versions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    macro_id    INTEGER NOT NULL REFERENCES macros(id) ON DELETE CASCADE,
    script      TEXT NOT NULL,
    saved_at    INTEGER NOT NULL
);";

pub const CREATE_MACRO_VERSIONS_IDX: &str = "
CREATE INDEX IF NOT EXISTS idx_mv_macro ON macro_versions(macro_id, saved_at DESC);";

pub const CREATE_SESSION_HISTORY: &str = "
CREATE TABLE IF NOT EXISTS session_history (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    directory       TEXT NOT NULL,
    opened_at       INTEGER NOT NULL,
    closed_at       INTEGER,
    file_count      INTEGER,
    commands_run    INTEGER DEFAULT 0
);";

pub const CREATE_CONSOLE_HISTORY: &str = "
CREATE TABLE IF NOT EXISTS console_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    executed_at INTEGER NOT NULL,
    command     TEXT NOT NULL,
    output      TEXT,
    success     INTEGER NOT NULL DEFAULT 1
);";

pub const CREATE_CRASH_RECOVERY: &str = "
CREATE TABLE IF NOT EXISTS crash_recovery (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    file_list           TEXT,
    current_frame_index INTEGER,
    autostretch_enabled INTEGER,
    zoom_level          TEXT,
    active_panel        TEXT,
    written_at          INTEGER NOT NULL
);";

pub const CREATE_CRASH_RECOVERY_SEED: &str = "
INSERT OR IGNORE INTO crash_recovery (id, written_at) VALUES (1, 0);";


// ----------------------------------------------------------------------
