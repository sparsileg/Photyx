# Photyx — Specification & Requirements Document

**Version:** 25 **Date:** 13 May 2026 **Status:** Active Development — Phase 10 in progress

---

## 1. Overview

Photyx is a high-performance desktop application for reading, viewing, processing, and analyzing astrophotography image files. Designed for astrophotographers who need fast image review, batch processing, keyword management, quantitative analysis, and scriptable automation — all in a single extensible platform.

---

## 2. Goals & Design Philosophy

- **Speed first.** Image loading, blinking, and rendering must feel instantaneous.
- **Extensible by design.** All functionality is implemented as plugins — core I/O, processing, and analysis alike.
- **Scriptable and automatable.** The pcode macro language allows reusable workflows triggerable from the UI, REST API, or command line.
- **Cross-platform.** Windows, macOS, and Linux from day one.
- **Open architecture.** Analysis, format support, and processing are discrete, independently testable modules.

---

## 3. Target Platforms

| Platform | Minimum Version                |
| -------- | ------------------------------ |
| Windows  | Windows 11 (64-bit)            |
| macOS    | macOS 12 Monterey              |
| Linux    | Ubuntu 22.04 LTS or equivalent |

Distribution: .msi / .exe (Windows), .dmg (macOS), .AppImage or .deb (Linux).

---

## 4. Technology Stack

| Layer       | Technology                                                      |
| ----------- | --------------------------------------------------------------- |
| Frontend    | Tauri v2 + Svelte + TypeScript; OS-native WebView (no Chromium) |
| Backend     | Rust; Rayon for parallelism; Tauri IPC for frontend ↔ backend   |
| REST API    | Axum (local HTTP, deferred)                                     |
| Logging     | Rust `tracing` crate; rolling file log in OS app data directory |
| Plugins     | Built-in native (Rust) + user WASM via Wasmtime                 |
| Persistence | Embedded SQLite via `rusqlite` (statically linked)              |
| Updates     | `tauri-plugin-updater` via GitHub Releases (deferred)           |

Key crates in use: `fitsio` (FITS), `tiff`, `rayon`, `tracing`, `serde_json`, `bytemuck`, `once_cell`, `chrono`, `rusqlite`.

The `photyx-xisf` crate (MIT OR Apache-2.0) is a standalone workspace member implementing the XISF reader/writer with zero-copy pixel deserialization.

---

## 5. File Format Support

See `photyx_reference.md` §6 for the full format/keyword coverage table.

### 5.1 Supported Formats

| Format                 | Read | Write | Keywords                               |
| ---------------------- | ---- | ----- | -------------------------------------- |
| FITS (.fit/.fits/.fts) | ✓    | ✓     | Full                                   |
| XISF (.xisf)           | ✓    | ✓     | Full (FITSKeyword + Properties blocks) |
| TIFF (.tif/.tiff)      | ✓    | ✓     | AstroTIFF convention                   |
| PNG (.png)             | ✓    | ✓     | None                                   |
| JPEG (.jpg/.jpeg)      | ✓    | ✓     | None                                   |

### 5.2 Internal Pixel Format

- Supported bit depths: 8-bit integer, 16-bit integer, 32-bit float
- Supported color modes: Monochrome (1 channel), RGB (3 channel)
- U32 data is downconverted to U16 on load (high 16 bits)
- All write operations use atomic temp-file-then-rename to protect against partial writes

### 5.3 Format Conversion

Format conversion is read-plugin → write-plugin with no special conversion layer. Any readable format can be converted to any writable format via pcode.

### 5.4 Debayering

CFA (Bayer) files are loaded and displayed as mono by default. Debayering is on demand via `DebayerImage`. Supported algorithms: Nearest Neighbor, Bilinear (default), VNG, AHD.

### 5.5 FITS-to-XISF Keyword Mapping

See `photyx_reference.md` §3 for the full mapping table.

---

## 6. Plugin Architecture

### 6.1 Plugin Model

