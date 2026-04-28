# Photyx — Persistence Inventory for Phase 9

**Version:** 2
**Date:** 28 April 2026
**Purpose:** Comprehensive inventory of all data Photyx needs to persist, organized by storage strategy, priority, and proposed database schema. This document drives the Phase 9 SQLite implementation.

---

## 1. Storage Strategy Overview

Phase 9 uses a single embedded SQLite database file at `APPDATA/Photyx/photyx.db`. SQLite is statically linked via `rusqlite` — no external dependencies, no service, just a file.

**Three categories of data:**

| Category                        | Description                                                          | Storage                                               |
| ------------------------------- | -------------------------------------------------------------------- | ----------------------------------------------------- |
| **User preferences**            | Theme, UI state, format filters, performance tuning                  | SQLite `preferences` table (key/value)                |
| **Structured application data** | Analysis results, threshold profiles, session history, macro content | SQLite typed tables                                   |
| **Migration from localStorage** | Theme, Quick Launch assignments (currently in browser localStorage)  | One-time migration on first launch with Phase 9 build |

**Not in SQLite:**

- Log files (managed by `tracing-appender` — stay as rolling files)
- Image buffers and display caches (in-memory only — correct)
- PXFLAG and other keywords (written directly to image file headers — correct)

**Macros are stored in the database** (not as `.phs` files on disk). The Macros directory is eliminated. This keeps everything in one portable file regardless of OS, and enables version history without a separate file management system.

---

## 2. Priority Tiers

**Tier 1 — Must have for Phase 9 launch:** Migration of localStorage data, theme, last directory, Quick Launch assignments, basic user preferences, macros table.

**Tier 2 — High value, same phase:** Threshold profiles, algorithm versioning, analysis results persistence, session history, macro version history.

**Tier 3 — Speculative / future:** File tagging, cross-session statistics aggregation, equipment profile FK into Astryx data model when apps merge.

---

## 3. Proposed Schema

### 3.1 `preferences` — Key/Value Store

General-purpose preferences table. New preferences can be added without schema migrations.

```sql
CREATE TABLE preferences (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL  -- Unix timestamp
);
```

**Keys and their values:**

| Key                            | Type    | Default    | Notes                                                              |
| ------------------------------ | ------- | ---------- | ------------------------------------------------------------------ |
| `theme`                        | string  | `"matrix"` | `"matrix"` \| `"dark"` \| `"light"`                                |
| `last_directory`               | string  | `""`       | Full path of last active directory                                 |
| `jpeg_quality`                 | integer | `75`       | 1–100                                                              |
| `overwrite_behavior`           | string  | `"prompt"` | `"prompt"` \| `"always"` \| `"never"`                              |
| `format_filter`                | string  | `"all"`    | `"all"` \| `"fits"` \| `"xisf"` \| `"tiff"` \| `"png"` \| `"jpeg"` |
| `console_history_size`         | integer | `1000`     | Max console history rows to retain                                 |
| `macro_editor_font_size`       | integer | `13`       | px                                                                 |
| `autostretch_enabled`          | boolean | `true`     | Auto-STF toggle state                                              |
| `autostretch_shadow_clip`      | float   | `-2.8`     | PixInsight convention                                              |
| `autostretch_target_bg`        | float   | `0.15`     | Photyx default                                                     |
| `rayon_thread_count`           | integer | `0`        | 0 = num_cpus - 1 (auto)                                            |
| `blink_precache_all`           | boolean | `true`     | Pre-cache all frames vs. on-demand                                 |
| `quick_launch_columns`         | integer | `4`        | Grid column count                                                  |
| `quick_launch_visible`         | boolean | `true`     | Panel collapsed state                                              |
| `recent_directories_max`       | integer | `10`       | How many recent dirs to keep                                       |
| `api_port`                     | integer | `7171`     | REST API port (deferred)                                           |
| `api_key_required`             | boolean | `false`    | REST API auth (deferred)                                           |
| `api_localhost_only`           | boolean | `true`     | REST API binding (deferred)                                        |
| `crash_recovery_interval_secs` | integer | `60`       | How often to write recovery state                                  |
| `active_threshold_profile_id`  | integer | `null`     | FK → threshold_profiles.id                                         |
| `localStorage_migrated`        | boolean | `false`    | Set true after one-time migration                                  |

