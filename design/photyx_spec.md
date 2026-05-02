# Photyx — Specification & Requirements Document

**Version:** 21 **Date:** 2 May 2026 **Status:** Active Development — Phase 9 in progress

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

File: Select Directory, Load Single Image, Close Session, Exit
Edit: Preferences, Analysis Parameters
View: Theme (Dark / Light / Matrix)
Analyze: Analyze Frames, Analysis Results, Analysis Graph, Contour Plot
Tools: Backup Database, Restore Database, Log Viewer
Help: About Photyx, Documentation

### 8.3 Toolbar

Channel selector (RGB / R / G / B), Zoom controls (Fit / 25% / 50% / 100% / 200%), AutoStretch toggle.

### 8.4 Viewer Region

Canvas-based image viewer with starfield placeholder when no image is loaded. Overlays: quality flag border (blink mode), star annotations (ComputeFWHM), filename overlay.

### 8.5 Icon Sidebar & Sliding Panels

Icon sidebar with 4 panel icons. Panels slide in from the left:

- File Browser — directory picker, format filter dropdown, Load button, file list
- Keyword Editor — inline editing; name is read-only; Write Changes writes current frame
- Macro Library — list, run, edit, pin, delete macros
- Plugin Manager — list native and WASM plugins

### 8.6 File Browser

Directory path bar with Browse button. Format filter dropdown (All Supported / FITS Only / XISF Only / TIFF Only). Load button. File list with click-to-navigate.

### 8.7 Image Viewer

Canvas-based display. Zoom: Fit / 25% / 50% / 100% / 200%. Pan at non-fit zoom levels with momentum. Pixel-accurate readout via `get_pixel` (raw buffer, not JPEG). Full-res cache for high zoom. JPEG disclaimer bar.

### 8.8 Info Panel

Tabs: Pixels (coordinate + pixel value + WCS), Metadata (size/bit depth/color/image center), Histogram (per-channel RGB or mono), Blink (play controls + resolution + delay + quality flags toggle).

### 8.9 pcode Console

Line-oriented command input with tab completion, command history (↑/↓), Trace/No Trace toggle, and expandable full-screen mode. All commands route through `run_script`.

### 8.10 Status Bar

Notification bar showing latest notification with type-specific color and icon. Click to open notification history overlay. `running` type triggers pulse animation AND expands bar to 3× height (66px) with dark semi-transparent overlay — provides clear visual indication of long-running operations. Bar shrinks back smoothly when operation completes.

### 8.11 Analysis Results

Viewer-region component (replaces image viewer). Sortable table of per-frame quality metrics with Refresh button. Columns: #, Filename, FWHM, Eccentricity, Stars, SNR, Bg Median, Bg Std Dev, Bg Gradient, PXFLAG.

### 8.12 Analysis Graph

Viewer-region component. Line chart of up to two metrics across all loaded frames. Metric 1: solid line, colored dots (red for REJECT, white for PASS). Metric 2: dotted line in warning color. Reject threshold lines drawn for both metrics (primary: red left-aligned, secondary: warning color right-aligned, both outlined in black for visibility). Reject lines reflect thresholds used in the last AnalyzeFrames run — not the current active profile. Click on a dot navigates to that frame (requires vertical proximity to dot). Refresh button re-fetches data.

### 8.13 Edit > Preferences

Modal dialog (540px wide). Draft-copy pattern — nothing written until OK or Apply. Cancel discards. Covers 8 user-facing preference fields. Theme excluded (View menu). Threshold profiles excluded (Edit > Analysis Parameters). Clicking outside the dialog does NOT close it.

### 8.14 Edit > Analysis Parameters (Threshold Profiles)

Modal dialog (400px wide). Manages named sets of AnalyzeFrames rejection thresholds.

**Profile selector row:** `[🗑] [profile dropdown] [＋]`

- Trash deletes selected profile (inline confirmation bar); any profile including the last one can be deleted; deleting the last profile re-seeds a "Default" profile
- ＋ reveals a name input row for creating a new profile with default threshold values
- Selecting a profile in the dropdown makes it the one being edited (not immediately active)

**Active profile indicator:** "Active profile: [name]" line below the selector row, updated on OK/Apply.

**Threshold fields:** 7 fields in label/direction/input layout:

- Background Median: > +σ (0.5–5.0, default 2.5)
- Background Std Dev: > +σ (0.5–5.0, default 2.5)
- Background Gradient: > +σ (0.5–5.0, default 2.5)
- SNR Estimate: < −σ (−0.5 to −5.0, default −2.5)
- FWHM: > +σ (0.5–5.0, default 2.5)
- Star Count: < −σ (−0.5 to −5.0, default −1.5)
- Eccentricity: > absolute (0.10–1.00, default 0.85)

**Unsaved changes:** Switching profiles with unsaved edits shows inline confirmation bar.

**OK/Apply:** Saves profile to DB and sets it as the active profile (propagated to AppContext immediately).

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

### 11.2 Metrics & Thresholds

See `photyx_reference.md` §4 for the full metrics table and classification rules.

Seven metrics: Background Median (+σ), Background Std Dev (+σ), Background Gradient (+σ), SNR Estimate (−σ), FWHM (+σ), Star Count (−σ), Eccentricity (absolute).

Thresholds are user-configurable via named profiles (Edit > Analysis Parameters). The active profile's thresholds are used when AnalyzeFrames runs. The thresholds actually used in the last run are stored in `ctx.last_analysis_thresholds` and returned by `get_analysis_results` for display in the Analysis Graph.

**Note on metric redundancy:** Cross-session correlation analysis shows Bg Std Dev is highly correlated with Bg Median (r = 0.97–0.99) in all sessions examined. Removal of Bg Std Dev is planned pending additional dataset confirmation.

**Planned: Iterative sigma clipping.** Extreme outliers (clouds, satellites) inflate session std dev and distort rejection thresholds for all other frames. Fix: two-pass computation — compute initial stats, exclude extreme outliers, recompute stats, classify. Outlier-excluded frames will be visually marked in the Analysis Graph.

### 11.3 Workflow

1. Run `AnalyzeFrames` — writes PXFLAG to each file immediately
2. Fast blink pass — red border overlay on REJECT frames provides peripheral awareness
3. Deliberate review — step manually; P / R keys override any frame's flag (written immediately)
4. Delete confirmed rejects via `DeleteRejected` or equivalent UI

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
| Phase 7     | ✅ Complete               | AnalyzeFrames (7 metrics), PXFLAG, Analysis Graph, star annotations, consolePipe, blink overlay                                                                                                                                                                  |
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
- Iterative sigma clipping in session stats
- AnalyzeFrames metric caching (skip Pass 1 when metrics already cached)
- Bg Std Dev metric removal (pending additional dataset confirmation)

---

## 14. Out of Scope (v1.0)

- GPU acceleration — deferred until CPU pipeline is stable and benchmarked
- Python plugin support — WASM is the preferred extensibility path

---

*Previous version: 20 — Next review: Upon completion of Phase 9*
