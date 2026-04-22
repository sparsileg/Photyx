# Photyx — Developer Notes

**Version:** 14
**Last updated:** April 2026
**Status:** Active development — Phase 2 complete, Phase 3 substantially complete

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
│           ├── read_all_files.rs
│           ├── read_fits.rs
│           ├── read_xisf.rs
│           ├── select_directory.rs
│           ├── set_frame.rs
│           └── write_xisf.rs
├── crates/               ← Workspace crates
│   └── photyx-xisf/      ← XISF reader/writer crate (MIT OR Apache-2.0)
│       ├── src/
│       │   ├── lib.rs
│       │   ├── reader.rs
│       │   ├── writer.rs
│       │   ├── types.rs
│       │   ├── error.rs
│       │   └── compress.rs
│       └── tests/
│           └── reader_tests.rs
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
├── Cargo.toml            ← Workspace root (members: src-tauri, crates/photyx-xisf)
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

# Run photyx-xisf tests
cargo test -p photyx-xisf -- --nocapture
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

`ImageBuffer.display_width` records the actual pixel width of the display cache entry for that image, so the frontend can compute zoom thresholds dynamically without hardcoding 1200px. **`display_width` is set exclusively by AutoStretch** — never by the background cache builder. The frontend uses `display_width == 0` to detect whether AutoStretch has run for a frame.

### 3.3 AutoStretch Performance

AutoStretch operates on a display-resolution downsampled copy of the image, not the full buffer:

1. Downsample to max 1200px wide using box-filter averaging (handles NaN/Inf bad pixels)
2. Compute Auto-STF parameters per channel — RGB images get independent STF per channel
3. Apply MTF stretch in-place
4. Encode to JPEG and store in `display_cache`
5. Store computed STF parameters `(c0, m)` in `AppContext.last_stf_params` for reuse by `get_full_frame`
6. Record `display_width` in `ImageBuffer` and invalidate `full_res_cache` for this path

This is a **~50x reduction** in pixel count versus operating on the full buffer. AutoStretch takes well under 500ms for a 3008x3008 U16 image.

The shadow clip default is -2.8 (PixInsight convention). For RGB images, STF parameters are computed independently per channel, matching PixInsight's Auto-STF behavior.

### 3.4 Full-Resolution Cache

`get_full_frame` encodes the full-resolution raw buffer as a JPEG at quality 90, applying the same STF stretch parameters that AutoStretch computed (`AppContext.last_stf_params`). The result is cached in `full_res_cache` and reused on subsequent requests. The cache entry is invalidated whenever AutoStretch runs on that path. RGB images are handled correctly — each channel is stretched using the stored STF params.

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

Pixel value lookup uses the `get_pixel` Tauri command which reads directly from `image_buffers` (the raw unmodified buffer), not the JPEG display cache. This ensures the Raw and Val readouts are always accurate regardless of display compression. RGB images return R/G/B values formatted as `r/g/b`.

WCS coordinate computation (RA/Dec) is pure TypeScript math in `InfoPanel.svelte` using FITS WCS keywords from the session store. It prefers the CD matrix (`CD1_1`, `CD1_2`, `CD2_1`, `CD2_2`) and falls back to `CDELT1`/`CDELT2`. A cos(Dec) correction is applied to the RA offset.

### 3.9 Blink State Management

Multiple blink-related fields live in the `ui` store rather than component-local state:

| Field | Purpose |
|---|---|
| `blinkCached` | Whether blink cache has been built |
| `blinkCaching` | Whether blink cache build is in progress |
| `blinkPlaying` | Whether blink is actively playing |
| `blinkTabActive` | Whether the Blink tab is selected |
| `blinkModeActive` | Whether viewer is in blink display mode (true while on Blink tab, including while paused) |
| `blinkResolution` | Currently selected blink resolution ('12' or '25') |
| `blinkImageUrl` | Current blink frame data URL |

`blinkModeActive` is distinct from `blinkPlaying` — it remains true while blink is paused so the viewer maintains the blink scale and the last blink frame stays visible. It is only cleared when the user switches away from the Blink tab.

The blink filename overlay is threaded from `InfoPanel.svelte` → `+page.svelte` via an `onBlinkFrame` callback prop rather than through the store, for the same reason as pixel tracking.

### 3.10 Blink UI Jitter — Known Issue

During blink playback, the toolbar, Quick Launch bar, and other UI chrome exhibit a visible jitter. DevTools Performance profiling confirms layout shifts are being registered on every blink frame. The culprit was originally identified as `img#viewer-image` being an unsized image element causing layout reflow on src swap. Switching to a fixed-size canvas eliminated that specific culprit.

After the canvas switch, DevTools reports "Could not detect any layout shift culprits" and the CLS score is 0.01, but visual jitter persists. The remaining shifts appear to be subpixel compositor artifacts in the Tauri WebView on Windows rather than genuine DOM reflows. Further investigation is deferred — the issue does not affect functionality.

