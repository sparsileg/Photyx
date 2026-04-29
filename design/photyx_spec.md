# Photyx — Specification & Requirements Document

**Version:** 20
**Date:** 28 April 2026
**Status:** Active Development — Phase 8 substantially complete

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

| Layer    | Technology                                                      |
| -------- | --------------------------------------------------------------- |
| Frontend | Tauri v2 + Svelte + TypeScript; OS-native WebView (no Chromium) |
| Backend  | Rust; Rayon for parallelism; Tauri IPC for frontend ↔ backend   |
| REST API | Axum (local HTTP, deferred)                                     |
| Logging  | Rust `tracing` crate; rolling file log in OS app data directory |
| Plugins  | Built-in native (Rust) + user WASM via Wasmtime                 |
| Settings | `tauri-plugin-store` (Phase 9)                                  |
| Updates  | `tauri-plugin-updater` via GitHub Releases (Phase 9)            |

Key crates in use: `fitsio` (FITS), `tiff`, `rayon`, `tracing`, `serde_json`, `bytemuck`, `once_cell`, `chrono`.

The `photyx-xisf` crate (MIT OR Apache-2.0) is a standalone workspace member implementing the XISF reader/writer with zero-copy pixel deserialization. See `development_notes.md` §3.17 for details.

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

Format conversion is read-plugin → write-plugin with no special conversion layer. Any readable format can be converted to any writable format via pcode. Keyword fidelity is required: all source keywords must be preserved in the output to the extent the target format supports them; any that cannot be represented are logged as a warning.

### 5.4 Debayering

CFA (Bayer) files are loaded and displayed as mono by default. Debayering is on demand via `DebayerImage`. Supported algorithms: Nearest Neighbor, Bilinear (default), VNG, AHD.

### 5.5 FITS-to-XISF Keyword Mapping

See `photyx_reference.md` §3 for the full mapping table. All FITS keywords go into the FITSKeyword block verbatim; keywords with a known XISF Property equivalent are additionally written to the Properties block.

---

## 6. Plugin Architecture

### 6.1 Plugin Model

Every operation in Photyx is a plugin: file readers, writers, keyword operations, processing, stretch, analysis. The core engine is a plugin host with no hard-coded operations.

- **Built-in native plugins** — compiled into the binary; maximum performance; version-locked with core
- **User WASM plugins** — loaded via Wasmtime; sandboxed; cross-platform; one `.wasm` runs on all platforms

All plugins implement the `PhotonPlugin` trait (`name`, `version`, `description`, `parameters`, `execute`). See `development_notes.md` §3 for the trait definition.

### 6.2 Plugin Status

See `photyx_reference.md` §9 for the full plugin designation and status table.

### 6.3 Plugin Settings & Distribution

Each plugin may define its own settings namespace (stored under `plugin.<Name>.*` in the settings store). User WASM plugins are installed by placing them in the plugins directory with a TOML manifest.

---

## 7. Macros — pcode

### 7.1 Overview

pcode is a line-oriented macro language. Each line is a command name followed by zero or more named arguments. Macros are saved as `.phs` files and executable from the console, REST API, or command line.

### 7.2 Language Features

- Named arguments: `CommandName arg=value arg2="string value"`
- Comments: lines beginning with `#`
- Variables: `Set name = "M31"` (string literals must use **double quotes**)
- Arithmetic: `+`, `-`, `*`, `/`, `^`; grouping with `( )`
- Math functions: `sqrt()`, `abs()`, `round()`, `floor()`, `ceil()`, `min()`, `max()`
- Conditionals: `If / ElseIf / Else / EndIf`
- Loops: `For i = 1 To N / EndFor` and `ForEach / EndForEach`
- Error handling: halt-on-error by default; configurable
- `$NEW_FILE` convention: plugins that create output files store the path here for use in subsequent commands
- `@param` token system: macros declare named parameters at the top using special comment lines; values are supplied at run time via a prompt dialog (Macro Library, Quick Launch) or as named arguments (console); this makes macros fully generic without embedding paths or values in the script

### 7.3 pcode Command Reference

See `photyx_reference.md` §1 for the full command dictionary, aliases, scope parameter, and string literal rules.

