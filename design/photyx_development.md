# Photyx — Developer Notes

**Version:** 26 **Last updated:** 2 May 2026 **Status:** Active development — Phase 9 sub-phase E in progress

---

## 1. Project Structure

```
Photyx/
├── src-svelte/           ← Svelte frontend (target stack)
│   ├── lib/
│   │   ├── commands.ts   ← Shared backend command helpers (selectDirectory, loadFiles, displayFrame, etc.)
│   │   ├── pcodeCommands.ts   ← Single source of truth for all pcode command names (imported by Console and MacroEditor)
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
│   │   ├── settings/     ← Frontend settings constants (mirrors defaults.rs)
│   │   │   └── constants.ts
│   │   └── stores/       ← Svelte writable stores
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
│       ├── lib.rs        ← Tauri entry point, command handlers
│       ├── logging.rs    ← Rolling file logger (tracing + tracing-appender)
│       ├── utils.rs      ← Shared utilities: resolve_path, get_log_dir, get_macros_dir
│       ├── settings/     ← AppSettings global object and defaults
│       │   ├── mod.rs    ← AppSettings struct; ThresholdProfile struct; load_from_db(); load_threshold_profiles(); save_preference()
│       │   └── defaults.rs ← Single source of truth for all hard-coded values and bounds
│       ├── plugin/       ← Plugin host infrastructure
│       │   ├── mod.rs    ← PhotonPlugin trait, ArgMap, PluginOutput, PluginError, ParamSpec
│       │   └── registry.rs ← Plugin registry: register, lookup, dispatch, list_with_details
│       ├── context/
│       │   └── mod.rs    ← AppContext, ImageBuffer, PixelData, KeywordEntry, BlinkCacheStatus
│       ├── analysis/     ← Pure computation modules (no Tauri dependencies)
│       │   ├── mod.rs
│       │   ├── background.rs
│       │   ├── eccentricity.rs
│       │   ├── fwhm.rs
│       │   ├── metrics.rs
│       │   ├── profiles.rs
│       │   ├── session_stats.rs
│       │   └── stars.rs
│       ├── pcode/        ← pcode interpreter
│       │   ├── mod.rs
│       │   └── tokenizer.rs
│       └── plugins/      ← Built-in native plugin implementations
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
├── crates/               ← Workspace crates
│   └── photyx-xisf/      ← XISF reader/writer crate (MIT OR Apache-2.0)
├── static/               ← Static assets served by Vite
│   ├── css/              ← Module CSS files (theme-neutral)
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
│   └── themes/           ← Theme CSS files (dark, light, matrix)
├── Cargo.toml            ← Workspace root
├── .cargo/
│   └── config.toml       ← Sets PKG_CONFIG env vars for cfitsio
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

# Frontend only (no Tauri IPC — for UI layout work)
npm run dev
# then open http://localhost:1420

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

AutoStretch operates on a dynamic display-resolution downsampled copy of the image. ~50x reduction in pixel count vs operating on the full buffer.

### 3.4 Full-Resolution Cache

`get_full_frame` encodes the full-resolution raw buffer as JPEG at quality 90, applying the same STF stretch parameters that AutoStretch computed. RGB images are handled correctly — each channel is stretched using the stored STF params.

### 3.5 Canvas-Based Image Display

The image viewer uses an HTML5 canvas element (`#viewer-image-canvas`) rather than an `<img>` tag. This eliminates layout shifts caused by image src swaps during blink playback.

### 3.6 Zoom Implementation

Zoom levels are implemented via `drawImage()` math in `Viewer.svelte`. The canvas is always viewport-sized; zoom is achieved by scaling the drawn image dimensions.

### 3.7 Pan Implementation

Panning is implemented as `panX`/`panY` offsets applied to the centered draw position in `getDrawRect()`. Pan only active at zoom levels above Fit. Momentum with friction on mouse release.

### 3.8 Pixel Tracking

