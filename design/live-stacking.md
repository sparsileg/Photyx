# Live stacking — implementation plan

Version 2 · June 2026 · Photyx Phase 11 continuation

---

## Overview

Live stacking incrementally integrates frames as they arrive from a capture directory in real time, updating the display after each new frame. It is implemented as one new pcode command — StartLiveStack — backed by a persistent LiveStackState in AppContext, a dedicated stop Tauri command, and a minimal frontend polling extension.

The implementation follows the existing batch stacking pipeline as closely as possible. Key differences: frames arrive one at a time from the file system rather than from a pre-loaded session buffer; accumulation buffers persist across file arrivals rather than being rebuilt per run; and a generation counter drives incremental display updates rather than a one-shot job result.

While live stacking is active, all other pcode operations are blocked. This is an accepted constraint for v1.

---

## Architecture summary

### Start/stop mechanism

StartLiveStack path= is a standard pcode command submitted via run_script — from the pcode console, a macro, or a UI button in StackingWorkspace. All three paths construct the same pcode string and call run_script via the fire-and-forget pattern, with jobOwner = 'stackingworkspace' when triggered from the UI.

Stop is not a pcode command. Because run_script is blocked for the duration of StartLiveStack, no pcode command submitted after start would execute until the loop exits. Stop is instead a dedicated Tauri command — stop_live_stack_cmd — called directly by the Stop button in StackingWorkspace. It acquires the AppContext lock, sets the stop flag, and returns immediately without going through the plugin registry.

### New backend components

| Component                  | Location          | Purpose                                                          |
|----------------------------|-------------------|------------------------------------------------------------------|
| LiveStackState             | context/mod.rs    | Accumulation buffers, reference stars, stop flag                 |
| LIVE_STACK_GENERATION      | lib.rs            | AtomicU32 incremented after each frame integration               |
| get_live_stack_frame       | lib.rs            | Tauri command — encodes current mean buffer as JPEG data URL     |
| stop_live_stack_cmd        | lib.rs            | Tauri command — sets stop flag directly; bypasses run_script     |
| start_live_stack.rs        | plugins/          | StartLiveStack plugin — init, watcher loop, per-frame pipeline   |

### New frontend components

| Component                  | Location                         | Purpose                                                          |
|----------------------------|----------------------------------|------------------------------------------------------------------|
| liveStackGeneration store  | progress.ts                      | Exported writable updated by the existing 500ms polling loop     |
| Live stack UI              | StackingWorkspace.svelte         | Start/Stop controls; path input; reacts to generation changes    |
| CSS                        | static/css/stackingworkspace.css | Live stack UI styles                                             |

### Stop mechanism detail

LiveStackState contains an Arc stop flag. StartLiveStack stores a clone in AppContext then releases the lock before entering the blocking watcher loop. Each time a file event arrives, it re-acquires the lock briefly to process the frame, then releases it. stop_live_stack_cmd acquires the lock, sets the flag to true, and clears ctx.live_stack. The watcher loop checks the flag after each event and exits cleanly. Dropping LiveStackState also drops the stop flag Arc; the watcher handle lives on the StartLiveStack stack for the duration of the loop.

### Display update path

The existing get_progress Tauri command is extended to return live_stack_generation as a fourth element alongside the existing (label, current, total) tuple. The frontend 500ms polling loop already fires continuously; it will also update a liveStackGeneration writable store. When the store value changes, StackingWorkspace.svelte calls get_live_stack_frame and updates the viewer. No additional polling loop is needed.

---

## Phase 1 — Backend state and globals

Establish the persistent state structure and global counter. No plugin logic yet — just the data model.

### 1.1 — LiveStackState in context/mod.rs

- Add LiveStackState struct:

    pub struct LiveStackState {
        pub mean_buf: Vec,
        pub m2_buf: Vec,
        pub count_buf: Vec,
        pub width: usize,
        pub height: usize,
        pub channels: usize,
        pub frame_count: u32,
        pub ref_stars: Vec,
        pub ref_filter: Option,
        pub stop_flag: Arc,
        pub watch_path: PathBuf,
    }