Every operation in Photyx is a plugin: file readers, writers, keyword operations, processing, stretch, analysis. The core engine is a plugin host with no hard-coded operations.

- **Built-in native plugins** — compiled into the binary; maximum performance; version-locked with core
- **User WASM plugins** — loaded via Wasmtime; sandboxed; cross-platform; one `.wasm` runs on all platforms

### 6.2 Plugin Status

See `photyx_reference.md` §9 for the full plugin designation and status table.

---

## 7. Macros — pcode

### 7.1 Overview

pcode is a line-oriented macro language. Each line is a command name followed by zero or more named arguments. Macros are stored in SQLite and executable from the console, REST API, or command line.

### 7.2 Language Features

- Named arguments: `CommandName arg=value arg2="string value"`
- Comments: lines beginning with `#`
- Variables: `Set name = "M31"` (string literals must use **double quotes**)
- Arithmetic: `+`, `-`, `*`, `/`, `^`; grouping with `( )`
- Math functions: `sqrt()`, `abs()`, `round()`, `floor()`, `ceil()`, `min()`, `max()`
- Conditionals: `If / Else / EndIf`
- Loops: `For i = 1 To N / EndFor`
- Error handling: halt-on-error by default; configurable
- `$NEW_FILE` convention: plugins that create output files store the path here
- `@param` token system: macros declare named parameters at the top

### 7.3 pcode Command Reference

See `photyx_reference.md` §1 for the full command dictionary.

### 7.4 Trace Mode

The console header Trace / No Trace toggle controls execution verbosity.

### 7.5 Macro Library

Macros are stored in the SQLite database (`photyx.db`). The Macro Library panel lists all macros; the Macro Editor creates and edits them. Every save of an existing macro preserves the previous version in `macro_versions` for recovery.

---

## 8. User Interface

### 8.1 Application Shell

Single-window SPA. Layout from top to bottom: Menu Bar (28px) → Toolbar (34px) → Content Area (flex: 1) → Status Bar (22px).

Content Area contains: Icon Sidebar (40px) | Viewer Region (flex: 1). The Quick Launch panel (34px) sits above the Viewer Region.

### 8.2 Menu Bar

Standard application menu with six top-level menus: File, Session, Edit, View, Analyze, Tools, Help.

**File menu:**

- Load Single Image…
- ─────────────
- Exit

**Session menu:**

- Add Files… (Ctrl+O)
- Close Session
- ─────────────
- Export Session JSON…
- Import Session JSON…

**Edit menu:**

- Preferences
- Analysis Parameters

**View menu:**

- Theme: Dark / Light / Matrix

**Analyze menu:**

- Analyze Frames
- Analysis Results
- Analysis Graph
- ─────────────
- Contour Plot

**Tools menu:**

- Backup Database
- Restore Database
- ─────────────
- Log Viewer

**Help menu:**

- About Photyx
- Documentation

### 8.3 Toolbar

34px fixed height below the menu bar. Contains viewer controls and file/directory count display.

### 8.4 Icon Sidebar

40px fixed width. Icons for panels (File Browser, Keyword Editor, Macro Library, Plugin Manager). Icons trigger sliding panels.

### 8.5 Viewer Region

Fills remaining content area. Shows image viewer by default. Replaced by viewer-region components (Analysis Graph, Analysis Results) when active. All viewer-region visibility controlled exclusively via `ui.showView()`.

### 8.6 Status Bar

22px fixed height at bottom. Shows active notification. Expands to 66px with pulse animation when `notifications.running()` is active.

### 8.7 Sliding Panels

Slide in from the left over the viewer region. Width: standard (varies by panel) or wide (75vw, for Keyword Editor). Triggered by icon sidebar icons.

### 8.8 Quick Launch Panel

34px bar between the toolbar and viewer region. Buttons run pcode scripts. Button assignments persisted to `quick_launch_buttons` table.

### 8.9 pcode Console

Collapsible panel at the bottom of the viewer region. Expands to 60vh full-width overlay. Trace / No Trace toggle. History navigation.