---

### 3.2 `quick_launch_buttons` — Quick Launch Panel

Replaces the current localStorage implementation.

```sql
CREATE TABLE quick_launch_buttons (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    position    INTEGER NOT NULL,       -- display order (0-based)
    label       TEXT NOT NULL,          -- button label text
    script      TEXT NOT NULL,          -- pcode invocation, e.g. "RunMacro name=ProcessLights"
    updated_at  INTEGER NOT NULL
);
```

**Important:** If the macro referenced by a Quick Launch button has `@param` declarations, clicking the button prompts for parameter values at run time — no parameter values are stored in this table. The button is always a shortcut to the macro, never to a specific parameterized invocation of it.

---

### 3.3 `recent_directories` — Directory History

```sql
CREATE TABLE recent_directories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,
    last_used   INTEGER NOT NULL,       -- Unix timestamp
    use_count   INTEGER NOT NULL DEFAULT 1
);
```

Trimmed to `recent_directories_max` entries by `last_used` desc. Updated (not inserted) on re-visit.

---

### 3.4 `threshold_profiles` — AnalyzeFrames Rejection Thresholds

Named sets of rejection thresholds, independent of equipment configuration.

```sql
CREATE TABLE threshold_profiles (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    name                        TEXT NOT NULL UNIQUE,
    description                 TEXT,
    bg_median_reject_sigma      REAL NOT NULL DEFAULT 2.5,
    bg_stddev_reject_sigma      REAL NOT NULL DEFAULT 2.5,
    bg_gradient_reject_sigma    REAL NOT NULL DEFAULT 2.5,
    snr_reject_sigma            REAL NOT NULL DEFAULT 2.5,   -- reject if below -N σ
    fwhm_reject_sigma           REAL NOT NULL DEFAULT 2.5,
    star_count_reject_sigma     REAL NOT NULL DEFAULT 1.5,   -- reject if below -N σ
    eccentricity_reject_abs     REAL NOT NULL DEFAULT 0.85,  -- absolute threshold
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);
```

A default "Standard" profile is inserted on first launch. The active profile id is stored in `preferences.active_threshold_profile_id`.

**Note:** Equipment profiles (telescope, sensor, focal length, site) are tracked in Astryx, which will eventually merge with Photyx. For now, analysis results carry a free-text `equipment_profile_name` field rather than a structured equipment table.

---

### 3.5 `algorithm_sets` — Algorithm Version Registry

Records exactly which algorithm versions were active for each algorithm set version. Every time any individual algorithm changes, a new algorithm set version is created.

```sql
CREATE TABLE algorithm_sets (
    version                         INTEGER PRIMARY KEY,  -- increments on any algorithm change
    bg_algorithm_version            TEXT NOT NULL,        -- covers median, stddev, gradient (one module)
    snr_algorithm_version           TEXT NOT NULL,
    fwhm_algorithm_version          TEXT NOT NULL,
    eccentricity_algorithm_version  TEXT NOT NULL,
    star_count_algorithm_version    TEXT NOT NULL,
    released_at                     INTEGER NOT NULL,
    notes                           TEXT                  -- human-readable description of what changed
);
```

The current algorithm set version is a compile-time constant in Rust, bumped manually when any algorithm changes. This table is pre-populated at build time and shipped with the application.

---

### 3.6 `frame_analysis_results` — Per-Frame Quality Metrics

Persists analysis results across sessions. Keyed on `(file_path, algorithm_set_version)` — one row per image file per algorithm set version. History is preserved when algorithms change.

