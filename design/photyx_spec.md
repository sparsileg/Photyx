# Photyx — Specification & Requirements Document

**Version:** 22 **Date:** 3 May 2026 **Status:** Active Development — Phase 9 in progress

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

Standard application menu. File, Edit, View, Tools, Help.

### 8.3 Toolbar

34px fixed height below the menu bar. Contains viewer controls.

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

**Toolbar:** Metric 1 dropdown | Metric 2 dropdown | ↻ Refresh | ✓ Commit Results | ✕ Close

### 8.11 Analysis Results

Viewer-region component. Sortable table of per-frame metrics and PXFLAG values. Click column headers to sort.

**Toolbar:** ↻ Refresh | ✓ Commit Results | ✕ Close

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

**Threshold fields:** 5 fields in label/direction/input layout:

- Background Median: > +σ (0.5–4.0, default 2.5)
- SNR Estimate: < −σ (−0.5 to −4.0, default −2.5)
- FWHM: > +σ (0.5–4.0, default 2.5)
- Star Count: < −σ (−0.5 to −4.0, default −1.5)
- Eccentricity: > absolute (0.10–1.00, default 0.85)

**Unsaved changes:** Switching profiles with unsaved edits shows inline confirmation bar.

**OK/Apply:** Saves profile to DB and sets it as the active profile (propagated to AppContext immediately). Classification in the Analysis Graph and Results table updates automatically on next refresh — no need to rerun AnalyzeFrames.

Clicking outside the dialog does NOT close it.

### 8.15 Quick Launch Panel

Bar of shortcut buttons below the toolbar. Buttons run pcode scripts via `run_script`. Right-click to remove. Pin macros from the Macro Library. Button assignments persisted to `quick_launch_buttons` table.

### 8.16 Log Viewer

Modal overlay. File picker → log content with level filters (ERROR/WARN/INFO/DEBUG). Auto-tail polls every 2 seconds; auto-scroll suspends when user scrolls up.

### 8.17 Blink Tab

Play/pause/step controls. Resolution dropdown (12.5% / 25%). Min Delay dropdown. Highlight Rejected toggle (red border overlay on REJECT frames). Cache built on first play; invalidated when resolution changes or file list changes.

---

## 9. Settings & Persistence

Settings are stored in the embedded SQLite database (`photyx.db`) in the OS app data directory (`APPDATA/Photyx/` on Windows, `~/.local/share/Photyx/` on Linux). See `photyx_reference.md` §5 for all settings tables and `photyx_persistence_inventory.md` for the full schema.

**Threshold profiles:** Named sets of AnalyzeFrames rejection thresholds. Multiple profiles supported; managed via Edit > Analysis Parameters. Active profile propagated into `AppContext.analysis_thresholds` immediately on change. See §8.14 and `photyx_persistence_inventory.md` §5.

**Crash recovery:** Session recovery state written every 60 seconds. On next launch after crash, Photyx offers to restore the previous session.

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

| Metric            | Type     | Direction          | Default threshold |
| ----------------- | -------- | ------------------ | ----------------- |
| Background Median | Sigma    | +σ (high is worse) | 2.5σ              |
| SNR Estimate      | Sigma    | −σ (low is worse)  | −2.5σ             |
| FWHM              | Sigma    | +σ (high is worse) | 2.5σ              |
| Star Count        | Sigma    | −σ (low is worse)  | −1.5σ             |
| Eccentricity      | Absolute | > threshold        | 0.85              |

**Removed metrics:** Background Std Dev (r = 0.92–0.999 with Bg Median across all sessions — redundant in every case examined) and Background Gradient (session-dependent with sign reversal between broadband and narrowband — unreliable as a general metric). Both pcode commands (`BackgroundStdDev`, `BackgroundGradient`) are retained as deprecated stubs for script compatibility.

Thresholds are user-configurable via named profiles (Edit > Analysis Parameters). The active profile's thresholds are applied on every `get_analysis_results` call.

### 11.3 Classification

PASS / REJECT only — no SUSPECT. A frame is REJECT if any single metric exceeds its threshold. `triggered_by` records which metrics caused the REJECT.

### 11.4 Session Statistics & Iterative Sigma Clipping

Classification is session-relative. `AnalyzeFrames` computes stats across all loaded frames using two-pass iterative sigma clipping:

- **Pass 1** — compute raw per-frame metrics for all frames
- **Pass 2a** — compute initial session stats (mean + std dev per metric) across all frames
- **Pass 2b** — identify extreme outliers (> 4σ from initial mean on any metric); these frames are excluded from stat recomputation but still classified
- **Pass 2c** — recompute clean session stats excluding outlier frames; classify all frames against clean stats