### 8.10 Analysis Graph

Viewer-region component. Two-metric line chart with sigma bands, mean line, and reject threshold lines. Click dot to navigate to frame. Metric dropdowns for Metric 1 and Metric 2.

**Toolbar:** Metric 1 dropdown | Metric 2 dropdown | ↻ Refresh | ✓ Commit Results | ⎘ Copy | ⬇ Save Image | ✕ Close

**Dot appearance:**

- All dots have a 2px black border for visibility against any background color
- PASS: white fill
- REJECT — Optical (O): red fill
- REJECT — Transparency (T): yellow fill
- REJECT — Sky Brightness (B): blue fill
- REJECT — Multi-category: split semicircle in respective colors, slightly larger radius

**Legend:** Fixed in top-left corner of the graph canvas. Always visible. Shows: Pass, Reject — Optical, Reject — Transparency, Reject — Sky Brightness, with corresponding dot appearance.

**Commit Results** is disabled for imported sessions.

### 8.11 Analysis Results

Viewer-region component. Sortable table of per-frame metrics, PXFLAG values, and rejection categories.

**Toolbar row 1:** Analysis Results title | ↻ Refresh | ✓ Commit Results | ⎘ Copy | ✕ Close

**Toolbar row 2:** [IMPORTED badge if applicable] Session path: `<derived from file list>`

**Columns:** # | Filename | FWHM | Eccentricity | Stars | Signal Weight | Bg Median | PXFLAG | Category

**Category column:** Shows rejection category badge for REJECT frames (O, T, B, OT, OB, BT, OBT). Centered. Color-coded: O=red, T=yellow, B=blue, multi=purple.

**PXFLAG toggle:** Right-click any row to show a context menu:

- REJECT row → "Set to PASS"
- PASS row → "Set to REJECT"

Toggled rows are highlighted with an amber left border and subtle background tint. All underlying metric data (triggered_by, rejection_category) is preserved regardless of toggle direction so the user can toggle back if needed.

**Commit Results behavior:** Non-terminal operation. On success:

1. Toggled flag changes are pushed to Rust
2. Each REJECT file is moved to `<source_dir>/rejected/<name>.<ext>.rejected`
3. Rejected files are removed from the session; pass frames remain loaded
4. View closes, viewer clears
5. Session stays open — pass frames are ready for subsequent operations (e.g. stacking)

PXFLAG is **not** written to files. The file move is the sole persistence action.

**Imported sessions:** When loaded via Session → Import Session JSON…, an IMPORTED badge appears in the session path row and Commit Results is disabled. All display functionality works normally.

### 8.12 Info Panel

Pixel coordinates, raw value, WCS coordinates (if available). Always visible when viewer has an image.

### 8.13 Edit > Preferences

Modal dialog (540px wide). Draft-copy pattern — nothing written until OK or Apply. Cancel discards. Covers 8 user-facing preference fields. Theme excluded (View menu). Threshold profiles excluded (Edit > Analysis Parameters). Clicking outside the dialog does NOT close it.

### 8.14 Edit > Analysis Parameters (Threshold Profiles)

Modal dialog (400px wide). Manages named sets of AnalyzeFrames rejection thresholds.

**Profile selector row:** `[🗑] [profile dropdown] [＋]`

- Trash deletes selected profile (inline confirmation bar); any profile including the last one can be deleted; deleting the last profile re-seeds a "Default" profile
- ＋ reveals a name input row for creating a new profile with default threshold values
- Selecting a profile in the dropdown makes it the one being edited (not immediately active)

**Active profile indicator:** "Active profile: [name]" line below the selector row, updated on OK/Apply.

**Threshold fields:** 5 fields in label/direction/input/unit layout:

- Background Median: > +σ (0.5–4.0, default 2.5)
- FWHM: > +σ (0.5–4.0, default 2.5)
- Eccentricity: > absolute (0.10–1.00, default 0.85)
- Star Count: < −σ (−0.5 to −4.0, default −3.0)
- Signal Weight: < −σ (−0.5 to −4.0, default −2.5)