- Add `pub live_stack: Option` field to AppContext
- Initialize to None in AppContext::new() and clear_session()
- Add impl LiveStackState: constructor new(width, height, channels, ref_stars, ref_filter, watch_path) that pre-allocates all three buffers and initializes stop_flag to false
- Add fn accumulate(&mut self, pixels: &[f32]) — Welford online mean/M2 update per pixel; increments frame_count
- Add fn mean_as_f32(&self) -> Vec — returns current mean buffer cast to f32 for display encoding

NOTE: The watcher handle is not stored in LiveStackState. It lives on the stack inside StartLiveStack::execute() for the duration of the blocking loop. LiveStackState in AppContext holds only the stop flag, accumulation data, and reference information.

### 1.2 — LIVE_STACK_GENERATION in lib.rs

- Add `pub static LIVE_STACK_GENERATION: AtomicU32 = AtomicU32::new(0);` alongside the existing PROGRESS_CURRENT etc.
- Reset to 0 at the start of each run_script call (same location as the other atomic resets)

### 1.3 — get_live_stack_frame Tauri command in lib.rs

- Acquire AppContext lock; if ctx.live_stack is None, return Ok(None)
- Call live_stack.mean_as_f32() to get current pixel data
- Construct a temporary ImageBuffer from the mean pixels (width, height, channels) so the existing display encoding path can be reused
- Encode at display resolution using the same downsample + AutoStretch + JPEG path as get_current_frame
- Return Ok(Some(data_url))

### 1.4 — stop_live_stack_cmd Tauri command in lib.rs

- Acquire AppContext lock
- If ctx.live_stack is None, return Ok("no live stack active")
- Set ctx.live_stack.as_ref().unwrap().stop_flag.store(true, Ordering::Relaxed)
- Set ctx.live_stack = None
- Return Ok("live stack stopped")

### 1.5 — Extend get_progress return type

- Change return type from (String, u32, u32) to (String, u32, u32, u32) — adding live_stack_generation as the fourth element
- Read from LIVE_STACK_GENERATION.load(Ordering::Relaxed)
- Update progress.ts to destructure the fourth element and update a new liveStackGeneration writable store (export it alongside the existing exports)

TEST: Verify app compiles and runs. Confirm get_progress returns a four-tuple. Confirm get_live_stack_frame returns null when no live stack is active. Confirm stop_live_stack_cmd returns gracefully when no live stack is active.

---

## Phase 2 — StartLiveStack plugin

Core backend logic. This is the largest and most complex phase.

### 2.1 — Plugin skeleton in plugins/start_live_stack.rs

- Implement PhotonPlugin trait boilerplate (name, version, execute)
- Register in plugins/mod.rs and the plugin registry — alphabetical order in both
- Add StartLiveStack to pcodeCommands.ts
- Parse required argument path= (watch directory); validate it exists and is a directory
- Guard: return error if ctx.live_stack is already Some

### 2.2 — Reference frame initialization

- Scan the watch directory for supported image files (.fit, .fits, .fts, .xisf, .tif, .tiff) already present at start time; collect into a HashSet of already-seen paths to avoid reprocessing when Create events fire for them
- If files exist: load the first via read_image_file(), debayer if Bayer CFA, run star detection, read filter keyword into ref_filter, initialize LiveStackState buffers from its dimensions, accumulate it as frame 1, increment LIVE_STACK_GENERATION
- If no files exist yet: LiveStackState is not constructed until the first Create event arrives — hold a local Option initialized to None and initialize on first successful load

### 2.3 — Per-frame pipeline

Called for each new file detected (both pre-existing on start and arriving via watcher).

