# Photyx — Technical Reference

The authoritative reference for Photyx's current architecture,
persistence, scripting, analysis, and UI. Describes what the
application does today; it is not a history of how it got here. Photyx
is in release mode — expect bug fixes and minor UI adjustments, not
new features, unless noted otherwise.

---

## 1. Overview

Photyx is a desktop application for reading, viewing, processing, and
analyzing astrophotography image files — fast image review, batch
processing, keyword management, quantitative frame analysis, and
scriptable automation in a single extensible platform.

**Target platforms:** Windows 11+, macOS 12+, Ubuntu 22.04+ (or equivalent).

**Stack:**

| Layer       | Technology                                                     |
| ------------ | ---------------------------------------------------------------- |
| Frontend     | Tauri v2 + Svelte 5 + TypeScript; OS-native WebView              |
| Backend      | Rust; Rayon for parallelism; Tauri IPC for frontend ↔ backend    |
| Persistence  | Embedded SQLite via `rusqlite` (statically linked)               |
| Logging      | Rust `tracing` crate; rolling file log in OS app data directory  |
| Plugins      | Built-in native (Rust); all shipped plugins are native — no WASM plugins ship by default |

Key crates: `fitsio` (FITS), `tiff`, `rayon`, `tracing`, `serde_json`,
`bytemuck`, `once_cell`, `chrono`, `rusqlite`. The `photyx-xisf` crate
(MIT OR Apache-2.0) is a standalone workspace member implementing the
XISF reader/writer with zero-copy pixel deserialization.

**Design principle:** every operation — file readers, writers, keyword
operations, processing, analysis — is a plugin. The core engine is a
plugin host with no hard-coded operations.

---

## 2. Architecture

### 2.1 Project Structure

```
Photyx/
├── src-svelte/                ← Svelte 5 frontend
│   ├── lib/
│   │   ├── commands.ts        ← Shared backend command helpers
│   │   ├── pcodeCommands.ts   ← Single source of truth for all pcode command names
│   │   ├── components/
│   │   │   ├── panels/        ← Sliding panel components (FileBrowser, KeywordEditor, MacroEditor, MacroLibrary, PluginManager)
│   │   │   ├── AnalysisGraph.svelte
│   │   │   ├── AnalysisResults.svelte
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
│   │   ├── settings/constants.ts   ← Frontend mirror of defaults.rs
│   │   └── stores/                 ← consoleHistory, notifications, quickLaunch, session, settings, thresholdProfiles, ui, progress
│   └── routes/+page.svelte    ← Main application shell
├── src-tauri/                 ← Rust backend
│   └── src/
│       ├── lib.rs
│       ├── settings/{mod.rs, defaults.rs}
│       ├── plugin/{mod.rs, registry.rs}
│       ├── context/mod.rs     ← AppContext
│       ├── analysis/          ← background, eccentricity, fwhm, metrics, profiles, session_stats, stars, fft_align, star_align
│       ├── pcode/{mod.rs, tokenizer.rs}
│       ├── db/{schema.rs, mod.rs}
│       └── plugins/           ← one file per plugin (see §11)
├── crates/photyx-xisf/        ← XISF reader/writer crate
└── static/css/                ← one CSS file per major UI module; static/themes/
```

### 2.2 AppContext & Display Cache

Session state lives in a single `AppContext` struct behind a
`Mutex`. Raw pixel buffers are loaded once and never modified; all
display representations are derived JPEG copies:

```
AppContext
├── image_buffers: HashMap   ← raw pixels, NEVER modified
├── display_cache: HashMap>       ← display-res JPEG bytes
├── full_res_cache: HashMap>      ← full-resolution JPEG bytes
├── blink_cache_12: HashMap>      ← blink-res 12.5% JPEG bytes
└── blink_cache_25: HashMap>      ← blink-res 25% JPEG bytes
```

Design rule: display plugins read from `image_buffers` and write to
caches; they never modify `image_buffers`. Because `AppContext` is
behind a single Mutex, any plugin holding `&mut AppContext` for a
long-running operation blocks all other Tauri commands — including
frame display — for its duration. Extract owned data before any Rayon
parallel section; `&mut AppContext` cannot be borrowed inside Rayon
closures.

**Memory management (Linux):** `pin_mmap_threshold()` in `lib.rs`,
called as the first statement of `run()`, sets glibc's mmap threshold
to a static 1 MB via `mallopt(M_MMAP_THRESHOLD)` (gated behind
`#[cfg(target_os = "linux")]`; a no-op elsewhere). Without this,
glibc's dynamic threshold adaptation raises the threshold above the
~17 MB per-frame pixel-buffer size after the first few frees, shifting
subsequent large allocations onto the brk heap — where freed blocks
cannot be returned to the OS while interleaved small allocations pin
the heap top, leaving multi-GB freed-but-resident residuals after
`ClearSession`. With the threshold pinned, every allocation ≥ 1 MB
gets its own mmap and is returned to the OS immediately on free.
Related discipline: all bulk pixel-processing paths — `AnalyzeFrames`,
`CacheFrames`, and `start_background_cache` — snapshot pixel data in
chunks via `plugins/pixel_chunking.rs` (chunk size =
`rayon_thread_count`), bounding peak memory to one chunk of raw
buffers rather than a full-session clone. `start_background_cache`
additionally takes the `AppContext` lock per chunk rather than for the
whole build, so display commands are not starved for its duration, and
uses the global Rayon pool like its sibling plugins.

`AutoStretch` operates on a dynamic display-resolution downsampled
copy (~50x pixel-count reduction vs. the full
buffer). `get_full_frame` encodes the full-resolution raw buffer as
JPEG, applying the same STF stretch parameters AutoStretch computed at
display resolution — zoom is therefore approximate at high zoom
levels.

Relevant constants from `settings/defaults.rs` (non-persisted runtime
constants, not user preferences): `DISPLAY_MAX_WIDTH_PX = 1200` (the
box-filter downsample ceiling for display resolution),
`DISPLAY_JPEG_QUALITY = 92`, `BLINK_JPEG_QUALITY = 85`. There is no
dedicated full-resolution JPEG quality constant in `defaults.rs` — see
§14 for the resulting discrepancy against documented behavior.

### 2.3 Session & File Model

Photyx uses a **global file context** — a flat list of file paths
(`ctx.file_list`) with no concept of an "active directory." Files from
multiple directories coexist in a single session.

- `AddFiles` and `ReadImages` are the two entry points for loading
  images into the session. `AddFiles` appends a comma-separated list
  of explicit file paths and/or glob patterns (`*`, `?`, `[...]`,
  usable in any path segment) — it does not accept a bare directory.
  `ReadImages` takes a single `path` argument that is either one file
  or one directory (all supported files within the directory are
  loaded) — it does not accept glob patterns or a comma-separated
  list. Both skip files already loaded and neither clears the session
  on its own. Memory is checked against the buffer pool limit before
  loading, based on first-file dimensions × total count. Use
  `ClearSession` first to start fresh.
- `ctx.source_directories()` returns the unique parent directories of
  all loaded files; `ctx.common_parent()` returns the common parent if
  all files share one, else `None`. Relative paths in pcode commands
  resolve against `common_parent()` when available.
- `ctx.remove_rejected_files()` removes rejected paths from
  `file_list` and all caches after commit.
- Status bar displays `N files · M directories`, derived from
  `file_list` — nothing is shown when no files are loaded.
- `ClearSession` clears the file list, all buffers, and all analysis
  results, resetting the session entirely.

Session state consists of: the loaded file list (full absolute paths),
per-file pixel buffers and derived caches, and analysis results. There
is no active-directory field anywhere in the model.

### 2.4 Plugin Architecture

Every operation is a `PhotonPlugin` trait object registered in a
plugin registry, dispatched either interactively via the pcode console
or programmatically via the script runner (`dispatch_command` for
single commands, `run_script` for full scripts). All shipped plugins
are built-in native Rust — compiled into the binary, version-locked
with core. The plugin framework supports WASM user plugins via
Wasmtime, but none ship by default.

Plugins must call `set_progress("", 0, 0)` before returning to clear
their own progress state — plugin cleanup is the plugin's own
responsibility, not the caller's.

### 2.5 Cross-Boundary Side Effects (client_actions)

When a plugin needs the frontend to do something after a command
completes (refresh a view, open a modal), it returns explicit action
tokens in `PluginOutput::Data` rather than the frontend inferring
behavior from the command name. The frontend dispatches on these
`client_actions` strings, not on which command was run — every entry
point that can trigger a command (menu, console, macro runner, Quick
Launch) goes through the same dispatch table.

| Action | Emitted by | Frontend effect |
| ------------------------ | ---------------- | ----------------------------------- |
| `refresh_autostretch` | AutoStretch | Calls `applyAutoStretch()` |
| `refresh_annotations` | ComputeFWHM | Calls `ui.refreshAnnotations()` |
| `open_keyword_modal` | ListKeywords | Calls `ui.openKeywordModal()` |

### 2.6 AppContext (Full Field Reference)

`AppContext` (`src-tauri/src/context/mod.rs`) is the single struct,
held in `Mutex` inside `PhotoxState`, that carries all session state
through every plugin call.