**Unsaved changes:** Switching profiles with unsaved edits shows inline confirmation bar.

**OK/Apply:** Saves profile to DB and sets it as the active profile (propagated to AppContext immediately).

Clicking outside the dialog does NOT close it.

### 8.15 Quick Launch Panel

Bar of shortcut buttons below the toolbar. Buttons run pcode scripts via `run_script`. Right-click to remove. Pin macros from the Macro Library. Button assignments persisted to `quick_launch_buttons` table.

### 8.16 Log Viewer

Modal overlay. File picker → log content with level filters (ERROR/WARN/INFO/DEBUG). Auto-tail polls every 2 seconds; auto-scroll suspends when user scrolls up.

### 8.17 Blink Tab

Play/pause/step controls. Resolution dropdown (12.5% / 25%). Min Delay dropdown. Highlight Rejected toggle (red border overlay on REJECT frames). Cache built on first play; invalidated when resolution changes or file list changes.

### 8.18 Session JSON Export/Import

**Export (Session → Export Session JSON…):**

Exports the current session's analysis results as a portable JSON archive. Default filename derived from the first frame: `<target>_<YYYYMMDD>.json`. JSON contains: `photyx_version`, `exported_at`, `threshold_profile_name`, `thresholds`, `session_stats`, `outlier_paths`, and `frames[]` (per-frame: full path, all 5 raw metric values, flag, triggered_by, rejection_category). All filenames stored as full absolute paths to support multi-directory sessions.

**Import (Session → Import Session JSON…):**

Clears the current session and loads analysis results from a JSON file. No images are loaded — display only. An IMPORTED badge appears in the Analysis Results toolbar. Commit Results is disabled. On import, the Analysis Results view opens automatically.

---

## 9. Settings & Persistence

Settings are stored in the embedded SQLite database (`photyx.db`) in the OS app data directory (`APPDATA/Photyx/` on Windows, `~/.local/share/Photyx/` on Linux). See `photyx_reference.md` §5 for all settings tables and `photyx_persistence_inventory.md` for the full schema.

**Threshold profiles:** Named sets of AnalyzeFrames rejection thresholds. Multiple profiles supported; managed via Edit > Analysis Parameters. Active profile propagated into `AppContext.analysis_thresholds` immediately on change. See §8.14 and `photyx_persistence_inventory.md` §5.

**Crash recovery:** Session recovery state (file list + current frame) written every 60 seconds. On next launch after crash, Photyx offers to restore the previous session by reloading the same files.

**Database backup:** Manual backup triggered from the Tools menu. Timestamped ZIP archive containing `photyx.db` and a `macros/` subfolder with each macro as a plain-text `.phs` file.

---

## 10. Logging

- Location: `{APPDATA}/Photyx/logs/`
- Rolling policy: new file per session; last 10 retained
- Levels: ERROR, WARN, INFO, DEBUG (default INFO in release, DEBUG in dev)
- Error-level events also surface in the notification bar

---

## 11. Frame Analysis & Rejection

### 11.1 Philosophy

Photyx flags obvious disasters only. Borderline frames are left for downstream tools (PixInsight SubframeSelector). Classification is session-relative — never cross-session absolute.

### 11.2 Metrics

Five metrics are computed for each frame:

| Metric            | Type     | Direction          | Default threshold | Rejection driver |
| ----------------- | -------- | ------------------ | ----------------- | ---------------- |
| Background Median | Sigma    | +σ (high is worse) | 2.5σ              | ✓                |
| FWHM              | Sigma    | +σ (high is worse) | 2.5σ              | ✓                |
| Eccentricity      | Absolute | > threshold        | 0.85              | ✓                |
| Star Count        | Sigma    | −σ (low is worse)  | −3.0σ             | ✓                |
| Signal Weight     | Sigma    | −σ (low is worse)  | −2.5σ             | ✓                |