### 7.4 Trace Mode

The console header Trace / No Trace toggle controls execution verbosity. Trace shows command echo and Set assignment output. No Trace (default) suppresses both. `Assert` is always silent on pass in both modes. See `photyx_reference.md` §1.4.

### 7.5 Macro Library

Macros are stored in the SQLite database (`photyx.db`) rather than as files
on disk. This gives cross-platform consistency and enables version
history. The Macro Library panel lists all macros; the Macro Editor creates
and edits them. Every save of an existing macro preserves the previous
version in `macro_versions` for recovery. **This is implemented as of Phase
9 sub-phase D.** No `.phs` files are written or read during normal
operation.

We store text versions of the macros in the backup of the Photyx database
so they be more easily recovered if necessary.

Three-tier command model:

```
Tier 1: Built-in Native — ReadFIT, AutoStretch, AnalyzeFrames, …
Tier 2: User WASM plugins
Tier 3: User Macros (stored in photyx.db)
```

#### @param Token System

Macros declare runtime parameters using `@param` comment lines at the top of the script:

```
@param INPUT_DIR "Source directory" required
@param OUTPUT_DIR "Destination directory" required
@param TARGET_NAME "Object name" optional default="Unknown"
SelectDirectory path="$INPUT_DIR"
ReadFIT
AddKeyword name=OBJECT value="$TARGET_NAME"
WriteFIT destination="$OUTPUT_DIR"
```

When a macro with `@param` declarations is run from the Macro Library or Quick Launch, Photyx presents a parameter prompt — one field per declared parameter, with browse buttons for directory-type params. From the console, parameters are passed as named arguments:

```
RunMacro ProcessLights INPUT_DIR="D:/M31" OUTPUT_DIR="D:/Output"
```

Quick Launch buttons store only `RunMacro name=X` — no parameter values are embedded. Parameters are always resolved at run time, keeping macros fully generic.

### 7.6 Path Conventions

See `photyx_reference.md` §8. Forward slashes always; backend translates to OS-native. `~` expands to home directory. UNC paths supported (`//host/share`). Paths with spaces must be double-quoted.

### 7.7 Interrogation Properties

See `photyx_reference.md` §2 for GetImageProperty, GetKeyword, GetSessionProperty, and Test tables.

---

## 8. User Interface

### 8.1 Layout

Single main window, single-window SPA. No floating OS windows.

```
┌─ Menu bar (28px) ──────────────────────────────────┐
├─ Toolbar (34px) ───────────────────────────────────┤
├─ Quick Launch (34px, collapsible) ─────────────────┤
├─ Icon sidebar │ Viewer region (dominant)            │
│               │                                    │
├─ Console (1/3)│ Info Panel (2/3) ──────────────────┤
└─ Status / Notification bar (full width) ───────────┘
```

### 8.2 Menu Bar

| Menu    | Items                                                                                |
| ------- | ------------------------------------------------------------------------------------ |
| File    | Select Directory, Load Single Image, Clear Session, Exit                             |
| Edit    | Preferences                                                                          |
| View    | Dark, Light, Matrix                                                                  |
| Process | Auto Stretch                                                                         |
| Analyze | FWHM, Star Count, Eccentricity, Median Value, Contour Plot (Heatmap), Analysis Graph |
| Tools   | Settings, Log Viewer                                                                 |
| Help    | About, Documentation, Check for Updates                                              |

### 8.3 Icon Sidebar & Panels

| Icon   | Panel          | Notes                                                               |
| ------ | -------------- | ------------------------------------------------------------------- |
| Folder | File Browser   | File list, directory bar, format filter, Load button                |
| Tag    | Keyword Editor | Inline editing; wide panel (75vw); Write Changes calls WriteFrame   |
| Code   | Macro Editor   | Opens exclusively from Macro Library (Edit / New buttons)           |
| List   | Macro Library  | Lists .phs files; Pin, Run, Edit, Rename, Delete per entry          |
| Puzzle | Plugin Manager | Lists plugins; enable/disable for WASM only; native = always Active |

**Macro Library — Run behavior:** clicking Run closes the panel automatically so console output is visible.