| Field                      | Type                                          | Purpose                                                                 |
| ---------------------------- | ------------------------------------------------ | -------------------------------------------------------------------------- |
| `file_list`                | `Vec`                                    | Flat list of file paths in the session                                    |
| `image_buffers`             | `HashMap`                   | Raw pixel data, keyed by path — never modified                            |
| `display_cache`             | `HashMap>`                       | Display-resolution JPEG bytes                                             |
| `full_res_cache`            | `HashMap>`                       | Full-resolution JPEG bytes, built on demand                               |
| `blink_cache_12`            | `HashMap>`                       | Blink cache at 12.5% resolution                                            |
| `blink_cache_25`            | `HashMap>`                       | Blink cache at 25% resolution                                              |
| `blink_cache_status`        | `BlinkCacheStatus` (Idle / Building / Ready)      | Background cache build status                                              |
| `current_frame`             | `usize`                                          | Index of the currently displayed frame                                    |
| `variables`                 | `HashMap`                        | pcode variable store                                                       |
| `last_histogram`            | `Option`                         | Last computed histogram, for frontend retrieval                            |
| `last_stf_params`           | `Option<(f32, f32)>`                             | Last Auto-STF (c0, m) params — reused by `get_full_frame`                  |
| `autostretch_shadow_clip`   | `f32`                                            | Mirrored from `AppSettings` at startup and on preference change            |
| `autostretch_target_bg`     | `f32`                                            | Mirrored from `AppSettings` at startup and on preference change            |
| `analysis_thresholds`       | `AnalysisThresholds`                             | Active AnalyzeFrames thresholds, loaded at startup and on profile change   |
| `last_analysis_thresholds`  | `Option`                     | Thresholds actually used in the last AnalyzeFrames run                     |
| `analysis_results`          | `HashMap`                | Per-frame analysis results, keyed by path                                  |
| `outlier_frame_paths`       | `HashSet`                                | Frames excluded from session-stat recomputation in the last run            |
| `last_session_stats`        | `Option`                           | Clean session stats from the last run (outliers excluded)                  |
| `log_dir`                   | `Option`                                 | Configurable log directory; falls back to the Tauri app data dir if `None` |
| `buffer_pool_bytes`         | `i64`                                            | Memory limit gating loads; mirrored from `AppSettings`                     |
| `rayon_thread_count`        | `i64`                                            | `-1` means `num_cpus - 1` at runtime; mirrored from `AppSettings`          |
| `current_session_id`        | `Option`                                    | Row ID in `session_history`; set by `open_session`, cleared by `close_session` |
| `is_imported_session`       | `bool`                                           | True when analysis results came from a JSON import, not a live run        |
| `stack_result`               | `Option`                            | Transient StackFrames output — no source file path                        |
| `stack_contributions`       | `Vec`                         | Per-frame contribution metrics from the last StackFrames run              |
| `stack_summary`             | `Option`                           | Summary metrics from the last StackFrames run                             |

**Key methods:**

- `source_directories()` / `common_parent()` — derive directory info
  from `file_list`; there is no stored directory field
- `sync_from_settings(&AppSettings)` — refreshes the
  AppSettings-mirrored fields (AutoStretch defaults, buffer pool
  limit, thread count); called at startup and on every preference
  change
- `current_image()` / `current_image_mut()` — resolve
  `file_list[current_frame]` into the buffer
- `total_memory_used()` — sums buffer sizes across `image_buffers`
  (bytes, accounting for bit depth)
- `clear_session()` — full reset: file list, all four caches, analysis
  state, variables, imported-session flag, and stack state all cleared
- `remove_frame_data(path)` — removes one file's buffer, all four
  caches, its analysis result, and its outlier flag; does **not**
  touch `file_list` — callers remove the path themselves
- `remove_rejected_files(paths)` — the post-commit cleanup: retains
  only non-rejected paths in `file_list`, calls `remove_frame_data`
  for each rejected path, clears analysis results/outliers/session
  stats, resets `current_frame` to 0, and clears `is_imported_session`
- `analysis_result_for(path)` — get-or-insert accessor into
  `analysis_results`
- `clear_stack()` — discards `stack_result`, `stack_contributions`,
  and `stack_summary`; called by `ClearStack` and at the start of
  every `StackFrames` run

### 2.7 Progress Reporting Pathway

Long-running plugin work is fire-and-forget on both ends: dispatching
a command returns immediately rather than blocking until completion,
and the frontend does not receive the result back from that initiating
call either — it polls for both progress and the eventual result
independently.

**Backend side:** a plugin reports progress via
`crate::set_progress(label: &str, current: u32, total: u32)`, callable
from anywhere in `execute()`:

- Called with `(label, 0, 0)` to mark the start of a phase with no
  meaningful count yet (e.g. `set_progress("Stacking analysis", 0,
  0)`)
- Called repeatedly with real counts during incremental work
  (e.g. once per frame during registration)
- **Must** be called with `("", 0, 0)` immediately before the plugin
  returns, on every path including errors — clearing progress is the
  plugin's own responsibility. A plugin that returns early without
  this call leaves a stale progress indicator active.

**Frontend side (`stores/progress.ts`):** a single `setInterval` on a
500ms cadence drives two independent polls:

- `invoke('get_progress')` → `[label, current, total]` tuple → written
  into the `progress` writable store
- `invoke('get_job_result')` → `JobResult | null` → written into the
  `jobResult` writable store whenever non-null

Both calls are wrapped in try/catch that silently ignores failures —
the assumption being the backend isn't ready yet, not that something
is wrong.

**`JobResult` shape:** `{ results: ScriptResult[], session_changed,
display_changed, client_actions }` — an aggregate over the whole
dispatched script. Each `ScriptResult` covers one executed line:
`line_number`, `command`, `success`, `message`, `data`, `trace_line`,
and its own `client_actions`. This is the same `client_actions`
mechanism described in §2.5 — both the per-line result and the
job-level aggregate can carry action tokens for the frontend to
dispatch on.

A `jobOwner` writable store also exists alongside `progress` and
`jobResult`, presumably to track which UI component dispatched the
in-flight job — its write side isn't confirmed here.

---

## 3. User Interface Reference

Single-window SPA. Layout top to bottom: Menu Bar (28px) → Toolbar
(34px) → Content Area (flex: 1) → Status Bar (22px). Content Area
holds Icon Sidebar (40px) | Viewer Region (flex: 1); the Quick Launch
panel (34px) sits above the Viewer Region.

### 3.1 Menu Bar

Six top-level menus:

**File** — Load Single Image… · Exit

**Session** — Add Files… (Ctrl+O) · Close Session · Export Session
JSON… · Import Session JSON…

**Edit** — Preferences · Analysis Parameters

**View** — Theme: Dark / Light / Matrix

**Analyze** — Analyze Frames · Analysis Results · Analysis Graph ·
Contour Plot

Analyze Frames requires an explicit threshold profile selection before
running: clicking it opens a popup listing all saved profiles,
pre-selected to whichever is currently active. Confirming runs
`AnalyzeFrames` with the selected profile for that run only — the
saved active profile is unchanged regardless of what's picked.
Cancelling runs nothing. This popup only appears for the menu trigger;
`AnalyzeFrames` invoked from Quick Launch, a saved macro, `RunMacro`,
or the console runs immediately as before, using whatever `profile=`
argument (or the active profile, if none given) the script specifies.

**Tools** — Backup Database · Restore Database · Log Viewer

**Help** — About Photyx · Documentation

### 3.2 Toolbar

34px fixed height. Viewer controls and the file/directory count
display (`N files · M directories`, derived from the session file
list; empty when no files loaded).

### 3.3 Icon Sidebar

40px fixed width. Icons for panels — File Browser, Keyword Editor,
Macro Library, Plugin Manager — each triggering a sliding panel.

### 3.4 Viewer Region

Fills the remaining content area. Shows the image viewer by default;
replaced by viewer-region components (Analysis Graph, Analysis
Results) when active. All visibility controlled exclusively via
`ui.showView()` — see §2's View Registry pattern.

### 3.5 Status Bar

22px fixed height. Shows the active notification; expands to 66px with
a pulse animation while `notifications.running()` is active.

### 3.6 Sliding Panels

Slide in from the left over the viewer region, triggered by the Icon
Sidebar. Width is either standard (varies by panel) or wide (75vw —
used by the Keyword Editor).

### 3.7 Quick Launch Panel

34px bar between the Toolbar and Viewer Region. Buttons run pcode
scripts via `run_script`; right-click to remove; macros can be pinned
from the Macro Library. The user may pin as many buttons as desired —
they wrap to the next row automatically. Assignments persist to the
`quick_launch_buttons` table (see §8).

### 3.8 pcode Console

Collapsible panel at the bottom of the viewer region. Expands to a
60vh, 85%-opacity full-width overlay when its header is clicked. Trace
/ No Trace toggle controls execution verbosity (see §4). History
navigation supported.

### 3.9 Analysis Graph

Viewer-region component (`activeView === 'analysisGraph'`). Two-metric
line chart with sigma bands, mean line, and reject threshold lines
drawn from `applied_thresholds` (the thresholds actually used in the
last run — see §8.5). Clicking a dot navigates to that frame.

**Toolbar:** Metric 1 dropdown | Metric 2 dropdown | ↻ Refresh | ✓
Commit Results | ⎘ Copy | ⬇ Save Image | ✕ Close

**Dot appearance:** every dot has a 2px black border. PASS = white
fill. REJECT — Optical (O) = red (`#dc3232`); Transparency (T) =
yellow (`#d4a820`); Sky Brightness (B) = blue
(`#3478dc`). Multi-category REJECT renders as a split semicircle in
the respective colors, slightly larger radius, with a black dividing
line.

**Reference frame:** the session's reference frame (selected by
highest `frame_quality_score()` — see §7.1 — among PASS frames,
falling back to all frames if none passed) renders as a gold 5-point
star instead of the normal PASS/REJECT dot. The star's stroke color
still signals the frame's real classification — black stroke for
PASS, red stroke (`#dc3232`) if the reference frame is itself REJECT
(rare, but possible when the fallback applies). The reference frame
is never hidden or miscategorized by being selected as REF.

**Legend:** fixed top-left corner of the canvas, always visible,
showing all four categories.

Commit Results is disabled for imported sessions (`is_imported` from
`get_analysis_results`).

### 3.10 Analysis Results

Viewer-region component. Sortable table of per-frame metrics, PXFLAG,
and rejection category.

**Toolbar row 1:** title | ↻ Refresh | ✓ Commit Results | ⎘ Copy | ✕
Close **Toolbar row 2:** [IMPORTED badge if applicable] | session path
(derived from the file list)

**Columns:** # | Filename | FWHM | Eccentricity | Stars | Bg Median |
PXFLAG | Category

