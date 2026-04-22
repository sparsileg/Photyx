# Photyx — Developer Notes

**Last updated:** April 2026  
**Status:** Active development — Phase 2 substantially complete

---

## 1. Project Structure

```
Photyx/
├── src/                  ← HTML/CSS/vanilla JS prototype (reference only — not active)
├── src-svelte/           ← Svelte frontend (target stack)
│   ├── lib/
│   │   ├── commands.ts   ← Shared backend command helpers (selectDirectory, loadFiles, displayFrame, etc.)
│   │   ├── components/   ← Svelte UI components
│   │   │   ├── panels/   ← Sliding panel components
│   │   │   ├── Console.svelte
│   │   │   ├── IconSidebar.svelte
│   │   │   ├── InfoPanel.svelte
│   │   │   ├── KeywordModal.svelte
│   │   │   ├── MenuBar.svelte
│   │   │   ├── QuickLaunch.svelte
│   │   │   ├── StatusBar.svelte
│   │   │   ├── Toolbar.svelte
│   │   │   └── Viewer.svelte
│   │   └── stores/       ← Svelte writable stores
│   │       ├── notifications.ts
│   │       ├── session.ts
│   │       └── ui.ts
│   └── routes/
│       └── +page.svelte  ← Main application shell
├── src-tauri/            ← Rust backend
│   └── src/
│       ├── lib.rs        ← Tauri entry point, command handlers
│       ├── logging.rs    ← Rolling file logger (tracing + tracing-appender)
│       ├── plugin/       ← Plugin host infrastructure
│       │   ├── mod.rs    ← PhotonPlugin trait, ArgMap, PluginOutput, PluginError, ParamSpec
│       │   └── registry.rs ← Plugin registry: register, lookup, dispatch
│       ├── context/
│       │   └── mod.rs    ← AppContext, ImageBuffer, PixelData, KeywordEntry, BlinkCacheStatus
│       └── plugins/      ← Built-in native plugin implementations
│           ├── mod.rs
│           ├── auto_stretch.rs
│           ├── cache_frames.rs
│           ├── clear_session.rs
│           ├── get_histogram.rs
│           ├── list_keywords.rs
│           ├── read_fits.rs
│           ├── select_directory.rs
│           └── set_frame.rs
├── static/               ← Static assets served by Vite
│   ├── css/              ← Module CSS files (theme-neutral)
│   │   ├── console.css
│   │   ├── infopanel.css
│   │   ├── layout.css
│   │   ├── modal.css
│   │   ├── sidebar.css
│   │   ├── statusbar.css
│   │   ├── toolbar.css
│   │   └── viewer.css
│   └── themes/           ← Theme CSS files (dark, light, matrix)
├── .cargo/
│   └── config.toml       ← Sets PKG_CONFIG env vars for cfitsio (eliminates manual setup)
├── svelte.config.js
├── vite.config.js
├── package.json
└── Cargo.lock            ← Committed (binary application, not library)
```

---

## 2. Development Environment

### Prerequisites

| Tool | Version | Notes |
|---|---|---|
| Rust | stable | Install via rustup.rs |
| Node.js | 18+ | Required for Svelte/Vite |
| Tauri CLI | 2.10.1 | `cargo install tauri-cli` |
| vcpkg | latest | Required for cfitsio on Windows |

### Windows-specific: cfitsio Setup

cfitsio is installed via vcpkg on `J:\vcpkg`. The PKG_CONFIG environment variables are now set automatically via `.cargo/config.toml`:

```toml
[env]
PKG_CONFIG = "J:\\vcpkg\\installed\\x64-windows\\tools\\pkgconf\\pkg-config.exe"
PKG_CONFIG_PATH = "J:\\vcpkg\\installed\\x64-windows\\lib\\pkgconfig"
```

No manual environment variable setup is required. The `PATH` addition for running pkg-config manually from the terminal is still needed if doing that, but `cargo build` works without it.

### Running the App

```powershell
# Development (hot reload for Svelte, Rust recompiles on change)
npm run tauri dev

# Frontend only (no Tauri IPC — for UI layout work)
npm run dev
# then open http://localhost:1420
```

---

## 3. Architecture Decisions & Implementation Notes

### 3.1 Prototype vs Target Stack

A browser-based HTML/CSS/vanilla JS prototype exists in `src/`. This was built first to establish the UI layout, pcode console, and theme system. It is kept as a reference but is not the target. All active development is in `src-svelte/` (Svelte) and `src-tauri/` (Rust).

