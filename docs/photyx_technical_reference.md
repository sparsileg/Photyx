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

```
Photyx/
├── src-svelte/                ← Svelte 5 frontend
│   ├── lib/
│   │   ├── clientCommands.ts  ← Client-only command dispatch (Console, MacroLibrary, QuickLaunch — Issue 98)
│   │   ├── commands.ts        ← Shared backend command helpers
│   │   ├── db.ts              ← Central database access layer (all Tauri DB commands)
│   │   ├── pcode.ts           ← pcode help database, HelpEntry type
│   │   ├── pcode_commands.json ← Single source of truth for all pcode command names
│   │   ├── components/
│   │   │   ├── panels/        ← Sliding panel components (FileBrowser, KeywordEditor, MacroEditor, MacroLibrary, PluginManager)
│   │   │   ├── AboutModal.svelte
│   │   │   ├── AnalysisGraph.svelte
│   │   │   ├── AnalysisResults.svelte
│   │   │   ├── AnalyzeFramesProfileDialog.svelte
│   │   │   ├── Console.svelte
│   │   │   ├── Dropdown.svelte
│   │   │   ├── FeaturePreferencesDialog.svelte  ← Edit > Feature Preferences (§3.14, Issue 130)
│   │   │   ├── HelpModal.svelte
│   │   │   ├── IconSidebar.svelte
│   │   │   ├── InfoPanel.svelte
│   │   │   ├── KeywordModal.svelte
│   │   │   ├── LogViewer.svelte
│   │   │   ├── MenuBar.svelte
│   │   │   ├── PreferencesDialog.svelte
│   │   │   ├── QuickLaunch.svelte
│   │   │   ├── StackingWorkspace.svelte
│   │   │   ├── StatusBar.svelte
│   │   │   ├── ThresholdProfilesDialog.svelte
│   │   │   ├── Toolbar.svelte
│   │   │   └── Viewer.svelte
│   │   ├── settings/constants.ts   ← Frontend mirror of defaults.rs
│   │   └── stores/                 ← analysisToggles, consoleHistory, featureFlags, notifications, progress, quickLaunch, session, settings, thresholdProfiles, ui
│   └── routes/+page.svelte    ← Main application shell
├── src-tauri/                 ← Rust backend
│   └── src/
│       ├── lib.rs
│       ├── main.rs
│       ├── constants.rs
│       ├── logging.rs
│       ├── render.rs
│       ├── utils.rs
│       ├── settings/{defaults.rs, mod.rs}
│       ├── plugin/{mod.rs, registry.rs}
│       ├── context/mod.rs     ← AppContext
│       ├── commands/          ← one file per command domain, incl. feature_flags.rs (Issue 130) — see §10
│       ├── analysis/          ← background, debayer, eccentricity, fft_align, fwhm, metrics, moffat, profiles, session_stats, stack_metrics, star_align, stars
│       ├── pcode/{expr.rs, mod.rs, tokenizer.rs}
│       ├── db/{migrations.rs, mod.rs, schema.rs}
│       └── plugins/           ← one file per plugin (see §11)
├── crates/photyx-xisf/        ← XISF reader/writer crate
└── static/css/                ← one CSS file per major UI module; static/themes/
```

### 2.2 AppContext & Display Cache

Session state lives in a single `AppContext` struct behind a
`Mutex`. Raw pixel buffers are loaded once and never modified; all
display representations are derived JPEG copies:

AppContext
├── image_buffers: HashMap   ← raw pixels, NEVER modified
├── display_cache: HashMap>       ← display-res JPEG bytes (see note below)
├── full_res_cache: HashMap>      ← full-resolution JPEG bytes
├── blink_cache_12: HashMap>      ← blink-res 12.5% JPEG bytes
└── blink_cache_25: HashMap>      ← blink-res 25% JPEG bytes

