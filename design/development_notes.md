# Photyx — Developer Notes

**Version:** 21
**Last updated:** 27 April 2026
**Status:** Active development — Phase 8 substantially complete; ContourHeatmap and display pipeline refactor complete

---

## 1. Project Structure

```
Photyx/
├── src-svelte/           ← Svelte frontend (target stack)
│   ├── lib/
│   │   ├── commands.ts   ← Shared backend command helpers (selectDirectory, loadFiles, displayFrame, etc.)
│   │   ├── pcodeCommands.ts   ← Single source of truth for all pcode command names (imported by Console and MacroEditor)
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
│   │   │   ├── IconSidebar.svelte
│   │   │   ├── InfoPanel.svelte
│   │   │   ├── KeywordModal.svelte
│   │   │   ├── LogViewer.svelte
│   │   │   ├── MenuBar.svelte
│   │   │   ├── QuickLaunch.svelte
│   │   │   ├── StatusBar.svelte
│   │   │   ├── Toolbar.svelte
│   │   │   └── Viewer.svelte
│   │   └── stores/       ← Svelte writable stores
│   │       ├── consoleHistory.ts
│   │       ├── notifications.ts
│   │       ├── quickLaunch.ts
│   │       ├── session.ts
│   │       └── ui.ts
│   └── routes/
│       └── +page.svelte  ← Main application shell
├── src-tauri/            ← Rust backend
│   └── src/
│       ├── lib.rs        ← Tauri entry point, command handlers
│       ├── logging.rs    ← Rolling file logger (tracing + tracing-appender)
│       ├── utils.rs      ← Shared utilities: resolve_path, get_log_dir, get_macros_dir
│       ├── plugin/       ← Plugin host infrastructure
│       │   ├── mod.rs    ← PhotonPlugin trait, ArgMap, PluginOutput, PluginError, ParamSpec
│       │   └── registry.rs ← Plugin registry: register, lookup, dispatch, list_with_details
│       ├── context/
│       │   └── mod.rs    ← AppContext, ImageBuffer, PixelData, KeywordEntry, BlinkCacheStatus
│       ├── analysis/     ← Pure computation modules (no Tauri dependencies)
│       │   ├── mod.rs
│       │   ├── background.rs
│       │   ├── eccentricity.rs
│       │   ├── fwhm.rs
│       │   ├── metrics.rs
│       │   ├── profiles.rs
│       │   ├── session_stats.rs
│       │   └── stars.rs
│       ├── pcode/        ← pcode interpreter
│       │   ├── mod.rs
│       │   └── tokenizer.rs
│       └── plugins/      ← Built-in native plugin implementations
│           ├── mod.rs
│           ├── analyze_frames.rs
│           ├── auto_stretch.rs
│           ├── background_median.rs
│           ├── cache_frames.rs
│           ├── clear_session.rs
│           ├── compute_eccentricity.rs
│           ├── compute_fwhm.rs
│           ├── contour_heatmap.rs
│           ├── get_histogram.rs
│           ├── image_reader.rs
│           ├── highlight_clipping.rs
│           ├── keywords.rs
│           ├── list_keywords.rs
│           ├── read_all_files.rs
│           ├── read_fits.rs
│           ├── read_tiff.rs
│           ├── read_xisf.rs
│           ├── run_macro.rs
│           ├── scripting.rs
│           ├── select_directory.rs
│           ├── set_frame.rs
│           ├── star_count.rs
│           ├── write_current_files.rs
│           ├── write_fits.rs
│           ├── write_frame.rs
│           ├── write_tiff.rs
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
│   │   ├── analysisgraph.css
│   │   ├── analysisresults.css
│   │   ├── console.css
│   │   ├── infopanel.css
│   │   ├── layout.css
│   │   ├── logviewer.css
│   │   ├── macroeditor.css
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

**Note:** The vanilla JS prototype (`src/`) has been deleted. It served its purpose establishing the UI layout and theme system and has been fully superseded by the Svelte implementation.

---

## 2. Development Environment

### Prerequisites

| Tool      | Version | Notes                           |
| --------- | ------- | ------------------------------- |
| Rust      | stable  | Install via rustup.rs           |
| Node.js   | 18+     | Required for Svelte/Vite        |
| Tauri CLI | 2.10.1  | `cargo install tauri-cli`       |
| vcpkg     | latest  | Required for cfitsio on Windows |

### Development Stack

Photyx uses three environments simultaneously:

- **Rust / Cargo** — compiles `src-tauri/`. Recompile required for any `.rs` file change, `Cargo.toml`, `capabilities/default.json`, or `tauri.conf.json`.
- **Node.js / Vite / Svelte** — hot-reloads `.svelte` and `.ts` file changes instantly. CSS files in `static/` are NOT hot-reloaded — requires manual browser refresh.
- **Tauri CLI** — orchestrates both. `npm run tauri dev` starts Vite dev server and Rust backend simultaneously. The Tauri WebView points to the Vite dev server.

### Tauri Permissions

Frontend access to OS APIs (filesystem, dialogs, etc.) requires explicit permission entries in `src-tauri/capabilities/default.json`. This file is the single source of truth for what the frontend is allowed to do. If a Tauri plugin API call fails silently with no console error, a missing permission here is the first thing to check.

Current permissions granted:

- `core:default` — core Tauri APIs
- `opener:default` — open URLs/files externally
- `dialog:allow-open` — file open dialog
- `dialog:allow-save` — file save dialog
- `dialog:allow-confirm` — confirmation dialogs
- `core:window:allow-close` — window close
- `fs:allow-read-text-file` with scope `$APPDATA/Photyx/**`, `$HOME/**`, `$DOCUMENT/**`
- `fs:allow-write-text-file` with scope `$APPDATA/Photyx/**`, `$HOME/**`, `$DOCUMENT/**`

**Important:** Filesystem permissions require path scopes. Without explicit scope entries, `readTextFile` and `writeTextFile` will fail with "forbidden path" errors for paths outside the default allowed set (e.g. AppData on Windows).

---

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

The vanilla JS prototype that previously lived in `src/` has been deleted. It was built first to establish the UI layout, pcode console, and theme system, and has been fully superseded by the Svelte/Tauri implementation. All active development is in `src-svelte/` (Svelte) and `src-tauri/` (Rust).

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

AutoStretch operates on a dynamic display-resolution downsampled copy of the image, not the full buffer:

1. Downsample to max 1200px wide using box-filter averaging (handles NaN/Inf bad pixels)
2. Compute Auto-STF parameters per channel — RGB images get independent STF per channel
3. Apply MTF stretch in-place
4. Encode to JPEG and store in `display_cache`
5. Store computed STF parameters `(c0, m)` in `AppContext.last_stf_params` for reuse by `get_full_frame`
6. Record `display_width` in `ImageBuffer` and invalidate `full_res_cache` for this path

This is a **~50x reduction** in pixel count versus operating on the full buffer. AutoStretch takes well under 500ms for a 3008x3008 U16 image.

The shadow clip default is -2.8 (PixInsight convention). The target background default is 0.15 (reduced from the PixInsight default of 0.25 for better visual results with astrophotography data). Both values are exposed as constants at the top of `auto_stretch.rs` for easy tuning:

```rust
const DEFAULT_SHADOW_CLIP:       f32 = -2.8;
const DEFAULT_TARGET_BACKGROUND: f32 = 0.15;
```

For RGB images, STF parameters are computed independently per channel, matching PixInsight's Auto-STF behavior.

**Note:** AutoStretch assumes linear (unstretched) input. Applying it to already-stretched images (e.g., exported 8-bit TIFFs) will produce extreme results. This is expected behavior — AutoStretch is designed for linear astrophotography data.

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

| Field             | Purpose                                                                                   |
| ----------------- | ----------------------------------------------------------------------------------------- |
| `blinkCached`     | Whether blink cache has been built                                                        |
| `blinkCaching`    | Whether blink cache build is in progress                                                  |
| `blinkPlaying`    | Whether blink is actively playing                                                         |
| `blinkTabActive`  | Whether the Blink tab is selected                                                         |
| `blinkModeActive` | Whether viewer is in blink display mode (true while on Blink tab, including while paused) |
| `blinkResolution` | Currently selected blink resolution ('12' or '25')                                        |
| `blinkImageUrl`   | Current blink frame data URL                                                              |

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

The Tauri scaffold puts the Svelte source in `src/` by default. We renamed it to `src-svelte/` to avoid collision with the prototype (now deleted). `svelte.config.js` has been updated accordingly.

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

Long-running pcode commands (`ReadAllFITFiles`, `WriteAllXISFFiles`, etc.) block the JavaScript event loop during execution, preventing pixel tracking, console expansion, and other UI interaction while the command runs. Root cause: Tauri `invoke` is awaited synchronously in the JS dispatch path. Fix requires switching to Tauri's event system — Rust emits a completion event rather than returning a response, allowing JS to return immediately and stay responsive. Deferred.

### 3.19 XISF RGB Display — Fixed

XISF files with RGB color space (3-channel interleaved) were initially displaying incorrectly. Two bugs were fixed:

1. **AutoStretch** was treating RGB pixel data as mono — the downsampling loop used `v[sy * src_w + sx]` without accounting for channel stride. Fixed to use `v[(sy * src_w + sx) * channels + ch]`.
2. **Background cache builder** similarly used single-channel indexing for RGB data, producing garbled images. The background builder now correctly handles the multi-channel case.
3. **`commands.ts`** was hardcoding `colorSpace: 'Mono'` — fixed to use `channels === 3 ? 'RGB' : 'Mono'`.

### 3.20 TIFF Reader

The TIFF reader uses the `tiff` crate (pure Rust, no native dependencies — no vcpkg required). It supports U8, U16, U32, and F32 pixel formats and Grayscale and RGB color spaces.

U32 TIFFs are downconverted to U16 by taking the high 16 bits (`v >> 16`). This matches the behavior of the XISF reader for U32 data. U32 is not a supported internal pixel type (§5.5).

Already-stretched images (e.g., 8-bit exported TIFFs from PixInsight or similar tools) will appear blown out when AutoStretch is applied, because AutoStretch assumes linear input. This is expected behavior, not a bug.

### 3.21 FITS Writer — Signed/Unsigned 16-bit

FITS `BITPIX = 16` is signed 16-bit by convention. To write unsigned 16-bit data (u16), Photyx casts to `i16` before writing and adds `BZERO = 32768` / `BSCALE = 1` keywords so compliant readers reconstruct the correct unsigned values. On read-back, Photyx reads as `i32` (wide enough to hold the BZERO-offset values 0–65535) and clamps to u16. This handles both ASIAir-style unsigned files and other signed BITPIX=16 files correctly.

### 3.22 Blink Cache Quality

The background cache builder now builds blink caches (12.5% and 25%) by decoding the already-stretched display cache JPEG and resizing it using a Triangle filter, rather than processing raw pixel data independently. This ensures blink frames are visually consistent with the display cache — same stretch parameters, same tone curve. Building is faster since JPEG decode + resize is much cheaper than raw pixel STF computation.

### 3.23 AstroTIFF Keyword Round-Trip

`read_tiff_file` now parses the TIFF `ImageDescription` tag (tag 270) on load. Keywords are stored in FITS-style `NAME    = VALUE / comment` format, one per line. On read, lines are parsed by checking for `=` at position 8, then splitting on ` /` for the comment. This completes the AstroTIFF keyword round-trip: keywords written by `write_tiff_file` survive a reload.

### 3.24 Relative Path Resolution

Write plugins (`WriteAllFITFiles`, `WriteAllXISFFiles`, `WriteAllTIFFFiles`) now resolve relative `destination` paths against `ctx.active_directory` via `crate::utils::resolve_path()`. A new `src-tauri/src/utils.rs` module holds this shared utility.

### 3.25 Window Resize Fix

The `resize()` function in `Viewer.svelte` was measuring dimensions from `starCanvas.offsetWidth/offsetHeight`, which returns 0 when the star canvas is hidden (i.e. when an image is loaded). Fixed to measure from the `viewer-wrap` container div instead, which is always visible. The function also re-acquires the 2D rendering context for both canvases after resize, since setting `canvas.width` or `canvas.height` clears the canvas and invalidates the context.

### 3.26 SelectDirectory Frontend Sync

When `SelectDirectory` is called via the pcode console or UI, the frontend now immediately clears the file list, loaded images, and viewer in the Svelte session store. Previously the stale file list from the previous session remained visible in the file browser until a new read command was run.

### 3.27 pwd Console Command

`pwd` is implemented as a client-side console command in `Console.svelte`. It reads `$session.activeDirectory` from the store and prints it directly — no backend IPC call required.

### 3.28 pcode Interpreter

The pcode interpreter lives in `src-tauri/src/pcode/` as a standalone module with no Tauri dependencies. It takes a script string, `&mut AppContext`, and `&PluginRegistry` and returns a `Vec<PcodeResult>`.

**Architecture:**

- `tokenizer.rs` — parses each line into `PcodeLine::Command`, `PcodeLine::Assignment`, or `PcodeLine::Skip`. Handles quoted values, named arguments (key=value), and comment lines (#).
- `mod.rs` — sequential executor: variable store, `$var` and `${var}` substitution in all argument values, plugin registry dispatch per line, halt-on-error behavior, `Log` command handled internally.

**Key behaviors:**

- Variables are local to the script execution and also written to `AppContext.variables`
- `Log` writes results since the previous `Log` call (segmented), not all results from the start
- Nested macros share the same `AppContext` — a called macro executes in the parent's session context
- `GLOBAL_REGISTRY` (once_cell) provides registry access to `RunMacro` without Tauri state threading
- `chrono` crate used for Log file timestamps

**`run_script` Tauri command** — executes a script string directly from the frontend (macro editor Run button), returns a `ScriptResponse` struct containing:

- `results` — array of `ScriptResult` (line_number, command, success, message)
- `session_changed` — true if any read, select, clear, or move command succeeded; frontend syncs session state
- `display_changed` — true if AutoStretch or similar display command succeeded; frontend triggers frame refresh

This eliminates command-name matching on the frontend — components simply react to the flags.

**Flow control** — the interpreter now pre-parses scripts into a block tree before execution, supporting:

- `If <expr> / Else / EndIf` — conditional blocks with `==`, `!=`, `<`, `>`, `<=`, `>=` operators (numeric and string, case-insensitive)
- `For varname = N to M / EndFor` — numeric loop; loop variable available as `$varname` inside the body
- `GetKeyword` result auto-stored into `$KEYWORDNAME` (uppercase) for use in conditionals

### 3.29 Console Expansion

The pcode console expands to a full-width overlay (60vh, 85% opacity) when the header is clicked. Key implementation details:

- Expanded console uses `position: absolute` within `#bottom-panel`, requiring `position: relative` on `#viewer-region` for correct stacking context
- The `#viewer-placeholder` fades to opacity 0 (not hidden) when console is expanded, using CSS transition matched to the console slide timing
- Font size increases from 11px to 14px for output lines in expanded state
- All three themes supported via CSS custom properties

---

### 3.30 Macro Editor Architecture

The Macro Editor (`MacroEditor.svelte`) is rendered at the `#content-area` level in `+page.svelte` rather than inside `#panel-container`. This is required because `#panel-container` uses `transform: translateX()` for its slide animation, which creates a new stacking context and breaks `position: fixed` on child elements. Rendering outside the container allows the editor to fill the full content area as an overlay.

The editor uses the backdrop technique for syntax highlighting — a `<div>` with `@html` rendered highlighted content sits behind a transparent `<textarea>`. The textarea handles all input; the backdrop provides colour. Scroll sync between the two is maintained via the `onscroll` event.

**The Macro Editor is now opened exclusively from the Macro Library panel.** It is no longer accessible via a sidebar icon. The sidebar icon for Macro Editor has been removed. Entry points are:

- Clicking **Edit** on a macro entry in the Macro Library
- Clicking **New** in the Macro Library header

The editor always saves to the Macros directory (`APPDATA/Photyx/Macros/`) — no free-form path selection. The Load button has been removed. Save As prompts for a name only (no folder picker).

**Unsaved changes detection:** The `dirty` state flag is set on any `oninput` event. When the user clicks ← Library with `dirty = true`, an inline confirmation bar appears (not a native OS dialog — see §3.31). The `lastLoadedPath` guard in the `$effect` that loads file contents prevents spurious re-runs when other reactive state changes.

### 3.31 Inline Confirmation Bar Pattern

Native OS dialogs (`window.confirm`) are not reliably available in Tauri WebView without specific permissions, and even with permissions they can be unreliable. The established pattern for destructive action confirmation is an inline bar within the component:

```svelte
{#if confirmingAction}
    <div class="confirm-bar" onclick={(e) => e.stopPropagation()}>
        <span>Message here.</span>
        <button onclick={(e) => { e.stopPropagation(); doAction(); }}>Confirm</button>
        <button onclick={(e) => { e.stopPropagation(); confirmingAction = false; }}>Cancel</button>
    </div>
{/if}
```

The `stopPropagation()` calls are mandatory — without them, click events bubble up to parent handlers and cause unexpected behavior (e.g. the editor closing on Cancel).

### 3.32 WriteCurrent Atomic Writes

`WriteCurrent` (and `WriteFIT`, `WriteTIFF`, `WriteXISF`, `WriteFrame`) use a write-to-temp-then-rename pattern for all formats. The file is written to `<originalpath>.tmp` first, then atomically renamed over the original. This:

- Ensures deleted keywords are not preserved (full rewrite from buffer, not in-place edit)
- Eliminates duplicate keyword issues caused by cfitsio's in-place `write_key` adding new records rather than updating existing ones
- Protects against partial writes leaving a corrupt file

`write_fits_inplace` is retained in `write_fits.rs` but should only be used when it is certain no keywords have been deleted from the buffer.

### 3.33 pcodeCommands.ts

`src-svelte/lib/pcodeCommands.ts` exports a single `PCODE_COMMANDS` Set that is the authoritative list of all valid pcode command names. Both `Console.svelte` (tab completion) and `MacroEditor.svelte` (syntax highlighting) import from this file. When adding, renaming, or removing commands, update only this file.

---

### 3.34 WriteFITS U16 Sign Conversion Bug (Fixed)

FITS `BITPIX=16` is signed. When writing u16 pixel data, the correct convention is to subtract 32768 before casting to i16, then write `BZERO=32768`. The original code used `v as i16` which silently reinterprets bits above 32767 as negative numbers. On read-back, adding BZERO=32768 gives the wrong result. Fixed to `(v as i32 - 32768) as i16`. This bug was causing all previously written FITS files to have corrupted pixel values (appearing stretched in histogram and analysis).

---

### 3.35 Analysis Layer Architecture

Analysis code lives in `src-tauri/src/analysis/` as pure computation modules with no Tauri or plugin dependencies:

| Module             | Purpose                                                                                                                  |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------ |
| `background.rs`    | Sigma-clipped background median, std dev, gradient (8×8 grid)                                                            |
| `stars.rs`         | Star detection: local maximum finding, flood fill, centroid, minimum 5-pixel filter                                      |
| `fwhm.rs`          | Moment-based FWHM: `2.355 * sqrt((Mxx + Myy) / 2)` — geometric mean of axes, matches PI                                  |
| `eccentricity.rs`  | Second-order intensity-weighted moments → eccentricity                                                                   |
| `metrics.rs`       | SNR estimate (signal/noise via star pixels and background std dev)                                                       |
| `profiles.rs`      | Camera pixel size lookup table and plate scale formula                                                                   |
| `session_stats.rs` | Session mean/stddev per metric, `classify_frame` returning (PxFlag, Vec<triggered>)                                      |
| `mod.rs`           | Shared types: `PxFlag`, `AnalysisResult`, `StarDetectionConfig`, `BackgroundConfig`, `SigmaClipConfig`, `to_luminance()` |

**Design rule:** `AnalyzeFrames` calls analysis functions directly in a two-pass Rayon parallel loop. Standalone plugins (`ComputeFWHM`, `CountStars`, `ComputeEccentricity`) are thin wrappers for interactive use on the current frame only.

---

### 3.36 AnalyzeFrames Classification

**PASS / REJECT only** — SUSPECT classification was removed. The philosophy is that Photyx removes extreme outliers only; PixInsight PSFSW weighting handles fine-grained quality differentiation.

**PXSCORE removed** — a weighted score that could contradict PXFLAG was actively misleading. PXFLAG is the only output.

**Current reject thresholds (`session_stats.rs` defaults):**

| Metric              | Type     | Reject |
| ------------------- | -------- | ------ |
| Background Median   | +σ       | 2.5σ   |
| Background Std Dev  | +σ       | 2.5σ   |
| Background Gradient | +σ       | 2.5σ   |
| SNR Estimate        | -σ       | 2.5σ   |
| FWHM                | +σ       | 2.5σ   |
| Star Count          | -σ       | 1.5σ   |
| Eccentricity        | absolute | 0.85   |

**triggered_by** — `classify_frame` returns `(PxFlag, Vec<String>)` where the Vec contains the names of metrics that triggered REJECT. Stored in `AnalysisResult.triggered_by` and returned by `get_analysis_results` for tooltip display in the Analysis Graph.

---

### 3.37 Analysis Graph

The Analysis Graph is a viewer-region component (`AnalysisGraph.svelte`) that replaces the image viewer when `$ui.activeView === 'analysisGraph'`. It is NOT a floating window. Key features:

- 7 metrics in Metric 1 dropdown; Metric 2 defaults to None
- Metric 1: solid line with larger red dots for REJECT, white for PASS
- Metric 2: dotted line, no dots
- Sigma bands (metric 1 only): transparent at mean, more opaque toward ±3σ
- Red dashed reject threshold line with "REJECT" label
- Two-line tooltip: line 1 = metric value + flag + triggered metrics, line 2 = filename
- Tooltip position-aware: left/center/right thirds of chart
- Click on dot: closes graph via `ui.showView(null)`, navigates to frame
- Refresh button: re-fetches from Rust, calls `resizeCanvas()` after load to fix blob rendering
- Theme-aware: reads CSS variables via `getComputedStyle` at draw time
- Data from `get_analysis_results` Tauri command

---

### 3.38 Star Annotation Overlay

`ComputeFWHM` (standalone plugin) triggers a star annotation overlay on the viewer canvas:

1. Plugin runs successfully → frontend calls `ui.refreshAnnotations()` (from Console or QuickLaunch)
2. `Viewer.svelte` watches `$ui.annotationToken` — guarded by `lastAnnotationToken` to prevent spurious re-fetches
3. On token change: calls `drawStarAnnotations()` which invokes `get_star_positions` Tauri command (re-runs star detection, returns `{cx, cy, fwhm, r}` per star)
4. Positions cached in `cachedStars` — pan/zoom/resize calls synchronous `paintStarAnnotations()` from cache only
5. `ClearAnnotations` console command or frame navigation clears the overlay

**Critical rule:** `drawStarAnnotations()` (Rust fetch) must NEVER be called from `renderBitmap()` — it runs star detection and will lock up the app during panning. Only `paintStarAnnotations()` (cache-only, synchronous) should be called from `renderBitmap()`.

**Annotation clearing** — star annotations are cleared in three places:

- `commands.ts` `displayFrame()` — cleared at the start of every frame navigation, regardless of trigger
- `InfoPanel.svelte` — cleared when the Blink tab is activated
- `Viewer.svelte` — cleared when a new blink frame URL arrives

---

### 3.39 consolePipe Store

External components that need to write to the pcode console (e.g. `QuickLaunch.svelte`, `MacroLibrary.svelte`, `MenuBar.svelte`) use the `consolePipe` writable store exported from `consoleHistory.ts`. `Console.svelte` watches it via `$effect` and appends non-null values then resets to null. See `photyx_ui_patterns.md` Pattern 4 for usage.

---

### 3.40 View Registry (showView)

Viewer-region component visibility is managed through a central registry in `ui.ts`. This replaces the previous ad-hoc boolean flags approach.

```typescript
// Single source of truth — add one entry to register a new view
export const VIEWS = [
    'analysisGraph',
    'analysisResults',
] as const;

export type ViewName = typeof VIEWS[number];
```

`UIState` contains `activeView: ViewName | null`. `null` means the image viewer is shown.

**All viewer-region visibility is controlled exclusively via `ui.showView()`:**

- `ui.showView('analysisGraph')` — show Analysis Graph
- `ui.showView('analysisResults')` — show Analysis Results
- `ui.showView(null)` — return to image viewer

Close buttons in viewer-region components always call `ui.showView(null)`. Individual boolean setters for viewer-region visibility do not exist and must not be added. See `photyx_ui_patterns.md` Pattern 6 for the full pattern.

---

### 3.41 Analysis Results

The Analysis Results component (`AnalysisResults.svelte`) is a viewer-region component that displays per-frame quality metrics as a sortable, scrollable table. It uses the same `get_analysis_results` Tauri command as the Analysis Graph.

Columns (in order): #, Filename, FWHM, Eccentricity, Star Count, SNR, Bg Median, Bg Std Dev, Bg Gradient, PXFLAG.

Filename display is truncated to `first16chars…last5chars.ext` to prevent long filenames from dominating the layout.

Sorting is client-side — clicking any column header sorts ascending; clicking again sorts descending. Sort indicator (▲/▼) appears in the header.

---

### 3.42 notifications.running() — Pulse Animation

The `running` notification type triggers a CSS pulse animation on the status bar text and icon, providing a clear visual indication that a long-running operation is in progress.

```typescript
notifications.running('AnalyzeFrames running…');
// ... operation completes ...
notifications.success('AnalyzeFrames complete.');
```

The animation is defined in `statusbar.css`:

```css
@keyframes status-pulse {
    0%   { opacity: 1; }
    50%  { opacity: 0.35; }
    100% { opacity: 1; }
}
```

Use `notifications.running()` for any operation that takes more than ~0.5 seconds and has a clear completion state. The pulse stops automatically when the next notification replaces it.

---

### 3.43 Log Viewer

The Log Viewer (`LogViewer.svelte`) is a modal overlay following the same pattern as `KeywordModal.svelte`. It is triggered from Tools > Log Viewer.

**Log directory resolution:** `list_log_files` uses `crate::utils::get_log_dir()` — the same function used by `logging.rs`. This ensures both always agree on the log location. `get_log_dir()` reads from environment variables (`APPDATA` on Windows, `HOME` on macOS/Linux) and appends `Photyx/logs/`. It does NOT use Tauri's `app_data_dir()` since that uses the bundle identifier rather than the hardcoded "Photyx" folder name.

**Auto-tail:** Polls `read_log_file` every 2 seconds. Auto-scroll is suspended when the user has scrolled up (detected by checking if scroll position is within 20px of the bottom). Auto-scroll resumes when the user scrolls back to the bottom.

**Log parsing:** Done in Rust (`parse_log_line`). Format: `{timestamp}Z  {level} {module}: {message}`. Non-conforming lines are returned with `level: "RAW"` and treated as DEBUG in the filter.

**File list:** Sorted newest first by `modified_secs`. File is not auto-opened — user must click to select.

---

### 3.44 Macro Library

The Macro Library (`MacroLibrary.svelte`) dynamically scans `APPDATA/Photyx/Macros/` for `.phs` files via the `list_macros` Tauri command. The directory is created automatically if it does not exist.

**Per-entry display:**

- Row 1: macro name, Edit button, Rename button, Delete button
- Row 2: line count, Pin button, Run button

**Tooltip:** The first contiguous block of `#` comment lines at the top of each `.phs` file is extracted by `extract_macro_tooltip()` in `lib.rs`, stripped of `#` characters, and returned as the tooltip string. Empty if no leading comments.

**Pinned state:** Derived reactively from `$quickLaunch` via a `$effect` that watches the store. When a Quick Launch entry is removed, the corresponding Pin button reverts to "Pin" automatically. The `$effect` normalizes path separators before comparing (Windows uses backslashes, the regex uses forward slashes).

**Delete protection:** If a macro is currently pinned, clicking Delete shows a warning bar rather than a confirmation bar: "Remove from Quick Launch first." The macro cannot be deleted while pinned.

**Tauri commands added:**

- `list_macros` — scans Macros directory, returns name, filename, path, line count, tooltip
- `delete_macro` — removes a `.phs` file from disk
- `rename_macro` — renames a `.phs` file; validates the new name, returns the new path
- `get_macros_dir` — returns the Macros directory path as a forward-slash string

---

### 3.45 Macro Editor (Revised)

See §3.30 for the architectural overview. Key changes from the original implementation:

- **Library-only entry** — the Macro Editor sidebar icon has been removed. The editor is opened exclusively via Edit or New in the Macro Library panel.
- **Macros directory only** — the Load button has been removed. Save always writes to `APPDATA/Photyx/Macros/`. Save As prompts for a name only (no folder picker).
- **`lastLoadedPath` guard** — the `$effect` that loads file contents is guarded by a `lastLoadedPath` variable. The effect only re-reads the file if the path has actually changed, preventing spurious re-runs when other reactive state (e.g. `confirmingLeave`) changes.
- **`ui.macroEditorFile`** — the file to open is passed via `ui.ts` state (`{ path: string; name: string } | null`). `openMacroEditor()` sets both `macroEditorFile` and `activePanel: 'macro-editor'` atomically. `showMacroLibrary()` sets `activePanel: 'macro-lib'`.
- **Unsaved changes** — handled via inline confirmation bar (Pattern 8). Discard returns to Library; Cancel dismisses the bar and keeps the editor open.

---

### 3.46 Plugin Manager

The Plugin Manager panel (`PluginManager.svelte`) displays all registered plugins with name, version, and type. Plugin type (`Native` or `WASM`) is returned from the Rust side via `list_plugins` — the `PhotonPlugin` trait has a `plugin_type()` method with a default implementation returning `"Native"`. WASM plugins will override this when implemented in Phase 10.

`list_plugins` now returns `Vec<serde_json::Value>` (name, version, plugin_type) via `registry.list_with_details()` rather than `Vec<String>`. The frontend `PluginManager.svelte` uses this structured data directly.

**Enable/Disable scope:** Enable/disable is intentionally restricted to WASM plugins only. Native plugins are compiled into the binary and cannot be disabled. The UI should reflect this — native plugin rows have no toggle, only a static "Active" status. WASM plugin rows will have a toggle when Phase 10 is implemented. Do not add enable/disable controls to native plugin rows.

---

### 3.47 Keyword Editor

The Keyword Editor (`KeywordEditor.svelte`) is now fully implemented with inline editing.

**Editing model:**

- Keyword name column is **read-only** — renaming a keyword requires delete + add (prevents accidental corruption)
- Value and comment columns are editable via double-click anywhere in the cell
- On Enter or blur: dispatches `ModifyKeyword scope=current` to Rust
- On Escape: cancels the edit

**FITS keyword constraints enforced on the frontend:**

- Keyword name: maximum 8 characters; letters, digits, hyphens, underscores only; forced uppercase
- Value + comment combined: maximum 68 characters (FITS record limit after name and `= ` prefix)
- If value + comment exceeds 68 characters, the comment is automatically truncated to fit. A `notifications.warning()` is issued. The reload after commit is called silently (`reload(true)`) so the truncation warning is not immediately overwritten.

**Write Changes button:** Calls `WriteFrame` (not `WriteCurrent`) to write only the current frame to disk. `WriteFrame` is a new built-in native plugin that writes the active frame back to its source format using the same atomic temp-rename pattern as `WriteCurrent`.

**Wide panel:** The Keyword Editor panel uses a `wide` CSS class on `#panel-container` (set via `class:wide={$ui.activePanel === 'keywords'}` in `IconSidebar.svelte`) to expand to 75vw.

---

### 3.48 WriteFrame Plugin

`WriteFrame` is a built-in native plugin that writes the currently active frame (as set by `SetFrame` / `ctx.current_frame`) back to its source file in its original format. It supports FITS, XISF, and TIFF. It uses the same atomic temp-rename pattern as `WriteCurrent`. Unsupported formats return a `UNSUPPORTED_FORMAT` error.

`WriteFrame` is distinct from `WriteCurrent`:

- `WriteCurrent` — writes all loaded frames to their source paths
- `WriteFrame` — writes only the active frame

Use `WriteFrame` when a single-frame keyword edit needs to be persisted (e.g. from the Keyword Editor). Use `WriteCurrent` for batch operations.

---

### 3.49 utils.rs — Shared Path Utilities

`src-tauri/src/utils.rs` contains shared utility functions:

- `resolve_path(path, base)` — resolves relative paths against `base`, expands `~`, handles UNC paths
- `get_log_dir()` — returns OS-appropriate log directory (`APPDATA/Photyx/logs/` on Windows)
- `get_macros_dir()` — returns OS-appropriate macros directory (`APPDATA/Photyx/Macros/` on Windows)

`logging.rs` delegates to `get_log_dir()` rather than duplicating the logic. `lib.rs` uses both `get_log_dir()` (for `list_log_files`) and `get_macros_dir()` (for `list_macros`, `get_macros_dir` command). This ensures all three always agree on directory locations.

---

### 3.50 Histogram ADU Mouse Tracking (To Do)

When the mouse moves over the histogram canvas in the Info Panel, track and display the ADU value corresponding to the mouse's horizontal position. The display should show the ADU value (0–65535 scale) at the cursor's x-position on the histogram, updated in real time as the mouse moves. This gives the user a quick way to read specific ADU values off the histogram without needing to hover over the image itself.

---

## 4. Tauri Commands (Implemented)

| Command                  | Description                                                                                                        |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| `dispatch_command`       | Dispatches a pcode command to the plugin registry                                                                  |
| `run_script`             | Executes a pcode script string; returns ScriptResponse with results, session_changed, display_changed              |
| `debug_buffer_info`      | Returns buffer metadata including display_width and color_space                                                    |
| `delete_macro`           | Deletes a .phs macro file from the Macros directory                                                                |
| `get_analysis_results`   | Returns per-frame analysis metrics, flags, triggered_by, and session stats for Analysis Graph and Analysis Results |
| `get_blink_cache_status` | Returns blink cache build status: idle / building / ready                                                          |
| `get_blink_frame`        | Returns a blink frame as JPEG data URL from blink cache (by index + resolution)                                    |
| `get_autostretch_frame` | Computes Auto-STF stretch on current frame and returns JPEG data URL; does not cache |
| `get_current_frame` | Returns current image as raw (unstretched) JPEG data URL, rendered on the fly |
| `get_variable` | Returns a pcode variable value from ctx.variables by name |
| `load_file` | Reads a single image file from disk, injects into session, returns JPEG data URL |
| `get_frame_flags`        | Returns PXFLAG values for all loaded frames (used by blink overlay)                                                |
| `get_full_frame` | Returns current image at full resolution with last STF params applied; cached after first call |
| `get_histogram`          | Computes and returns histogram bins + stats for current frame (per-channel for RGB)                                |
| `get_keywords`           | Returns all keywords for current frame as a keyed map                                                              |
| `get_macros_dir`         | Returns the Macros directory path as a forward-slash string                                                        |
| `get_pixel`              | Returns raw pixel value(s) at source coordinates (x, y) from the raw image buffer                                  |
| `get_session`            | Returns current session state (directory, file list, current frame)                                                |
| `get_star_positions`     | Re-runs star detection on current frame, returns {cx, cy, fwhm, r} per star for annotation overlay                 |
| `list_log_files`         | Lists available log files in the logs directory, sorted newest first                                               |
| `list_macros`            | Lists .phs files in the Macros directory with name, path, line count, and tooltip                                  |
| `list_plugins`           | Returns list of registered plugins with name, version, and type                                                    |
| `read_log_file`          | Reads and parses a log file into structured {timestamp, level, module, message} lines                              |
| `rename_macro`           | Renames a .phs macro file; validates name, returns new path                                                        |
| `start_background_cache` | Spawns background task to build stretched blink cache JPEGs (Pass 1 display cache removed — normal display renders raw pixels on the fly) |

---

## 5. Plugins Implemented

| Plugin          | Category        | Status     | Notes                                                                               |
| --------------- | --------------- | ---------- | ----------------------------------------------------------------------------------- |
| AddKeyword      | Keyword         | ✅ Complete | scope=all\|current parameter                                                        |
| Assert          | Scripting       | ✅ Complete | Halts on false expression                                                           |
| AutoStretch     | Processing      | ✅ Complete | Mono and RGB, display-res only, raw buffer preserved; defaults exposed as constants |
| CacheFrames     | Blink           | ✅ Complete | Rayon parallel, both resolutions                                                    |
| ClearSession    | Session         | ✅ Complete |                                                                                     |
| CopyKeyword     | Keyword         | ✅ Complete |                                                                                     |
| CountFiles      | Scripting       | ✅ Complete | Stores result in $filecount                                                         |
| DeleteKeyword   | Keyword         | ✅ Complete | scope=all\|current parameter                                                        |
| GetHistogram    | Analysis        | ✅ Complete | Mono and RGB per-channel, true median                                               |
| GetKeyword      | Scripting       | ✅ Complete | Stores result in $KEYWORDNAME                                                       |
| ListKeywords    | Keyword         | ✅ Complete |                                                                                     |
| ModifyKeyword   | Keyword         | ✅ Complete | scope=all\|current parameter                                                        |
| ContourHeatmap | Analysis | ✅ Complete | Spatial FWHM heatmap; adaptive grid 5×5–15×15; viridis/plasma/coolwarm palettes; writes XISF to active directory; stores path in `$NEW_FILE` |
| LoadFile | Scripting | ✅ Complete | Loads single file into session for display; stores path in `$LOAD_FILE_PATH` |
| MoveFile | File Management | ✅ Complete | Moves current frame file or arbitrary path (source= param); removes from session if present |
| Print           | Scripting       | ✅ Complete | Outputs literal message                                                             |
| ReadAll         | I/O Reader      | ✅ Complete | FITS + XISF + TIFF from same directory (ReadAllFiles alias)                         |
| ReadFIT         | I/O Reader      | ✅ Complete | Sequential only (ReadAllFITFiles alias)                                             |
| ReadTIFF        | I/O Reader      | ✅ Complete | U8, U16, U32→U16, F32 (ReadAllTIFFFiles alias)                                      |
| ReadXISF        | I/O Reader      | ✅ Complete | (ReadAllXISFFiles alias)                                                            |
| RunMacro        | Scripting       | ✅ Complete |                                                                                     |
| SelectDirectory | File Management | ✅ Complete |                                                                                     |
| SetFrame        | Navigation      | ✅ Complete |                                                                                     |
| WriteCurrent    | I/O Writer      | ✅ Complete | Atomic temp-file writes; writes all loaded frames (WriteCurrentFiles alias)         |
| WriteFIT        | I/O Writer      | ✅ Complete | Creates proper FITS files from any source format (WriteAllFITFiles alias)           |
| WriteFrame      | I/O Writer      | ✅ Complete | Writes active frame only to source format; atomic temp-rename                       |
| WriteTIFF       | I/O Writer      | ✅ Complete | AstroTIFF keyword embedding (WriteAllTIFFFiles alias)                               |
| WriteXISF       | I/O Writer      | ✅ Complete | Uncompressed default; compress=true for LZ4HC (WriteAllXISFFiles alias)             |

**Command naming convention:** Read/Write commands follow the pattern `ReadFIT`, `ReadTIFF`, `ReadXISF`, `ReadAll`, `WriteFIT`, `WriteTIFF`, `WriteXISF`, `WriteCurrent`, `WriteFrame`. Old names retained as backward-compatible aliases but should not be used in new scripts.

**Keyword scope parameter:** `AddKeyword`, `DeleteKeyword`, and `ModifyKeyword` accept an optional `scope=all` (default) or `scope=current` parameter. `scope=current` operates only on the current frame as set by `SetFrame`.

---

## 6. UI State Store (`ui.ts`) — Key Fields

Photyx will use an embedded SQLite database via the rusqlite crate, which statically links SQLite into the binary with no external dependencies. The tauri-plugin-sql plugin exposes the database to the Svelte frontend via invoke. Planned uses include: replacing localStorage for Quick Launch buttons, theme, and user preferences; a macro library table storing name, description, script text, created date, and last run date; a frame analysis results table (one row per file per analysis type — FWHM, star count, eccentricity, median value) so results persist across sessions and can be queried; and a session history log. This is deferred to Phase 9 alongside other persistence work.

| Field               | Purpose                                                                                                  |
| ------------------- | -------------------------------------------------------------------------------------------------------- |
| `activePanel`       | Currently open sidebar panel                                                                             |
| `activeView`        | Currently active viewer-region view (`'analysisGraph'`, `'analysisResults'`, or `null` for image viewer) |
| `aboutOpen`         | Whether the About modal is open                                                                          |
| `blinkCached`       | Whether blink cache has been built                                                                       |
| `blinkCaching`      | Whether blink cache build is in progress                                                                 |
| `blinkImageUrl`     | Current blink frame data URL (null when not in blink mode)                                               |
| `blinkModeActive`   | Whether viewer is in blink display mode (true while on Blink tab including paused)                       |
| `blinkPlaying`      | Whether blink is actively playing                                                                        |
| `blinkResolution`   | Currently selected blink resolution ('12' = 12.5%, '25' = 25%)                                           |
| `blinkTabActive`    | Whether the Blink tab is currently selected                                                              |
| `consoleExpanded`   | Whether console history is expanded                                                                      |
| `frameRefreshToken` | Incremented to trigger viewer frame reload                                                               |
| `keywordModalOpen`  | Whether the keyword modal dialog is open                                                                 |
| `logViewerOpen`     | Whether the Log Viewer modal is open                                                                     |
| `macroEditorFile`   | File currently open in Macro Editor (`{ path, name }` or `null`)                                         |
| `theme`             | Active theme (dark / light / matrix), persisted to localStorage                                          |
| `viewerClearToken`  | Incremented to clear viewer and restore starfield                                                        |
| `zoomLevel`         | Current zoom level                                                                                       |
| `annotationToken` | Positive = show annotations, negative = clear annotations |
| `autostretchImageUrl` | Data URL of AutoStretch result for current frame; cleared on frame change |
| `displayImageUrl` | Data URL of temporary display image (heatmap, single loaded file); cleared on frame change |

**View management:** `activeView` replaces the previous `showAnalysisGraph` and `showAnalysisResults` boolean flags. Always use `ui.showView()` to change the active view — never set `activeView` directly. See §3.40.

---

## 7. Known Issues & Deferred Items

| Issue                                    | Notes                                                                                                                                                                                                        |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| cfitsio parallel loading crashes         | Thread-safety issue — sequential loading used for now                                                                                                                                                        |
| Blink UI jitter                          | Toolbar/Quick Launch chrome jitters during blink; DevTools CLS = 0.01, culprit undetected after canvas switch; suspected Tauri WebView compositor artifact on Windows; deferred                              |
| Full-res frames are JPEG not lossless    | Disclosed via disclaimer bar; pixel readout always uses raw buffer                                                                                                                                           |
| Long-running commands block UI           | pcode invoke awaits Rust response, freezing JS; fix requires Tauri event system; deferred                                                                                                                    |
| Zoom is approximate at high levels       | Full-res cache uses AutoStretch STF params computed on display-res downsample; minor difference possible                                                                                                     |
| XISF Vector/Matrix properties            | Read as placeholder string, skipped on write; deferred pending test files                                                                                                                                    |
| Rayon thread count not user-configurable | Hardcoded to num_cpus-1; §9.7 setting not yet wired                                                                                                                                                          |
| No persistent settings store             | tauri-plugin-store not yet implemented (Phase 9)                                                                                                                                                             |
| No crash recovery                        | Phase 9 item                                                                                                                                                                                                 |
| Last used directory lost on restart      | Session state is in-memory only; Phase 9 persistence will restore it                                                                                                                                         |
| stderr log output in dev mode            | Log entries are duplicated to the terminal during `npm run tauri dev`. This is the `fmt::layer().with_writer(std::io::stderr)` layer in `logging.rs`. It can be removed when no longer needed for debugging. |
| Histogram ADU mouse tracking | Implemented — hover shows normalized value, ADU, and per-channel percentages |
| Single file load blink isolation | Files loaded via LoadFile/load_file are added to ctx.file_list; scripts that loop the file list will include them until next batch load |
| AutoStretch performance in dev mode | 3–5 seconds for RGB 9MP images in debug build; expected to be near-instant in release build |

---

## 8. Phase Completion Status

| Phase    | Status                   | Notes                                                                                                                                                                                                                                                                                                              |
| -------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Phase 1  | ✅ Complete               | Scaffold, plugin host, FITS reader, notification bar, logging                                                                                                                                                                                                                                                      |
| Phase 2  | ✅ Complete               | Display cache, AutoStretch, blink engine, histogram, keywords, UI file browser, pixel tracking, WCS, zoom, pan, full-res cache, canvas viewer                                                                                                                                                                      |
| Phase 3  | ✅ Complete               | photyx-xisf crate (reader + writer), ReadAllXISFFiles, WriteAllXISFFiles, ReadAllTIFFFiles, ReadAllFiles, RGB display/histogram, background display cache                                                                                                                                                          |
| Phase 4  | ✅ Complete               | Keyword plugins, WriteAllFITFiles, WriteAllTIFFFiles, WriteCurrentFiles, AstroTIFF keyword round-trip, FITS signed/unsigned 16-bit, blink cache quality, relative path resolution, window resize fix, pwd command                                                                                                  |
| Phase 5  | ✅ Complete               | pcode interpreter with If/Else/EndIf and For/EndFor; Macro Editor UI with syntax highlighting; Quick Launch panel with store persistence and context menu; command rename refactor; scope parameter on keyword commands; WriteCurrent atomic writes; ScriptResponse flags; pcodeCommands.ts single source of truth |
| Phase 6  | ✅ Complete               | UI cleanup complete                                                                                                                                                                                                                                                                                                |
| Phase 7  | ✅ Complete               | AnalyzeFrames with 7 native metrics; PASS/REJECT classification; PXFLAG keyword; Analysis Graph viewer-region component; star annotation overlay; consolePipe store; blink red border overlay; viewer filename overlay; theme-aware chart colors                                                                   |
| Phase 8 | ✅ Substantially complete | Moment-based FWHM; 8×8 background gradient grid; 5-pixel minimum star filter; WriteFITS U16 sign conversion fix; histogram canvas width fix; UI audit pass; ContourHeatmap plugin (spatial FWHM heatmap, adaptive grid, 3 palettes, XISF output, `$NEW_FILE` convention); display pipeline refactor (raw display, explicit AutoStretch, blink-only cache); `image_reader.rs` format-agnostic reader; load_file Tauri command; File > Load Single Image menu item; LoadFile pcode command; DispatchResponse.data field; histogram hover readout |
| Phase 9  | ⬜ Not started            | Embedded SQLite, Settings persistence, rig profiles, themes, crash recovery, update mechanism, file associations                                                                                                                                                                                                   |
| Phase 10 | ⬜ Not started            | User plugin loading, plugin manifest system, macro library, plugin directory, Plugin Manager UI                                                                                                                                                                                                                    |
| Deferred | ⏸ Parked                 | Full keyword management UI, PNG/JPEG readers and writers, debayering, async dispatch, REST API (Axum), CLI access, WASM analysis plugins                                                                                                                                                                           |

## 9. Settings Persistence Batch (Phase 9)

The following settings are currently using localStorage or are lost on restart. They will be migrated to `tauri-plugin-store` in Phase 9:

- Active theme (currently in localStorage)
- Last used directory (currently lost on restart)
- Quick Launch button assignments (currently in localStorage)
- AutoStretch enabled state
- Macro editor font size
- Format filter selection (File Browser)
- Log directory (if user-configurable)
- Macros directory (if user-configurable)