Mouse pixel tracking is always-on when the viewer has an image. Coordinates flow `Viewer.svelte` → `+page.svelte` → `InfoPanel.svelte` as props rather than through the `ui` store (avoids reactive storm).

### 3.9 Blink State Management

Multiple blink-related fields live in the `ui` store. `blinkModeActive` is distinct from `blinkPlaying` — remains true while blink is paused so the viewer maintains the blink scale.

### 3.10 Blink UI Jitter — Known Issue

Toolbar/Quick Launch chrome jitters during blink. DevTools CLS = 0.01, culprit undetected after canvas switch; suspected Tauri WebView compositor artifact on Windows; deferred.

### 3.28 pcode Interpreter

- `If <expr> / Else / EndIf` — conditional blocks with `==`, `!=`, `<`, `>`, `<=`, `>=` operators
- `For varname = N to M / EndFor` — numeric loop
- `GetKeyword` result auto-stored into `$KEYWORDNAME`

### 3.29 Console Expansion

The pcode console expands to a full-width overlay (60vh, 85% opacity) when the header is clicked.

### 3.30 Macro Editor Architecture

The Macro Editor is rendered at the `#content-area` level in `+page.svelte` rather than inside `#panel-container`. Entry points: clicking Edit or New in the Macro Library panel.

### 3.31 Inline Confirmation Bar Pattern

Native OS dialogs are not reliably available in Tauri WebView. The established pattern for destructive action confirmation is an inline bar within the component. Always add `e.stopPropagation()` to prevent click-through to parent handlers.

### 3.32 WriteCurrent Atomic Writes

All write plugins use a write-to-temp-then-rename pattern. File written to `<path>.tmp` first, then atomically renamed over the original.

### 3.33 pcodeCommands.ts

Single source of truth for all valid pcode command names. Both `Console.svelte` (tab completion) and `MacroEditor.svelte` (syntax highlighting) import from this file.

### 3.34 WriteFITS U16 Sign Conversion

FITS `BITPIX=16` is signed. When writing u16 pixel data, subtract 32768 before casting to i16, then write `BZERO=32768`. Fixed from `v as i16` to `(v as i32 - 32768) as i16`.

### 3.35 Analysis Layer Architecture

Analysis code lives in `src-tauri/src/analysis/` as pure computation modules with no Tauri or plugin dependencies.

### 3.36 AnalyzeFrames Classification

**PASS / REJECT only** — SUSPECT classification was removed. A frame is REJECT if any single metric exceeds its threshold. `triggered_by` records which metrics caused the REJECT.

### 3.37 Analysis Graph

Viewer-region component replacing the image viewer when `$ui.activeView === 'analysisGraph'`. Key features:

- 7 metrics in Metric 1 dropdown; Metric 2 defaults to None
- Reject lines drawn via `drawRejectLine()` helper — primary (red, left-aligned), secondary (warning color, right-aligned, thinner)
- Both reject lines outlined in black on both sides for visibility against any background
- Reject threshold values come from `applied_thresholds` in `get_analysis_results` response (thresholds actually used in last AnalyzeFrames run) — NOT from current active profile, preventing stale display after profile changes
- When both metrics are the same, Y-scales are forced equal
- Dot click navigates to frame only if click is within dot radius + 6px vertically

### 3.38 Star Annotation Overlay

`ComputeFWHM` triggers a star annotation overlay. `drawStarAnnotations()` (Rust fetch) must NEVER be called from `renderBitmap()` — only `paintStarAnnotations()` (cache-only, synchronous).

### 3.39 consolePipe Store

**Always use `pipeToConsole()` — never call `consolePipe.set()` directly.** `consolePipe` is a queue (`ConsoleLine[]`). Direct `.set()` calls corrupt the queue and cause spread errors on the next `pipeToConsole()` call (the spread over a non-array throws a TypeError). This applies everywhere outside `Console.svelte`: `QuickLaunch.svelte`, `MacroLibrary.svelte`, `MenuBar.svelte`.