**`display_cache` is currently dead** (Issue 84, deferred): nothing in
source writes to it. `start_background_cache` computes stretched
display-resolution JPEGs for the whole session — the most expensive
pass it runs — but uses them only as blink-thumbnail sources and
discards the display-resolution output rather than storing it here;
frame navigation re-renders from raw pixels on every request instead of
reading a cached copy. Deferred rather than fixed because Stan
navigates frames via the file browser, not keyboard/arrow stepping, so
the cache-miss cost is low-impact under that usage pattern; revisit if
full-resolution scaled-down frame stepping becomes a workflow.

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
`DETAIL_JPEG_QUALITY = 90` (shared by both the full-resolution cache and
the display-resolution cache), `THUMBNAIL_JPEG_QUALITY = 75` (both blink
caches, 12.5% and 25%).

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

**File** — Load Single Image… · Save Session as FITS · Exit

**Session** — Add Files… (Ctrl+O) · Clear Session

**Edit** — Preferences · Analysis Parameters · Feature Preferences

**View** — Theme: Dark / Light / Matrix

**Analyze** — Analyze Frames · Analysis Results · Analysis Graph ·
Export Analysis Results · Import Analysis Results ·  Stacking Workspace

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
star instead of the normal PASS/REJECT dot, when the
`show_reference_frame_badge` feature flag is enabled (Edit > Feature
Preferences, §3.14) — default off (Issue 130). The star's stroke
color still signals the frame's real classification — black stroke
for PASS, red stroke (`#dc3232`) if the reference frame is itself
REJECT (rare, but possible when the fallback applies). The reference
frame is never hidden or miscategorized by being selected as REF.
With the flag off, the reference frame renders as an ordinary
PASS/REJECT dot, indistinguishable from any other frame in its
category — a deliberate UI-only removal (§7.1's selection logic,
`is_reference`, and the JSON export field are all unchanged);
StackFrames' own console/log output is the authoritative record of
what reference frame(s) it actually used to stack, which is not
always the same frame this dot would mark.

**Legend:** fixed top-left corner of the canvas, always visible,
showing the four rejection categories plus a fifth "Reference frame"
entry only when the `show_reference_frame_badge` flag is on.

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
multi-category = purple), centered. When the
`show_reference_frame_badge` feature flag is enabled (Edit > Feature
Preferences, §3.14 — default off, Issue 130), the session's reference
frame (see §3.9, §7.1) additionally shows a gold ★ badge in the
Category column, alongside its rejection category badge if it has
one. With the flag off (the default), the reference frame's row is
indistinguishable from any other row in its PASS/REJECT category.
Either way, the PXFLAG column always shows the frame's real
PASS/REJECT classification; being selected as reference never
overrides or hides it.

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
Count (`< σ`, 0.5–4.0, default 1.5)

Switching profiles with unsaved edits shows an inline confirmation
bar. OK/Apply saves to DB and sets the profile active (propagated to
`AppContext` immediately). Clicking outside does not close the dialog.

### 3.14 Edit > Feature Preferences

Modal dialog, draft-copy pattern (nothing written until OK/Apply;
Cancel discards), same shape as Edit > Preferences (§3.12) rather than
Edit > Analysis Parameters' multi-profile shape (§3.13) — there is
only one set of flags, not multiple named profiles. One row per entry
in the `FEATURE_FLAGS` registry (`settings/constants.ts`), each a
label and a Yes/No dropdown. Currently one flag:
`show_reference_frame_badge` (default off) — see §3.9, §3.10, Issue
130. Persisted via the `feature_flags` table (§8.2); the backend has
no fixed list of valid keys or seed data, the frontend registry is
authoritative for which flags exist.

### 3.15 Log Viewer

Modal overlay from Tools > Log Viewer. File picker → log content with
ERROR/WARN/INFO/DEBUG level filters. Auto-tail polls every 2 seconds;
auto-scroll suspends when the user scrolls up manually.

### 3.16 Blink Tab

Play/pause/step controls. Resolution dropdown (12.5% / 25%). Min Delay
dropdown. "Highlight Rejected" toggle overlays a red border on REJECT
frames during blink. Cache builds on first play; invalidated when
resolution changes or the file list changes.

### 3.17 Session Analysis JSON Export/Import