**Algorithm:** All metrics except Background Median are derived from elliptical 2D Moffat PSF fitting per detected star. FWHM and Eccentricity replace prior intensity-weighted second-order moment calculations. Star Count now counts only stars that pass Moffat PSF acceptance criteria (replaces lenient connected-pixel detection). Signal Weight is a PSF-based signal quality measure: A² / (A + B·π·a·b), where A is fitted peak amplitude, B is local background, and π·a·b is effective PSF area. PSF Residual is computed internally as the star acceptance gate but is not user-facing.

**Signal Weight:** Promotes to rejection metric. Catches transparency and thin-cloud events that Star Count misses, and correctly signals problems on frames where Star Count is inflated (e.g. satellite trails). Signal Weight rejections are assigned category T (Transparency).

**Star Count threshold:** −3.0σ. Mild transparency events are better handled by SubframeSelector weighting than hard rejection; only severe star count drops warrant culling.

**Removed metrics:** Background Std Dev (r = 0.92–0.999 with Bg Median) and Background Gradient (session-dependent with sign reversal). Both pcode commands retained as deprecated stubs for script compatibility. SNR Estimate removed as a user-facing metric; superseded by Signal Weight.

**SNR note:** SNR is computed and displayed as a diagnostic metric but does not drive PASS/REJECT classification. Cross-session analysis confirmed a PSF artifact — worse-seeing frames produce higher SNR due to bloated star flux; SNR never drove a unique rejection not already caught by FWHM or Star Count.

### 11.3 Classification

PASS / REJECT only — no SUSPECT. A frame is REJECT if any single metric exceeds its threshold. `triggered_by` records which metrics caused the REJECT.

### 11.4 Rejection Categories

Every REJECT frame is assigned one or more rejection categories:

| Category | Label          | Triggered by                           |
| -------- | -------------- | -------------------------------------- |
| O        | Optical        | FWHM and/or Eccentricity               |
| T        | Transparency   | Star Count (without Background Median) |
| B        | Sky Brightness | Background Median                      |

**Multi-category ordering:** O always leads. When B and T are both present, B leads T (sky brightness is the root cause of star suppression). Examples: OT, OB, BT, OBT.

### 11.5 Session Statistics & Iterative Sigma Clipping

Classification is session-relative. `AnalyzeFrames` uses two-pass iterative sigma clipping — see `photyx_development.md` §3.60 for implementation details.

### 11.6 Committing Results

Commit Results is a **fast, non-terminal operation**:

1. Toggled flag changes are pushed to Rust
2. Each REJECT file is moved to `<source_dir>/rejected/<name>.<ext>.rejected`; each file lands in its own source directory's `rejected/` subfolder
3. Rejected files are removed from the session; pass frames remain loaded
4. Analysis results are cleared

PXFLAG is **not** written to files. The file move is the sole persistence action. After commit, the session remains open and pass frames are immediately available for subsequent operations (e.g. stacking).

### 11.7 On-the-Fly Reclassification

`get_analysis_results` reclassifies all frames on every call using cached metrics + current thresholds. Threshold changes take effect immediately on next Refresh without rerunning AnalyzeFrames. Skipped for imported sessions.

### 11.8 Blink Review Workflow

1. Run `AnalyzeFrames`
2. Review in Analysis Graph / Results table; adjust thresholds and refresh as needed
3. Optionally toggle individual frame flags via right-click context menu
4. Click **✓ Commit Results** — moves rejects to `rejected/` subfolders; pass frames remain loaded

---

## 12. External API (Deferred)

Local HTTP REST server via Axum. Deferred to post-Phase 9.

---

## 13. Development Phases