### 3.40 View Registry (showView)

Viewer-region component visibility managed through a central registry in `ui.ts`. All viewer-region visibility controlled exclusively via `ui.showView()`.

### 3.41 Analysis Results

Sortable, scrollable table with Refresh button. `loadData()` is extracted as a named function called from both `onMount` and the Refresh button.

### 3.42 notifications.running() — Pulse Animation + Expanded Bar

The `running` notification type triggers:

1. A CSS pulse animation on the status bar text and icon
2. The status bar expands to 3× its normal height (66px from 22px) with 33px font, overlaying content above it via `position: absolute`
3. Background becomes `rgba(0,0,0,0.85)` for readability
4. Transitions back smoothly when the next notification replaces it

`#app` must have `position: relative` for the absolute-positioned expanded bar to anchor correctly.

### 3.43 Log Viewer

Modal overlay triggered from Tools > Log Viewer. Auto-tail: polls `read_log_file` every 2 seconds.

### 3.44 Macro Library

Macros stored in SQLite (`photyx.db`) rather than as files on disk.

### 3.47 Keyword Editor

Inline editing model. Keyword name is read-only — renaming requires delete + add. Write Changes button calls `WriteFrame` (not `WriteCurrent`).

### 3.48 WriteFrame Plugin

Writes the currently active frame back to its source file. Distinct from `WriteCurrent` which writes all loaded frames.

### 3.49 utils.rs — Shared Path Utilities

`resolve_path`, `get_log_dir`, `get_macros_dir`.

### 3.51 pcode Implementation Details

Variables are resolved inside the evaluator via a `HashMap` parameter — **never pre-substituted**. `Print` and `Assert` are handled as special cases in `execute_line`.

### 3.52 ContourHeatmap Algorithm

Spatial FWHM heatmap. Star detection → adaptive grid → IDW interpolation → bicubic upscale → colormap. Output stored in `ctx.variables["NEW_FILE"]`.

### 3.53 Blink Review Workflow

1. Fast blink pass — PXFLAG red border overlay
2. Deliberate review — P/R keys write PXFLAG immediately
3. Delete confirmed rejects

### 3.54 client_actions — Cross-Boundary Side Effects

When a plugin needs the frontend to perform an action after execution, it declares this by emitting a `client_action` string in its `PluginOutput::Data` JSON.

| Action                | Emitted by   | Frontend effect                 |
| --------------------- | ------------ | ------------------------------- |
| `refresh_autostretch` | AutoStretch  | Calls `applyAutoStretch()`      |
| `refresh_annotations` | ComputeFWHM  | Calls `ui.refreshAnnotations()` |
| `open_keyword_modal`  | ListKeywords | Calls `ui.openKeywordModal()`   |

### 3.55 AppSettings Architecture

All application settings held in a single `AppSettings` struct stored in `PhotoxState` behind a `Mutex`. Two-source population: `defaults.rs` (hard-coded) + `preferences` table (persisted).

**Startup sequence:**

1. `AppSettings::new()` — initializes from `defaults.rs`
2. `load_from_db()` — overwrites persisted fields from `preferences` table
3. `load_threshold_profiles()` — loads threshold profiles; seeds "Default" profile if table empty
4. `AppContext` initialized with autostretch and analysis threshold values from `AppSettings`
5. `PhotoxState` constructed with `Mutex<AppSettings>`

### 3.56 Threshold Profile Architecture

`ThresholdProfile` struct in `settings/mod.rs` holds all 7 rejection thresholds. `AppSettings` holds `threshold_profiles: Vec<ThresholdProfile>` and `active_threshold_profile_id: Option<i64>`.

**Profile name:** Default profile is seeded with name "Default" (not "Standard").

**Sigma direction:** SNR and Star Count are `-σ` metrics (reject if below threshold). Their defaults and clamp bounds are negative:

- `DEFAULT_SNR_SIGMA = -2.5`, clamped to `[-SNR_SIGMA_MAX, -SNR_SIGMA_MIN]`
- `DEFAULT_STAR_COUNT_SIGMA = -1.5`, clamped to `[-STAR_COUNT_SIGMA_MAX, -STAR_COUNT_SIGMA_MIN]`

**Flow:** Dialog → `save_threshold_profile` → DB + in-memory vec → `set_active_threshold_profile` → DB + `ctx.analysis_thresholds` immediately propagated → `AnalyzeFrames` reads `ctx.analysis_thresholds.clone()`.

**Last run thresholds:** `ctx.last_analysis_thresholds: Option<AnalysisThresholds>` is set at the end of each `AnalyzeFrames` run. `get_analysis_results` returns these as `applied_thresholds` so the Analysis Graph always shows the thresholds actually used, regardless of current active profile.

**Delete behavior:** Any profile including the last one can be deleted. If all profiles are deleted, a "Default" profile is re-seeded and made active.

### 3.57 Dropdown Component (Dropdown.svelte)

Custom CSS-friendly select component that escapes the stacking context by appending its menu to `document.body`.

**Critical implementation rules:**

- Document click listener uses **bubble phase** (`false`), not capture phase (`true`). Capture phase caused the listener to fire before menu item `onclick` handlers, breaking selection.
- Uses `createEventDispatcher` to emit `'change'` events. Parents must use `value={x} on:change={(e) => { x = e.detail; }}` — NOT `bind:value`. The Svelte 4/5 boundary means `bind:value` does not reliably propagate changes back to Svelte 5 parent components.
- `IconSidebar.svelte` outside-click handler includes `.dropdown-menu` in its exclusion list so clicking a menu item appended to `document.body` does not close the sliding panel.
- `ui.setBlinkResolution()` must be called explicitly when `blinkResolution` changes in `InfoPanel.svelte` so `Viewer.svelte` uses the correct scale factor.

### 3.58 Analysis Graph — Applied Thresholds

The reject threshold lines on the Analysis Graph reflect the thresholds used when `AnalyzeFrames` ran, not the current active profile. This prevents confusion when the user changes profiles after running analysis. If no analysis has been run (`applied_thresholds` is null), no reject lines are drawn.

### 3.59 AnalyzeFrames — Metric Correlation Analysis

Cross-session Pearson correlation analysis (NGC6910 80 frames + M104 62 frames) shows:

- **Bg Std Dev** is near-perfectly correlated with Bg Median (r = 0.971–0.990) in all sessions examined — effectively redundant
- **Eccentricity** is orthogonal to all other metrics in all sessions — essential
- **FWHM** and **Star Count** are session-dependent (r = −0.70 to −0.96 depending on airmass)
- **SNR** dominance varies by session type

Planned: remove Bg Std Dev from the analysis engine pending additional dataset confirmation. All other metrics retained pending further analysis.

**Planned improvement — Iterative sigma clipping:** Extreme outliers (e.g. clouds, planes) inflate session std dev and compress effective sigma range for other frames, causing marginal frames to escape rejection. Fix: two-pass computation — Pass 1 computes initial stats, rejects extreme outliers (e.g. > 4σ FWHM), Pass 2 recomputes stats without outliers, then classifies all frames. Outlier-excluded frames visually marked in Analysis Graph. Not yet implemented.

**Planned improvement — AnalyzeFrames caching:** If `ctx.analysis_results` already has results for all files in `ctx.file_list`, skip Pass 1 (metric computation) entirely and run only Pass 2 (session stats → classify → write PXFLAG). This makes re-runs after threshold changes nearly instant. Not yet implemented.

---

## 4. Tauri Commands (Implemented)

