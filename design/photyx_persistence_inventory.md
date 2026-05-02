# Photyx — Persistence Inventory

**Version:** 4 **Last updated:** 2 May 2026 **Status:** Phase 9 sub-phases A–E in progress; threshold profiles complete.

This document is the authoritative reference for what data Photyx
persists, where it lives, and the database schema. The full DDL for
all tables is in `photyx_development.md` §10. The settings reference
(defaults, bounds, DB keys) is in `photyx_reference.md` §5.

---

## 1. Storage Strategy

All persistence uses a single embedded SQLite database at `APPDATA/Photyx/photyx.db`. SQLite is statically linked via `rusqlite` — no external dependencies, no service, just a file.

**What is NOT in SQLite:**

| Data                 | Location                               | Reason                               |
| -------------------- | -------------------------------------- | ------------------------------------ |
| Application logs     | Rolling files via tracing-appender     | Log infrastructure already correct   |
| Image pixel data     | In-memory `AppContext.image_buffers`   | Too large; ephemeral by design       |
| Display/blink caches | In-memory                              | Ephemeral; rebuilt on load           |
| STF parameters       | In-memory `AppContext.last_stf_params` | Session-scoped; recalculated on load |
| PXFLAG keyword       | Written to image file headers          | Results must travel with the file    |

---

## 2. Table Summary

| Table                    | Purpose                                 | Key                                  | Status        |
| ------------------------ | --------------------------------------- | ------------------------------------ | ------------- |
| `preferences`            | All user preferences (key/value)        | `key`                                | ✅ Complete    |
| `quick_launch_buttons`   | Quick Launch panel assignments          | `id`, ordered by `position`          | ✅ Complete    |
| `recent_directories`     | Directory history                       | `path` UNIQUE                        | ✅ Complete    |
| `macros`                 | Macro scripts and metadata              | `name` UNIQUE                        | ✅ Complete    |
| `macro_versions`         | Macro version history                   | `(macro_id, saved_at)`               | ✅ Complete    |
| `session_history`        | Directory session log / crash detection | `id`                                 | ✅ Complete    |
| `crash_recovery`         | Session recovery state (single row)     | `id = 1`                             | ✅ Complete    |
| `threshold_profiles`     | AnalyzeFrames rejection threshold sets  | `id`                                 | ✅ Complete    |
| `algorithm_sets`         | Algorithm version registry              | `version`                            | ⬜ Sub-phase E |
| `frame_analysis_results` | Per-frame quality metrics               | `(file_path, algorithm_set_version)` | ⬜ Sub-phase E |
| `console_history`        | Command history log                     | `id`                                 | ⬜ Sub-phase E |

---

## 3. Preferences Key Reference

The `preferences` table uses a key/value schema. All defaults, bounds,
and DB keys are documented in `photyx_reference.md` §5. The `AppSettings` Rust struct (`src-tauri/src/settings/mod.rs`) is the
in-memory mirror — populated at startup, all reads from memory, writes
go to both struct and DB via `save_preference()`. Hard-coded values
and bounds are defined as constants in `src-tauri/src/settings/defaults.rs`.

**Validation rule:** Bounds are enforced in `AppSettings` on read —
the DB stores raw values. This allows bounds to change without a
schema migration.

**Settings never persisted** (always use hard-coded default):

- AutoStretch enabled (always off)
- Overwrite behavior (always Prompt)
- Format filter selection (always All Supported)
- Rayon thread count (always num_cpus - 1)
- Blink pre-cache frames (always all loaded frames)
- Default zoom level, blink rate, channel view

---

## 4. Implementation Notes

**Rust side:**

- `rusqlite` with `bundled` feature — statically linked, no external deps
- `PRAGMA journal_mode=WAL` on open — allows concurrent reads while Rust writes
- `PRAGMA foreign_keys=ON` on open
- Schema migrations via `PRAGMA user_version` — check version, apply pending scripts, bump version
- `db::now_unix()` in `db/mod.rs` is the single source of truth for Unix timestamps — always use it
- The `backup` rusqlite feature must remain enabled — required by `backup_database`
- `restore_database` checkpoints WAL before writing, deletes WAL/SHM after writing, reopens connection in-place — no app restart required