Category badges are color-coded (O = red, T = yellow, B = blue,
multi-category = purple), centered. The session's reference frame
(see §3.9, §7.1) additionally shows a gold ★ badge in the Category
column, alongside its rejection category badge if it has one — the
PXFLAG column always shows the frame's real PASS/REJECT
classification; being selected as reference never overrides or hides
it.

**PXFLAG toggle:** right-click any row → "Set to PASS" (on a REJECT
row) or "Set to REJECT" (on a PASS row). Local state only until
Commit, held in a shared store (not per-view) so a toggle made here is
honored even if the user commits from Analysis Graph instead — see
Commit sequence below. Toggled rows get an amber left border and
subtle background tint. All underlying metric data is preserved
regardless of toggle direction, so a REJECT→PASS toggle keeps its
category badge visible and can be toggled back. Refresh discards all
pending toggles in both views.

**Commit sequence:** shared with Analysis Graph — committing from
either view runs the identical sequence: sync any pending toggled
flags to Rust → `commit_analysis_results` → on success: sync session
from backend → `ui.showView(null)` → `ui.clearViewer()` → clear
pending toggles. Non-terminal — the session stays open and pass
frames remain loaded and ready for subsequent operations (e.g.
stacking). Disabled for imported sessions in both views.

### 3.11 Info Panel

Pixel coordinates, raw value, and WCS coordinates (if
available). Always visible when the viewer has an image loaded.

### 3.12 Edit > Preferences

Modal dialog, 540px wide, draft-copy pattern (nothing written until
OK/Apply; Cancel discards). Covers 8 user-facing preference
fields. Theme is excluded (lives under View instead); threshold
profiles are excluded (under Edit > Analysis Parameters
instead). Clicking outside the dialog does not close it.

### 3.13 Edit > Analysis Parameters (Threshold Profiles)

Modal dialog, 400px wide, managing named threshold profiles (see §8.5
for the underlying data model).

**Profile selector row:** `[🗑] [profile dropdown] [＋]` — trash
deletes the selected profile via an inline confirmation bar (any
profile, including the last, can be deleted; deleting the last
re-seeds "Default"); ＋ reveals a name input for a new profile seeded
with default values; selecting a profile in the dropdown makes it the
one being edited, not immediately active.

**Active profile indicator:** "Active profile: [name]" line, updated
on OK/Apply.

**Threshold fields** (label / direction / input / unit): Background
Median (`> +σ`, 0.5–4.0, default 2.5) · FWHM (`> +σ`, 0.5–4.0, default
2.5) · Eccentricity (`> absolute`, 0.10–1.00, default 0.85) · Star
Count (`< σ`, 0.5–5.0, default 1.5)

Switching profiles with unsaved edits shows an inline confirmation
bar. OK/Apply saves to DB and sets the profile active (propagated to
`AppContext` immediately). Clicking outside does not close the dialog.

### 3.14 Log Viewer

Modal overlay from Tools > Log Viewer. File picker → log content with
ERROR/WARN/INFO/DEBUG level filters. Auto-tail polls every 2 seconds;
auto-scroll suspends when the user scrolls up manually.

### 3.15 Blink Tab

Play/pause/step controls. Resolution dropdown (12.5% / 25%). Min Delay
dropdown. "Highlight Rejected" toggle overlays a red border on REJECT
frames during blink. Cache builds on first play; invalidated when
resolution changes or the file list changes.

### 3.16 Session JSON Export/Import

**Export** (Session → Export Session JSON…): exports the current
session's analysis results as a portable JSON archive. Default
filename `_.json`, derived from the first frame's basename. Contains
`photyx_version`, `exported_at`, `threshold_profile_name`,
`thresholds`, `session_stats`, `outlier_paths[]`, and `frames[]` (per
frame: full path, all raw metric values, flag, `triggered_by`,
`rejection_category`). All filenames are stored as full absolute
paths, to support multi-directory sessions.

**Import** (Session → Import Session JSON…): clears the current
session and loads analysis results from a JSON file — no images are
loaded, display only. An IMPORTED badge appears in the Analysis
Results toolbar and Commit Results is disabled; all other display
functionality works normally. Opens the Analysis Results view
automatically on import.

---

## 4. pcode Scripting Reference

pcode is a line-oriented macro language: each line is a command name
followed by zero or more named arguments. Macros are stored in SQLite
(`macros` table, §8.2) and executable from the console, Quick Launch,
or `RunMacro`. For language mechanics — variables, arithmetic,
conditionals, loops, `@param` declarations, trace mode, string literal
rules — see the pcode Guide. This section is the command dictionary
only.

### 4.1 Command Dictionary

| Command              | Category         | Description                                                                                                                | Key Arguments                                           |
| ---------------------- | ------------------ | ------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| AddFiles              | Session           | Appends explicit file paths to the session; skips duplicates; checks memory limit before loading                              | paths                                                       |
| AddKeyword            | Keyword           | Adds or replaces a keyword on loaded images                                                                                    | name, value, [comment], [scope=all\|current]                |
| AnalyzeFrames         | Frame Analysis    | Computes five quality metrics for all loaded frames, classifies each as PASS or REJECT                                        | [profile]                                                    |
| Assert                | Scripting         | Halts execution with an error if expression is false; silent on pass in both Trace and No Trace modes                          | expression                                                   |
| AutoStretch           | Processing        | Applies Auto-STF stretch to current frame (display only — raw buffer unchanged)                                                | [shadowClip], [targetBackground]                             |
| CacheFrames           | Blink & View      | Pre-decodes and caches all frames for blinking at both resolutions                                                             | —                                                            |
| ClearAnnotations      | Display           | Removes all star and analysis overlay annotations from the viewer                                                              | —                                                            |
| ClearSession          | Session           | Clears all loaded images and resets session state                                                                              | —                                                            |
| ClearStack            | Stacking          | Discards the transient stack result and per-frame contribution data                                                            | —                                                            |
| CommitAnalysis        | Frame Analysis    | Moves all REJECT frames to a rejected/ subfolder within each frame's source directory and removes them from the session; pass frames remain loaded | [append]                                                     |
| CommitStretch         | Stacking          | Permanently applies Auto-STF stretch to the stack result pixel buffer; after commit the buffer holds non-linear data           | [shadow_clip], [target_bg]                                   |
| ComputeEccentricity   | Analysis          | Calculates eccentricity for detected stars on current frame                                                                    | —                                                            |
| ComputeFWHM           | Analysis          | Calculates FWHM for detected stars; displays per-star circle annotations on viewer overlay                                     | —                                                            |
| ContourHeatmap        | Analysis          | Generates spatial FWHM heatmap for current frame; writes XISF to source file's directory; stores output path in `$NEW_FILE`    | [palette], [contour_levels], [threshold], [saturation]       |
| CopyFile              | File Management   | Copies a file to a destination directory; defaults to current frame if source= not specified; destination created automatically; stores path in `$NEW_FILE` | [source], destination                                        |
| CopyKeyword           | Keyword           | Copies a keyword value to a new keyword name                                                                                   | from, to                                                     |
| CountFiles            | Scripting         | Stores number of files in current list in `$filecount`                                                                        | —                                                            |
| CountStars            | Analysis          | Counts detected stars in current frame                                                                                         | —                                                            |
| DebayerImage          | Processing        | Debayers a Bayer CFA image on demand; pattern read from BAYERPAT keyword if present                                            | [pattern=RGGB\|BGGR\|GRBG\|GBRG], [method=bilinear]          |
| DeleteKeyword         | Keyword           | Removes a keyword from loaded images                                                                                           | name, [scope=all\|current]                                   |
| ExportAnalysisReport  | Frame Analysis    | Exports the current analysis results as a Photyx session JSON file; if path is omitted, derives filename from the first frame and writes to the system Downloads folder | [path]                                                       |
| FilterByKeyword       | File Management   | Filters the active file list by keyword value                                                                                  | name, value                                                  |
| GetHistogram          | Processing        | Computes histogram statistics for current frame (median, std dev, clipping %)                                                  | —                                                            |
| GetKeyword            | Interrogation     | Retrieves a keyword value; auto-stores in `$` (uppercase) — e.g. `GetKeyword name=FILTER` stores result in `$FILTER`      | name                                                         |
| ListKeywords          | Keyword           | Lists all keywords for the current image                                                                                       | —                                                            |
| LoadFile              | File Management   | Loads a single image file for display without adding it to the session; stores path in `$LOAD_FILE_PATH`                       | path                                                         |
| Log                   | Scripting         | Writes collected macro output since last Log call to a file                                                                    | path, [append]                                               |
| ModifyKeyword         | Keyword           | Changes the value of an existing keyword                                                                                       | name, value, [comment], [scope=all\|current]                 |
| MoveFile              | File Management   | Moves a file to a destination directory; defaults to current frame if source= not specified; stores path in `$NEW_FILE`        | [source], destination                                        |
| Print                 | Scripting         | Outputs a message to the pcode console; accepts bare expressions — `Print $x + 1` and `Print "hello"` are both valid           | message (positional or bare expression)                      |
| ReadImages            | Session           | Reads a single image or all supported files in a directory                                                                     | path                                                          |
| RejectCurrentFrame    | Session           | Moves a single frame to a rejected/ subfolder within its own source directory, removing it from the session and all caches; defaults to the current frame | [index], [append]                                            |
| RunMacro              | Scripting         | Executes a saved macro by name from the database                                                                               | name                                                          |
| Set                   | Scripting         | Assigns a value to a variable; string literals on the RHS must use double quotes                                               | varname = value                                              |
| SetFrame              | Navigation        | Sets the current active frame by index (0-based)                                                                               | index                                                         |
| ShowAnalysisGraph     | Display           | Opens the Analysis Graph view in the viewer region                                                                             | —                                                            |
| ShowAnalysisResults   | Display           | Opens the Analysis Results table view in the viewer region                                                                     | —                                                            |
| StackFrames           | Stacking          | Stacks all session frames using FFT alignment and sigma-clipped mean combination; result stored as transient stack buffer      | [calibration_dir]                                             |
| WriteCurrent          | I/O               | Writes all buffered images back to their source paths in their original format (atomic temp-rename)                            | —                                                            |
| WriteFIT              | I/O               | Writes all buffered images as FITS files (atomic temp-rename)                                                                  | destination, [overwrite]                                     |
| WriteFrame            | I/O               | Writes the currently active frame only back to its source format (atomic temp-rename)                                          | —                                                            |
| WriteTIFF             | I/O               | Writes all buffered images as TIFF files with AstroTIFF keyword embedding (atomic temp-rename)                                 | destination, [overwrite]                                     |
| WriteXISF             | I/O               | Writes all buffered images as XISF files; use stack=true to export the transient stack result instead                          | destination, [overwrite], [compress=true\|false], [stack=true\|false] |
| pwd                   | Console           | Prints the unique source directories of all files currently loaded in the session (client-side only)                           | —                                                            |