| Command                           | Description                                                                           |
| --------------------------------- | ------------------------------------------------------------------------------------- |
| `dispatch_command`                | Dispatches a single pcode command (legacy interactive path)                           |
| `run_script`                      | Executes a pcode script string; returns ScriptResponse                                |
| `debug_buffer_info`               | Returns buffer metadata                                                               |
| `get_analysis_results`            | Returns per-frame metrics, flags, triggered_by, session stats, and applied_thresholds |
| `get_active_threshold_profile_id` | Returns the currently active threshold profile id                                     |
| `get_autostretch_frame`           | Computes Auto-STF stretch on current frame, returns JPEG data URL                     |
| `get_blink_cache_status`          | Returns blink cache build status                                                      |
| `get_blink_frame`                 | Returns a blink frame as JPEG data URL                                                |
| `get_current_frame`               | Returns current image as raw JPEG data URL                                            |
| `get_full_frame`                  | Returns current image at full resolution with STF applied                             |
| `get_histogram`                   | Computes histogram bins + stats for current frame                                     |
| `get_keywords`                    | Returns all keywords for current frame                                                |
| `get_pixel`                       | Returns raw pixel value(s) at source coordinates                                      |
| `get_session`                     | Returns current session state                                                         |
| `get_star_positions`              | Re-runs star detection, returns positions for annotation overlay                      |
| `get_threshold_profiles`          | Returns all threshold profiles from AppSettings                                       |
| `get_variable`                    | Returns a pcode variable value                                                        |
| `list_log_files`                  | Lists available log files                                                             |
| `list_macros`                     | Lists macros from DB                                                                  |
| `list_plugins`                    | Returns list of registered plugins                                                    |
| `load_file`                       | Reads a single image file, injects into session                                       |
| `read_log_file`                   | Reads and parses a log file                                                           |
| `save_threshold_profile`          | Insert or update a threshold profile                                                  |
| `delete_threshold_profile`        | Delete a threshold profile; re-seeds Default if last one deleted                      |
| `set_active_threshold_profile`    | Sets active profile id; propagates thresholds into AppContext immediately             |
| `start_background_cache`          | Spawns background task to build blink cache JPEGs                                     |
| `check_crash_recovery`            | Returns crash recovery candidate if present                                           |
| `close_session`                   | Marks current session as closed                                                       |
| `open_session`                    | Records session open in session_history                                               |
| `write_crash_recovery`            | Writes crash recovery state                                                           |
| `backup_database`                 | Creates timestamped ZIP backup of photyx.db                                           |
| `restore_database`                | Restores photyx.db from ZIP backup                                                    |
| `delete_macro`                    | Deletes a macro from the DB                                                           |
| `get_macros`                      | Returns all macros from DB                                                            |
| `get_macro_versions`              | Returns version history for a macro                                                   |
| `increment_macro_run_count`       | Increments run_count for a macro                                                      |
| `rename_macro`                    | Renames a macro                                                                       |
| `restore_macro_version`           | Restores a macro to a previous version                                                |
| `save_macro`                      | Saves (insert or update) a macro                                                      |
| `get_all_preferences`             | Returns all preferences as key/value map                                              |
| `set_preference`                  | Writes a single preference to DB and AppSettings                                      |
| `get_quick_launch_buttons`        | Returns Quick Launch button assignments                                               |
| `save_quick_launch_buttons`       | Saves Quick Launch button assignments                                                 |
| `get_recent_directories`          | Returns recent directory history                                                      |
| `record_directory_visit`          | Records a directory visit                                                             |

---

## 5. Plugins Implemented

See §3.35 and `photyx_reference.md` §9 for plugin status table.

---

## 6. UI State Store (`ui.ts`) — Key Fields

