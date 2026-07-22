// src-tauri/src/db/schema.rs
// All CREATE TABLE statements. Each is idempotent (IF NOT EXISTS).
//
// This is the single, current-state schema (Issue 163). Prior to this,
// the live schema was reached via seven incremental migrations
// (migrate_v1 through migrate_v7); since no installs predating that
// history exist in the wild, the incremental chain was squashed into
// this single definition, applied as schema version 1. See
// db/migrations.rs and TR §8.2 for details.

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

// Feature flags — dynamically toggleable UI-facing switches (Issue 130).
// Unlike threshold_profiles, there is no server-side seed: the frontend's
// FEATURE_FLAGS registry (settings/constants.ts) is the single source of
// truth for which keys exist and their defaults. A key absent from this
// table simply means "not yet toggled from its registry default" — the
// backend stays deliberately ignorant of what flags exist, same as
// preferences.
pub const CREATE_FEATURE_FLAGS: &str = "
CREATE TABLE IF NOT EXISTS feature_flags (
    key         TEXT PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL
);";

pub const CREATE_THRESHOLD_PROFILES: &str = "
CREATE TABLE IF NOT EXISTS threshold_profiles (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    name                        TEXT NOT NULL UNIQUE,
    description                 TEXT,
    bg_median_reject_sigma      REAL NOT NULL DEFAULT 2.5,
    fwhm_reject_sigma           REAL NOT NULL DEFAULT 2.5,
    star_count_reject_sigma     REAL NOT NULL DEFAULT 1.5,
    eccentricity_reject_abs     REAL NOT NULL DEFAULT 0.85,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);";

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

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