**Macro Editor — entry points:** Edit button on a macro entry, or New button in the library header. No standalone sidebar icon. Always saves to `APPDATA/Photyx/Macros/`.

### 8.4 Viewer Region

The dominant UI element. Viewer-region components (image viewer, Analysis Graph, Analysis Results, ContourHeatmap) are mutually exclusive and controlled via `ui.showView()` in the view registry. See `photyx_ui_patterns.md` Pattern 6.

- Zoom: Fit (default), 25%, 50%, 100%, 200% — keyboard shortcuts 0–4
- Pan: click-drag at any zoom level; momentum on release
- Pixel tracking: always-on; fires only when source pixel under cursor changes
- Star annotation overlay: drawn by ComputeFWHM; cleared on frame navigation

### 8.5 Info Panel

Always visible; two-thirds of bottom area.

- **Pixel tab:** X/Y coordinates, Raw (0.0–1.0) and Val (0–65535) readouts; RGB triplet for color images; RA/Dec if WCS keywords present; clipping indicators (red = highlight, blue = shadow)
- **Histogram tab:** Live log-scale histogram; per-channel for RGB; Median, Std Dev, Clipping % stats; ADU hover readout
- **Blink tab:** Play/Pause/Previous/Next controls; frame counter; resolution selector (12.5% / 25%); delay selector

### 8.6 Console

One-third of bottom area; always visible.

- `>` prompt; command history (up/down arrows); tab completion
- Trace / No Trace toggle in header
- Click header to expand to full-width terminal overlay (80vh, `position: fixed`, z-index 300)
- `help <command>` opens the Help Modal (upper-right, z-index 500, data from `pcodeHelp.ts`)
- All commands route through `run_script` — not `dispatch_command`

### 8.7 Analysis Graph

Viewer-region component. Triggered from Analyze menu or `ShowAnalysisGraph` console command.

- 7 metrics available; Metric 1 solid line with dots (REJECT = larger red dots), Metric 2 dotted
- Sigma bands at ±1σ/±2σ/±3σ; red dashed reject threshold line
- Y axis always scales to include the reject threshold line
- Two-line tooltip: value + flag + triggered metrics / filename
- Click dot → navigate to frame; Close → return to viewer

### 8.8 Analysis Results

Viewer-region component. Sortable table of per-frame metrics. Very small values displayed in scientific notation. Filename truncated to `first16chars…last5chars.ext`.

### 8.9 Themes

Matrix (default, green-on-black), Dark, Light. Switching is immediate; theme persisted across sessions.

### 8.10 Keyboard Shortcuts

| Key   | Action                                 |
| ----- | -------------------------------------- |
| Space | Blink play / pause                     |
| J     | Previous frame (blink)                 |
| K     | Next frame (blink)                     |
| P     | Mark frame PASS (writes immediately)   |
| R     | Mark frame REJECT (writes immediately) |
| 0–4   | Zoom: Fit, 25%, 50%, 100%, 200%        |

### 8.11 Status Bar & Notifications

Full-width single line at bottom. Background color reflects type (neutral / blue / amber / red). Click to open Notification History. Long-running operations use `notifications.running()` pulse animation; replaced by `notifications.success()` or `notifications.error()` on completion.

### 8.12 Log Viewer

Modal overlay (Tools > Log Viewer). Left panel: log file list sorted newest first. Right panel: parsed log contents with ERROR/WARN/INFO/DEBUG level filters, auto-tail every 2 seconds, auto-scroll suspended when user scrolls up.

---

## 9. Settings & Persistence

Settings are stored in the embedded SQLite database (`photyx.db`) in the OS
app data directory (`APPDATA/Photyx/` on Windows). See
`photyx_reference.md` §5 for all settings tables. Note: earlier versions of
this spec referenced `tauri-plugin-store` — that approach was superseded by
the SQLite implementation in Phase 9.

Key items currently lost on restart (to be fixed in Phase 9): active theme
(localStorage), last used directory, Quick Launch assignments
(localStorage), AutoStretch enabled state.

**Rig profiles:** Named threshold sets for AnalyzeFrames. Multiple
profiles; active profile shown in status bar. See `photyx_reference.md`
§5.6 for defaults.