```sql
CREATE TABLE frame_analysis_results (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path               TEXT NOT NULL,
    algorithm_set_version   INTEGER NOT NULL REFERENCES algorithm_sets(version),
    threshold_profile_id    INTEGER REFERENCES threshold_profiles(id),
    equipment_profile_name  TEXT,               -- free text until Astryx merge
    analyzed_at             INTEGER NOT NULL,
    -- computed metrics
    bg_median               REAL,
    bg_stddev               REAL,
    bg_gradient             REAL,
    snr_estimate            REAL,
    fwhm_median_px          REAL,               -- pixels
    fwhm_median_arcsec      REAL,               -- arcseconds (if plate scale available)
    eccentricity            REAL,
    star_count              INTEGER,
    -- session statistics at time of analysis (for sigma display when file viewed in isolation)
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
    -- classification
    pxflag                  TEXT NOT NULL DEFAULT 'PASS',   -- 'PASS' | 'REJECT'
    triggered_by            TEXT,               -- comma-separated metric names e.g. "FWHM,SNR"
    user_override           INTEGER NOT NULL DEFAULT 0,     -- 1 if user set flag via P/R key
    UNIQUE(file_path, algorithm_set_version)
);

CREATE INDEX idx_far_path ON frame_analysis_results(file_path);
CREATE INDEX idx_far_version ON frame_analysis_results(algorithm_set_version);
```

**Notes:**

- Re-running AnalyzeFrames with the same algorithm set version upserts (replaces) the existing row
- Re-running after an algorithm upgrade creates new rows alongside old ones
- `user_override = 1` protects the user's manual P/R decision from being overwritten by a re-run
- `triggered_by` stored as comma-separated text — clean and queryable without JSON parsing
- Raw pixel data is immutable, so results never become stale within the same algorithm version

**Example queries:**

```sql
-- Compare FWHM across algorithm versions for a directory
SELECT file_path, algorithm_set_version, fwhm_median_px, pxflag
FROM frame_analysis_results
WHERE file_path LIKE 'D:/M31/%'
ORDER BY file_path, algorithm_set_version;

-- All frames ever rejected for FWHM
SELECT file_path, fwhm_median_px, analyzed_at
FROM frame_analysis_results
WHERE pxflag = 'REJECT' AND triggered_by LIKE '%FWHM%'
ORDER BY analyzed_at DESC;
```

---

### 3.7 `macros` — Macro Scripts

Macros are stored in the database rather than as `.phs` files on disk. The Macros directory is eliminated. This gives cross-platform consistency and enables version history.

```sql
CREATE TABLE macros (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL UNIQUE,   -- used in RunMacro; no spaces; lowercase-friendly
    display_name    TEXT,                   -- friendly name shown in Macro Library UI
    script          TEXT NOT NULL,          -- full pcode script content including @param declarations
    tags            TEXT,                   -- comma-separated tags for future filtering
    run_count       INTEGER NOT NULL DEFAULT 0,
    last_run_at     INTEGER,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
```

**`@param` declarations** are stored verbatim in `script` as special comment lines at the top. They are parsed at run time — not at storage time. See spec §7 for the full `@param` syntax.

**Documentation** lives in `#` comment lines within `script`, co-located with the code. There is no separate description column — one documentation source only.

---

### 3.8 `macro_versions` — Macro Version History

Every time a macro is saved over an existing version, the previous content is preserved here. Cheap insurance against accidental overwrites.

```sql
CREATE TABLE macro_versions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    macro_id    INTEGER NOT NULL REFERENCES macros(id) ON DELETE CASCADE,
    script      TEXT NOT NULL,          -- script content at the time of this version
    saved_at    INTEGER NOT NULL        -- when this version was superseded
);

CREATE INDEX idx_mv_macro ON macro_versions(macro_id, saved_at DESC);
```

**Notes:**

- A new row is inserted here *before* the `macros.script` column is overwritten on Save
- The current version is always in `macros.script`; history is in this table
- `ON DELETE CASCADE` — if a macro is deleted, its version history is deleted with it
- No automatic pruning planned — macro scripts are small; history accumulation is negligible

---

### 3.9 `session_history` — Directory Sessions Log

Lightweight log of directory sessions; also used for crash recovery detection.

```sql
CREATE TABLE session_history (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    directory       TEXT NOT NULL,
    opened_at       INTEGER NOT NULL,
    closed_at       INTEGER,            -- NULL if session crashed or is still active
    file_count      INTEGER,
    commands_run    INTEGER DEFAULT 0
);
```

A row with `closed_at IS NULL` and `opened_at` within the crash recovery window = crash recovery candidate on next launch.

---

### 3.10 `console_history` — Command History Log

Proper relational table, not JSON. Trimmed to `console_history_size` preference value (default 1000).