| Phase       | Status      | Focus                                                                                                                                                                                                                                                                |
| ----------- | ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 1     | Complete    | Scaffold, plugin host, FITS reader, viewer, logging                                                                                                                                                                                                                  |
| Phase 2     | Complete    | Blink engine, Auto-STF, zoom, pan, pixel tracking, Info Panel                                                                                                                                                                                                        |
| Phase 3     | Complete    | photyx-xisf crate, XISF read/write, TIFF read/write, RGB display, background cache                                                                                                                                                                                   |
| Phase 4     | Complete    | Keyword plugins, write plugins, AstroTIFF round-trip, FITS u16 fix, path resolution                                                                                                                                                                                  |
| Phase 5     | Complete    | pcode interpreter (If/For/variables), Macro Editor, Quick Launch, GetKeyword, RunMacro, atomic writes                                                                                                                                                                |
| Phase 6     | Complete    | UI audit and cleanup                                                                                                                                                                                                                                                 |
| Phase 7     | Complete    | AnalyzeFrames (5 metrics), PXFLAG, Analysis Graph, star annotations, consolePipe, blink overlay                                                                                                                                                                      |
| Phase 8     | Complete    | Moment FWHM, ContourHeatmap, display pipeline refactor, LoadFile, histogram hover, keyword editor, UI pass                                                                                                                                                           |
| Phase 9     | Complete    | SQLite , Quick Launch , session history , crash recovery , macros in SQLite , AppSettings , Preferences , threshold profiles , rejection categories , Session JSON export/import , commit file move , PXFLAG toggle , analysis results persistence , console history |
| Phase 10    | Complete    | Memory audit, UI audit, update Star Count algorithm, fix known bugs, replace active directory  with selected files as the global context.                                                                                                                            |
| Phase 11    | In Progress | Stacking, Live Stacking                                                                                                                                                                                                                                              |
| Final Phase | ⬜ Planned   | UI audit & testing                                                                                                                                                                                                                                                   |

### 13.1 Deferred Items

- Simple stacking and live stacking
- Full Async dispatch
- Based on analysis, set default parameters for both broad and narrow band image sessions.

---

## 14. Out of Scope (v1.0)

- GPU acceleration — deferred until CPU pipeline is stable and benchmarked
- Python plugin support — WASM is the preferred extensibility path

---

## 15. File Selection & Session Model

### 15.1 Philosophy

Photyx uses a **global file context** — a flat list of file paths that persists across operations. There is no concept of an "active directory." Files may come from any number of directories and coexist in a single session.

### 15.2 AddFiles

`AddFiles` is the primary entry point for loading images. It opens a multi-file picker allowing the user to select one or more files. The selected files are **appended** to the current session — existing files are not cleared.

- Replaces `SelectDirectory` in both the UI and pcode
- Accepts explicit file paths only — no directory expansion
- Skips files already loaded (duplicate detection)
- Checks memory limit before loading (based on first file dimensions × total count)
- Use `ClearSession` first if starting a fresh session is desired
- The menu item is "Add Files…" (Ctrl+O)

### 15.3 Session State

Session state consists of:

- The loaded file list (full absolute paths)
- Per-file pixel buffers and derived caches
- Analysis results

There is no active directory field. Where a single directory is needed (e.g. for relative path resolution or log labeling), it is derived as the common parent of all loaded files (`ctx.common_parent()`), or None if files span unrelated directories.

### 15.4 Write Operations

All write operations use each file's original source path. Atomic temp-rename behavior is unchanged. Relative destination paths resolve against `ctx.common_parent()`.

### 15.5 Commit Results

When committing analysis results:

- Each REJECT file is moved to a `rejected/` subdirectory **within its own source directory**
- If `rejected/` does not exist it is created automatically
- PXFLAG is **not** written to files — the move is the sole persistence action
- Rejected files are removed from the session; pass frames remain loaded
- The session stays open after commit

### 15.6 Status Bar

The toolbar displays loaded file count and directory count derived from the file list. Example: `157 files · 3 directories`. Shows nothing when no files are loaded.

### 15.7 Close Session

Clears the file list, all buffers, and all analysis results. Resets the session entirely.

### 15.8 Crash Recovery

Session recovery state stores the full file list (absolute paths) and current frame index. On recovery, `AddFiles` is called with the stored paths to restore the session.

---

*Previous version: 24 — Next review: Upon completion of Phase 10*