### 3.2 Display Cache Architecture

The display pipeline keeps raw pixel data strictly separate from display data:

```
AppContext
├── image_buffers: HashMap<path, ImageBuffer>   ← raw pixels, original bit depth, NEVER modified
├── display_cache: HashMap<path, Vec<u8>>       ← display-res (max 1200px wide) JPEG bytes
├── blink_cache_12: HashMap<path, Vec<u8>>      ← blink-res 12.5% JPEG bytes (~376px wide)
└── blink_cache_25: HashMap<path, Vec<u8>>      ← blink-res 25% JPEG bytes (~752px wide)
```

This is a design rule: **display plugins read from `image_buffers` and write to caches. They never modify `image_buffers`.**

`get_current_frame` serves from `display_cache`. `get_blink_frame` serves from `blink_cache_12` or `blink_cache_25` based on the resolution parameter.

### 3.3 AutoStretch Performance

AutoStretch operates on a display-resolution downsampled copy of the image, not the full buffer:

1. Downsample to max 1200px wide using box-filter averaging (handles NaN/Inf bad pixels)
2. Compute Auto-STF parameters on the downsampled data (~180k pixels for 3008×3008) — no sampling needed at this size
3. Apply MTF stretch in-place
4. JPEG encode and store in `display_cache`

This is a **~50x reduction** in pixel count versus operating on the full buffer. AutoStretch takes well under 500ms for a 3008×3008 U16 image.

The shadow clip default is -2.8 (PixInsight convention), not 0.0 as originally implemented.

### 3.4 Blink Cache Architecture

Both blink resolutions (12.5% and 25%) are pre-rendered into `blink_cache_12` and `blink_cache_25` automatically in the background immediately after `ReadAllFITFiles` completes. Key design decisions:

- **Box-filter downsampling** preserves fine detail (thin clouds, gradients) better than point sampling
- **Rayon parallelism** processes all frames simultaneously using a dedicated thread pool
- **Reserved core**: thread pool uses `num_cpus - 1` threads to leave one core for UI/system
- **JPEG quality 75** — sufficient for blink quality assessment
- **Both resolutions cached**: whichever the user selects in the UI, it's already ready

Cache build time for 64 × 3008×3008 U16 frames: approximately 3-8 seconds depending on CPU.

Background cache flow in `lib.rs`:
1. `loadFiles()` in frontend calls `ReadAllFITFiles` → files loaded into `image_buffers`
2. Frontend immediately calls `start_background_cache` Tauri command
3. `start_background_cache` spawns an async task via `tauri::async_runtime::spawn`
4. Task builds a dedicated Rayon thread pool, collects pixel data snapshots (clones), releases mutex
5. Processes all frames in parallel, re-acquires mutex only to store results per resolution
6. Sets `blink_cache_status = Ready` when complete
7. Frontend polls `get_blink_cache_status` when Play is pressed

### 3.5 Dynamic FITS Keyword Reading

Keywords are read dynamically using raw cfitsio FFI (`ffghsp` + `ffgrec`), not a fixed list. This reads all keywords in the primary HDU header. COMMENT, HISTORY, and END records are skipped. String values are unquoted.

The keyword store (`ImageBuffer.keywords`) is a `HashMap<String, KeywordEntry>` keyed by uppercase keyword name.

### 3.6 Rayon + cfitsio Incompatibility

Parallel FITS loading using Rayon causes a `STATUS_STACK_BUFFER_OVERRUN` crash on Windows. Root cause: cfitsio's internal C state is not thread-safe across Rayon worker threads.

**Workaround:** Sequential loading is used for `ReadAllFITFiles`.
**Future fix:** Use thread-local `FitsFile` handles, one per Rayon thread, to isolate cfitsio state.

Note: `CacheFrames` and `start_background_cache` use Rayon safely because they operate only on already-loaded `Vec<f32>`/`Vec<u16>` data in `image_buffers`, not on cfitsio handles.

### 3.7 SvelteKit Configuration

The Tauri scaffold puts the Svelte source in `src/` by default. We renamed it to `src-svelte/` to avoid collision with the prototype. `svelte.config.js` has been updated accordingly.

### 3.8 Svelte A11y Warnings

Svelte's accessibility linter warnings are suppressed project-wide via `compilerOptions.warningFilter` in `svelte.config.js`. Acceptable for a desktop application.

### 3.9 Blink State Management