| Field                    | Purpose                                                             |
| ------------------------ | ------------------------------------------------------------------- |
| `aboutOpen`              | Whether the About modal is open                                     |
| `activePanel`            | Currently open sidebar panel                                        |
| `activeView`             | Currently active viewer-region view (null = image viewer)           |
| `analysisParametersOpen` | Whether the Analysis Parameters (threshold profiles) dialog is open |
| `annotationToken`        | Positive = show annotations, negative = clear annotations           |
| `autostretchImageUrl`    | Data URL of AutoStretch result                                      |
| `blinkCached`            | Whether blink cache has been built                                  |
| `blinkCaching`           | Whether blink cache build is in progress                            |
| `blinkImageUrl`          | Current blink frame data URL                                        |
| `blinkModeActive`        | Whether viewer is in blink display mode                             |
| `blinkPlaying`           | Whether blink is actively playing                                   |
| `blinkResolution`        | Currently selected blink resolution ('12' or '25')                  |
| `blinkTabActive`         | Whether the Blink tab is currently selected                         |
| `consoleExpanded`        | Whether console is expanded                                         |
| `currentBlinkFlag`       | PXFLAG value for the currently displayed blink frame                |
| `displayImageUrl`        | Data URL of temporary display image                                 |
| `frameRefreshToken`      | Incremented to trigger viewer frame reload                          |
| `keywordModalOpen`       | Whether the keyword modal is open                                   |
| `logViewerOpen`          | Whether the Log Viewer modal is open                                |
| `macroEditorFile`        | File currently open in Macro Editor                                 |
| `preferencesOpen`        | Whether the Preferences dialog is open                              |
| `quickLaunchVisible`     | Whether the Quick Launch bar is visible                             |
| `showQualityFlags`       | Whether PXFLAG reject borders are shown during blink                |
| `theme`                  | Active theme (dark / light / matrix)                                |
| `viewerClearToken`       | Incremented to clear viewer and restore starfield                   |
| `zoomLevel`              | Current zoom level                                                  |

---

## 7. Known Issues & Deferred Items

| Issue                                         | Notes                                                                                     |
| --------------------------------------------- | ----------------------------------------------------------------------------------------- |
| cfitsio parallel loading crashes              | Thread-safety issue — sequential loading used for now                                     |
| Blink UI jitter                               | Suspected Tauri WebView compositor artifact on Windows; deferred                          |
| Full-res frames are JPEG not lossless         | Disclosed via disclaimer bar; pixel readout always uses raw buffer                        |
| Long-running commands block UI                | pcode invoke awaits Rust response, freezing JS; fix requires Tauri event system; deferred |
| Zoom is approximate at high levels            | Full-res cache uses AutoStretch STF params computed on display-res downsample             |
| XISF Vector/Matrix properties                 | Read as placeholder string, skipped on write; deferred                                    |
| Rayon thread count not user-configurable      | Hardcoded to num_cpus-1; setting exists but not yet wired                                 |
| stderr log output in dev mode                 | Duplicated to terminal via fmt::layer(); remove when no longer needed                     |
| Sidebar icon tooltips clipped by Quick Launch | CSS stacking context issue; deferred                                                      |
| Plugin boilerplate is verbose                 | Deferred to Phase 10 or later                                                             |
| Single file load blink isolation              | Files loaded via LoadFile included in ctx.file_list                                       |
| AutoStretch performance in dev mode           | 3–5 seconds for RGB 9MP in debug build; near-instant in release                           |
| AutoStretch lost on Pixels tab switch         | Viewer reverts to raw display; deferred                                                   |
| SNR label vs PixInsight convention            | Our SNR is inverse of PI's Noise Ratio; label should be revisited before v1.0 release     |
| AnalyzeFrames progress reporting              | No per-frame progress; requires Tauri event system; deferred with all async dispatch work |
| Iterative sigma clipping in session stats     | Extreme outliers inflate std dev and distort rejection; planned, not yet implemented      |
| AnalyzeFrames metric caching                  | Re-runs recompute all metrics; fast-path for threshold-only changes planned               |
| Bg Std Dev metric removal                     | Highly correlated with Bg Median (r = 0.97–0.99); pending additional dataset confirmation |

---

## 8. Phase Completion Status

