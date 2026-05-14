# Photyx — Developer Notes

**Version:** 28 **Last updated:** 4 May 2026 **Status:** Active development — Phase 9 in progress

---

## 1. Project Structure

```
Photyx/
├── src-svelte/           ← Svelte frontend (target stack)
│   ├── lib/
│   │   ├── commands.ts   ← Shared backend command helpers (selectDirectory, loadFiles, displayFrame, closeSession, etc.)
│   │   ├── pcodeCommands.ts   ← Single source of truth for all pcode command names
│   │   ├── components/   ← Svelte UI components
│   │   │   ├── panels/   ← Sliding panel components
│   │   │   │   ├── FileBrowser.svelte
│   │   │   │   ├── KeywordEditor.svelte
│   │   │   │   ├── MacroEditor.svelte
│   │   │   │   ├── MacroLibrary.svelte
│   │   │   │   └── PluginManager.svelte
│   │   │   ├── AnalysisGraph.svelte
│   │   │   ├── AnalysisResults.svelte
│   │   │   ├── AboutModal.svelte
│   │   │   ├── Console.svelte
│   │   │   ├── Dropdown.svelte
│   │   │   ├── IconSidebar.svelte
│   │   │   ├── InfoPanel.svelte
│   │   │   ├── KeywordModal.svelte
│   │   │   ├── LogViewer.svelte
│   │   │   ├── MenuBar.svelte
│   │   │   ├── PreferencesDialog.svelte
│   │   │   ├── QuickLaunch.svelte
│   │   │   ├── StatusBar.svelte
│   │   │   ├── ThresholdProfilesDialog.svelte
│   │   │   ├── Toolbar.svelte
│   │   │   └── Viewer.svelte
│   │   ├── settings/
│   │   │   └── constants.ts  ← Frontend mirror of defaults.rs; THRESHOLD_FIELDS uses actual signed min/max
│   │   └── stores/
│   │       ├── consoleHistory.ts
│   │       ├── notifications.ts
│   │       ├── quickLaunch.ts
│   │       ├── session.ts
│   │       ├── settings.ts
│   │       ├── thresholdProfiles.ts
│   │       └── ui.ts
│   └── routes/
│       └── +page.svelte  ← Main application shell
├── src-tauri/            ← Rust backend
│   └── src/
│       ├── lib.rs
│       ├── logging.rs
│       ├── utils.rs
│       ├── settings/
│       │   ├── mod.rs
│       │   └── defaults.rs
│       ├── plugin/
│       │   ├── mod.rs
│       │   └── registry.rs
│       ├── context/
│       │   └── mod.rs
│       ├── analysis/
│       │   ├── mod.rs
│       │   ├── background.rs
│       │   ├── eccentricity.rs
│       │   ├── fwhm.rs
│       │   ├── metrics.rs
│       │   ├── profiles.rs
│       │   ├── session_stats.rs
│       │   └── stars.rs
│       ├── pcode/
│       │   ├── mod.rs
│       │   └── tokenizer.rs
│       └── plugins/
│           ├── mod.rs
│           ├── analyze_frames.rs
│           ├── auto_stretch.rs
│           ├── background_median.rs
│           ├── cache_frames.rs
│           ├── clear_session.rs
│           ├── compute_eccentricity.rs
│           ├── compute_fwhm.rs
│           ├── contour_heatmap.rs
│           ├── get_histogram.rs
│           ├── image_reader.rs
│           ├── highlight_clipping.rs
│           ├── keywords.rs
│           ├── list_keywords.rs
│           ├── read_all_files.rs
│           ├── read_fits.rs
│           ├── read_tiff.rs
│           ├── read_xisf.rs
│           ├── run_macro.rs
│           ├── scripting.rs
│           ├── select_directory.rs
│           ├── set_frame.rs
│           ├── star_count.rs
│           ├── write_current_files.rs
│           ├── write_fits.rs
│           ├── write_frame.rs
│           ├── write_tiff.rs
│           └── write_xisf.rs
├── crates/
│   └── photyx-xisf/      ← XISF reader/writer crate (MIT OR Apache-2.0)
├── static/
│   ├── css/
│   │   ├── analysisgraph.css
│   │   ├── analysisresults.css
│   │   ├── console.css
│   │   ├── dropdown.css
│   │   ├── infopanel.css
│   │   ├── layout.css
│   │   ├── logviewer.css
│   │   ├── macroeditor.css
│   │   ├── modal.css
│   │   ├── preferences.css
│   │   ├── sidebar.css
│   │   ├── statusbar.css
│   │   ├── thresholdprofiles.css
│   │   ├── toolbar.css
│   │   └── viewer.css
│   └── themes/
├── Cargo.toml
├── .cargo/config.toml    ← PKG_CONFIG env vars for cfitsio
├── svelte.config.js
├── vite.config.js
├── package.json
└── Cargo.lock
```