### 3.11 Background Cache Architecture

The background cache builder runs immediately after any file load operation. It builds the display cache, blink cache at 12.5%, and blink cache at 25% in a single background task using a dedicated Rayon thread pool (`num_cpus - 1` threads).

Build order: display cache first (1200px), then blink 12.5% (376px), then blink 25% (752px). This ensures display cache is ready as early as possible.

Key design decisions:
- **Box-filter downsampling** preserves fine detail better than point sampling
- **Rayon parallelism** processes all frames simultaneously
- **display_width is NOT set by the background builder** — only AutoStretch sets it, so `display_width == 0` reliably signals "not yet user-stretched"
- **`or_insert_with`** used for display cache entries — background results are only stored if AutoStretch hasn't already run for that frame
- **JPEG quality 75** for blink caches; display cache uses quality from JPEG encoder default

The frontend checks `display_width > 0` before calling AutoStretch. If the background builder has already populated `display_cache` for a frame but `display_width` is still 0, AutoStretch runs and overwrites with a properly-stretched version.

### 3.12 Histogram

The histogram is computed from raw pixel data (not the display cache). For mono images: single 256-bin histogram. For RGB images: three independent 256-bin histograms (R, G, B channels).

Median is computed as a **true median** using `select_nth_unstable_by` (O(n) partial sort), not an approximation from binned data. This is important for processed astrophotography images where nearly all pixels may land in bin 0 of a 256-bin histogram.

Std dev is computed from the full pixel set, not from bins.

The histogram canvas draws RGB channels with additive blending (lighter composite operation) for a natural RGB histogram appearance.

### 3.13 Dynamic FITS Keyword Reading

Keywords are read dynamically using raw cfitsio FFI (`ffghsp` + `ffgrec`), not a fixed list. This reads all keywords in the primary HDU header. COMMENT, HISTORY, and END records are skipped. String values are unquoted.

The keyword store (`ImageBuffer.keywords`) is a `HashMap<String, KeywordEntry>` keyed by uppercase keyword name.

### 3.14 Rayon + cfitsio Incompatibility

Parallel FITS loading using Rayon causes a `STATUS_STACK_BUFFER_OVERRUN` crash on Windows. Root cause: cfitsio's internal C state is not thread-safe across Rayon worker threads.

**Workaround:** Sequential loading is used for `ReadAllFITFiles`.
**Future fix:** Use thread-local `FitsFile` handles, one per Rayon thread, to isolate cfitsio state.

Note: `CacheFrames` and `start_background_cache` use Rayon safely because they operate only on already-loaded `Vec<f32>`/`Vec<u16>` data in `image_buffers`, not on cfitsio handles.

### 3.15 SvelteKit Configuration

The Tauri scaffold puts the Svelte source in `src/` by default. We renamed it to `src-svelte/` to avoid collision with the prototype. `svelte.config.js` has been updated accordingly.

### 3.16 Svelte A11y Warnings

Svelte's accessibility linter warnings are suppressed project-wide via `compilerOptions.warningFilter` in `svelte.config.js`. Acceptable for a desktop application.

### 3.17 photyx-xisf Crate

The XISF reader/writer lives in `crates/photyx-xisf/` as a standalone Cargo workspace member, independently licensed MIT OR Apache-2.0. It has no dependency on Photyx internals — all types are self-contained.

**Architecture:**
- `reader.rs` — `XisfReader::open()` parses the XML header only (fast). `read_image(n)` loads and decompresses pixel data on demand.
- `writer.rs` — `XisfWriter::write()` serializes pixels, optionally compresses, builds XML header with stable data block position computation, writes binary file.
- `compress.rs` — byte-shuffle, unshuffle, compress, decompress for LZ4, LZ4HC, zstd, zlib codecs.
- `types.rs` — all public types: `XisfImage`, `XisfImageMeta`, `FitsKeyword`, `XisfProperty`, `PropertyValue`, `PixelData`, `SampleFormat`, `ColorSpace`, `WriteOptions`, `Codec`, and internal `DataBlockLocation`/`CompressionInfo`.
- `error.rs` — `XisfError` with `thiserror`.

**Performance:**
- Reader uses `bytemuck::cast_slice` for zero-copy pixel deserialization (reinterprets `&[u8]` as `&[u16]`/`&[f32]` directly on little-endian systems). 38-second read time reduced to under 1 second.
- Writer uses `bytemuck::cast_slice` for zero-copy pixel serialization in the same direction.
- Planar-to-interleaved and interleaved-to-planar conversion uses a generic function for all pixel types.

**What is supported:**
- Monolithic XISF files only
- Attachment, inline, and embedded data block locations
- LZ4, LZ4HC, zlib, zstd compression + byte shuffling
- UInt8, UInt16, UInt32, Float32, Float64 pixel formats (UInt32 and Float64 downconverted when loading into Photyx)
- Grayscale, RGB, CFA color spaces
- FITSKeyword block — full read and write
- XISF scalar and string Properties — read and write
- Multiple images per file (reader supports index selection)