**Frontend side:**

- All database access via Tauri commands — the frontend never holds a connection
- `db.ts` wraps all Tauri command calls; components never call `invoke` for DB operations directly

**Commands module:**

- All Tauri commands are in `src-tauri/src/commands/` submodules
- The invoke handler in `lib.rs` uses fully qualified paths (`commands::preferences::set_preference`) — follow this pattern for all new commands

---

## 5. Threshold Profiles — Completed

Threshold profiles are fully implemented as of 2 May 2026. The following items from the original sub-phase E plan are complete:

**Commands (`src-tauri/src/commands/threshold_profiles.rs`):**

- `get_threshold_profiles` — returns all profiles from `AppSettings.threshold_profiles`
- `get_active_threshold_profile_id` — returns active profile id from `AppSettings`
- `save_threshold_profile` — insert or update profile in DB and in-memory vec; clamps all values to bounds
- `delete_threshold_profile` — delete by id; re-seeds "Default" profile if last one deleted; updates active id if needed
- `set_active_threshold_profile` — writes to preferences table and propagates thresholds into `AppContext.analysis_thresholds` immediately

**Settings layer:**

- `ThresholdProfile` struct in `settings/mod.rs` with `default_profile()` constructor
- `AppSettings.threshold_profiles: Vec<ThresholdProfile>` — loaded at startup
- `AppSettings.load_threshold_profiles()` — seeds "Default" profile if table is empty
- Called at startup in `lib.rs` after `load_from_db()`

**Frontend:**

- `src-svelte/lib/stores/thresholdProfiles.ts` — store with `hydrate()`, `saveProfile()`, `deleteProfile()`, `setActiveProfile()`
- `ThresholdProfilesDialog.svelte` — modal dialog accessible from Edit > Analysis Parameters
- `src-svelte/lib/settings/constants.ts` — `THRESHOLD_FIELDS` metadata array with per-metric direction, type, min/max/step
- Hydrated at startup in `+page.svelte`

**AnalyzeFrames integration:**

- `AppContext.analysis_thresholds` — active thresholds, loaded at startup and updated immediately on `set_active_threshold_profile`
- `AppContext.last_analysis_thresholds` — thresholds actually used in last run; returned by `get_analysis_results` as `applied_thresholds`
- Analysis Graph uses `applied_thresholds` for reject lines, not current active profile

**Important threshold notes:**

- Default profile name is "Default" (not "Standard")
- SNR and Star Count are `-σ` metrics — their defaults and clamp bounds are negative
- `DEFAULT_SNR_SIGMA = -2.5`, `DEFAULT_STAR_COUNT_SIGMA = -1.5`
- Clamp bounds: `snr_reject_sigma.clamp(-SNR_SIGMA_MAX, -SNR_SIGMA_MIN)` etc.
- HTML `min`/`max` attributes are omitted from threshold inputs to prevent browser clamping of negative values

---

## 6. Sub-phase E — Remaining Work

Sub-phases A through D are complete. Threshold profiles are complete. Remaining sub-phase E work:

1. Implement `save_analysis_results` — upserts rows into `frame_analysis_results`; respects `user_override` flag
2. Implement `get_persisted_analysis_results(directory)` — returns results for files under a given path; used by Analysis Graph and Results table on load
3. Update `AnalyzeFrames` to call `save_analysis_results` after completion
4. Update Analysis Graph and Analysis Results to call `get_persisted_analysis_results` on open, falling back to in-memory results
5. Implement `save_console_history` and `get_console_history` Tauri commands; wire into `Console.svelte`
6. Update status bar to show active threshold profile name

**Completion criterion:** Analysis results survive an app restart.
Re-opening a directory shows previous results without re-running
AnalyzeFrames. Active threshold profile shown in status bar.

---

## 7. Known Issues

- AutoStretch stretch is lost when switching from the Blink tab back to the Pixels tab — the viewer reverts to raw unstretched display. Behavior may be pre-existing; deferred.