**Export** (Session → Export Session JSON…): exports the current
session's analysis results as a portable JSON archive. Default
filename is derived from the first frame — `<target>_<date>_analysis.json`
when both a target name and capture date can be parsed from it,
`<target>_analysis.json` if only the target is found, else
`session.json` — written to the system Downloads folder unless
`path=` is given. Top-level fields: `photyx` (`photyx_version`,
`exported_at`), `thresholds`, `session_stats`, `outliers[]`, and
`frames[]` (per frame: `filename`, `fwhm`, `eccentricity`,
`star_count`, `background_median`, `flag` — the frame's true
PASS/REJECT classification, never collapsed to reflect reference-frame
status — `is_reference`, `triggered_by`, `rejection_category`).
`thresholds` reflects whatever thresholds the exported run actually
used (`last_analysis_thresholds`), falling back to the active profile
only if nothing has been analyzed yet in the session — not necessarily
the profile active at the moment of export.

`filename` and every `outliers[]` entry are basenames, not full paths
— deliberate: an exported report is meant to remain a valid archival
record after a completed project's files are moved out of their
original session directories, and a stored absolute path would go
stale the moment that happens.

**Import** (Session → Import Session JSON…): clears the current
session and loads analysis results from a JSON file — no images are
loaded, display only. `ctx.file_list` is populated directly from each
frame's `filename` (a basename, per the convention above) rather than
a resolvable path; this is safe because an imported session never
loads pixel data, and Commit Results is refused server-side
(`is_imported_session`) for imported sessions regardless of UI state,
so nothing downstream needs those entries to resolve on disk. An
IMPORTED badge appears in the Analysis Results toolbar and Commit
Results is disabled; all other display functionality works normally.
Opens the Analysis Results view automatically on import.

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

Command syntax, arguments, and examples for every pcode command are
documented in the pcode Guide's Command Reference section
(`Photyx_pcode_guide.md`) — that document is the single source of
truth and is not duplicated here. The guide groups commands by
category (Session, Write/Export, Keywords, Analysis, Stacking, Display
& Navigation, Scripting Utilities) in its Table of Contents.


### 4.2 Notes on Specific Entries

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
command) — none of that was ever implemented.