1. Load file via read_image_file(). On error, log and skip (see §2.5 for retry logic).
2. Validate filter keyword matches ref_filter if set — log warning and skip on mismatch
3. Debayer if Bayer CFA (same logic as StackFrames)
4. Background normalization — estimate_background() + subtract (same as StackFrames uncalibrated path)
5. Align to reference: FFT phase correlation via fft_align (downsampled to <=1024px) followed by RANSAC rotation refinement via estimate_rigid_transform
6. Resample: resample_frame_affine() if |θ| >= 0.001 rad or flip encoded; otherwise resample_frame()
7. Accumulate into LiveStackState via accumulate(); drop pixel data immediately
8. Increment LIVE_STACK_GENERATION
9. Call set_progress("Live stacking", frame_count, 0) — total=0 signals indefinite to the status bar

NOTE: validate_alignment() remains disabled, consistent with StackFrames. Alignment failures are logged and the frame is skipped rather than halting the session.

### 2.4 — Watcher loop

- Construct LiveStackState (or leave as None if no pre-existing files), clone the stop flag, store state into ctx.live_stack, then release the AppContext lock
- Construct notify::RecommendedWatcher with an mpsc::channel; watch the directory in RecursiveMode::NonRecursive
- Loop over rx.recv():
  - On RecvError: exit — channel closed because stop flag was set and state dropped
  - On Ok(Event) where event.kind.is_create(): for each path with a supported extension not in the already-seen set: check stop flag; if set, break; otherwise re-acquire lock, run per-frame pipeline (§2.3), release lock, add path to already-seen set
  - On other event kinds: ignore
- After loop exits: acquire lock; if ctx.live_stack is still Some (stop came from channel close rather than stop_live_stack_cmd), clear it
- Return PluginOutput with final summary message including total frames integrated

### 2.5 — File write timing robustness

Some capture software writes files in two steps (temp name then rename, or incomplete write then flush). The Create event may fire before the file is fully written.

- When read_image_file() returns an error on a newly detected file, retry up to ~5 times with 250ms sleeps before logging a skip
- Log the path and error on final failure

TEST: Start StartLiveStack path=/some/dir from the console. Manually copy a FITS file into the directory. Confirm it is detected and integrated and LIVE_STACK_GENERATION increments. Copy a second file; confirm it integrates correctly. Confirm get_live_stack_frame returns a valid JPEG data URL after integration. Invoke stop_live_stack_cmd from the browser console (Tauri devtools) and confirm the loop exits cleanly.

---

## Phase 3 — Frontend integration

### 3.1 — progress.ts — liveStackGeneration store

- Export a new writable store liveStackGeneration initialized to 0
- In the existing 500ms polling loop, destructure the fourth element from get_progress and call liveStackGeneration.set()

### 3.2 — StackingWorkspace.svelte — live stack controls

- Add a "Start live stack" button and an inline path input field to the workspace toolbar. The path input is always visible when live stacking is not active; the Stop button replaces it when active
- "Start live stack" constructs StartLiveStack path= and submits via run_script using the existing fire-and-forget pattern with jobOwner = 'stackingworkspace'
- "Stop live stack" calls invoke('stop_live_stack_cmd') directly
- Disable "Start live stack" while a job is running (use existing $jobOwner or $progress state to detect this)
- Show "Stop live stack" button only while live stacking is active — track with a local liveStackActive boolean, set to true when start is submitted and false when the job result is received or stop is clicked

### 3.3 — StackingWorkspace.svelte — display update

- Import liveStackGeneration from progress.ts
- In a $effect, watch $liveStackGeneration; when it changes and is greater than 0, call invoke('get_live_stack_frame') and update the viewer display image (same pattern as autostretch image URL updates)
- Show frame count in a status line below the viewer: "Live stacking — N frames integrated" derived from $progress.current while liveStackActive is true
- Clear the live stack display image when stop is confirmed (job result received)

### 3.4 — CSS in static/css/stackingworkspace.css

- Styles for path input field, Start/Stop button states, and frame count status line
- No inline styles; all theme CSS variables

TEST: Full end-to-end. Start live stack from the UI path input. Copy files into the watched directory. Confirm the viewer updates after each frame and the frame count increments. Click Stop; confirm the loop exits, the display freezes on the last integrated frame, and the UI returns to idle state. Confirm starting from the pcode console also works.

---

## Phase 4 — Edge cases and guards

### 4.1 — Session interaction guard