### 4.2 Retired Commands

No longer available; listed so old scripts and macros can be diagnosed:

| Retired Command     | Replacement | Notes                                                                   |
| ---------------------- | ------------- | ---------------------------------------------------------------------------- |
| SelectDirectory       | AddFiles      | Directory as a first-class entity is replaced by explicit file paths          |
| GetImageProperty      | (removed)     | Not implemented; interrogation via GetKeyword and CountFiles instead          |
| GetSessionProperty    | (removed)     | Not implemented; interrogation via GetKeyword and CountFiles instead          |
| ListFiles             | (removed)     | Not implemented                                                               |
| Test                  | (removed)     | Not implemented                                                               |
| CropImage             | (removed)     | Not implemented in this release                                               |
| ReadAll               | AddFiles      | Use AddFiles with explicit paths; ClearSession first if starting fresh        |
| ReadFIT               | AddFiles      | Format filtering is now the user's responsibility at selection time           |
| ReadTIFF              | AddFiles      | Format filtering is now the user's responsibility at selection time           |
| ReadXISF              | AddFiles      | Format filtering is now the user's responsibility at selection time           |

### 4.3 Notes on Specific Entries

- **Keyword scope:** `AddKeyword`, `DeleteKeyword`, and
  `ModifyKeyword` accept `scope=all` (default, applies to all loaded
  frames) or `scope=current` (applies only to the frame set by
  `SetFrame`).
- **`$NEW_FILE` convention:** any plugin that creates a new file
  stores its path in `ctx.variables["NEW_FILE"]`, usable immediately
  as `$NEW_FILE` in the next line — e.g. `ContourHeatmap` followed by
  `MoveFile source="$NEW_FILE" destination="D:/heatmaps/"`.

---

## 5. Interrogation Properties

`GetKeyword` is the only interrogation mechanism in pcode. Earlier
documentation described a broader property/test system
(`GetImageProperty`, `GetSessionProperty`, a `Test` boolean-expression
command) — none of that was ever implemented; see §4.2, Retired
Commands.

`pwd` (§4.1, Console category) also surfaces current state — the
session's unique source directories — but prints directly to console
output rather than storing into a variable, so it isn't an
interrogation property in the same sense as `GetKeyword`.

### 5.1 GetKeyword

`GetKeyword name=X` retrieves a keyword value from the current frame's
header and auto-stores it in `$` (uppercase). Any keyword present in
the file header can be retrieved; the table below lists common
astrophotography keywords as examples, not an exhaustive or enforced
list.

| Keyword | Type | Description | Example Value |
| ---------- | --------- | --------------------------------------- | ----------------------- |
| OBJECT | String | Target object name | M31 |
| TELESCOP | String | Telescope name | Celestron EdgeHD 8 |
| INSTRUME | String | Camera/instrument name | ZWO ASI2600MC |
| EXPTIME | Float | Exposure time in seconds | 300.0 |
| GAIN | Integer | Camera gain setting | 100 |
| OFFSET | Integer | Camera offset setting | 30 |
| TEMP | Float | Sensor temperature in Celsius | -10.0 |
| FILTER | String | Filter name | Ha, OIII, Lum, duo |
| BAYERPAT | String | Bayer pattern from capture software | RGGB |
| XBINNING | Integer | Horizontal binning factor | 1 |
| YBINNING | Integer | Vertical binning factor | 1 |
| FOCALLEN | Float | Focal length in mm | 2032.0 |
| APERTURE | Float | Aperture in mm | 203.2 |
| RA | Float | Right ascension of target in degrees | 10.6848 |
| DEC | Float | Declination of target in degrees | 41.2692 |
| DATE-OBS | String | Date and time of observation (UTC) | 2024-11-15T22:30:00 |
| SITELONG | Float | Observatory longitude | -105.1786 |
| SITELAT | Float | Observatory latitude | 40.5853 |
| SITEELEV | Float | Observatory elevation in meters | 1524.0 |
| IMAGETYP | String | Frame type | Light, Dark, Flat, Bias |
| SWCREATE | String | Software that created the file | Photyx |

---

## 6. Frame Analysis & Rejection

### 6.1 Philosophy

Photyx flags obvious disasters only — borderline frames are left for
downstream tools (PixInsight SubframeSelector) to weight rather than
being hard-excluded. Classification is always session-relative, never
a cross-session absolute. The bias is toward keeping frames; only
extreme outliers are removed. See the cross-session metric correlation
findings for the empirical basis of this approach and the current
thresholds.

### 6.2 Metrics

Five metrics are computed per frame:

| Metric             | Type      | Direction            | Default Reject | Drives Rejection |
| --------------------- | ----------- | ----------------------- | ----------------- | ------------------- |
| Background Median   | Sigma      | `+σ` (high is worse)    | 2.5σ                | ✓                    |
| FWHM                | Sigma      | `+σ` (high is worse)    | 2.5σ                | ✓                    |
| Eccentricity         | Absolute   | `> threshold`            | 0.85                | ✓                    |
| Star Count           | Sigma      | `−σ` (low is worse)      | 1.5σ                | ✓                    |

All metrics except Background Median are derived from elliptical 2D
Moffat PSF fitting per detected star. Star Count only counts stars
that pass Moffat PSF acceptance criteria (a lenient connected-pixel
detector was replaced by this). PSF Residual is computed internally as
the star-acceptance gate but is not user-facing.

**SNR** is computed and displayed as a diagnostic value only — it does
**not** drive classification. Cross-session analysis confirmed a PSF
artifact: worse-seeing frames produce *higher* SNR due to bloated star
flux, and SNR never uniquely drove a rejection that FWHM or Star Count
didn't already catch.

**Removed metrics:** Background Std Dev (r = 0.92–0.999 correlated
with Background Median) and Background Gradient (sign reversal is
session-dependent) were dropped as rejection metrics. Both
corresponding pcode commands remain as deprecated stubs for script
compatibility; the underlying values are still stored in
`frame_analysis_results` (§8.2) but unused for classification.

### 6.3 Classification

`classify_frame()` in `analysis/session_stats.rs` — PASS/REJECT only,
no SUSPECT tier. A frame is REJECT if **any single metric** crosses
its threshold:

- Background Median, FWHM: REJECT if `sigma_deviation ≥
  threshold.reject`
- Star Count: REJECT if `sigma_deviation ≤
  −threshold.reject`
- Eccentricity: REJECT if the raw value `≥ threshold.reject`
  (absolute, not sigma-based)

`triggered_by` records the name of every metric that fired, not just
the first.

### 6.4 Session Statistics — Two-Pass Iterative Sigma Clipping

`compute_session_stats_iterative()`:

1. Compute initial session stats across the full population — Star
   Count uses bimodal-aware anchoring (§6.5); the other four metrics
   use plain mean/stddev.
2. Flag outliers: any frame where a metric (Eccentricity excluded)
   deviates beyond `OUTLIER_SIGMA_THRESHOLD` (confirmed 4.0σ in
   `defaults.rs`) from the *initial* stats is marked an outlier.
3. Recompute session stats on the outlier-free subset — but **only**
   for the non-bimodal metric (Background Median, FWHM). Star Count's
   bimodal anchor is carried forward unchanged from step 1 rather than
   recomputed, specifically to prevent the anchor from drifting
   between passes, which would otherwise make classification
   non-deterministic.
4. Returns the final `SessionStats` plus the set of outlier frame
   paths.

### 6.5 Bimodality Detection (Star Count)

`bimodality_coefficient()` computes a bimodality coefficient (BC) from
skewness and excess kurtosis (Pfister et al. 2013 formulation). BC >
0.555 indicates a bimodal distribution. When Star Count is bimodal:

1. `find_valley()` locates the deepest point between the two dominant
   peaks in a smoothed 20-bin histogram of the values.
2. Mean and stddev are recomputed using only the upper cluster (values
   above the valley, since higher star count is better) — provided at
   least 2 values fall in that cluster.
3. This anchors the Star Count threshold to the clear-sky population,
   so a large block of cloud-degraded frames can't pull the session
   mean down and collapse the reject threshold.

If BC doesn't exceed the bimodality threshold, or the upper cluster
has fewer than 2 values, the full population's plain mean/stddev is
used instead — identical to non-bimodal behavior. This mechanism
currently applies only to Star Count; other metrics use plain stats
unconditionally.

### 6.6 Rejection Categories

`categorize_rejection()` derives a category string from which metrics
triggered:

| Category | Label          | Triggered by                                    |
| ---------- | ---------------- | ---------------------------------------------------- |
| O          | Optical         | FWHM and/or Eccentricity                              |
| T          | Transparency    | Star Count without Background Median |
| B          | Sky Brightness  | Background Median                                      |

**Ordering:** O always leads when present. When both B and T are
present, B leads (`...BT`, not `...TB`) — sky brightness is treated as
the root cause of star suppression, not a coincidental
co-occurrence. Possible category strings: `O`, `B`, `T`, `OB`, `OT`,
`BT`, `OBT`.

### 6.7 Commit Results

`commit_analysis_results` is a fast, **non-terminal** operation:

1. Any locally toggled PXFLAG changes are pushed to Rust first
   (`set_frame_flag` per toggled frame).
2. Every REJECT file is moved to `/rejected/..rejected` — within *its
   own* source directory, so a multi-directory session produces
   multiple `rejected/` subfolders.
