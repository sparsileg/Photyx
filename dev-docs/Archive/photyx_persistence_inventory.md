## Persistence Architecture

All persistence uses a single embedded SQLite database at `APPDATA/Photyx/photyx.db`. SQLite is statically linked via `rusqlite` — no external dependencies, no service, just a file. The full DDL for all tables follows in the next section.

### What is NOT in SQLite

| Data                 | Location                               | Reason                               |
| --------------------- | --------------------------------------- | -------------------------------------- |
| Application logs      | Rolling files via tracing-appender      | Log infrastructure already correct     |
| Image pixel data      | In-memory `AppContext.image_buffers`    | Too large; ephemeral by design         |
| Display/blink caches  | In-memory                               | Ephemeral; rebuilt on load             |
| STF parameters        | In-memory `AppContext.last_stf_params`  | Session-scoped; recalculated on load   |
| PXFLAG keyword        | Written to image file headers           | Results must travel with the file      |

### Table Index

One-line purpose for each table; full column definitions in the DDL below.

| Table                    | Purpose                                 | Key                                  |
| ------------------------- | ----------------------------------------- | --------------------------------------- |
| `preferences`             | All user preferences (key/value)          | `key`                                   |
| `quick_launch_buttons`    | Quick Launch panel assignments            | `id`, ordered by `position`             |
| `recent_directories`      | Directory history                         | `path` UNIQUE                           |
| `macros`                  | Macro scripts and metadata                | `name` UNIQUE                           |
| `macro_versions`          | Macro version history                     | `(macro_id, saved_at)`                  |
| `session_history`         | Directory session log / crash detection   | `id`                                    |
| `crash_recovery`          | Session recovery state (single row)       | `id = 1`                                |
| `threshold_profiles`      | AnalyzeFrames rejection threshold sets    | `id`                                    |
| `algorithm_sets`          | Algorithm version registry                | `version`                               |
| `frame_analysis_results`  | Per-frame quality metrics                 | `(file_path, algorithm_set_version)`    |
| `console_history`         | Command history log                       | `id`                                    |

### Preferences

The `preferences` table uses a key/value schema. Defaults, bounds, and DB keys are documented in `photyx_reference.md` §5. The `AppSettings` Rust struct (`src-tauri/src/settings/mod.rs`) is the in-memory mirror — populated at startup, all reads from memory, writes go to both struct and DB via `save_preference()`. Hard-coded values and bounds are defined as constants in `src-tauri/src/settings/defaults.rs`.

**Validation rule:** Bounds are enforced in `AppSettings` on read — the DB stores raw values. This allows bounds to change without a schema migration.

**Settings never persisted** (always use hard-coded default):

- AutoStretch enabled (always off)
- Overwrite behavior (always Prompt)
- Format filter selection (always All Supported)
- Rayon thread count (always num_cpus - 1)
- Blink pre-cache frames (always all loaded frames)
- Default zoom level, blink rate, channel view

### Threshold Profiles — Business Logic Notes

- Default profile name is "Default" (not "Standard").
- All thresholds are stored and displayed as positive values. Signal Weight and Star Count are `-σ` metrics — negation is applied at classification time in `check_low!()`, not at storage time.
- `DEFAULT_SIGNAL_WEIGHT_SIGMA = 2.5`, `DEFAULT_STAR_COUNT_SIGMA = 1.5`.
- Values are clamped to bounds on save, e.g. `signal_weight_reject_sigma.clamp(SIGNAL_WEIGHT_SIGMA_MIN, SIGNAL_WEIGHT_SIGMA_MAX)`.
- Star Count uses bimodal-aware anchoring — the 1.5σ threshold is relative to the clear-sky upper cluster, not the full mixed population.
- `THRESHOLD_FIELDS` in `constants.ts` uses positive `min`/`max` bounds for all fields; the `direction` field (`+` or `-`) controls the `>` or `<` indicator shown in the dialog.
- `set_active_threshold_profile` propagates thresholds into `AppContext.analysis_thresholds` immediately. `AppContext.last_analysis_thresholds` holds the thresholds actually used in the last analysis run, returned as `applied_thresholds` — the Analysis Graph uses this for reject lines, not the current active profile.

### Implementation Conventions

**Rust side:**

- `rusqlite` with `bundled` feature — statically linked, no external deps.
- `PRAGMA journal_mode=WAL` on open — allows concurrent reads while Rust writes.
- `PRAGMA foreign_keys=ON` on open.
- Schema migrations via `PRAGMA user_version` — check version, apply pending scripts, bump version.
- `db::now_unix()` in `db/mod.rs` is the single source of truth for Unix timestamps — always use it.
- The `backup` rusqlite feature must remain enabled — required by `backup_database`.
- `restore_database` checkpoints WAL before writing, deletes WAL/SHM after writing, reopens connection in-place — no app restart required.

**Frontend side:**

- All database access via Tauri commands — the frontend never holds a connection.
- `db.ts` wraps all Tauri command calls; components never call `invoke` for DB operations directly.

**Commands module:**

- All Tauri commands are in `src-tauri/src/commands/` submodules.
- The invoke handler in `lib.rs` uses fully qualified paths (`commands::preferences::set_preference`) — follow this pattern for all new commands.

---

## DDL

[existing §10 content follows here]