**What is not yet supported:**
- Vector and Matrix properties (read as placeholder string, skipped on write) — deferred pending test files with astrometric solution matrices
- Table core elements
- Resolution, ICCProfile, Thumbnail core elements
- Normal (interleaved) pixel storage model — only planar supported
- Complex pixel formats (C32, C64)

**Reference implementations consulted:**
- `sergio-dr/xisf` (Python, GPL3) — primary algorithm reference for read/write
- `bcolyn/xisf4j` (Java, Apache 2.0) — secondary reference
- `wrenby/xisf-rs` (Rust, read-only) — API design reference
- `gitea.nouspiro.space/nou/libXISF` (C++, GPL3) — battle-tested reference; GPL3 so no code copied
- XISF 1.0 specification — authoritative source

**License boundary:** GPL3 references (sergio-dr, libXISF) were used to understand the format only. No GPL3 code was copied into `photyx-xisf`. The crate is independently implemented and licensed MIT OR Apache-2.0.

### 3.18 Long-Running pcode Commands Block UI — Known Issue

Long-running pcode commands (`ReadAllFITFiles`, `WriteAllXISFFiles`, etc.) block the JavaScript event loop during execution, preventing pixel tracking, console expansion, and other UI interaction while the command runs. Root cause: Tauri `invoke` is awaited synchronously in the JS dispatch path. Fix requires switching to Tauri's event system — Rust emits a completion event rather than returning a response, allowing JS to return immediately and stay responsive. Deferred to Phase 5.

### 3.19 XISF RGB Display — Fixed

XISF files with RGB color space (3-channel interleaved) were initially displaying incorrectly. Two bugs were fixed:

1. **AutoStretch** was treating RGB pixel data as mono — the downsampling loop used `v[sy * src_w + sx]` without accounting for channel stride. Fixed to use `v[(sy * src_w + sx) * channels + ch]`.
2. **Background cache builder** similarly used single-channel indexing for RGB data, producing garbled images. The background builder now correctly handles the multi-channel case.
3. **`commands.ts`** was hardcoding `colorSpace: 'Mono'` — fixed to use `channels === 3 ? 'RGB' : 'Mono'`.

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
| `start_background_cache` | Spawns background task to build display cache and both blink caches |
| `get_keywords` | Returns all keywords for current frame as a keyed map |
| `get_histogram` | Computes and returns histogram bins + stats for current frame (per-channel for RGB) |
| `get_pixel` | Returns raw pixel value(s) at source coordinates (x, y) from the raw image buffer |
| `debug_buffer_info` | Returns buffer metadata including display_width and color_space |

---

## 5. Plugins Implemented

| Plugin | Category | Status |
|---|---|---|
| SelectDirectory | File Management | ✅ Complete |
| ReadAllFITFiles | I/O Reader | ✅ Complete (sequential only) |
| ReadAllXISFFiles | I/O Reader | ✅ Complete (FITS + XISF via photyx-xisf crate) |
| ReadAllFiles | I/O Reader | ✅ Complete (loads both FITS and XISF from same directory) |
| WriteAllXISFFiles | I/O Writer | ✅ Complete (uncompressed default; compress=true for LZ4HC) |
| AutoStretch | Processing | ✅ Complete (mono and RGB, display-res only, raw buffer preserved) |
| SetFrame | Navigation | ✅ Complete |
| ClearSession | Session | ✅ Complete |
| CacheFrames | Blink | ✅ Complete (Rayon parallel, both resolutions) |
| ListKeywords | Keyword | ✅ Complete |
| GetHistogram | Analysis | ✅ Complete (mono and RGB per-channel, true median) |

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
| Long-running commands block UI | pcode invoke awaits Rust response, freezing JS; fix requires Tauri event system; deferred to Phase 5 |
| Zoom is approximate at high levels | Full-res cache uses AutoStretch STF params computed on display-res downsample; minor difference possible |
| XISF Vector/Matrix properties | Read as placeholder string, skipped on write; deferred pending test files |
| Rayon thread count not user-configurable | Hardcoded to num_cpus-1; §9.7 setting not yet wired |
| No persistent settings store | tauri-plugin-store not yet implemented (Phase 9) |
| No crash recovery | Phase 9 item |

---

## 8. Phase Completion Status

| Phase | Status | Notes |
|---|---|---|
| Phase 1 | ✅ Complete | Scaffold, plugin host, FITS reader, notification bar, logging |
| Phase 2 | ✅ Complete | Display cache, AutoStretch, blink engine, histogram, keywords, UI file browser, pixel tracking, WCS, zoom, pan, full-res cache, canvas viewer |
| Phase 3 | ✅ Substantially complete | photyx-xisf crate (reader + writer), ReadAllXISFFiles, WriteAllXISFFiles, ReadAllFiles, RGB display/histogram, background display cache, XISF optimizations |
| Phase 4-10 | ⬜ Not started | |