3. `ctx.remove_rejected_files()` removes the rejected paths from
   `file_list` and all caches, and clears `analysis_results`,
   `outlier_frame_paths`, `last_session_stats`, and
   `last_analysis_thresholds` entirely — for the *whole* session, not
   just the rejected frames — and resets `current_frame` to 0 and
   `is_imported_session` to `false`.

**PXFLAG is never written to the files themselves.** The move to
`rejected/` is the sole persistence action, which keeps commit fast
(well under a second for 100+ frames) and avoids rewriting raw image
data. Pass frames remain loaded and ready for subsequent operations
(e.g. stacking) — the session stays open.

**Frontend sequencing matters:** sync toggled flags →
`commit_analysis_results` → on success, sync session state from
`get_session` → `ui.showView(null)` → `ui.clearViewer()`. The session
sync must happen *before* `showView(null)`, so reactive components
still update while mounted.

### 6.8 On-the-Fly Reclassification

`get_analysis_results` reclassifies every frame on every call, using
cached per-frame metrics — it does not re-run `AnalyzeFrames`. It
classifies against `ctx.last_analysis_thresholds` (the thresholds the
last `AnalyzeFrames` run, or a JSON import, actually used) when
present, falling back to the active profile
(`ctx.analysis_thresholds`) only if nothing has been analyzed yet.
This is what keeps a `profile=`-pinned run's classifications stable
across Refresh — see §8.5 for how a *deliberate* threshold change
still takes effect live.

1. Returns empty if `analysis_results` is empty.
2. Skipped entirely if `is_imported_session` — an imported session's
   classifications (from the JSON file) are authoritative and are not
   recomputed.
3. Otherwise: runs `compute_session_stats_iterative`, updates session
   stats in `ctx`, reclassifies each frame (`classify_frame` +
   `categorize_rejection`) against the thresholds described above,
   updates `flag`/`triggered_by`/`rejection_category` in place, and
   returns the results plus `applied_thresholds` (the thresholds
   actually used for this classification) and the `is_imported` flag.

### 6.9 PXFLAG Toggle (Analysis Results and Analysis Graph)

Right-click a row in Analysis Results → "Set to PASS" (REJECT row) or
"Set to REJECT" (PASS row). This is local UI state only — held in a
shared frontend store (`analysisToggles`), not per-view or persisted
to the row itself — until Commit. Being shared means a toggle made in
Analysis Results is honored even if the user commits from Analysis
Graph instead, and vice versa; both views' Commit buttons run the
same shared sequence (§3.10). `set_frame_flag` is called per pending
toggle just before commit and updates
`ctx.analysis_results[path].flag` directly with no reclassification
side effects. All underlying metric data and category badge are
preserved regardless of toggle direction, so a toggle can be reversed
before commit without losing information. A Refresh in either view
discards all pending toggles.

---

## 7. Stacking (StackFrames)

`StackFrames` performs two-pass, meridian-flip-aware stacking with
sigma-clipped mean combination. Implementation lives in
`plugins/stack_frames.rs`, with alignment primitives in
`analysis/fft_align.rs` and `analysis/star_align.rs`.

### 7.1 Pipeline Overview

1. **Debayer-first.** Each frame is debayered (if Bayer) to RGB before
   luminance extraction, rather than reversing a raw Bayer buffer —
   this avoids a Bayer-pattern mismatch that a raw-buffer flip would
   introduce.
2. **Rotational grouping.** Frames are grouped by `ROTATOR` keyword
   and imaging-session continuity. A new group starts when either:
   - the rotator changes by more than `MERIDIAN_FLIP_THRESHOLD` (90°)
     between consecutive frames, regardless of time gap, **or**
   - the time gap exceeds `SESSION_GAP_MINUTES` (120 min) **and** the
     rotator has also changed by more than `ROTATOR_GROUP_TOLERANCE`
     (10°)

   A time gap alone, with an unchanged rotator, does not start a new
   group.
3. **Master group.** The largest group by frame count is the master
   group. Its best-quality frame (highest `frame_quality_score()` =
   `1/FWHM + (1 − eccentricity)`) becomes the master reference for the
   whole stack. This is the same shared quality function `AnalyzeFrames`
   uses to select the session's displayed reference frame (§3.9, §6.2)
   — one definition of "best frame" for both, restricted to PASS
   frames on the `AnalyzeFrames` side (StackFrames has no PASS/REJECT
   concept of its own to restrict against).
4. **Per-group reference.** Every group — master or not — selects its
   own best-quality frame as a group reference. Frames align natively
   to their own group's reference, avoiding a per-frame buffer
   reversal and its associated Bayer-pattern issues.
5. **Cross-group solve (`M_cross`).** For each non-master group, one
   transform is solved that maps that group's reference into master
   coordinates: an explicit 180° pre-rotation
   (`AffineRigid::flip_180`) composed with a triangle-based rigid
   match (`estimate_rigid_transform_triangles`) between the flipped
   group reference and the master reference. If triangle matching
   fails, this falls back to FFT-translation-only. `M_cross` is solved
   once per group, not once per frame.
6. **M_cross verification is logging-only.** After each solve,
   group-reference stars are transformed by `M_cross` and matched
   against master-reference stars within 10px; mean/max residual is
   logged. This does not gate acceptance — a poor residual is visible
   in logs but does not cause the group to be excluded or retried.
7. **Per-frame transform.** `T = compose(M_cross, G)`, where `G` is
   the within-group transform (FFT phase correlation, optionally
   refined by RANSAC via `estimate_rigid_transform`). For master-group
   frames, `M_cross` is identity, so `T = G`. Resampling uses the
   affine resampler (`resample_frame_affine` /
   `resample_frame_rgb_affine`) when `|θ| ≥ 0.001 rad` or `a < 0.5`
   (near-180°-flip scale sign); otherwise the faster translation-only
   resampler is used.
8. **Color awareness.** If the master reference is Bayer or RGB, all
   three channels are accumulated and the output is `ColorSpace::RGB`;
   a mono master reference produces grayscale output.

### 7.2 Alignment Primitives

**FFT phase correlation** (`fft_align::compute_translation`) — both
frames are downsampled to ≤1024px on the long axis, apodized with a 2D
Hann window, cross-correlated in the frequency domain via normalized
cross-power spectrum, and refined to sub-pixel accuracy via 2D
parabolic interpolation around the correlation peak. Returns `None` on
empty input or a degenerate peak.

**Star-based rigid refinement** — two strategies, both producing an
`AffineRigid` (rotation + translation, scale fixed at 1.0; no
assumption about rotation center — the center is implicit in the
solved translation):

- `estimate_rigid_transform()` — FFT-primed RANSAC. Pre-translates
  frame stars by the FFT offset, greedy nearest-neighbor matching
  within `MATCH_TOLERANCE` (15px), then 50 RANSAC iterations with
  `INLIER_TOLERANCE` (2px), followed by least-squares refinement over
  the inlier set. Sanity checks reject results with rotation beyond
  `MAX_ROTATION_RAD` (~30°) or translation beyond
  `MAX_TRANSLATION_DEVIATION` (20px). Used for within-group per-frame
  alignment.
- `estimate_rigid_transform_triangles()` — scale-invariant triangle
  matching, no FFT pre-translation required. Builds descriptors from
  the `TRI_MAX_STARS` (30) brightest stars, matches by descriptor
  distance (`TRI_DESC_TOLERANCE` = 0.02) with matching triangle
  orientation required, votes on the implied transform in binned `(tx,
  ty, θ)` space, and returns the winning voted transform directly — no
  least-squares refinement, since that's numerically unstable with
  centroids far from the origin. Requires at least `TRI_MIN_INLIERS`
  (6) inliers under the winning transform to accept. Used exclusively
  for the cross-group `M_cross` solve.

### 7.3 Combination — Two-Pass Sigma-Clipped Mean