- In ClearSession plugin: if ctx.live_stack is Some, set the stop flag before clearing — prevents the watcher loop from continuing to process frames into a cleared session

### 4.2 — Crash recovery guard

- Confirm that crash recovery state written during a live stack session does not attempt to restore a live stack on next launch — crash recovery stores file list only, which is unaffected; no changes needed, but verify during testing

### 4.3 — Duplicate event guard

- Some platforms emit multiple Create events for the same file (write + close). The already-seen HashSet from §2.2 handles this — confirm it is checked before the retry loop, not after

### 4.4 — Dimension mismatch guard

- If a newly arrived frame has different dimensions than the reference frame, log an error and skip — do not attempt to accumulate into mismatched buffers

---

## Phase 5 — Documentation

### 5.1 — photyx_reference.md

- §1 command dictionary: add StartLiveStack (category: Stacking; key argument: path)
- §7 Tauri commands: add get_live_stack_frame, stop_live_stack_cmd; update get_progress return type note
- §9 plugin status: add StartLiveStack as Complete

### 5.2 — photyx_development.md

- §3 architecture decisions: add section documenting LiveStackState, the stop mechanism, the generation counter pattern, and the lock-release discipline in the watcher loop
- §4 Tauri commands: add get_live_stack_frame, stop_live_stack_cmd; update get_progress
- §6 UI state stores: add liveStackGeneration

### 5.3 — photyx_spec.md

- Phase 11 entry: add live stacking to the focus description
- §13.1 deferred items: remove live stacking from the list
- Add §8.x for the live stacking UI in StackingWorkspace if warranted

---

## Files created

| File                                          | Action |
|-----------------------------------------------|--------|
| src-tauri/src/plugins/start_live_stack.rs     | New    |
| static/css/stackingworkspace.css              | New    |

## Files modified

| File                                          | Change                                                                                    |
|-----------------------------------------------|-------------------------------------------------------------------------------------------|
| src-tauri/Cargo.toml                          | Add notify = "6" dependency                                                               |
| src-tauri/src/context/mod.rs                  | Add LiveStackState struct and live_stack field on AppContext                               |
| src-tauri/src/lib.rs                          | Add LIVE_STACK_GENERATION static; get_live_stack_frame; stop_live_stack_cmd; extend get_progress |
| src-tauri/src/plugins/mod.rs                  | Declare start_live_stack module (alphabetical)                                            |
| src-tauri/src/plugin/registry.rs             | Register StartLiveStack (alphabetical)                                                    |
| src-svelte/lib/stores/progress.ts             | Add liveStackGeneration store; destructure fourth element from get_progress               |
| src-svelte/lib/components/StackingWorkspace.svelte | Add live stack controls, path input, display update logic                            |
| src-svelte/lib/pcodeCommands.ts               | Add StartLiveStack                                                                        |
| photyx_reference.md                           | Add command, Tauri commands, plugin status entry                                          |
| photyx_development.md                         | Add architecture notes, store and command documentation                                   |
| photyx_spec.md                                | Update Phase 11 scope; remove live stacking from deferred list                            |

---

## Confirmed design decisions

| Decision                        | Resolution                                                                                      |
|---------------------------------|-------------------------------------------------------------------------------------------------|
| Display update delivery         | Generation counter polled via existing get_progress loop; frontend fetches frame on change      |
| Watch directory specification   | path= argument on StartLiveStack; specified via console, macro, or UI path input                |
| Stop mechanism                  | Dedicated stop_live_stack_cmd Tauri command only — no StopLiveStack pcode command               |
| Accumulation buffer home        | Dedicated LiveStackState struct in AppContext; isolated from batch stacking state               |
| Pre-existing files at start     | Processed and integrated before watcher loop begins; tracked in already-seen set                |
| Display resolution              | Display resolution only (same as get_current_frame); full-res live frame not in scope for v1   |
| Sigma clipping                  | M2 buffer accumulated for future use; display always shows running mean; sigma-clipped output not in scope for v1 |
| Blocking behavior               | All pcode operations blocked while live stacking is active; accepted constraint for v1          |