| Phase    | Status                   | Notes                                                                                                                                                                                                                                                                                                                                                                                                                        |
| -------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 1  | ✅ Complete               | Scaffold, plugin host, FITS reader, notification bar, logging                                                                                                                                                                                                                                                                                                                                                                |
| Phase 2  | ✅ Complete               | Display cache, AutoStretch, blink engine, histogram, keywords, UI file browser, pixel tracking, WCS, zoom, pan, full-res cache, canvas viewer                                                                                                                                                                                                                                                                                |
| Phase 3  | ✅ Complete               | photyx-xisf crate, ReadAllXISFFiles, WriteAllXISFFiles, ReadAllTIFFFiles, ReadAllFiles, RGB display/histogram, background display cache                                                                                                                                                                                                                                                                                      |
| Phase 4  | ✅ Complete               | Keyword plugins, WriteAllFITFiles, WriteAllTIFFFiles, WriteCurrentFiles, AstroTIFF keyword round-trip, FITS signed/unsigned 16-bit, blink cache quality, relative path resolution, window resize fix, pwd command                                                                                                                                                                                                            |
| Phase 5  | ✅ Complete               | pcode interpreter with If/Else/EndIf and For/EndFor; Macro Editor UI with syntax highlighting; Quick Launch panel with store persistence and context menu; command rename refactor; scope parameter on keyword commands; WriteCurrent atomic writes; ScriptResponse flags; pcodeCommands.ts single source of truth                                                                                                           |
| Phase 6  | ✅ Complete               | UI cleanup complete                                                                                                                                                                                                                                                                                                                                                                                                          |
| Phase 7  | ✅ Complete               | AnalyzeFrames with 7 native metrics; PASS/REJECT classification; PXFLAG keyword; Analysis Graph viewer-region component; star annotation overlay; consolePipe store; blink red border overlay; viewer filename overlay; theme-aware chart colors                                                                                                                                                                             |
| Phase 8  | ✅ Substantially complete | Moment-based FWHM; 8×8 background gradient grid; 5-pixel minimum star filter; WriteFITS U16 sign conversion fix; histogram canvas width fix; UI audit pass; ContourHeatmap plugin; display pipeline refactor; image_reader.rs; load_file Tauri command; LoadFile pcode command; DispatchResponse.data field; histogram hover readout                                                                                         |
| Phase 9  | 🔄 In progress           | Embedded SQLite (✅), Quick Launch persistence (✅), session history (✅), crash recovery (✅), macros migrated to SQLite (✅), AppSettings global object (✅), Preferences dialog (✅), threshold profiles (✅ complete — ThresholdProfilesDialog, 5 Tauri commands, DB persistence, wired to AnalyzeFrames and Analysis Graph); remaining: analysis results persistence, console history persistence, status bar profile indicator |
| Phase 10 | ⬜ Not started            | User plugin loading, plugin manifest system, macro library, plugin directory, Plugin Manager UI                                                                                                                                                                                                                                                                                                                              |
| Deferred | ⏸ Parked                 | Full keyword management UI, PNG/JPEG readers and writers, debayering, async dispatch, REST API (Axum), CLI access, WASM analysis plugins                                                                                                                                                                                                                                                                                     |

## 9. Settings Persistence (Phase 9)

All settings persistence is driven by the SQLite database (`photyx.db`) via `rusqlite`. The authoritative reference for the schema, persistence tiers, and implementation plan is `photyx_persistence_inventory.md`. The authoritative reference for which settings exist, their defaults, and their persistence/user-pref classification is `photyx_reference.md` §5.

The `AppSettings` struct is the global in-memory settings object. Loaded at startup from `defaults.rs` and the `preferences` table, then `load_threshold_profiles()` loads threshold profiles (seeding "Default" if the table is empty). All reads come from the struct; writes go to both the struct and the DB simultaneously via `save_preference()`. See §3.55 for the full architecture.

Settings that remain in localStorage: none — migration complete as of Phase 9 sub-phase B.

---

## 10. Database Schema

See `photyx_persistence_inventory.md` for the full DDL and schema documentation. All tables live in `APPDATA/Photyx/photyx.db`.