`pwd` (see the pcode Guide's Console Built-ins) also surfaces current
state — the session's unique source directories — but prints directly
to console output rather than storing into a variable, so it isn't an
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

All metrics except Background Median are derived from intensity-weighted
second-order image moments per detected star (`analysis/fwhm.rs`,
`analysis/eccentricity.rs`) — not Moffat PSF fitting. FWHM is
`2.355 × sqrt((Mxx + Myy) / 2)` (the quadratic mean of the two axis
variances); Eccentricity comes from the eigenvalues of the moment matrix
`[[Mxx, Mxy], [Mxy, Myy]]`. Star Count is the count of stars with a
valid moment-based FWHM in the 0.5–50px range — not a Moffat
acceptance gate; detection itself is peak-finding with flood-fill on a
sigma-clipped, background-subtracted image (`analysis/stars.rs`), no
PSF model involved.

An elliptical 2D Moffat PSF fitter (`analysis/moffat.rs`) exists in the
codebase but is entirely `#[allow(dead_code)]` — it was Signal Weight's
only caller before that metric was deprecated, and is retained
intentionally rather than deleted, since its per-star fit (semi-axes,
centroid) could feed a future FWHM/Eccentricity/PSF-residual pass. See
issue #70. It does not run today and does not gate anything described
in this section.

**SNR** is computed and displayed as a diagnostic value only — it does
**not** drive classification. Cross-session analysis confirmed a PSF
artifact: worse-seeing frames produce *higher* SNR due to bloated star
flux, and SNR never uniquely drove a rejection that FWHM or Star Count
didn't already catch.

**Removed metrics:** Background Std Dev (r = 0.92–0.999 correlated
with Background Median) and Background Gradient (sign reversal is
session-dependent) were dropped as rejection metrics. Both
corresponding pcode commands remain as deprecated stubs for script
compatibility. Their values live only in `ctx.analysis_results`
(in-memory, per frame) and the JSON export — `frame_analysis_results`,
the table originally intended to persist them, was never wired up with
a reader or writer and was dropped via migration v5 (§8.2); no database
table persists these two metrics today.

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
2. Every REJECT file is moved to a `rejected/` subfolder with `.reject`
   appended to its filename (e.g. `frame001.fit.reject`) — within *its
   own* source directory, so a multi-directory session produces
   multiple `rejected/` subfolders. The suffix comes from
   `REJECT_FILE_SUFFIX` (`"reject"`, not `"rejected"`); the frontend's
   `commitAnalysis()` passes it explicitly as `commit_analysis_results`'s
   `append` argument.
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
   - the rotator changes by more than `MERIDIAN_FLIP_THRESHOLD` (90°,
     `defaults.rs`) between consecutive frames, regardless of time gap, **or**
   - the time gap exceeds `SESSION_GAP_MINUTES` (120 min, `defaults.rs`)
     **and** the rotator has also changed by more than
     `ROTATOR_GROUP_TOLERANCE` (10°, `defaults.rs`)

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
6. **M_cross verification gates group inclusion (Issue 128/134).**
   After each solve, group-reference stars are transformed by
   `M_cross` and matched against master-reference stars within
   `CROSS_GROUP_VERIFY_MATCH_RADIUS_PX` (10px, `defaults.rs`);
   mean/max residual is logged. A group is now excluded from the stack
   entirely if fewer than `CROSS_GROUP_MIN_MATCHED` stars matched or
   the mean residual exceeds `CROSS_GROUP_MAX_RESIDUAL_PX` (both
   `defaults.rs`, Issue 127/128) — this replaced an earlier
   logging-only version of the check, which recorded a bad solve but
   stacked the group anyway. A companion rotation-plausibility check
   that ran alongside this gate was removed in Issue 134: it assumed a
   180°-flip-only relationship between groups that no longer holds now
   that arbitrary relative orientations are accepted, and a spurious
   match is expected to already fail the residual check on its own.
7. **Per-frame transform.** `T = compose(M_cross, G)`, where `G` is
   the within-group transform (FFT phase correlation, optionally
   refined by RANSAC via `estimate_rigid_transform`). For master-group
   frames, `M_cross` is identity, so `T = G`. Resampling uses the
   affine resampler (`resample_frame_affine` /
   `resample_frame_rgb_affine`) when `|θ| ≥ MIN_ROTATION_TO_APPLY`
   (0.001 rad, local to `stack_frames.rs` — a numerical guard, not a
   tuning knob) or `a < 0.5` (near-180°-flip scale sign); otherwise the
   faster translation-only resampler is used.
8. **Color awareness.** If the master reference is Bayer or RGB, all
   three channels are accumulated and the output is `ColorSpace::RGB`;
   a mono master reference produces grayscale output.

### 7.2 Alignment Primitives

**FFT phase correlation** (`fft_align::compute_translation`) — both
frames are downsampled to ≤ `REG_SIZE` (1024px, local to
`fft_align.rs` — an FFT tractability limit, not a tuning knob) on the
long axis, apodized with a 2D Hann window, cross-correlated in the
frequency domain via normalized cross-power spectrum, and refined to
sub-pixel accuracy via 2D parabolic interpolation around the
correlation peak. Returns `None` on empty input or a degenerate peak.
The cross-power-spectrum peak location is negated before being
returned as the signed translation (Issue 132) — a documented
sign-convention correction, confirmed by test.

**Star-based rigid refinement** — two strategies, both producing an
`AffineRigid` (rotation + translation; scale is a free parameter
rather than fixed at 1.0, to capture real focus/backfocus-driven scale
differences on cross-group solves between sessions — see the
`AffineRigid` struct doc. Within-group fits converge to scale ≈ 1.0
naturally. No assumption about rotation center — the center is
implicit in the solved translation):

- `estimate_rigid_transform()` — FFT-primed RANSAC. Pre-translates
  frame stars by the FFT offset, greedy nearest-neighbor matching
  within `MATCH_TOLERANCE` (15px, `defaults.rs`); requires at least
  `MIN_MATCHES` (4, `defaults.rs`) candidate matches to proceed. Runs
  `RANSAC_ITERATIONS` (50, local to `star_align.rs` — an iteration
  count, not a tuning knob) iterations with `INLIER_TOLERANCE` (2px,
  `defaults.rs`), requiring at least `MIN_INLIERS` (4, `defaults.rs`)
  inliers on the winning hypothesis, followed by least-squares
  refinement over the inlier set. Sanity checks reject results with
  rotation beyond `MAX_ROTATION_RAD` (~30°, `defaults.rs`) or
  translation beyond `MAX_TRANSLATION_DEVIATION` (20px, `defaults.rs`).
  Used for within-group per-frame alignment.
- `estimate_rigid_transform_triangles()` — scale-invariant triangle
  matching, no FFT pre-translation required. Builds descriptors from
  the `TRI_MAX_STARS` (30, `defaults.rs`) brightest stars, matches by
  descriptor distance (`TRI_DESC_TOLERANCE` = 0.02, local to
  `star_align.rs` — a private descriptor-space unit, not a physical
  pixel tolerance) with matching triangle orientation required, votes
  on the implied transform in binned `(tx, ty, θ)` space (bin widths
  `TRI_TX_BIN`/`TRI_TY_BIN`/`TRI_THETA_BIN`, local to `star_align.rs`
  — vote-binning granularity with no meaning outside the voting step),
  and returns the winning voted transform directly — no least-squares
  refinement at the voting stage, since that's numerically unstable
  with centroids far from the origin. Inliers under the winning
  transform are then collected within `TRI_INLIER_TOLERANCE` (3px,
  `defaults.rs`) and refined by least squares; at least
  `TRI_MIN_INLIERS` (6, `defaults.rs`) inliers are required to accept.
  Used exclusively for the cross-group `M_cross` solve.

### 7.3 Combination — Two-Pass Sigma-Clipped Mean

**Pass 1 (Welford online mean/variance):** for every included frame, pixels
are normalized by that frame's background median (via
`estimate_background`), resampled into alignment, then folded into a
running per-pixel mean and M2 (Welford's algorithm) — avoiding the need to
hold all aligned frames in memory simultaneously. Frames are excluded from
Pass 1 (and the stack entirely) on filter mismatch against the master
reference's filter, or if FFT alignment fails outright. Per-pixel standard
deviation is derived from M2 using the unbiased sample form, `M2 / (n − 1)`,
not the population form `M2 / n` (Issue 144) — the population form
systematically underestimates σ, most severely at small frame counts
(~10.6% low at n=5, ~5.1% at n=10, ~1.7% at n=30), which meant a small stack
was effectively clipping tighter than its nominal threshold. The existing
`count > 1` guard (below) already establishes n ≥ 2, so `n − 1` is safe
wherever it's used.

**Known limitation (Issue 144):** the mean and σ used to gate Pass 2 are
computed from every included frame's pixels unconditionally — they are not
recomputed from an outlier-free subset the way
`compute_session_stats_iterative()` does for frame-level analysis (§6.4). A
bright transient (satellite trail, aircraft, cosmic ray hit) on one frame
therefore contributes to the very mean and σ used to judge whether that
transient is an outlier, which can inflate σ enough to pull the transient
inside its own clipping threshold — most visible on small stacks, where one
outlier frame is a larger fraction of the population. This is a deliberate
scope decision, not an oversight: an iterative refinement pass would cost
re-resampling every frame a second time (Pass 2's chunked design
deliberately drops aligned buffers per chunk to bound memory — retaining
them for a refinement pass would scale memory with session size divided by
chunk size, reintroducing the unbounded growth this pipeline was built to
avoid). Photyx's single-method sigma clip is an intentional scope choice
(contrast with e.g. Siril's PERCENTILE/SIGMA/MAD/SIGMEDIAN/WINSORIZED/
LINEARFIT/GESDT rejection methods); this limitation is a known consequence
of that choice, not a bug to fix incidentally.

**Pass 2 (sigma-clipped accumulation):** processed in chunks of
`rayon_thread_count` frames at a time — pixel loading/debayering is
sequential per chunk, background estimation and resampling are parallelized
within the chunk, and accumulation into the running sum is sequential. A
pixel is accepted into the final sum if it falls within `STACK_SIGMA_CLIP`
(2.5σ, `defaults.rs`) of the Pass 1 per-pixel mean (using the luma channel's
deviation to gate all three RGB channels together, when color). The
batched-chunk approach bounds peak Pass 2 memory to roughly one batch of
aligned frames rather than the whole session.

A pixel covered by fewer than two contributing frames has σ = 0 from Pass 1
(the `count > 1` guard above), which Pass 2 treats as "cannot be clipped"
and accepts unconditionally (`sd_luma < 1e-10` fallback) — correct given a
single sample can't be judged against its own spread, but it means that
pixel carries no outlier protection at all. This is most common at frame
edges under significant dither, or whenever the Issue 111 common-overlap
crop (below) degenerates to the full uncropped canvas rather than trimming
low-coverage edges away. The count of such pixels is tracked as
`low_coverage_pixels` on `StackSummary` (Issue 144) and, when nonzero,
surfaced as a line in the printed Stack Quality Summary — silent otherwise,
so a normally-overlapped stack's summary doesn't carry a permanent "0
pixels" line.

Output: the per-pixel mean of accepted values, stretched to the
[0.0, 1.0] display range via normalize_output and stored as a transient
ImageBuffer in ctx.stack_result — no source file path, since it isn't
backed by a file until explicitly written out. This stretch uses the 0.1st
and 99.99th percentile pixel values as its bounds (Issue 145), not the
frame's absolute min/max — a single hot pixel or cosmic ray hit can no
longer single-handedly set the scale and compress the rest of the frame.
The color path applies one global bound across all three channels together,
preserving relative channel ratios (per-channel normalization would destroy
color balance).

ctx.stack_result is a display-normalized preview, not linear data
(Issue 145). Each run's stretch is derived independently from that run's
own pixel population, so two stacks of the same target from different
sessions are not on a comparable scale, and the stack's true background
level (real sky-brightness information, discarded by the stretch's zero
point) is not preserved. Photyx's stacking output is intended as a
quick-look validation step — confirming the data is worth carrying forward
— ahead of real processing in PixInsight from the original frames, not as
an image meant for further downstream processing itself. ctx.stack_summary
and ctx.stack_contributions carry per-run and per-frame metrics
respectively (SNR improvement estimate, alignment success rate, background
uniformity, low-coverage pixel count, exclusion reasons).



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

All tables below are created via IF NOT EXISTS in src-tauri/src/db/schema.rs
and reflect the live, current schema (as of migration v5). schema.rs also
contains four additional CREATE TABLE constants (algorithm_sets,
frame_analysis_results, session_history, console_history) used only by
migrate_v1 for fresh-install historical fidelity — those tables are created
and then immediately dropped again by migrate_v5 for a from-scratch
install, and are not part of the live schema. See the note below for why.

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
    fwhm_reject_sigma           REAL NOT NULL DEFAULT 2.5,
    star_count_reject_sigma     REAL NOT NULL DEFAULT 1.5,
    eccentricity_reject_abs     REAL NOT NULL DEFAULT 0.85,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);

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

