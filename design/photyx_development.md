# Photyx — Developer Notes

**Version:** 29 **Last updated:** 13 May 2026 **Status:** Active development — Phase 9 in progress

---

## 1. Project Structure

```
Photyx/
├── src-svelte/           ← Svelte frontend (target stack)
│   ├── lib/
│   │   ├── commands.ts   ← Shared backend command helpers (addFiles, displayFrame, closeSession, etc.)
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
│           ├── add_files.rs
│           ├── analyze_frames.rs
│           ├── auto_stretch.rs
│           ├── background_median.rs
│           ├── cache_frames.rs
│           ├── clear_session.rs
│           ├── clear_stack.rs
│           ├── commit_analysis.rs
│           ├── compute_eccentricity.rs
│           ├── compute_fwhm.rs
│           ├── commit_stretch.rs
│           ├── contour_heatmap.rs
│           ├── export_analysis_report.rs
│           ├── get_histogram.rs
│           ├── image_reader.rs
│           ├── highlight_clipping.rs
│           ├── keywords.rs
│           ├── list_keywords.rs
│           ├── run_macro.rs
│           ├── scripting.rs
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

### 3.11 Session Model — Global File Context

Photyx uses a **global file context** — a flat list of file paths (`ctx.file_list`) with no concept of an "active directory."

- `AddFiles` appends explicit file paths to the session; duplicates are skipped
- Files from multiple directories coexist in a single session
- `ClearSession` resets the entire session
- `ctx.source_directories()` — returns unique parent directories of all loaded files
- `ctx.common_parent()` — returns the common parent if all files share one, else None
- `ctx.remove_rejected_files()` — removes rejected paths from file_list and all caches after commit
- Relative paths in pcode commands resolve against `common_parent()` when available

**Status bar display:** `N files · M directories` derived from `ctx.file_list`.

**`pwd` command:** Lists unique source directories from the current file list.

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

**Commit sequence:** sync toggled flags → `commit_analysis_results` → on success: sync session from backend → `ui.showView(null)` → `ui.clearViewer()`. Session stays open; pass frames remain loaded.

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

`resolve_path(path, active_directory)` — resolves relative paths against a base directory. Callers pass `ctx.common_parent().as_ref().and_then(|p| p.to_str())` as the base. `get_log_dir`.

### 3.51 pcode Implementation Details

Variables resolved inside the evaluator via `HashMap` — never pre-substituted. `Print` and `Assert` handled as special cases in `execute_line`.

### 3.52 ContourHeatmap Algorithm

Star detection → adaptive grid → IDW interpolation → bicubic upscale → colormap. Output written to the source file's parent directory. Path stored in `ctx.variables["NEW_FILE"]`.

### 3.53 Blink Review Workflow

1. Fast blink pass — PXFLAG red border overlay
2. Deliberate review — P/R keys update flag in `ctx.analysis_results`
3. Commit Results — moves rejects, pass frames remain loaded

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
6. Returns results + `applied_thresholds` (SNR excluded) + `is_imported`

### 3.62 Commit Results

`commit_analysis_results` — non-terminal operation; session stays open:

1. Guard: error if `analysis_results` is empty or `is_imported_session`
2. Collect REJECT paths from `ctx.file_list`
3. Move each REJECT file: `<path>` → `<parent>/rejected/<name>.<ext>.rejected`; each file lands in its own source directory's `rejected/` subfolder
4. Call `ctx.remove_rejected_files(&reject_paths)` — removes from `file_list`, `image_buffers`, all caches; clears analysis results
5. Return success message

**PXFLAG is NOT written to files.** The file move is the sole persistence action. This keeps commit fast (< 1 second for 100+ frames) and avoids rewriting raw image data.

**Frontend:** sync toggled flags → `commit_analysis_results` → success: sync session from `get_session` → `session.setFileList()` → `ui.showView(null)` → `ui.clearViewer()`. Pass frames remain loaded and ready for subsequent operations.

**Order of frontend updates matters:** session sync must happen before `ui.showView(null)` so reactive components update while still mounted.

### 3.63 AppContext.clear_session()

Clears all session state. Resets `is_imported_session` to `false`. Does not preserve any directory reference.

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
- All filenames stored as **full absolute paths** (multi-directory sessions require this)
- Outlier paths stored as full absolute paths
- JSON: `photyx_version`, `exported_at`, `threshold_profile_name`, `thresholds`, `session_stats`, `outlier_paths[]`, `frames[]`
- `writeTextFile` requires `fs:allow-write-text-file` capability

**Import (`importSessionJson()` in `MenuBar.svelte`):**

- `readTextFile` → validate → `load_analysis_json` Tauri command
- Rust: clears session, treats `filename` fields as full paths directly (no directory prefix construction), populates analysis state, sets `is_imported_session = true`
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

### 3.67 Image Reader Consolidation

All format reading is consolidated in `plugins/image_reader.rs`:

- `read_image_file(path)` — dispatches to format-specific reader by extension
- `read_fits_file(path)` — FITS reader
- `read_xisf_file(path)` — XISF reader
- `read_tiff_file(path)` — TIFF reader
- `peek_fits_dimensions(path)` — peek FITS header without reading pixels
- `peek_xisf_dimensions(path)` — peek XISF header without reading pixels
- `peek_tiff_dimensions(path)` — peek TIFF header without reading pixels

The peek functions are used by `AddFiles` for memory limit estimation.

### 3.68 AddFiles Plugin

`plugins/add_files.rs` — appends explicit file paths to the session:

1. Parse comma-separated paths from `paths=` argument
2. Validate all paths exist
3. Filter out paths already in `ctx.file_list` (duplicate detection)
4. Peek first file to estimate memory usage; check against `ctx.buffer_pool_bytes`
5. Load each new file via `read_image_file()`; insert into `ctx.image_buffers` and `ctx.file_list`

Does **not** call `ctx.clear_session()`. To start fresh, call `ClearSession` first.

### 3.69 DB Schema Migration v3

Migration v3 (applied automatically on startup if DB is at version 2):

```sql
ALTER TABLE crash_recovery DROP COLUMN active_directory;
```

Crash recovery now stores `file_list` (JSON array of full paths) only. On recovery, paths are passed directly to `AddFiles`.

### 3.70 Linux GTK File Picker — Known Issue

On Linux, the native GTK file picker with `multiple: true` silently
refuses to confirm a selection that includes both files and
directories (e.g. when Ctrl+A selects files alongside a `rejected/`
subfolder). Workaround: avoid using Ctrl+A when a `rejected/`
subfolder is present in the directory; select files manually instead.

### 3.71 StackFrames — Alignment Architecture

Two alignment modules in `src-tauri/src/analysis/`:

- **`fft_align.rs`** — FFT phase correlation for coarse
  translation. Downsamples to ≤1024px, apodizes with Hann window,
  returns sub-pixel `(dx, dy)`. Used for within-group per-frame
  alignment.
- **`star_align.rs`** — Two alignment strategies:
  - `estimate_rigid_transform()` — FFT-primed RANSAC. Pre-translates
    frame stars by FFT offset, greedy nearest-neighbour matching
    within `MATCH_TOLERANCE=15px`, RANSAC over 50 iterations with
    `INLIER_TOLERANCE=2px`. Used for within-group rotation refinement.
  - `estimate_rigid_transform_triangles()` — Triangle-based matching
    without FFT pre-translation. Builds scale-invariant descriptors
    from top 60 brightest stars (~34k triangles), matches by
    descriptor distance, votes on implied transforms in binned `(tx,
    ty, θ)` space, returns winning voted transform directly (no
    least-squares refinement — numerically unstable with centroids far
    from origin). Used exclusively for cross-group M_cross solve.

**Rotational grouping** — frames grouped by ROTATOR keyword within
`ROTATOR_GROUP_TOLERANCE=10°` AND session continuity
(`SESSION_GAP_MINUTES=120`). Frames with same ROTATOR but DATE-OBS gap
> 2 hours are separate groups. Each non-master group gets its own
M_cross solve. Master group = largest group by frame count.

**M_cross verification** — after each M_cross solve, group-ref stars
are transformed and matched against master-ref stars within
10px. Mean/max residual logged. Typical results: mean 0.26–0.58px, max
<10px.

**Per-frame transform** — `T = compose(M_cross, G)` where G is
within-group FFT + optional RANSAC rotation. Resampled via
`resample_frame_affine()` when `|θ| ≥ 0.001rad` or flip encoded;
otherwise `resample_frame()` (translation-only, faster).

**Known limitation** — `validate_alignment` disabled (all frames
accepted). Within-group RANSAC fires on ~50% of frames; sanity check
threshold 15px.

---

## 4. Tauri Commands (Implemented)

| Command                           | Description                                                                                                          |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `dispatch_command`                | Dispatches a single pcode command (legacy interactive path)                                                          |
| `run_script`                      | Executes a pcode script string; returns ScriptResponse                                                               |
| `debug_buffer_info`               | Returns buffer metadata                                                                                              |
| `commit_analysis_results`         | Moves REJECT files to per-source rejected/ subfolders; removes from session; pass frames remain. Fast, non-terminal. |
| `get_analysis_results`            | Reclassifies frames (skipped for imported); returns frames, stats, outliers, is_imported                             |
| `get_active_threshold_profile_id` | Returns active threshold profile id                                                                                  |
| `get_autostretch_frame`           | Computes Auto-STF stretch, returns JPEG data URL                                                                     |
| `get_blink_cache_status`          | Returns blink cache build status                                                                                     |
| `get_blink_frame`                 | Returns blink frame as JPEG data URL                                                                                 |
| `get_current_frame`               | Returns current image as raw JPEG data URL                                                                           |
| `get_full_frame`                  | Returns current image at full resolution with STF applied                                                            |
| `get_histogram`                   | Computes histogram bins + stats                                                                                      |
| `get_keywords`                    | Returns all keywords for current frame                                                                               |
| `get_pixel`                       | Returns raw pixel value(s) at source coordinates                                                                     |
| `get_session`                     | Returns current session state (fileList, currentFrame) — no activeDirectory                                          |
| `get_star_positions`              | Re-runs star detection, returns positions for annotation overlay                                                     |
| `get_threshold_profiles`          | Returns all threshold profiles from AppSettings                                                                      |
| `get_variable`                    | Returns a pcode variable value                                                                                       |
| `list_log_files`                  | Lists available log files                                                                                            |
| `list_plugins`                    | Returns list of registered plugins                                                                                   |
| `load_analysis_json`              | Clears session; populates analysis state from JSON payload; sets is_imported_session = true                          |
| `load_file`                       | Reads a single image file, injects into session                                                                      |
| `read_log_file`                   | Reads and parses a log file                                                                                          |
| `save_threshold_profile`          | Insert or update a threshold profile                                                                                 |
| `delete_threshold_profile`        | Delete a threshold profile; re-seeds Default if last deleted                                                         |
| `set_active_threshold_profile`    | Sets active profile; propagates thresholds into AppContext immediately                                               |
| `set_frame_flag`                  | Updates PASS/REJECT flag for a single frame in ctx.analysis_results by path                                          |
| `start_background_cache`          | Spawns background task to build blink cache JPEGs                                                                    |
| `check_crash_recovery`            | Returns crash recovery candidate if present (file_list + current_frame_index)                                        |
| `close_session`                   | Marks session closed in DB; resets is_imported_session                                                               |
| `open_session`                    | Records session open in session_history                                                                              |
| `write_crash_recovery`            | Writes crash recovery state (file_list, current_frame_index)                                                         |
| `backup_database`                 | Creates timestamped ZIP backup of photyx.db                                                                          |
| `restore_database`                | Restores photyx.db from ZIP backup                                                                                   |
| `delete_macro`                    | Deletes a macro from DB                                                                                              |
| `get_macros`                      | Returns all macros from DB                                                                                           |
| `get_macro_versions`              | Returns version history for a macro                                                                                  |
| `increment_macro_run_count`       | Increments run_count for a macro                                                                                     |
| `rename_macro`                    | Renames a macro                                                                                                      |
| `restore_macro_version`           | Restores a macro to a previous version                                                                               |
| `save_macro`                      | Saves (insert or update) a macro                                                                                     |
| `get_all_preferences`             | Returns all preferences as key/value map                                                                             |
| `set_preference`                  | Writes a single preference to DB and AppSettings                                                                     |
| `get_quick_launch_buttons`        | Returns Quick Launch button assignments                                                                              |
| `save_quick_launch_buttons`       | Saves Quick Launch button assignments                                                                                |

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

## 7. Session Store (`session.ts`) — Key Fields

| Field / Derived            | Purpose                                                |
| -------------------------- | ------------------------------------------------------ |
| `fileList`                 | Ordered list of full file paths in the current session |
| `loadedImages`             | Record of image metadata keyed by file path            |
| `currentFrame`             | Zero-based index of the currently displayed frame      |
| `variables`                | pcode variable store (mirrors ctx.variables)           |
| `fileCount` (derived)      | `fileList.length`                                      |
| `directoryCount` (derived) | Number of unique parent directories in `fileList`      |
| `currentImage` (derived)   | `loadedImages[fileList[currentFrame]]`                 |

Note: `activeDirectory` has been removed. Directory information is always derived from `fileList`.

---

## 8. Known Issues & Deferred Items

| Issue                                         | Notes                                                                                                                                      |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| cfitsio parallel loading crashes              | Thread-safety — sequential loading used                                                                                                    |
| Blink UI jitter                               | Suspected Tauri WebView compositor artifact on Windows; deferred                                                                           |
| Full-res frames are JPEG not lossless         | Disclosed via disclaimer bar; pixel readout uses raw buffer                                                                                |
| Long-running commands block UI                | Requires Tauri event system; deferred                                                                                                      |
| Zoom approximate at high levels               | Full-res cache uses STF params from display-res downsample                                                                                 |
| XISF Vector/Matrix properties                 | Read as placeholder, skipped on write; deferred                                                                                            |
| Rayon thread count not configurable           | Hardcoded to num_cpus-1                                                                                                                    |
| stderr log output in dev mode                 | Duplicated to terminal; remove when no longer needed                                                                                       |
| Sidebar icon tooltips clipped by Quick Launch | CSS stacking context; deferred                                                                                                             |
| Plugin boilerplate is verbose                 | Deferred to Phase 10                                                                                                                       |
| Single file load blink isolation              | Files loaded via LoadFile included in ctx.file_list                                                                                        |
| AutoStretch performance in dev mode           | 3–5 seconds for RGB 9MP in debug build; near-instant in release                                                                            |
| AutoStretch lost on Pixels tab switch         | Viewer reverts to raw display; deferred                                                                                                    |
| SNR estimator PSF artifact                    | Worse-seeing frames produce higher SNR; confirmed across sessions; excluded from rejection; estimator revision planned                     |
| AnalyzeFrames progress reporting              | No per-frame progress; requires Tauri event system; deferred                                                                               |
| threshold_profiles orphaned columns           | bg_stddev_reject_sigma and bg_gradient_reject_sigma remain in schema; migration deferred                                                   |
| Memory leak suspected                         | 103GB virtual / 20GB RSS observed after multiple sessions; audit deferred                                                                  |
| Linux GTK file picker multi-select            | Silently refuses to confirm selection containing both files and folders (e.g. rejected/ subfolder); avoid Ctrl+A when rejected/ is present |

---

## 9. Phase Completion Status

See Section 13 in the Specification.

## 10. Settings Persistence (Phase 9)

All persistence via SQLite (`photyx.db`). See `photyx_persistence_inventory.md` for schema and `photyx_reference.md` §5 for settings tables.

`AppSettings` is the global in-memory settings object. Loaded from `defaults.rs` + `preferences` table; `load_threshold_profiles()` seeds "Default" if table empty. All reads from struct; writes to struct + DB via `save_preference()`.

Settings that remain in localStorage: none — migration complete as of Phase 9 sub-phase B.

---

## 11. Database Schema

See `photyx_persistence_inventory.md` for full DDL. All tables live in `APPDATA/Photyx/photyx.db`.

DB schema is at version 3. Migration history:

- v1: initial schema
- v2: renamed `snr_reject_sigma` → `signal_weight_reject_sigma` in `threshold_profiles`
- v3: dropped `active_directory` column from `crash_recovery`