---

## 2. Development Environment

### Prerequisites

| Tool      | Version | Notes                           |
| --------- | ------- | ------------------------------- |
| Rust      | stable  | Install via rustup.rs           |
| Node.js   | 18+     | Required for Svelte/Vite        |
| Tauri CLI | 2.10.1  | `cargo install tauri-cli`       |
| vcpkg     | latest  | Required for cfitsio on Windows |

### Running the App

```powershell
# Development (hot reload for Svelte, Rust recompiles on change)
npm run tauri dev

# Frontend only (no Tauri IPC)
npm run dev

# Run photyx-xisf tests
cargo test -p photyx-xisf -- --nocapture
```

---

## 3. Architecture Decisions & Implementation Notes

### 3.1 Prototype vs Target Stack

The vanilla JS prototype that previously lived in `src/` has been deleted. All active development is in `src-svelte/` (Svelte) and `src-tauri/` (Rust).

### 3.2 Display Cache Architecture

```
AppContext
├── image_buffers: HashMap<path, ImageBuffer>   ← raw pixels, NEVER modified
├── display_cache: HashMap<path, Vec<u8>>       ← display-res JPEG bytes
├── full_res_cache: HashMap<path, Vec<u8>>      ← full-resolution JPEG bytes
├── blink_cache_12: HashMap<path, Vec<u8>>      ← blink-res 12.5% JPEG bytes
└── blink_cache_25: HashMap<path, Vec<u8>>      ← blink-res 25% JPEG bytes
```

Design rule: **display plugins read from `image_buffers` and write to caches. They never modify `image_buffers`.**

### 3.3 AutoStretch Performance

AutoStretch operates on a dynamic display-resolution downsampled copy. ~50x reduction in pixel count vs operating on the full buffer.

### 3.4 Full-Resolution Cache

`get_full_frame` encodes the full-resolution raw buffer as JPEG at quality 90, applying the same STF stretch parameters that AutoStretch computed.

### 3.5 Canvas-Based Image Display

The image viewer uses an HTML5 canvas element rather than an `<img>` tag. Eliminates layout shifts during blink playback.

### 3.6 Zoom Implementation

Zoom levels implemented via `drawImage()` math in `Viewer.svelte`. Canvas is always viewport-sized; zoom achieved by scaling drawn image dimensions.

### 3.7 Pan Implementation

`panX`/`panY` offsets applied to centered draw position in `getDrawRect()`. Pan only active above Fit zoom. Momentum with friction on mouse release.

### 3.8 Pixel Tracking

Always-on when viewer has an image. Coordinates flow `Viewer.svelte` → `+page.svelte` → `InfoPanel.svelte` as props (avoids reactive storm).

### 3.9 Blink State Management

`blinkModeActive` is distinct from `blinkPlaying` — remains true while paused so viewer maintains blink scale.

### 3.10 Blink UI Jitter — Known Issue

Suspected Tauri WebView compositor artifact on Windows; deferred.

### 3.28 pcode Interpreter

- `If <expr> / Else / EndIf` — conditionals with `==`, `!=`, `<`, `>`, `<=`, `>=`
- `For varname = N to M / EndFor` — numeric loop
- `GetKeyword` result auto-stored into `$KEYWORDNAME`

### 3.29 Console Expansion

Expands to full-width overlay (60vh, 85% opacity) when header is clicked.

### 3.30 Macro Editor Architecture

Rendered at `#content-area` level in `+page.svelte` rather than inside `#panel-container`.

### 3.31 Inline Confirmation Bar Pattern

Native OS dialogs not reliably available in Tauri WebView. Use inline bars within the component. Always `e.stopPropagation()`.