CREATE TABLE IF NOT EXISTS feature_flags (
    key         TEXT PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 0,
    updated_at  INTEGER NOT NULL
);
```
```

**Note on Background Std Dev, Background Gradient, and SNR:** these three
values are still computed by `AnalyzeFrames` per frame but do not drive
classification — Background Std Dev and Background Gradient were dropped as
rejection metrics (highly correlated with Background Median), and SNR is
retained as a diagnostic value only. They live only in `ctx.analysis_results`
(in-memory) and the JSON export — no database table persists them.
`frame_analysis_results`, which was designed to do exactly that (algorithm-
versioned, keyed by `(file_path, algorithm_set_version)`), was never wired
up with a reader or writer and was removed — see the note below. The
corresponding pcode commands (`BackgroundStdDev`, `BackgroundGradient`)
remain as deprecated stubs for script compatibility.

**Note on removed tables and columns (Issue 89):** `algorithm_sets`,
`frame_analysis_results` (plus its two indexes), `session_history`, and
`console_history` were created with real design intent — algorithm-
versioned analysis-result caching to skip redundant re-analysis,
a session work-log, and persistent console history across restarts,
respectively — but none was ever given a runtime reader or writer, and all
four were dropped via migration v5. `threshold_profiles.signal_weight_reject_sigma`
was dropped in the same migration — the last of three dead columns left
over from the Signal Weight metric's removal; the other two,
bg_stddev_reject_sigma` and `bg_gradient_reject_sigma`, were dropped
earlier in migration v4. All migrations are now historically accurate as of
this cleanup — `migrate_v1` correctly reflects the schema as it existed at
that point in time, so a genuinely fresh install chains through the full
migration sequence correctly instead of erroring on columns/tables the
current canonical schema no longer creates (a real bug this cleanup found
and fixed, previously undetectable since no fresh install had been tested
against the schema in its pre-cleanup state).

**Note on crash recovery removal (Issue 107, migration v6):** the crash
recovery feature — `crash_recovery` table, `check_crash_recovery`/
`write_crash_recovery` commands, the `crash_recovery_interval_secs`
setting, and all frontend wrappers/UI — was removed outright rather than
fixed, after confirming it was never relied on in practice. This is a
distinct migration from the Issue 89 cleanup above; `crash_recovery` was
still present at v5 and dropped in v6.

**Note on threshold default consistency — confirmed via
`defaults.rs`:** `DEFAULT_STAR_COUNT_SIGMA = 1.5` in
`settings/defaults.rs`, which states explicitly in its header comment
that it is *"the single source of truth. No magic numbers or default
strings anywhere else."* The DB column default
(`star_count_reject_sigma REAL NOT NULL DEFAULT 1.5`) matches this
exactly, and `AnalysisThresholds::default()` in
`analysis/session_stats.rs` correctly sources its `star_count` value
from `DEFAULT_STAR_COUNT_SIGMA` rather than hardcoding a literal —
confirmed fixed (issue #67). `defaults.rs` bounds `star_count` to
`STAR_COUNT_SIGMA_MIN`/`MAX` of `0.5`–`4.0`, matching §8.5's table
below; §3.13's Edit > Analysis Parameters field description had drifted
to `0.5`–`5.0` and is corrected in this pass (Issue 97).

**Note on feature_flags (Issue 130, migration v7):** unlike every other
table in this schema, `feature_flags` has no server-side seed data and
no fixed, backend-known list of valid keys — the frontend's
`FEATURE_FLAGS` registry (`src-svelte/lib/settings/constants.ts`) is
the single source of truth for which flags exist, their labels, and
their defaults. A key absent from the table means "not yet toggled
from its registry default," not "invalid" — `get_feature_flags`
returns whatever rows happen to exist, and the frontend merges that
over the registry defaults on hydration. Built as general-purpose
infrastructure (Edit > Feature Preferences, §3.14) when Issue 130
needed a UI-accessible way to hide the reference-frame badge (§3.9,
§3.10); the mechanism itself is reusable for future flags at the cost
of one registry entry each.

### 8.3 Preferences

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
| Last-used directory        | (empty)             | X           |             | `last_directory`              | —       | —      |
| JPEG quality                | 75%                 | X           | X           | `jpeg_quality`                | 1       | 100    |
| Recent directories max      | 10                  | X           | X           | `recent_directories_max`      | 1       | 50     |
| Backup directory            | Downloads folder    | X           | X           | `backup_directory`            | —       | —      |
| Console history size        | 500                 | X           | X           | `console_history_size`        | 100     | 5000   |
| Macro editor font size      | 13px                | X           | X           | `macro_editor_font_size`      | 8       | 24     |
| Buffer pool memory limit    | 4 GB                | X           | X           | `buffer_pool_memory_limit`    | 512 MB  | 32 GB  |
| Shadow clip (AutoStretch)   | -2.8                | X           | X           | `autostretch_shadow_clip`     | -5.0    | 0.0    |
| Target background (AutoStretch) | 0.15            | X           | X           | `autostretch_target_bg`       | 0.01    | 0.50   |
| Active threshold profile ID | null                | X           | (internal)  | `active_threshold_profile_id` | —       | —      |
| Quick Launch bar visible    | true                | X           | (internal)  | `quick_launch_visible`        | —       | —      |

Last-used directory is populated automatically (not a user-facing
preference toggle — "Persisted" but not "User Pref" in the table above,
same category as theme). Its exact write path and relationship to the
separate `recent_directories` table (§8.2 — multiple directories with
usage counts, a different mechanism) was not traced source-side in this
pass; worth a follow-up if the two are ever found to disagree.

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
  on-demand via `DebayerImage`, which always uses bilinear interpolation
  (`debayer_bilinear()`) — no other algorithm exists in source and none
  is selectable; the Bayer pattern is read from the `BAYERPAT`/
  `BAYER_PATTERN` keyword (`analysis/debayer.rs`, Issue 122),
  defaulting to RGGB if absent (Issue 97 — this list previously named
  Nearest Neighbor, VNG, and AHD as supported, none of which exist)

Note on the stack result specifically: the "normalized 0.0–1.0"
convention above describes per-frame image_buffers data, which is a
straightforward min/max stretch of a single frame's own raw pixel
range.  ctx.stack_result (StackFrames' output) is normalized
differently and for a different reason — see §7.3 — and should not be
assumed comparable to a single frame's normalized range, or to another
stack's.

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
| `get_feature_flags`                 | Returns all feature_flags rows as a key/bool map (§8.2, §3.14); keys absent from the table are not included — the frontend merges this over registry defaults |
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
| `set_feature_flag`                  | Upserts one feature_flags row (§8.2, §3.14) — key, enabled                                                          |
| `set_frame_flag`                    | Updates the PASS/REJECT flag for a single frame in `ctx.analysis_results` by path; used before Commit to sync toggled flags |
| `set_preference`                    | Upserts a single preference key/value; writes through the `AppSettings` struct                                     |
| `start_background_cache`            | Spawns a background task that builds display-resolution JPEGs and both blink caches, snapshotting pixel data in chunks via `pixel_chunking` with a short `AppContext` lock per chunk (§2.2) |

---

## 11. Plugin Reference

All plugins are built-in native Rust, fully implemented and shipped.
The plugin framework supports WASM user plugins via Wasmtime (§2.4),
but none ship by default. Not every pcode command is a plugin — `Set`
and `pwd` are handled directly by the interpreter rather than
registered in the plugin registry. Command syntax and category
grouping live in the pcode Guide (see §4.1).

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
| `featurePreferencesOpen` | Whether the Feature Preferences dialog is open |
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
| `display_cache` never written (Issue 84, deferred) | `start_background_cache` computes display-resolution JPEGs but discards them; frame navigation re-renders from raw pixels every time instead of reusing a cached copy — see §2.2. Deferred: low-impact under file-browser-only navigation |
| XISF Vector/Matrix properties             | Read as a placeholder; skipped on write                                                                    |
| Rayon thread count not user-configurable  | Hardcoded to `num_cpus - 1`; not exposed as a preference despite `RAYON_THREAD_COUNT_MIN` existing in defaults |
| Sidebar icon tooltips clipped by Quick Launch | CSS stacking context issue                                                                              |
| Single-file-load blink isolation          | Files loaded via `LoadFile` are included in `ctx.file_list`, not kept separate                             |
| AutoStretch performance in dev builds     | 3–5 seconds for a 9MP RGB frame in debug builds; near-instant in release builds                             |
| AutoStretch lost on Blink→Pixels tab switch | Viewer reverts to raw unstretched display                                                                 |
| SNR estimator PSF artifact                | Worse-seeing frames produce higher SNR due to bloated star flux; excluded from rejection classification — see §6.2 |
| `threshold_profiles` orphaned columns     | `bg_stddev_reject_sigma`/`bg_gradient_reject_sigma` may still exist on pre-cleanup databases — see §8.2   |
| `validate_alignment()` unused in StackFrames | Defined but never called; all frames pass without this validation step — see §7.4                       |
| Linux GTK file picker multi-select        | Silently refuses to confirm a selection containing both files and folders (e.g. Ctrl+A when a `rejected/` subfolder is present) — select files manually instead |
| Separate RGB channel views not working correctly | Pre-existing display bug                                                                          |
| `TRI_MAX_STARS = 30` unvalidated on sparse-star sessions | Current value works for typical sessions; not yet confirmed as a safe floor for sparse-star fields — see §7.2 |
