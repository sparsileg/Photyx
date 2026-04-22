# Photyx — Developer Notes

**Version:** 13
**Last updated:** April 2026
**Status:** Active development — Phase 2 complete, Phase 3 starting

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
├── display_cache: HashMap<path, Vec<u8>>       ← display-res JPEG bytes (width = min(src_width, 1200))
├── full_res_cache: HashMap<path, Vec<u8>>      ← full-resolution JPEG bytes, built on demand for high zoom
├── blink_cache_12: HashMap<path, Vec<u8>>      ← blink-res 12.5% JPEG bytes (~376px wide)
└── blink_cache_25: HashMap<path, Vec<u8>>      ← blink-res 25% JPEG bytes (~752px wide)
```

This is a design rule: **display plugins read from `image_buffers` and write to caches. They never modify `image_buffers`.**

`get_current_frame` serves from `display_cache`. `get_full_frame` serves from `full_res_cache` (built on demand, cached thereafter). `get_blink_frame` serves from `blink_cache_12` or `blink_cache_25` based on the resolution parameter.

`ImageBuffer.display_width` records the actual pixel width of the display cache entry for that image, so the frontend can compute zoom thresholds dynamically without hardcoding 1200px.

### 3.3 AutoStretch Performance

AutoStretch operates on a display-resolution downsampled copy of the image, not the full buffer:

1. Downsample to max 1200px wide using box-filter averaging (handles NaN/Inf bad pixels)
2. Compute Auto-STF parameters on the downsampled data (~180k pixels for 3008×3008) — no sampling needed at this size
3. Apply MTF stretch in-place
4. JPEG encode and store in `display_cache`
5. Store computed STF parameters `(c0, m)` in `AppContext.last_stf_params` for reuse by `get_full_frame`
6. Record `display_width` in `ImageBuffer` and invalidate `full_res_cache` for this path

This is a **~50x reduction** in pixel count versus operating on the full buffer. AutoStretch takes well under 500ms for a 3008×3008 U16 image.

The shadow clip default is -2.8 (PixInsight convention), not 0.0 as originally implemented.

### 3.4 Full-Resolution Cache

`get_full_frame` encodes the full-resolution raw buffer as a JPEG at quality 90, applying the same STF stretch parameters that AutoStretch computed (`AppContext.last_stf_params`). The result is cached in `full_res_cache` and reused on subsequent requests. The cache entry is invalidated whenever AutoStretch runs on that path.

The `needsFullRes` derived in `Viewer.svelte` computes whether the current zoom level would upscale the display cache image beyond its native resolution, taking into account both the actual viewer pixel width and the zoom factor. When `needsFullRes` transitions from false to true, `loadFullFrame()` is called; when it transitions back, `loadCurrentFrame()` restores the display cache version.

Full-res frames are JPEG encoded (not lossless) — this is disclosed to the user via the disclaimer bar at the top of the viewer. Pixel tracking always reads from the raw buffer via `get_pixel`, not from the JPEG display.

### 3.5 Canvas-Based Image Display

The image viewer uses an HTML5 canvas element (`#viewer-image-canvas`) rather than an `<img>` tag for displaying frames. This eliminates layout shifts caused by image src swaps, which were causing the toolbar and other UI chrome to jitter during blink playback.

Key design points:
- The canvas is always fixed size (matches the viewer viewport exactly) — it never resizes, so no layout reflow occurs
- `createImageBitmap()` + `drawImage()` handles all zoom and fit math — the compositor manages rendering independently from the DOM layout engine
- The current `ImageBitmap` is retained in memory (`currentBitmap`) so zoom changes can trigger a redraw without re-fetching from Rust
- Blink frames are drawn to the canvas via the `$ui.blinkImageUrl` effect — no img src swap, no layout involvement

### 3.6 Zoom Implementation

Zoom levels are implemented via `drawImage()` math in `Viewer.svelte`. The canvas is always viewport-sized; zoom is achieved by scaling the drawn image dimensions:

- **Fit** — `scale = min(canvasWidth / bitmapWidth, canvasHeight / bitmapHeight)`, image centered
- **25% / 50% / 100% / 200%** — `scale = zoomFactor * (sourceWidth / bitmapWidth)`, image centered

The zoom threshold between display cache and full-res cache is computed dynamically:
- At Fit zoom: full-res needed if `viewerWidth > displayCacheWidth`
- At other zoom levels: full-res needed if `zoomFactor * sourceWidth > displayCacheWidth`

Zoom controls are disabled while the Blink tab is active (`$ui.blinkTabActive`). Pan is reset when zoom level changes or a new frame loads.