### 3.32 WriteCurrent Atomic Writes

Write-to-temp-then-rename pattern. File written to `<path>.tmp` first, then atomically renamed over the original.

### 3.33 pcodeCommands.ts

Single source of truth for all valid pcode command names. Both `Console.svelte` and `MacroEditor.svelte` import from this file.

### 3.34 WriteFITS U16 Sign Conversion

FITS `BITPIX=16` is signed. Subtract 32768 before casting to i16, write `BZERO=32768`.

### 3.35 Analysis Layer Architecture

Analysis code lives in `src-tauri/src/analysis/` as pure computation modules — no Tauri or plugin dependencies.

### 3.36 AnalyzeFrames Classification

**PASS / REJECT only.** A frame is REJECT if any single metric (excluding SNR) exceeds its threshold. `triggered_by` records which metrics caused the REJECT.

**SNR excluded from rejection classification.** Retained in `AnalysisResult` and displayed as diagnostic only. Cross-session analysis confirmed a PSF artifact where worse-seeing frames produce higher SNR due to bloated star flux. SNR never drove a unique rejection not already caught by FWHM or Star Count.

### 3.37 Analysis Graph

Viewer-region component when `$ui.activeView === 'analysisGraph'`. Key implementation details:

- SNR excluded from `applied_thresholds` — no reject line drawn for it
- Dot appearance: all dots have 2px black border; PASS=white; O=red (#dc3232); T=yellow (#d4a820); B=blue (#3478dc)
- Multi-category dots: split semicircle (left half = first color, right half = second color), slightly larger radius, black dividing line
- `drawDot()` uses `categoryColors()` to extract ordered colors from category string
- Legend: `drawLegend()` called at end of `drawChart()`; fixed top-left corner (PL+8, PT+8); always visible; 4 entries
- Commit Results disabled for imported sessions (`is_imported` from `get_analysis_results`)

### 3.38 Star Annotation Overlay

`drawStarAnnotations()` (Rust fetch) must NEVER be called from `renderBitmap()` — only `paintStarAnnotations()` (cache-only, synchronous).

### 3.39 consolePipe Store

**Always use `pipeToConsole()` — never call `consolePipe.set()` directly.** Direct `.set()` corrupts the queue and causes spread TypeErrors.

### 3.40 View Registry (showView)

All viewer-region visibility controlled exclusively via `ui.showView()`. Never individual boolean flags.

### 3.41 Analysis Results

Two-row toolbar. Row 1: buttons. Row 2: session path display + optional IMPORTED badge.

**PXFLAG toggle:** Right-click context menu shows "Set to PASS" or "Set to REJECT". Local state only until Commit. Toggled rows render with `.ar-row-toggled` (amber left border). All metric data preserved regardless of direction — category badge stays visible on REJECT→PASS toggles.

**Imported sessions:** `is_imported: true` in response → IMPORTED badge shown, Commit disabled.

**Commit sequence:** sync toggled flags → `commit_analysis_results` → on success: `ui.showView(null)`, `ui.clearViewer()`, `closeSession()`. Terminal operation.

### 3.42 notifications.running() — Pulse Animation + Expanded Bar

`running` triggers: CSS pulse, 3× height expansion (66px), dark overlay, smooth transition back. `#app` must have `position: relative`.

### 3.43 Log Viewer

Modal overlay from Tools > Log Viewer. Auto-tail polls `read_log_file` every 2 seconds.

### 3.44 Macro Library

Macros stored in SQLite (`photyx.db`).

### 3.47 Keyword Editor

Keyword name is read-only — renaming requires delete + add. Write Changes calls `WriteFrame`.

### 3.48 WriteFrame Plugin

Writes currently active frame only. Distinct from `WriteCurrent` which writes all loaded frames.

### 3.49 utils.rs — Shared Path Utilities

`resolve_path`, `get_log_dir`, `get_macros_dir`.

### 3.51 pcode Implementation Details

Variables resolved inside the evaluator via `HashMap` — never pre-substituted. `Print` and `Assert` handled as special cases in `execute_line`.

### 3.52 ContourHeatmap Algorithm

Star detection → adaptive grid → IDW interpolation → bicubic upscale → colormap. Output in `ctx.variables["NEW_FILE"]`.

### 3.53 Blink Review Workflow

1. Fast blink pass — PXFLAG red border overlay (after Commit)
2. Deliberate review — P/R keys write PXFLAG immediately
3. Delete confirmed rejects

### 3.54 client_actions — Cross-Boundary Side Effects

| Action                | Emitted by   | Frontend effect                 |
| --------------------- | ------------ | ------------------------------- |
| `refresh_autostretch` | AutoStretch  | Calls `applyAutoStretch()`      |
| `refresh_annotations` | ComputeFWHM  | Calls `ui.refreshAnnotations()` |
| `open_keyword_modal`  | ListKeywords | Calls `ui.openKeywordModal()`   |

### 3.55 AppSettings Architecture

Single `AppSettings` struct in `PhotoxState` behind `Mutex`. Two-source: `defaults.rs` + `preferences` table.

**Startup:** `new()` → `load_from_db()` → `load_threshold_profiles()` → populate `AppContext` → construct `PhotoxState`.

### 3.56 Threshold Profile Architecture

`ThresholdProfile` in `settings/mod.rs`. 5 rejection thresholds.

**Sigma direction:** SNR and Star Count are `−σ`. Stored as negative values in DB and frontend:

- `DEFAULT_SNR_SIGMA = -2.5`, clamped to `[-SNR_SIGMA_MAX, -SNR_SIGMA_MIN]`
- `DEFAULT_STAR_COUNT_SIGMA = -3.0`, clamped to `[-STAR_COUNT_SIGMA_MAX, -STAR_COUNT_SIGMA_MIN]`

**Frontend `THRESHOLD_FIELDS`:** `min`/`max` for negative-direction fields use actual signed values (e.g. `min: -4.0, max: -0.5`). No helper functions needed. `SNR_SIGMA_DEFAULT = -2.5`, `STAR_COUNT_SIGMA_DEFAULT = -3.0` in `constants.ts`.

**SNR in AppContext:** Stored as positive (`.abs()` applied on save) but excluded from `classify_frame()` and `applied_thresholds`.

**Flow:** Dialog → `save_threshold_profile` → DB + vec → `set_active_threshold_profile` → DB + `ctx.analysis_thresholds` → `get_analysis_results` reclassifies on next call.

**DB orphaned columns:** `bg_stddev_reject_sigma` and `bg_gradient_reject_sigma` remain in schema; Rust ignores them. Migration deferred.

**Delete:** Any profile including last can be deleted; Default re-seeded if all deleted.

### 3.57 Dropdown Component (Dropdown.svelte)

Appends menu to `document.body` to escape stacking contexts.

- Document click listener: **bubble phase** (`false`), not capture
- Use `value={x} on:change={(e) => { x = e.detail; }}` — NOT `bind:value`
- `IconSidebar.svelte` outside-click handler excludes `.dropdown-menu`
- `ui.setBlinkResolution()` must be called explicitly after resolution changes

### 3.58 Analysis Graph — Applied Thresholds

Always reflects current active profile thresholds. SNR excluded from `applied_thresholds`. After commit, `ctx.last_analysis_thresholds` updated.

### 3.59 AnalyzeFrames — Metric Correlation Analysis

Cross-session analysis (5 sessions, 489 frames total):

- Bg Std Dev: r = 0.92–0.999 with Bg Median — **removed**
- Bg Gradient: session-dependent sign reversal — **removed**
- Eccentricity: orthogonal to background metrics — **essential**
- FWHM + Star Count: strongest discriminators (r = −0.63 to −0.96)
- SNR: PSF artifact confirmed — **excluded from rejection classification**

Both `BackgroundStdDev` and `BackgroundGradient` pcode commands retained as deprecated stubs.

### 3.60 AnalyzeFrames — Two-Pass Iterative Sigma Clipping

`compute_session_stats_iterative()` in `session_stats.rs`:

- **Pass 2a** — initial stats across all frames
- **Pass 2b** — identify outliers: any metric > 4.0σ from initial mean (eccentricity excluded)
- **Pass 2c** — recompute stats excluding outliers; fall back to initial if all outliers

Returns `(SessionStats, HashSet<String>)`.

### 3.61 AnalyzeFrames — On-the-Fly Reclassification

`get_analysis_results` reclassifies on every call for live sessions. Skipped if `ctx.is_imported_session` is true.

1. Returns empty if `ctx.analysis_results` is empty
2. Skips if `is_imported_session`
3. Runs `compute_session_stats_iterative`; updates ctx
4. Reclassifies each frame; calls `categorize_rejection()` for REJECTs
5. Updates `flag`, `triggered_by`, `rejection_category` in place
6. Returns results + `applied_thresholds` (SNR excluded) + `session_path` + `is_imported`

### 3.62 Commit Results — Enhanced Pattern

`commit_analysis_results` — terminal operation:

1. Guard: error if `is_imported_session`
2. Write PXFLAG to all buffers in memory
3. Drop lock
4. Dispatch `WriteCurrent` — flush all to disk atomically
5. Create `<active_directory>/rejected/` if absent
6. Move each REJECT file: `<path>` → `<dir>/rejected/<name>.<ext>.rejected`
7. Re-key `file_list`, `image_buffers`, `analysis_results` to new paths
8. Return success message

**Order matters:** WriteCurrent must run before file moves — it looks up buffers by original path.

**Frontend:** sync toggled flags → `commit_analysis_results` → success: `ui.showView(null)`, `ui.clearViewer()`, `closeSession()`.

### 3.63 AppContext.clear_session()

Clears all session state. Resets `is_imported_session` to `false`. Preserves `active_directory`.

### 3.64 Rejection Categories

`categorize_rejection()` in `session_stats.rs`:

| Triggered                               | Category |
| --------------------------------------- | -------- |
| FWHM and/or Eccentricity only           | O        |
| StarCount only (no BackgroundMedian)    | T        |
| BackgroundMedian only                   | B        |
| FWHM/Ecc + StarCount                    | OT       |
| FWHM/Ecc + BackgroundMedian             | OB       |
| BackgroundMedian + StarCount            | BT       |
| FWHM/Ecc + BackgroundMedian + StarCount | OBT      |

O always leads (least recoverable). B leads T when both present (B is root cause of T). Unknown trigger → "O" fallback.

`rejection_category: Option<String>` in `AnalysisResult`. `None` for PASS; initialized to `None` in both `AnalyzeFrames` struct initializers.

### 3.65 Session JSON Export/Import

**Export (`exportSessionJson()` in `MenuBar.svelte`):**

- Calls `get_analysis_results` + `get_threshold_profiles`
- Default filename: `<target>_<YYYYMMDD>.json` from first frame basename (`Light_<target>_..._<YYYYMMDD>-...`)
- All filenames stored as basenames
- JSON: `photyx_version`, `exported_at`, `active_directory`, `threshold_profile_name`, `thresholds`, `session_stats`, `outlier_paths[]`, `frames[]`
- `writeTextFile` requires `fs:allow-write-text-file` capability

**Import (`importSessionJson()` in `MenuBar.svelte`):**

- `readTextFile` → validate → `load_analysis_json` Tauri command
- Rust: clears session, sets `active_directory`, reconstructs full paths (dir + "/" + basename), populates analysis state, sets `is_imported_session = true`
- Frontend: opens Analysis Results automatically
- No images loaded — display only

**Capability requirements:** `fs:allow-read-text-file` and `fs:allow-write-text-file` with `$HOME`, `$DESKTOP`, `$DOWNLOAD`, `$DOCUMENT`, `$APPDATA/Photyx/**`.

### 3.66 PXFLAG Toggle in Analysis Results

Right-click context menu. "Set to PASS" or "Set to REJECT" based on current flag. Local state only until Commit.

- `toggled: boolean` on `FrameResult` interface
- `.ar-row-toggled` class: amber left border, subtle background
- All data preserved regardless of direction; badge stays visible on REJECT→PASS toggles
- Before commit: `invoke('set_frame_flag', { path, flag })` for each toggled frame

`set_frame_flag`: updates `ctx.analysis_results[path].flag` directly. No reclassification side effects.

---

## 4. Tauri Commands (Implemented)

| Command                           | Description                                                                                                           |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `dispatch_command`                | Dispatches a single pcode command (legacy interactive path)                                                           |
| `run_script`                      | Executes a pcode script string; returns ScriptResponse                                                                |
| `debug_buffer_info`               | Returns buffer metadata                                                                                               |
| `commit_analysis_results`         | Writes PXFLAG to buffers, flushes via WriteCurrent, moves REJECT files to rejected/, re-keys ctx. Terminal operation. |
| `get_analysis_results`            | Reclassifies frames (skipped for imported); returns frames, stats, outliers, session_path, is_imported                |
| `get_active_threshold_profile_id` | Returns active threshold profile id                                                                                   |
| `get_autostretch_frame`           | Computes Auto-STF stretch, returns JPEG data URL                                                                      |
| `get_blink_cache_status`          | Returns blink cache build status                                                                                      |
| `get_blink_frame`                 | Returns blink frame as JPEG data URL                                                                                  |
| `get_current_frame`               | Returns current image as raw JPEG data URL                                                                            |
| `get_full_frame`                  | Returns current image at full resolution with STF applied                                                             |
| `get_histogram`                   | Computes histogram bins + stats                                                                                       |
| `get_keywords`                    | Returns all keywords for current frame                                                                                |
| `get_pixel`                       | Returns raw pixel value(s) at source coordinates                                                                      |
| `get_session`                     | Returns current session state                                                                                         |
| `get_star_positions`              | Re-runs star detection, returns positions for annotation overlay                                                      |
| `get_threshold_profiles`          | Returns all threshold profiles from AppSettings                                                                       |
| `get_variable`                    | Returns a pcode variable value                                                                                        |
| `list_log_files`                  | Lists available log files                                                                                             |
| `list_macros`                     | Lists macros from DB                                                                                                  |
| `list_plugins`                    | Returns list of registered plugins                                                                                    |
| `load_analysis_json`              | Clears session; populates analysis state from JSON payload; sets is_imported_session = true                           |
| `load_file`                       | Reads a single image file, injects into session                                                                       |
| `read_log_file`                   | Reads and parses a log file                                                                                           |
| `save_threshold_profile`          | Insert or update a threshold profile                                                                                  |
| `delete_threshold_profile`        | Delete a threshold profile; re-seeds Default if last deleted                                                          |
| `set_active_threshold_profile`    | Sets active profile; propagates thresholds into AppContext immediately                                                |
| `set_frame_flag`                  | Updates PASS/REJECT flag for a single frame in ctx.analysis_results by path                                           |
| `start_background_cache`          | Spawns background task to build blink cache JPEGs                                                                     |
| `check_crash_recovery`            | Returns crash recovery candidate if present                                                                           |
| `close_session`                   | Marks session closed in DB; resets is_imported_session                                                                |
| `open_session`                    | Records session open in session_history                                                                               |
| `write_crash_recovery`            | Writes crash recovery state                                                                                           |
| `backup_database`                 | Creates timestamped ZIP backup of photyx.db                                                                           |
| `restore_database`                | Restores photyx.db from ZIP backup                                                                                    |
| `delete_macro`                    | Deletes a macro from DB                                                                                               |
| `get_macros`                      | Returns all macros from DB                                                                                            |
| `get_macro_versions`              | Returns version history for a macro                                                                                   |
| `increment_macro_run_count`       | Increments run_count for a macro                                                                                      |
| `rename_macro`                    | Renames a macro                                                                                                       |
| `restore_macro_version`           | Restores a macro to a previous version                                                                                |
| `save_macro`                      | Saves (insert or update) a macro                                                                                      |
| `get_all_preferences`             | Returns all preferences as key/value map                                                                              |
| `set_preference`                  | Writes a single preference to DB and AppSettings                                                                      |
| `get_quick_launch_buttons`        | Returns Quick Launch button assignments                                                                               |
| `save_quick_launch_buttons`       | Saves Quick Launch button assignments                                                                                 |
| `get_recent_directories`          | Returns recent directory history                                                                                      |
| `record_directory_visit`          | Records a directory visit                                                                                             |

---

## 5. Plugins Implemented

See §3.35 and `photyx_reference.md` §9 for plugin status table.

---

## 6. UI State Store (`ui.ts`) — Key Fields

| Field                    | Purpose                                                   |
| ------------------------ | --------------------------------------------------------- |
| `aboutOpen`              | Whether the About modal is open                           |
| `activePanel`            | Currently open sidebar panel                              |
| `activeView`             | Currently active viewer-region view (null = image viewer) |
| `analysisParametersOpen` | Whether the Analysis Parameters dialog is open            |
| `annotationToken`        | Positive = show annotations, negative = clear annotations |
| `autostretchImageUrl`    | Data URL of AutoStretch result                            |
| `blinkCached`            | Whether blink cache has been built                        |
| `blinkCaching`           | Whether blink cache build is in progress                  |
| `blinkImageUrl`          | Current blink frame data URL                              |
| `blinkModeActive`        | Whether viewer is in blink display mode                   |
| `blinkPlaying`           | Whether blink is actively playing                         |
| `blinkResolution`        | Currently selected blink resolution ('12' or '25')        |
| `blinkTabActive`         | Whether the Blink tab is currently selected               |
| `consoleExpanded`        | Whether console is expanded                               |
| `currentBlinkFlag`       | PXFLAG value for the currently displayed blink frame      |
| `displayImageUrl`        | Data URL of temporary display image                       |
| `frameRefreshToken`      | Incremented to trigger viewer frame reload                |
| `keywordModalOpen`       | Whether the keyword modal is open                         |
| `logViewerOpen`          | Whether the Log Viewer modal is open                      |
| `macroEditorFile`        | File currently open in Macro Editor                       |
| `preferencesOpen`        | Whether the Preferences dialog is open                    |
| `quickLaunchVisible`     | Whether the Quick Launch bar is visible                   |
| `showQualityFlags`       | Whether PXFLAG reject borders are shown during blink      |
| `theme`                  | Active theme (dark / light / matrix)                      |
| `viewerClearToken`       | Incremented to clear viewer and restore starfield         |
| `zoomLevel`              | Current zoom level                                        |

---

## 7. Known Issues & Deferred Items

| Issue                                         | Notes                                                                                                                  |
| --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| cfitsio parallel loading crashes              | Thread-safety — sequential loading used                                                                                |
| Blink UI jitter                               | Suspected Tauri WebView compositor artifact on Windows; deferred                                                       |
| Full-res frames are JPEG not lossless         | Disclosed via disclaimer bar; pixel readout uses raw buffer                                                            |
| Long-running commands block UI                | Requires Tauri event system; deferred                                                                                  |
| Zoom approximate at high levels               | Full-res cache uses STF params from display-res downsample                                                             |
| XISF Vector/Matrix properties                 | Read as placeholder, skipped on write; deferred                                                                        |
| Rayon thread count not configurable           | Hardcoded to num_cpus-1                                                                                                |
| stderr log output in dev mode                 | Duplicated to terminal; remove when no longer needed                                                                   |
| Sidebar icon tooltips clipped by Quick Launch | CSS stacking context; deferred                                                                                         |
| Plugin boilerplate is verbose                 | Deferred to Phase 10                                                                                                   |
| Single file load blink isolation              | Files loaded via LoadFile included in ctx.file_list                                                                    |
| AutoStretch performance in dev mode           | 3–5 seconds for RGB 9MP in debug build; near-instant in release                                                        |
| AutoStretch lost on Pixels tab switch         | Viewer reverts to raw display; deferred                                                                                |
| SNR estimator PSF artifact                    | Worse-seeing frames produce higher SNR; confirmed across sessions; excluded from rejection; estimator revision planned |
| AnalyzeFrames progress reporting              | No per-frame progress; requires Tauri event system; deferred                                                           |
| threshold_profiles orphaned columns           | bg_stddev_reject_sigma and bg_gradient_reject_sigma remain in schema; migration deferred                               |
| Memory leak suspected                         | 103GB virtual / 20GB RSS observed after multiple sessions; audit deferred                                              |

---

## 8. Phase Completion Status

See Section 13 in the Specification.

## 9. Settings Persistence (Phase 9)

All persistence via SQLite (`photyx.db`). See `photyx_persistence_inventory.md` for schema and `photyx_reference.md` §5 for settings tables.

`AppSettings` is the global in-memory settings object. Loaded from `defaults.rs` + `preferences` table; `load_threshold_profiles()` seeds "Default" if table empty. All reads from struct; writes to struct + DB via `save_preference()`.

Settings that remain in localStorage: none — migration complete as of Phase 9 sub-phase B.

---

## 10. Database Schema

See `photyx_persistence_inventory.md` for full DDL. All tables live in `APPDATA/Photyx/photyx.db`.