**Pass 1 (Welford online mean/variance):** for every included frame,
pixels are normalized by that frame's background median (via
`estimate_background`), resampled into alignment, then folded into a
running per-pixel mean and M2 (Welford's algorithm) — avoiding the
need to hold all aligned frames in memory simultaneously. Frames are
excluded from Pass 1 (and the stack entirely) on filter mismatch
against the master reference's filter, or if FFT alignment fails
outright.

**Pass 2 (sigma-clipped accumulation):** processed in chunks of
`rayon_thread_count` frames at a time — pixel loading/debayering is
sequential per chunk, background estimation and resampling are
parallelized within the chunk, and accumulation into the running sum
is sequential. A pixel is accepted into the final sum if it falls
within `2.5σ` of the Pass 1 per-pixel mean (using the luma channel's
deviation to gate all three RGB channels together, when color). The
batched-chunk approach bounds peak Pass 2 memory to roughly one batch
of aligned frames rather than the whole session.

**Output:** the per-pixel mean of accepted values, normalized
(`normalize_output`), stored as a transient `ImageBuffer` in
`ctx.stack_result` — no source file path, since it isn't backed by a
file until explicitly written out. `ctx.stack_summary` and
`ctx.stack_contributions` carry per-run and per-frame metrics
respectively (SNR improvement estimate, alignment success rate,
background uniformity, exclusion reasons).

### 7.4 Known Limitation

`validate_alignment()` — a match-rate sanity check comparing predicted
vs. actual star positions — exists in the source but is not called
anywhere in the stacking pipeline. All frames that survive the earlier
FFT/RANSAC/triangle-matching stages are currently accepted without
this additional validation pass.

---

## 8. Persistence & Settings

### 8.1 Storage Strategy

All persistence uses a single embedded SQLite database at
`APPDATA/Photyx/photyx.db` (`~/.local/share/Photyx/` on Linux). SQLite
is statically linked via `rusqlite` (`bundled` feature) — no external
dependencies, no service, just a file.

**What is NOT in SQLite:**

| Data                  | Location                                | Reason                                |
| ----------------------- | ------------------------------------------ | ---------------------------------------- |
| Application logs       | Rolling files via tracing-appender         | Log infrastructure already correct       |
| Image pixel data        | In-memory `AppContext.image_buffers`       | Too large; ephemeral by design           |
| Display/blink caches    | In-memory                                  | Ephemeral; rebuilt on load                |
| STF parameters          | In-memory `AppContext.last_stf_params`     | Session-scoped; recalculated on load      |
| PXFLAG keyword          | Written to image file headers              | Results must travel with the file         |

**Rust conventions:** `PRAGMA journal_mode=WAL` on open (concurrent
reads while Rust writes); `PRAGMA foreign_keys=ON`; migrations via
`PRAGMA user_version`. `db::now_unix()` in `db/mod.rs` is the single
source of truth for Unix timestamps. The `backup` rusqlite feature
must remain enabled — required by
`backup_database`. `restore_database` checkpoints WAL before writing,
deletes WAL/SHM after writing, and reopens the connection in-place —
no app restart required.

**Frontend conventions:** all database access goes through Tauri
commands — the frontend never holds a connection. `db.ts` wraps all
Tauri command calls; components never call `invoke` for DB operations
directly.

### 8.2 Database Schema

All tables below are created via `IF NOT EXISTS` in `src-tauri/src/db/schema.rs`.

```sql
CREATE TABLE IF NOT EXISTS preferences (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS quick_launch_buttons (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    position    INTEGER NOT NULL,
    label       TEXT NOT NULL,
    script      TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS recent_directories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,
    last_used   INTEGER NOT NULL,
    use_count   INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS threshold_profiles (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    name                        TEXT NOT NULL UNIQUE,
    description                 TEXT,
    bg_median_reject_sigma      REAL NOT NULL DEFAULT 2.5,
    signal_weight_reject_sigma  REAL NOT NULL DEFAULT 2.5,
    fwhm_reject_sigma           REAL NOT NULL DEFAULT 2.5,
    star_count_reject_sigma     REAL NOT NULL DEFAULT 1.5,
    eccentricity_reject_abs     REAL NOT NULL DEFAULT 0.85,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS algorithm_sets (
    version                         INTEGER PRIMARY KEY,
    bg_algorithm_version            TEXT NOT NULL,
    snr_algorithm_version           TEXT NOT NULL,
    fwhm_algorithm_version          TEXT NOT NULL,
    eccentricity_algorithm_version  TEXT NOT NULL,
    star_count_algorithm_version    TEXT NOT NULL,
    released_at                     INTEGER NOT NULL,
    notes                           TEXT
);

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
);
CREATE INDEX IF NOT EXISTS idx_far_path ON frame_analysis_results(file_path);
CREATE INDEX IF NOT EXISTS idx_far_version ON frame_analysis_results(algorithm_set_version);

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
);

CREATE TABLE IF NOT EXISTS macro_versions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    macro_id    INTEGER NOT NULL REFERENCES macros(id) ON DELETE CASCADE,
    script      TEXT NOT NULL,
    saved_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mv_macro ON macro_versions(macro_id, saved_at DESC);

CREATE TABLE IF NOT EXISTS session_history (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    directory       TEXT NOT NULL,
    opened_at       INTEGER NOT NULL,
    closed_at       INTEGER,
    file_count      INTEGER,
    commands_run    INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS console_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    executed_at INTEGER NOT NULL,
    command     TEXT NOT NULL,
    output      TEXT,
    success     INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS crash_recovery (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    file_list           TEXT,
    current_frame_index INTEGER,
    autostretch_enabled INTEGER,
    zoom_level          TEXT,
    active_panel        TEXT,
    written_at          INTEGER NOT NULL
);
INSERT OR IGNORE INTO crash_recovery (id, written_at) VALUES (1, 0);
```

**Note on `frame_analysis_results`:** `bg_stddev`, `bg_gradient`, and
`snr_estimate` are stored but do not drive classification — Background
Std Dev and Background Gradient were dropped as rejection metrics
(highly correlated with Background Median), and SNR is retained as a
diagnostic value only. The corresponding pcode commands
(`BackgroundStdDev`, `BackgroundGradient`) remain as deprecated stubs
for script compatibility.

**Note on orphaned columns:** earlier versions of `threshold_profiles`
included `bg_stddev_reject_sigma` and `bg_gradient_reject_sigma`. They
are absent from the canonical schema above, so any fresh install won't
have them — but since `CREATE TABLE IF NOT EXISTS` never alters an
existing table, a database created before this cleanup may still carry
those two unused columns. Rust code ignores them either way.

**Note on threshold default consistency — confirmed via
`defaults.rs`:** `DEFAULT_STAR_COUNT_SIGMA = 1.5` in
`settings/defaults.rs`, which states explicitly in its header comment
that it is *"the single source of truth. No magic numbers or default
strings anywhere else."* The DB column default
(`star_count_reject_sigma REAL NOT NULL DEFAULT 1.5`) matches this
exactly, and `AnalysisThresholds::default()` in
`analysis/session_stats.rs` correctly sources its `star_count` value
from `DEFAULT_STAR_COUNT_SIGMA` rather than hardcoding a literal —
confirmed fixed (issue #67). One remaining discrepancy: `defaults.rs`
bounds `star_count` to `STAR_COUNT_SIGMA_MIN`/`MAX` of `0.5`–`4.0`,
while §8.5 below documents the bound as `0.5σ`–`5.0σ`. Worth
reconciling.

### 8.3 Preferences

The `preferences` table is a flat key/value store. The `AppSettings`
Rust struct (`src-tauri/src/settings/mod.rs`) is the in-memory mirror
— populated at startup, all reads from memory, writes go to both
struct and DB via `save_preference()`. Defaults and bounds are
constants in `src-tauri/src/settings/defaults.rs`; bounds are enforced
in `AppSettings` on read (the DB stores raw values), which lets bounds
change without a schema migration.

**Settings never persisted** (always reset to hard-coded default at
startup):

- AutoStretch enabled (always off)
- Overwrite behavior (always Prompt)
- Format filter selection (always All Supported)
- Rayon thread count (always `num_cpus - 1`)
- Blink pre-cache frames (always all loaded frames)
- Default zoom level, blink rate, channel view

### 8.4 Settings Reference

| Setting                  | Default          | Persisted | User Pref | DB Key                     | Min    | Max   |
| --------------------------- | ------------------ | ----------- | ----------- | ----------------------------- | -------- | ------- |
| Color theme                | Matrix              | X           |             | `theme`                      | —       | —      |
| JPEG quality                | 75%                 | X           | X           | `jpeg_quality`                | 1       | 100    |
| Recent directories max      | 10                  | X           | X           | `recent_directories_max`      | 1       | 50     |
| Backup directory            | Downloads folder    | X           | X           | `backup_directory`            | —       | —      |
| Console history size        | 500                 | X           | X           | `console_history_size`        | 100     | 5000   |
| Macro editor font size      | 13px                | X           | X           | `macro_editor_font_size`      | 8       | 24     |
| Buffer pool memory limit    | 4 GB                | X           | X           | `buffer_pool_memory_limit`    | 512 MB  | 32 GB  |
| Shadow clip (AutoStretch)   | -2.8                | X           | X           | `autostretch_shadow_clip`     | -5.0    | 0.0    |
| Target background (AutoStretch) | 0.15            | X           | X           | `autostretch_target_bg`       | 0.01    | 0.50   |
| Crash recovery interval     | 60s                 | X           | (internal)  | `crash_recovery_interval_secs`| 15      | 300    |
| Active threshold profile ID | null                | X           | (internal)  | `active_threshold_profile_id` | —       | —      |

Not persisted (always hard-coded default): Default zoom level (Fit),
default blink rate (0.1s/frame), default channel view (RGB), overwrite
behavior (Prompt), AutoStretch enabled (off), blink pre-cache (all
loaded), Rayon thread count (`num_cpus - 1`).

Quick Launch button assignments are stored in `quick_launch_buttons`,
not in `preferences` — the user can pin as many macros as desired;
buttons wrap to the next row automatically.

### 8.5 Threshold Profiles

Named sets of AnalyzeFrames rejection thresholds, stored in
`threshold_profiles`; the active profile is tracked by
`preferences.active_threshold_profile_id`.

| Metric | Direction | Default | Min | Max |
| ------------------------ | ----------- | --------- | ------- | ------- |
| Background Median reject | `> +σ` | 2.5σ | 0.5σ | 4.0σ |
| FWHM reject | `> +σ` | 2.5σ | 0.5σ | 4.0σ |
| Eccentricity reject | `> abs` | 0.85 | 0.10 | 1.00 |
| Star Count reject | `< σ` | 1.5σ | 0.5σ | 4.0σ |

Star Count uses bimodal-aware anchoring — the 1.5σ threshold is
relative to the clear-sky upper cluster, not the full mixed
population, so a cloud-induced population split doesn't distort the
threshold. Note that the recommended default for Star Count reject for
duo-band frames is 1.75σ.

**Business logic:**

- Default profile name is "Default" (not "Standard").
- All thresholds are stored and displayed as positive values
  regardless of metric direction; negation for `<σ` metrics (Star
  Count.
- Values are clamped to bounds on save.
- `set_active_threshold_profile` propagates thresholds into
  `AppContext.analysis_thresholds` immediately, and also updates
  `AppContext.last_analysis_thresholds` to match — an explicit active-
  profile change (via Edit > Analysis Parameters OK/Apply) is treated
  as a deliberate re-baseline. `AppContext.last_analysis_thresholds`
  otherwise holds the thresholds actually used in the last
  `AnalyzeFrames` run, returned as `applied_thresholds` by
  `get_analysis_results` (§6.8) — the Analysis Graph draws reject
  lines from this, not from the current active profile, so switching
  profiles doesn't retroactively redraw a run made under different
  thresholds unless done explicitly through this command.
- Deleting a profile — including the last one — is allowed; deleting
  the last profile re-seeds a "Default" profile.

---

## 9. File Format Support

### 9.1 Supported Formats

| Format                 | Read | Write | Keywords                                |
| ------------------------- | ------ | ------- | ------------------------------------------ |
| FITS (.fit/.fits/.fts)  | ✓    | ✓     | Full                                        |
| XISF (.xisf)             | ✓    | ✓     | Full (FITSKeyword + Properties blocks)      |
| TIFF (.tif/.tiff)       | ✓    | ✓     | AstroTIFF convention                         |
| PNG (.png)               | ✓    | ✓     | None                                          |
| JPEG (.jpg/.jpeg)        | ✓    | ✓     | None                                          |

All format reading is consolidated in `plugins/image_reader.rs` —
`read_image_file(path)` dispatches to a format-specific reader by
extension (`read_fits_file`, `read_xisf_file`,
`read_tiff_file`). `peek_*_dimensions()` variants read header
dimensions only, without pixel data, and are used by `AddFiles` for
memory-limit estimation before loading.

### 9.2 Read Support Detail

| Format                 | Notes                                                  |
| ------------------------- | --------------------------------------------------------- |
| FITS (.fit/.fits/.fts)  | Via `fitsio`/cfitsio; sequential loading only (parallel loading crashes — thread-safety issue, see §14) |
| XISF (.xisf)             | Via the `photyx-xisf` crate; supports LZ4, LZ4HC, zstd, zlib compression |
| TIFF (.tif/.tiff)       | U8, U16, U32→U16, F32; AstroTIFF keyword round-trip     |
| PNG (.png)               | Viewing and format conversion only; no keyword support   |
| JPEG (.jpg/.jpeg)        | Viewing and format conversion only; no keyword support   |

### 9.3 Write Support Detail

| Format             | Notes                                                     |
| --------------------- | -------------------------------------------------------------- |
| FITS (.fit/.fits)   | Full keyword support; `BZERO`/`BSCALE` for unsigned 16-bit (see §9.6) |
| XISF (.xisf)         | Dual-write to both the FITSKeyword block and the Properties block |
| TIFF (.tif/.tiff)   | AstroTIFF keyword embedding in the `ImageDescription` tag         |
| PNG (.png)           | 16-bit support                                                   |
| JPEG (.jpg)          | 8-bit; quality configurable, default 75% (`jpeg_quality` preference, §8.4) |

All write operations use atomic temp-file-then-rename to protect
against partial writes on failure.

### 9.4 Internal Pixel Format

- Bit depths: 8-bit integer, 16-bit integer, 32-bit float
- Color modes: Monochrome (1 channel), RGB (3 channel)
- U32 data is downconverted to U16 on load (high 16 bits retained)
- CFA (Bayer) files load and display as mono by default; debayering is
  on-demand via `DebayerImage` — supported algorithms: Nearest
  Neighbor, Bilinear (default), VNG, AHD

### 9.5 Format Conversion

No dedicated conversion layer — format conversion is simply a
read-plugin followed by a write-plugin. Any readable format can be
converted to any writable format via pcode.

### 9.6 FITS Signed 16-bit Convention

FITS `BITPIX=16` is a signed format. Photyx subtracts 32768 from
unsigned 16-bit pixel values before casting to `i16` for the write,
and sets `BZERO=32768` / `BSCALE=1` in the header so readers
reconstruct the original unsigned values. FITS stores color images as
planar `[R, G, B]` planes — these must be re-interleaved on read.

### 9.7 FITS ↔ XISF Keyword Mapping

When converting FITS to XISF, all FITS keywords are written verbatim
into the FITSKeyword block. Keywords with a known XISF Property
equivalent are additionally written into the Properties block:

| FITS Keyword | XISF Property                          |
| --------------- | ------------------------------------------ |
| OBJECT         | Observation:Object:Name                   |
| TELESCOP       | Instrument:Telescope:Name                 |
| INSTRUME       | Instrument:Camera:Name                    |
| EXPTIME        | Observation:Time:ExposureTime             |
| FILTER         | Instrument:Filter:Name                    |
| GAIN           | Instrument:Camera:Gain                    |
| TEMP           | Instrument:Camera:Temperature             |
| DATE-OBS       | Observation:Time:Start                    |
| RA             | Observation:Object:RA                     |
| DEC            | Observation:Object:Dec                    |
| CRVAL1         | Observation:Center:RA                     |
| CRVAL2         | Observation:Center:Dec                    |
| RADESYS        | Observation:CelestialReferenceSystem      |
| EQUINOX        | Observation:Equinox                       |
| SITELAT        | Observation:Location:Latitude             |
| SITELONG       | Observation:Location:Longitude            |
| SITEELEV       | Observation:Location:Elevation            |
| XBINNING       | Instrument:Camera:XBinning                |
| YBINNING       | Instrument:Camera:YBinning                |
| FOCALLEN       | Instrument:Telescope:FocalLength          |
| IMAGETYP       | Observation:Image:Type                    |

WCS transformation keywords (`CRPIX1/2`, `CD1_1`, `CD1_2`, `CD2_1`,
`CD2_2`, `CDELT1/2`, `CROTA1/2`, `LONPOLE`, `LATPOLE`, `PV1_*`, all PC
matrix keywords) have no XISF Property equivalent and are preserved
verbatim in the FITSKeyword block only.

### 9.8 Keyword Support by Format

| Format | Read Keywords | Write Keywords | Notes                                     |
| -------- | ---------------- | ------------------ | ---------------------------------------------- |
| FITS   | ✓               | ✓                 | Full FITS header                                |
| XISF   | ✓               | ✓                 | Both FITSKeyword and Properties blocks          |
| TIFF   | ✓               | ✓                 | AstroTIFF convention (`ImageDescription`)       |
| PNG    | ✗               | ✗                 | —                                                |
| JPEG   | ✗               | ✗                 | —                                                |

---

## 10. Tauri Commands Reference

One canonical table, merged from the two overlapping command lists in
the source documents (each was missing a handful of commands the other
had) plus `get_progress`/`get_job_result`, confirmed present in
`progress.ts` (§2.7) but absent from both prior tables.

| Command                            | Description                                                                                                     |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `backup_database`                   | Creates a timestamped ZIP backup of `photyx.db` in the configured backup directory                                |
| `check_crash_recovery`              | Returns crash recovery candidate if `written_at` is recent and a session is open (file list + current frame index) |
| `close_session`                     | Sets `closed_at` on the current `session_history` row; resets `is_imported_session`                                |
| `commit_analysis_results`           | Moves REJECT files to `rejected/` subfolders; removes them from the session; pass frames remain loaded. Fast, non-terminal (§6.7) |
| `debug_buffer_info`                 | Returns buffer metadata including `display_width` and `color_space`                                               |
| `delete_macro`                      | Deletes a macro and its version history from the database                                                          |
| `delete_threshold_profile`          | Deletes a threshold profile by id; re-seeds "Default" if the last one is deleted; updates active id if needed      |
| `dispatch_command`                  | Dispatches a single pcode command to the plugin registry (legacy interactive path)                                 |
| `get_active_threshold_profile_id`   | Returns the active threshold profile id                                                                             |
| `get_all_preferences`               | Returns all preferences as a key/value map; called at startup to hydrate the frontend                              |
| `get_analysis_results`              | Reclassifies frames (skipped for imported sessions); returns frames, session stats, outliers, `is_imported` (§6.8) |
| `get_autostretch_frame`             | Computes Auto-STF stretch on the current frame, returns JPEG data URL; does not cache                                |
| `get_autostretch_stack_frame`       | Computes Auto-STF stretch on the current stack result, returns JPEG data URL — the Phase B display path for StackFrames output |
| `get_blink_cache_status`            | Returns blink cache build status: idle / building / ready                                                            |
| `get_blink_frame`                   | Returns a blink frame as JPEG data URL from the blink cache (by index + resolution)                                |
| `get_current_frame`                 | Returns the current image as a raw (unstretched) JPEG data URL, rendered on the fly                                |
| `get_frame_flags`                   | Returns PXFLAG values for all loaded frames (used by the blink overlay)                                            |
| `get_full_frame`                    | Returns the current image at full resolution with the last STF params applied; cached after first call            |
| `get_histogram`                     | Computes histogram bins + stats for the current frame (per-channel for RGB)                                        |
| `get_job_result`                    | Returns the `JobResult` of the most recently completed script/command dispatch, or `null`; polled every 500ms (§2.7) |
| `get_keywords`                      | Returns all keywords for the current frame as a keyed map                                                          |
| `get_macro_versions`                | Returns version history for a macro, newest first                                                                   |
| `get_macros`                        | Returns all macros with name, display_name, script, run_count, last_run_at                                        |
| `get_pixel`                         | Returns raw pixel value(s) at source coordinates from the raw image buffer                                        |
| `get_progress`                      | Returns the current `[label, current, total]` progress tuple; polled every 500ms (§2.7)                            |
| `get_quick_launch_buttons`          | Returns the ordered list of Quick Launch button assignments                                                        |
| `get_session`                       | Returns current session state (file list, current frame) — no active-directory field                                |
| `get_stack_frame`                   | Returns the current stack result as a display-resolution JPEG data URL, linearly auto-scaled to the buffer's actual min/max pixel range (as opposed to get_autostretch_stack_frame's STF stretch); used by StackingWorkspace.svelte for a raw, unstretched preview |
| `get_star_positions`                | Re-runs star detection on the current frame; returns per-star `{cx, cy, fwhm, r}` for the annotation overlay         |
| `get_threshold_profiles`            | Returns all threshold profiles from `AppSettings`                                                                  |
| `get_variable`                      | Returns a pcode variable value from `ctx.variables` by name                                                        |
| `increment_macro_run_count`         | Updates `run_count` and `last_run_at` for a macro after successful execution                                       |
| `list_log_files`                    | Lists available log files, sorted newest first                                                                     |
| `list_plugins`                      | Returns the list of registered plugins with name, version, and type                                                |
| `load_analysis_json`                | Clears the session; populates analysis state from a JSON payload; sets `is_imported_session = true`                |
| `load_file`                         | Reads a single image file from disk, injects it into the session, returns a JPEG data URL                          |
| `open_session`                      | Inserts a `session_history` row with `closed_at = NULL`; returns the session id                                    |
| `read_log_file`                     | Reads and parses a log file into structured `{timestamp, level, module, message}` lines                            |
| `rename_macro`                      | Renames a macro; validates name uniqueness                                                                          |
| `restore_database`                  | Restores `photyx.db` from a ZIP backup; reopens the connection in-place, no app restart required                    |
| `restore_macro_version`             | Restores a previous macro version as the current script                                                            |
| `run_script`                        | Executes a pcode script string; the initiating call returns immediately — see §2.7 for how the result is retrieved |
| `save_macro`                        | Inserts or updates a macro; saves the previous version to `macro_versions` before overwriting                      |
| `save_quick_launch_buttons`         | Replaces all Quick Launch button assignments                                                                        |
| `save_threshold_profile`            | Inserts or updates a threshold profile; clamps all values to bounds                                                 |
| `set_active_threshold_profile`      | Sets the active profile; propagates thresholds into `AppContext` immediately                                       |
| `set_frame_flag`                    | Updates the PASS/REJECT flag for a single frame in `ctx.analysis_results` by path; used before Commit to sync toggled flags |
| `set_preference`                    | Upserts a single preference key/value; writes through the `AppSettings` struct                                     |
| `start_background_cache`            | Spawns a background task that builds display-resolution JPEGs and both blink caches, snapshotting pixel data in chunks via `pixel_chunking` with a short `AppContext` lock per chunk (§2.2) |
| `write_crash_recovery`              | Upserts the single `crash_recovery` row with current session state (file list, current frame)                      |

---

## 11. Plugin Reference

All plugins listed here are built-in native Rust, fully implemented
and shipped. The plugin framework supports WASM user plugins via
Wasmtime (§2.4), but none ship by default. Not every pcode command is
a plugin — `Set` and `pwd` are handled directly by the interpreter
rather than registered in the plugin registry. Descriptions for each
are in §4's command dictionary; this table exists to confirm current
implementation status and group plugins by category.

| Plugin | Category |
| ------------------------ | ------------------- |
| AddFiles | Session |
| AddKeyword | Keyword |
| AnalyzeFrames | Frame Analysis |
| Assert | Scripting |
| AutoStretch | Processing |
| CacheFrames | Blink & View |
| ClearAnnotations | Display |
| ClearSession | Session |
| ClearStack | Stacking |
| CommitAnalysis | Frame Analysis |
| CommitStretch | Stacking |
| ComputeEccentricity | Analysis |
| ComputeFWHM | Analysis |
| ContourHeatmap | Analysis |
| CopyFile | File Management |
| CopyKeyword | Keyword |
| CountFiles | Scripting |
| CountStars | Analysis |
| DebayerImage | Processing |
| DeleteKeyword | Keyword |
| ExportAnalysisReport | Frame Analysis |
| FilterByKeyword | File Management |
| GetHistogram | Processing |
| GetKeyword | Interrogation |
| ListKeywords | Keyword |
| LoadFile | File Management |
| ModifyKeyword | Keyword |
| MoveFile | File Management |
| ReadImages | Session |
| RejectCurrentFrame | Session |
| RunMacro | Scripting |
| SetFrame | Navigation |
| ShowAnalysisGraph | Display |
| ShowAnalysisResults | Display |
| StackFrames | Stacking |
| WriteCurrent | I/O Writer |
| WriteFIT | I/O Writer |
| WriteFrame | I/O Writer |
| WriteTIFF | I/O Writer |
| WriteXISF | I/O Writer |

Not currently implemented as plugins, despite appearing in earlier
planning documents: `CropImage`, `GetImageProperty`,
`GetSessionProperty`, `ListFiles`, `Test` (see §4.2, Retired
Commands).

---

## 12. Frontend State Reference

Stores live in `src-svelte/lib/stores/` (full list in
§2.1). Field-level detail below covers `ui.ts` and `session.ts`;
`progress.ts` is documented in full in §2.7. `consoleHistory.ts`,
`notifications.ts`, `quickLaunch.ts`, `settings.ts`, and
`thresholdProfiles.ts` exist but aren't broken out field-by-field
here.

### 12.1 UI State Store (`ui.ts`)

| Field | Purpose |
| --------------------------- | ---------------------------------------------------------------- |
| `aboutOpen` | Whether the About modal is open |
| `activePanel` | Currently open sidebar panel |
| `activeView` | Currently active viewer-region view (`null` = image viewer) |
| `analysisParametersOpen` | Whether the Analysis Parameters dialog is open |
| `annotationToken` | Positive = show annotations, negative = clear annotations |
| `autostretchImageUrl` | Data URL of the AutoStretch result |
| `blinkCached` | Whether the blink cache has been built |
| `blinkCaching` | Whether a blink cache build is in progress |
| `blinkImageUrl` | Current blink frame data URL |
| `blinkModeActive` | Whether the viewer is in blink display mode |
| `blinkPlaying` | Whether blink is actively playing |
| `blinkResolution` | Currently selected blink resolution (`'12'` or `'25'`) |
| `blinkTabActive` | Whether the Blink tab is currently selected |
| `consoleExpanded` | Whether the console is expanded |
| `currentBlinkFlag` | PXFLAG value for the currently displayed blink frame |
| `displayImageUrl` | Data URL of a temporary display image |
| `frameRefreshToken` | Incremented to trigger a viewer frame reload |
| `keywordModalOpen` | Whether the keyword modal is open |
| `logViewerOpen` | Whether the Log Viewer modal is open |
| `macroEditorFile` | File currently open in the Macro Editor |
| `preferencesOpen` | Whether the Preferences dialog is open |
| `quickLaunchVisible` | Whether the Quick Launch bar is visible |
| `showQualityFlags` | Whether PXFLAG reject borders are shown during blink |
| `theme` | Active theme (dark / light / matrix) |
| `viewerClearToken` | Incremented to clear the viewer and restore the starfield |
| `zoomLevel` | Current zoom level |

### 12.2 Session Store (`session.ts`)

| Field / Derived | Purpose |
| ------------------------------ | ---------------------------------------------------------------- |
| `fileList` | Ordered list of full file paths in the current session |
| `loadedImages` | Record of image metadata keyed by file path |
| `currentFrame` | Zero-based index of the currently displayed frame |
| `variables` | pcode variable store (mirrors `ctx.variables`) |
| `fileCount` (derived) | `fileList.length` |
| `directoryCount` (derived) | Number of unique parent directories in `fileList` |
| `currentImage` (derived) | `loadedImages[fileList[currentFrame]]` |

There is no `activeDirectory` field — directory information is always
derived from `fileList` (§2.3).

---

## 13. Path Conventions

| Convention | Rule |
| -------------------- | ------------------------------------------------------------------------------------- |
| Separator | Forward slash `/` always in pcode and stored paths; backend translates to OS-native before filesystem calls |
| Absolute paths | `D:/Astrophotos/M31` (Windows) or `/home/user/photos` (macOS/Linux) |
| Relative paths | Resolved against `common_parent()` of the current file list (§2.3) |
| Home shorthand | `~` expands to the current user's home directory on all platforms |
| UNC paths | `//192.168.1.100/Astrophotos/M31` — useful for ASIAir Pro over a local network |
| Spaces in paths | Must be enclosed in double quotes |

---

## 14. Known Issues

Current bugs and limitations. Not a changelog — items here are
believed still open as of this document.

| Issue                                   | Notes                                                                                          |
| ------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| cfitsio parallel loading crashes          | Thread-safety issue — sequential FITS loading is used instead                                          |
| Blink UI jitter                           | Suspected Tauri WebView compositor artifact on Windows                                                   |
| Full-res frames are JPEG, not lossless    | Disclosed via a disclaimer bar; pixel readout still uses the raw buffer, not the JPEG                     |
| AppContext mutex serializes long operations | A long-running plugin holding `&mut AppContext` blocks all other commands, including frame display, for its duration — see §2.2 |
| Zoom approximate at high levels           | Full-res cache reuses STF params computed at display resolution, not recomputed at full res — see §2.2   |
| XISF Vector/Matrix properties             | Read as a placeholder; skipped on write                                                                    |
| Rayon thread count not user-configurable  | Hardcoded to `num_cpus - 1`; not exposed as a preference despite `RAYON_THREAD_COUNT_MIN` existing in defaults |
| Sidebar icon tooltips clipped by Quick Launch | CSS stacking context issue                                                                              |
| Single-file-load blink isolation          | Files loaded via `LoadFile` are included in `ctx.file_list`, not kept separate                             |
| AutoStretch performance in dev builds     | 3–5 seconds for a 9MP RGB frame in debug builds; near-instant in release builds                             |
| AutoStretch lost on Blink→Pixels tab switch | Viewer reverts to raw unstretched display                                                                 |
| SNR estimator PSF artifact                | Worse-seeing frames produce higher SNR due to bloated star flux; excluded from rejection classification — see §6.2 |
| AnalyzeFrames progress reporting          | Documented as having no per-frame progress reporting (unlike StackFrames, which does — §2.7); not independently re-verified against AnalyzeFrames source in this pass |
| `threshold_profiles` orphaned columns     | `bg_stddev_reject_sigma`/`bg_gradient_reject_sigma` may still exist on pre-cleanup databases — see §8.2   |
| `validate_alignment()` unused in StackFrames | Defined but never called; all frames pass without this validation step — see §7.4                       |
| Full-res JPEG quality documented as 90, but no matching constant found | `defaults.rs` defines `DISPLAY_JPEG_QUALITY` (92) and `BLINK_JPEG_QUALITY` (85) but no full-res-specific constant — see §2.2, §9.3 |
| Linux GTK file picker multi-select        | Silently refuses to confirm a selection containing both files and folders (e.g. Ctrl+A when a `rejected/` subfolder is present) — select files manually instead |
| Separate RGB channel views not working correctly | Pre-existing display bug                                                                          |
| `TRI_MAX_STARS = 30` unvalidated on sparse-star sessions | Current value works for typical sessions; not yet confirmed as a safe floor for sparse-star fields — see §7.2 |