Outlier-excluded frames are displayed as disconnected floating dots in the Analysis Graph with a warning-color square outline. The line graph bridges across outlier positions.

### 11.5 On-the-Fly Reclassification

`get_analysis_results` reclassifies all frames on every call using cached metrics from `ctx.analysis_results` and current `ctx.analysis_thresholds`. This means:

- Changing a threshold profile and refreshing the Analysis Graph or Results table immediately shows updated flags — no need to rerun AnalyzeFrames
- The sigma bands and reject lines in the Analysis Graph always reflect the current active thresholds
- Iterative sigma clipping is rerun on every refresh using the cached metrics

### 11.6 Committing Results (Writing PXFLAG)

PXFLAG is **not** written to files automatically. The user must explicitly commit results:

1. Review results in the Analysis Graph or Results table
2. Adjust threshold profiles if needed and refresh to see updated classification
3. Click **✓ Commit Results** in either toolbar when satisfied
4. Commit writes PXFLAG to all image buffers and flushes to disk via `WriteCurrent`
5. `ctx.last_analysis_thresholds` is updated to reflect the committed thresholds

### 11.7 Blink Review Workflow

1. Run `AnalyzeFrames` — computes metrics and stores results; no file writes
2. Review in Analysis Graph / Results table; adjust thresholds and refresh as needed
3. Click **✓ Commit Results** when satisfied — writes PXFLAG to all files
4. Fast blink pass — red border overlay on REJECT frames
5. Deliberate review — P / R keys override any frame's PXFLAG (written immediately)

---

## 12. External API (Deferred)

Local HTTP REST server via Axum. Deferred to post-Phase 9.

---

## 13. Development Phases

| Phase       | Status                   | Focus                                                                                                                                                                                                                                                            |
| ----------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 1     | ✅ Complete               | Scaffold, plugin host, FITS reader, viewer, logging                                                                                                                                                                                                              |
| Phase 2     | ✅ Complete               | Blink engine, Auto-STF, zoom, pan, pixel tracking, Info Panel                                                                                                                                                                                                    |
| Phase 3     | ✅ Complete               | photyx-xisf crate, XISF read/write, TIFF read/write, RGB display, background cache                                                                                                                                                                               |
| Phase 4     | ✅ Complete               | Keyword plugins, write plugins, AstroTIFF round-trip, FITS u16 fix, path resolution                                                                                                                                                                              |
| Phase 5     | ✅ Complete               | pcode interpreter (If/For/variables), Macro Editor, Quick Launch, GetKeyword, RunMacro, atomic writes                                                                                                                                                            |
| Phase 6     | ✅ Complete               | UI audit and cleanup                                                                                                                                                                                                                                             |
| Phase 7     | ✅ Complete               | AnalyzeFrames (5 metrics), PXFLAG, Analysis Graph, star annotations, consolePipe, blink overlay                                                                                                                                                                  |
| Phase 8     | ✅ Substantially complete | Moment FWHM, ContourHeatmap, display pipeline refactor, LoadFile, histogram hover, keyword editor, UI pass                                                                                                                                                       |
| **Phase 9** | 🔄 In Progress           | SQLite (✅), Quick Launch persistence (✅), session history (✅), crash recovery (✅), macros in SQLite (✅), AppSettings (✅), Preferences dialog (✅), threshold profiles (✅); remaining: analysis results persistence, console history, status bar profile indicator |
| Phase 10    | ⬜ Planned                | UI audit pass                                                                                                                                                                                                                                                    |

### 13.1 Deferred Items

- PNG/JPEG readers/writers
- Debayering
- Async dispatch (long-running commands block UI; requires Tauri event system)
- REST API
- WASM analysis plugins
- User plugin loading, plugin manifest system, Plugin Manager UI
- Channel switching (R/G/B buttons)
- Recent Directories UI
- jpeg_quality — persisted but unwired
- buffer_pool_bytes — persisted but unwired
- console_history_size — persisted but unwired
- AnalyzeFrames progress reporting (requires async dispatch)
- SNR estimator revision (current estimator rewards PSF spreading; confirmed artifact across multiple sessions)

---

## 14. Out of Scope (v1.0)

- GPU acceleration — deferred until CPU pipeline is stable and benchmarked
- Python plugin support — WASM is the preferred extensibility path

---

*Previous version: 21 — Next review: Upon completion of Phase 9*