`blinkCached` and `blinkCaching` live in the `ui` store (not component-local `$state`) to survive Svelte component re-renders triggered by reactive updates during async cache builds. Component-local `$state` variables were reset by re-renders mid-async-operation, causing repeated cache builds.

`blinkPlaying` is similarly in the `ui` store so `Toolbar.svelte` can disable zoom controls during playback without prop drilling.

### 3.10 Zoom Implementation

Zoom levels are implemented by setting explicit pixel dimensions on the `<img>` element (not CSS transform scale), so the scroll container correctly reports overflow and scrollbars work to full image edges.

The display image is max 1200px wide. True 100% zoom scales by `sourceWidth / 1200` to approximate one source pixel = one screen pixel. This is an approximation — true pixel-perfect zoom requires the pyramid cache (Phase 2 spec §12.3).

Zoom controls are disabled while the Blink tab is active (`$ui.blinkTabActive`).

---

## 4. Tauri Commands (Implemented)

| Command | Description |
|---|---|
| `dispatch_command` | Dispatches a pcode command to the plugin registry |
| `list_plugins` | Returns list of registered plugin names |
| `get_session` | Returns current session state (directory, file list, current frame) |
| `get_current_frame` | Returns current image as JPEG data URL from display cache |
| `get_blink_frame` | Returns a blink frame as JPEG data URL from blink cache (by index + resolution) |
| `get_blink_cache_status` | Returns blink cache build status: idle / building / ready |
| `start_background_cache` | Spawns background task to build both blink caches |
| `get_keywords` | Returns all keywords for current frame as a keyed map |
| `get_histogram` | Computes and returns histogram bins + stats for current frame |
| `debug_buffer_info` | Returns buffer metadata for debugging |

---

## 5. Plugins Implemented

| Plugin | Category | Status |
|---|---|---|
| SelectDirectory | File Management | ✅ Complete |
| ReadAllFITFiles | I/O Reader | ✅ Complete (sequential only) |
| AutoStretch | Processing | ✅ Complete (display-res only, raw buffer preserved) |
| SetFrame | Navigation | ✅ Complete |
| ClearSession | Session | ✅ Complete |
| CacheFrames | Blink | ✅ Complete (Rayon parallel, both resolutions) |
| ListKeywords | Keyword | ✅ Complete |
| GetHistogram | Analysis | ✅ Complete |

---

## 6. UI State Store (`ui.ts`) — Key Fields

| Field | Purpose |
|---|---|
| `theme` | Active theme (dark / light / matrix), persisted to localStorage |
| `activePanel` | Currently open sidebar panel |
| `zoomLevel` | Current zoom level |
| `frameRefreshToken` | Incremented to trigger viewer frame reload |
| `viewerClearToken` | Incremented to clear viewer and restore starfield |
| `consoleExpanded` | Whether console history is expanded |
| `blinkImageUrl` | Current blink frame data URL (null when not blinking) |
| `blinkCached` | Whether blink cache has been built |
| `blinkCaching` | Whether blink cache build is in progress |
| `blinkPlaying` | Whether blink is actively playing |
| `blinkTabActive` | Whether the Blink tab is currently selected (disables zoom) |
| `keywordModalOpen` | Whether the keyword modal dialog is open |

---

## 7. Known Issues & Deferred Items

| Issue | Notes |
|---|---|
| cfitsio parallel loading crashes | Thread-safety issue — sequential loading used for now |
| Zoom is approximate above Fit | Display image is 1200px wide; true pixel-perfect zoom needs pyramid cache |
| Blink tab disables zoom | By design — zoom re-enables when switching to other tabs |
| `get_current_frame` before AutoStretch | Returns error if display cache not populated — user must select a file first |
| Rayon thread count not user-configurable | Hardcoded to num_cpus-1; §9.7 setting not yet wired |
| No persistent settings store | tauri-plugin-store not yet implemented (Phase 9) |
| No crash recovery | Phase 9 item |

---

## 8. Phase Completion Status

| Phase | Status | Notes |
|---|---|---|
| Phase 1 | ✅ Complete | Scaffold, plugin host, FITS reader, notification bar, logging |
| Phase 2 | 🔄 Substantially complete | Display cache, AutoStretch, blink engine, histogram, keywords, UI file browser |
| Phase 3–10 | ⬜ Not started | |

### Phase 2 Remaining Items

- Pixel tracking (mouse position → pixel value readout in Info Panel)
- WCS coordinate display in pixel tracker
- Pyramid cache for true zoom
- Rayon parallel FITS loading (blocked by cfitsio thread safety)
