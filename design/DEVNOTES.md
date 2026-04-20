# Photyx — Developer Notes

**Last updated:** April 2026  
**Status:** Active development — Phase 2 in progress

---

## 1. Project Structure

```
Photyx/
├── src/                  ← HTML/CSS/vanilla JS prototype (reference/testing only)
├── src-svelte/           ← Svelte frontend (target stack)
│   ├── lib/
│   │   ├── components/   ← Svelte UI components
│   │   │   └── panels/   ← Sliding panel components
│   │   └── stores/       ← Svelte writable stores (session, ui, notifications)
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
│       │   └── mod.rs    ← AppContext, ImageBuffer, PixelData, KeywordEntry
│       └── plugins/      ← Built-in native plugin implementations
│           ├── mod.rs
│           ├── select_directory.rs
│           ├── read_fits.rs
│           └── auto_stretch.rs
├── static/               ← Static assets served by Vite
│   ├── css/              ← Module CSS files (theme-neutral)
│   └── themes/           ← Theme CSS files (dark, light, matrix)
├── svelte.config.js      ← SvelteKit config (points to src-svelte/)
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

cfitsio is installed via vcpkg on `J:\vcpkg`. The following environment variables must be set in every new PowerShell session before running `npm run tauri dev`:

```powershell
$env:PATH = "J:\vcpkg\installed\x64-windows\bin;J:\vcpkg\installed\x64-windows\tools\pkgconf;" + $env:PATH
$env:PKG_CONFIG = "J:\vcpkg\installed\x64-windows\tools\pkgconf\pkg-config.exe"
$env:PKG_CONFIG_PATH = "J:\vcpkg\installed\x64-windows\lib\pkgconfig"
```

Consider adding these to your PowerShell profile (`$PROFILE`) to avoid setting them manually each session.

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

### 3.2 IPC Image Transfer

The spec does not define the mechanism for transferring pixel data from the Rust buffer pool to the viewer canvas. The established pattern is:

- A `get_current_frame` Tauri command encodes the current image buffer as a JPEG data URL
- Display resolution is capped at 1200px wide (box-filter downsampled during pixel extraction)
- The data URL is set as the `src` of an `<img>` element in `Viewer.svelte`
- The starfield canvas animation is stopped when an image is displayed

This is a pragmatic solution for Phase 2. The pyramid cache (Phase 2 spec §12.3) will replace this with pre-rendered display-resolution frames stored in a dedicated display cache.

### 3.3 Display Cache Architecture (Planned)

For blink to work at speed, the following architecture is needed:

- `CacheFrames` plugin pre-renders all loaded images to display-resolution JPEGs
- These are stored in a `display_cache: HashMap<String, Vec<u8>>` in `AppContext`
- `BlinkSequence` cycles through the display cache, emitting Tauri events to the frontend
- The viewer receives `frame-update` events and swaps the image src — no encoding on the fly

This decouples expensive stretch+encode (done once per image) from display (trivial swap).

### 3.4 Rayon + cfitsio Incompatibility

Parallel FITS loading using Rayon causes a `STATUS_STACK_BUFFER_OVERRUN` crash on Windows. Root cause: cfitsio's internal C state is not thread-safe across Rayon worker threads. 

**Workaround:** Sequential loading is used for now.  
**Future fix:** Use thread-local `FitsFile` handles, one per Rayon thread, to isolate cfitsio state.

### 3.5 AutoStretch Performance

The current AutoStretch implementation:
- Samples up to 50,000 pixels for median/MAD statistics (fast)
- Applies the MTF stretch to all ~9M pixels (slow — ~5-7 seconds for 3008×3008)
- Box-filter downsamples during JPEG encoding (acceptable quality)

The stretch is applied to the full-resolution buffer in place. For Phase 2 blink, stretch should be applied to the display-resolution copy only, not the full buffer. The full buffer should remain in its original bit depth for write operations.

### 3.6 SvelteKit Configuration

The Tauri scaffold puts the Svelte source in `src/` by default. We renamed it to `src-svelte/` to avoid collision with the prototype. `svelte.config.js` has been updated with:

```javascript
files: {
    routes: "src-svelte/routes",
    appTemplate: "src-svelte/app.html",
    assets: "static",
},
```

### 3.7 Svelte A11y Warnings

Svelte's accessibility linter warns about `<div>` and `<span>` elements with click handlers. These are suppressed project-wide with:

```javascript
compilerOptions: {
    warningFilter: (warning) => !warning.code.startsWith('a11y'),
},
```

This is acceptable for a desktop application where standard web accessibility concerns don't apply in the same way.

---

## 4. Tauri Commands (Implemented)

| Command | Description |
|---|---|
| `dispatch_command` | Dispatches a pcode command to the plugin registry |
| `list_plugins` | Returns list of registered plugin names |
| `get_session` | Returns current session state (directory, file list, current frame) |
| `get_current_frame` | Returns current image as JPEG data URL (display resolution) |
| `debug_buffer_info` | Returns buffer metadata for debugging (pixel type, dimensions, etc.) |

---

## 5. Plugins Implemented

| Plugin | Category | Status |
|---|---|---|
| SelectDirectory | File Management | ✅ Complete |
| ReadAllFITFiles | I/O Reader | ✅ Complete (sequential only) |
| AutoStretch | Processing | ✅ Working (slow on full-res buffer) |

---

## 6. Known Issues & Deferred Items

| Issue | Notes |
|---|---|
| AutoStretch is slow (~7s) | Operates on full 9M pixel buffer. Fix: stretch display-res copy only |
| Blink not yet implemented | Requires display cache architecture (§3.3 above) |
| Viewer image not perfectly centered | CSS `object-fit: contain` with absolute positioning — minor layout issue |
| cfitsio env vars not persistent | Must be set each PowerShell session — add to `$PROFILE` |
| GetImageProperty not implemented | AppContext has the data; plugin not yet written |
| GetSessionProperty not implemented | Same as above |
| Rayon parallel loading crashes | cfitsio thread-safety issue — deferred |

---

## 7. Phase Completion Status

| Phase | Status | Notes |
|---|---|---|
| Phase 1 | ✅ Complete | Scaffold, plugin host, FITS reader, notification bar, logging |
| Phase 2 | 🔄 In progress | IPC bridge done, AutoStretch done, blink engine pending |
| Phase 3–10 | ⬜ Not started | |