```sql
CREATE TABLE console_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    executed_at INTEGER NOT NULL,
    command     TEXT NOT NULL,
    output      TEXT,
    success     INTEGER NOT NULL DEFAULT 1   -- 1 = success, 0 = error
);
```

Trim query (run after each insert):

```sql
DELETE FROM console_history
WHERE id NOT IN (
    SELECT id FROM console_history
    ORDER BY id DESC
    LIMIT (SELECT CAST(value AS INTEGER) FROM preferences WHERE key = 'console_history_size')
);
```

Primary use case: debugging and tracing. Expected to be consulted rarely.

---

### 3.11 `crash_recovery` — Session Recovery State

Single-row table (enforced by `CHECK (id = 1)`). Written every `crash_recovery_interval_secs` seconds while a session is active.

```sql
CREATE TABLE crash_recovery (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    active_directory    TEXT,
    file_list           TEXT,               -- JSON array of file paths
    current_frame_index INTEGER,
    autostretch_enabled INTEGER,
    zoom_level          TEXT,               -- 'fit' | '25' | '50' | '100' | '200'
    active_panel        TEXT,
    written_at          INTEGER NOT NULL
);
```

On launch: if `written_at` is recent and `session_history` has an open session, offer recovery dialog.

---

## 4. Data That Lives Elsewhere (Not in SQLite)

| Data                 | Location                               | Reason                               |
| -------------------- | -------------------------------------- | ------------------------------------ |
| Application logs     | Rolling files via tracing-appender     | Log infrastructure already correct   |
| Image pixel data     | In-memory `AppContext.image_buffers`   | Too large; ephemeral by design       |
| Display/blink caches | In-memory                              | Ephemeral; rebuilt on load           |
| STF parameters       | In-memory `AppContext.last_stf_params` | Session-scoped; recalculated on load |
| PXFLAG keyword       | Written to image file headers          | Results must travel with the file    |

---

## 5. Migration Plan (localStorage → SQLite)

On first launch after Phase 9 upgrade (runs in Svelte `onMount` before UI renders):

1. Check `preferences.localStorage_migrated` — if true, skip entirely
2. Read `localStorage.getItem('theme')` → upsert into `preferences` as `theme`
3. Read Quick Launch JSON from localStorage → insert rows into `quick_launch_buttons`
4. Delete the localStorage keys
5. Set `preferences.localStorage_migrated = true`

---

## 6. Implementation Notes

**Rust side:**

- `rusqlite` directly — statically linked, no external deps, no vcpkg
- Database connection opened in `lib.rs` `run()` alongside `AppContext`; stored as `Mutex<Connection>` in `PhotoxState`
- `PRAGMA journal_mode=WAL;` on open — allows concurrent reads while Rust writes
- `PRAGMA foreign_keys=ON;` on open
- Schema migrations via `PRAGMA user_version` — check version, apply pending scripts, bump version

**Frontend side:**

- All database access via Tauri commands — the frontend never holds a connection
- New commands needed: `get_preferences`, `set_preference`, `get_quick_launch_buttons`, `save_quick_launch_buttons`, `get_macros`, `save_macro`, `delete_macro`, `get_analysis_results`, `get_console_history`, etc.

**File location:** `APPDATA/Photyx/photyx.db` — same directory as logs folder.

---

## 7. Schema Summary

| Table                    | Purpose                                 | Key                                  |
| ------------------------ | --------------------------------------- | ------------------------------------ |
| `preferences`            | All user preferences                    | `key`                                |
| `quick_launch_buttons`   | Quick Launch panel assignments          | `id`, ordered by `position`          |
| `recent_directories`     | Directory history dropdown              | `path` UNIQUE                        |
| `threshold_profiles`     | AnalyzeFrames rejection threshold sets  | `id`                                 |
| `algorithm_sets`         | Algorithm version registry              | `version`                            |
| `frame_analysis_results` | Per-frame quality metrics               | `(file_path, algorithm_set_version)` |
| `macros`                 | Macro scripts and metadata              | `name` UNIQUE                        |
| `macro_versions`         | Macro version history                   | `(macro_id, saved_at)`               |
| `session_history`        | Directory session log / crash detection | `id`                                 |
| `console_history`        | Command history log                     | `id`                                 |
| `crash_recovery`         | Session recovery state                  | Single row (`id = 1`)                |