### 3.7 Pan Implementation

Panning is implemented as `panX`/`panY` offsets applied to the centered draw position in `getDrawRect()`. Pan is only active at zoom levels above Fit.

- **Direct manipulation model**: image moves in the same direction as the mouse drag (not inverse)
- **Pan limits**: `clampPan()` prevents the image edge from going beyond the canvas edge. If the image is smaller than the canvas in a dimension, pan is locked to zero in that dimension
- **Momentum**: on mouse release, instantaneous velocity is captured and a `requestAnimationFrame` decay loop applies friction (`FRICTION = 0.88`) until velocity falls below `MIN_VELOCITY = 0.3px/frame`. Momentum respects pan limits — velocity is killed when an edge is hit
- Pan is reset on new frame load, zoom change, and session clear

### 3.8 Pixel Tracking

Mouse pixel tracking is always-on when the viewer has an image. It fires only when the source pixel under the cursor changes (pixel-change debounce), keeping IPC calls to a minimum.

Mouse coordinates are passed from `Viewer.svelte` → `+page.svelte` → `InfoPanel.svelte` as props rather than through the `ui` store. Writing mouse coordinates to the store on every mousemove caused a reactive storm that wiped the UI — the prop pattern keeps the store out of the hot path entirely.

Pixel value lookup uses the `get_pixel` Tauri command which reads directly from `image_buffers` (the raw unmodified buffer), not the JPEG display cache. This ensures the Raw and Val readouts are always accurate regardless of display compression.

WCS coordinate computation (RA/Dec) is pure TypeScript math in `InfoPanel.svelte` using FITS WCS keywords from the session store. It prefers the CD matrix (`CD1_1`, `CD1_2`, `CD2_1`, `CD2_2`) and falls back to `CDELT1`/`CDELT2`. A cos(Dec) correction is applied to the RA offset.

### 3.9 Blink State Management

Multiple blink-related fields live in the `ui` store rather than component-local state:

| Field | Purpose |
|---|---|
| `blinkCached` | Whether blink cache has been built |
| `blinkCaching` | Whether blink cache build is in progress |
| `blinkPlaying` | Whether blink is actively playing |
| `blinkTabActive` | Whether the Blink tab is selected |
| `blinkModeActive` | Whether the viewer is in blink display mode (true while on Blink tab, including while paused) |
| `blinkResolution` | Currently selected blink resolution ('12' or '25') |
| `blinkImageUrl` | Current blink frame data URL |

`blinkModeActive` is distinct from `blinkPlaying` — it remains true while blink is paused so the viewer maintains the blink scale and the last blink frame stays visible. It is only cleared when the user switches away from the Blink tab.

The blink filename overlay is threaded from `InfoPanel.svelte` → `+page.svelte` via an `onBlinkFrame` callback prop rather than through the store, for the same reason as pixel tracking.

### 3.10 Blink UI Jitter — Known Issue

During blink playback, the toolbar, Quick Launch bar, and other UI chrome exhibit a visible jitter. DevTools Performance profiling confirms layout shifts are being registered on every blink frame. The culprit was originally identified as `img#viewer-image` being an unsized image element causing layout reflow on src swap. Switching to a fixed-size canvas eliminated that specific culprit.

After the canvas switch, DevTools reports "Could not detect any layout shift culprits" and the CLS score is 0.01, but visual jitter persists. The remaining shifts appear to be subpixel compositor artifacts in the Tauri WebView on Windows rather than genuine DOM reflows. Further investigation is deferred — the issue does not affect functionality.

### 3.11 Blink Cache Architecture

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

### 3.12 Dynamic FITS Keyword Reading

Keywords are read dynamically using raw cfitsio FFI (`ffghsp` + `ffgrec`), not a fixed list. This reads all keywords in the primary HDU header. COMMENT, HISTORY, and END records are skipped. String values are unquoted.

The keyword store (`ImageBuffer.keywords`) is a `HashMap<String, KeywordEntry>` keyed by uppercase keyword name.

### 3.13 Rayon + cfitsio Incompatibility

Parallel FITS loading using Rayon causes a `STATUS_STACK_BUFFER_OVERRUN` crash on Windows. Root cause: cfitsio's internal C state is not thread-safe across Rayon worker threads.

**Workaround:** Sequential loading is used for `ReadAllFITFiles`.
**Future fix:** Use thread-local `FitsFile` handles, one per Rayon thread, to isolate cfitsio state.

Note: `CacheFrames` and `start_background_cache` use Rayon safely because they operate only on already-loaded `Vec<f32>`/`Vec<u16>` data in `image_buffers`, not on cfitsio handles.