**Crash recovery:** Session recovery file written every 60 seconds. On next
launch after crash, Photyx offers to restore the previous session.

**Database backup:** Manual backup is triggered from the Tools menu. The
backup is a timestamped ZIP archive (`photyx_backup_YYYYMMDD_HHMMSS.zip`)
written to `APPDATA/Photyx/backups/`. The archive contains two items: the
raw `photyx.db` file, and a `macros/` subfolder containing each macro
exported as a plain-text `.phs` file for human-readable recovery. Automatic
scheduled backup is deferred.


---

## 10. Logging

- Location: `{APPDATA}/Photyx/logs/` (Windows: `AppData\Roaming\Photyx\logs\`)
- Rolling policy: new file per session; last 10 retained
- Levels: ERROR, WARN, INFO, DEBUG (default INFO in release, DEBUG in dev)
- Error-level events also surface in the notification bar

---

## 11. Frame Analysis & Rejection

### 11.1 Philosophy

Photyx flags obvious disasters only. Borderline frames are left for downstream tools (PixInsight SubframeSelector). Classification is session-relative — never cross-session absolute.

### 11.2 Metrics & Thresholds

See `photyx_reference.md` §4 for the full metrics table and classification rules.

### 11.3 Workflow

1. Run `AnalyzeFrames` — writes PXFLAG to each file immediately
2. Fast blink pass — red border overlay on REJECT frames provides peripheral awareness
3. Deliberate review — step manually; P / R keys override any frame's flag (written immediately)
4. Delete confirmed rejects via `DeleteRejected` or equivalent UI

---

## 12. External API (Deferred)

Local HTTP REST server via Axum. Bound to localhost only by default; port 7171. Authentication middleware stub pre-wired (passthrough by default). Endpoints: `/api/macro/run`, `/api/macro/{name}`, `/api/images`, `/api/keywords/{filename}`, `/api/status`. Deferred to post-Phase 9.

---

## 13. Development Phases

| Phase       | Status                   | Focus                                                                                                                                                 |
| ----------- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 1     | ✅ Complete               | Scaffold, plugin host, FITS reader, viewer, logging                                                                                                   |
| Phase 2     | ✅ Complete               | Blink engine, Auto-STF, zoom, pan, pixel tracking, Info Panel                                                                                         |
| Phase 3     | ✅ Complete               | photyx-xisf crate, XISF read/write, TIFF read/write, RGB display, background cache                                                                    |
| Phase 4     | ✅ Complete               | Keyword plugins, write plugins, AstroTIFF round-trip, FITS u16 fix, path resolution                                                                   |
| Phase 5     | ✅ Complete               | pcode interpreter (If/For/variables), Macro Editor, Quick Launch, GetKeyword, RunMacro, atomic writes                                                 |
| Phase 6     | ✅ Complete               | UI audit and cleanup                                                                                                                                  |
| Phase 7     | ✅ Complete               | AnalyzeFrames (7 metrics), PXFLAG, Analysis Graph, star annotations, consolePipe, blink overlay                                                       |
| Phase 8     | ✅ Substantially complete | Moment FWHM, ContourHeatmap, display pipeline refactor, LoadFile, histogram hover, keyword editor, UI pass                                            |
| **Phase 9** | 🔄 In Progress | Embedded SQLite (✅), Quick Launch persistence (✅), session history (✅), crash recovery (✅), macros migrated to SQLite (✅); remaining: analysis results persistence, threshold profiles UI, console history persistence, status bar profile indicator, settings persistence |
| Phase 10 | ⬜ Planned |  UI audit |
| Deferred    | ⏸                        | PNG/JPEG readers/writers, debayering, async dispatch, REST API, WASM analysis plugins, User plugin loading, plugin manifest system, Plugin Manager UI, User plugin loading, plugin manifest system, plugin directory, |

---

## 14. Out of Scope (v1.0)

- GPU acceleration — deferred until CPU pipeline is stable and benchmarked
- Python plugin support — WASM is the preferred extensibility path

---

*Previous version: 19 — Next review: Upon completion of Phase 9*