### 3.14 SvelteKit Configuration

The Tauri scaffold puts the Svelte source in `src/` by default. We renamed it to `src-svelte/` to avoid collision with the prototype. `svelte.config.js` has been updated accordingly.

### 3.15 Svelte A11y Warnings

Svelte's accessibility linter warnings are suppressed project-wide via `compilerOptions.warningFilter` in `svelte.config.js`. Acceptable for a desktop application.

### 3.16 Phase 3 XISF Strategy

For the XISF reader, the plan is to use the `xisf-rs` crate (published as `xisf-rs` on crates.io, source at `github.com/wrenby/xisf`) as a Cargo dependency. It is a read-only implementation currently but covers the pixel formats, compression algorithms, XISF Properties, and FITS Keywords needed for typical astrophotography files.

For the XISF writer, the `sergio-dr/xisf` Python library (`github.com/sergio-dr/xisf`) will be used as a reference implementation to port to Rust. It supports full read/write including LZ4, LZ4HC, zlib, and zstd compression with byte-shuffling, and is clean enough to port function by function.

---

## 4. Tauri Commands (Implemented)

| Command | Description |
|---|---|
| `dispatch_command` | Dispatches a pcode command to the plugin registry |
| `list_plugins` | Returns list of registered plugin names |
| `get_session` | Returns current session state (directory, file list, current frame) |
| `get_current_frame` | Returns current image as JPEG data URL from display cache |
| `get_full_frame` | Returns current image as full-resolution JPEG data URL, with STF stretch applied; cached after first call |
| `get_blink_frame` | Returns a blink frame as JPEG data URL from blink cache (by index + resolution) |
| `get_blink_cache_status` | Returns blink cache build status: idle / building / ready |
| `start_background_cache` | Spawns background task to build both blink caches |
| `get_keywords` | Returns all keywords for current frame as a keyed map |
| `get_histogram` | Computes and returns histogram bins + stats for current frame |
| `get_pixel` | Returns raw pixel value(s) at source coordinates (x, y) from the raw image buffer |
| `debug_buffer_info` | Returns buffer metadata including display_width for debugging |

---

## 5. Plugins Implemented

| Plugin | Category | Status |
|---|---|---|
| SelectDirectory | File Management | ✅ Complete |
| ReadAllFITFiles | I/O Reader | ✅ Complete (sequential only) |
| AutoStretch | Processing | ✅ Complete (display-res only, raw buffer preserved, STF params stored) |
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
| `blinkImageUrl` | Current blink frame data URL (null when not in blink mode) |
| `blinkCached` | Whether blink cache has been built |
| `blinkCaching` | Whether blink cache build is in progress |
| `blinkPlaying` | Whether blink is actively playing |
| `blinkTabActive` | Whether the Blink tab is currently selected |
| `blinkModeActive` | Whether viewer is in blink display mode (true while on Blink tab including paused) |
| `blinkResolution` | Currently selected blink resolution ('12' = 12.5%, '25' = 25%) |
| `keywordModalOpen` | Whether the keyword modal dialog is open |

---

## 7. Known Issues & Deferred Items

| Issue | Notes |
|---|---|
| cfitsio parallel loading crashes | Thread-safety issue — sequential loading used for now |
| Blink UI jitter | Toolbar/Quick Launch chrome jitters during blink; DevTools CLS = 0.01, culprit undetected after canvas switch; suspected Tauri WebView compositor artifact on Windows; deferred |
| Full-res frames are JPEG not lossless | Disclosed via disclaimer bar; pixel readout always uses raw buffer |
| Zoom is approximate at high levels | Full-res cache uses AutoStretch STF params which were computed on the display-res downsample; minor difference possible |
| `get_current_frame` before AutoStretch | Returns error if display cache not populated — user must select a file first |
| Rayon thread count not user-configurable | Hardcoded to num_cpus-1; §9.7 setting not yet wired |
| No persistent settings store | tauri-plugin-store not yet implemented (Phase 9) |
| No crash recovery | Phase 9 item |

---

## 8. Phase Completion Status

| Phase | Status | Notes |
|---|---|---|
| Phase 1 | ✅ Complete | Scaffold, plugin host, FITS reader, notification bar, logging |
| Phase 2 | ✅ Complete | Display cache, AutoStretch, blink engine, histogram, keywords, UI file browser, pixel tracking, WCS, zoom, pan, full-res cache, canvas viewer |
| Phase 3 | 🔄 Starting | XISF reader (xisf-rs crate), XISF writer (port from sergio-dr/xisf Python) |
| Phase 4–10 | ⬜ Not started | |
