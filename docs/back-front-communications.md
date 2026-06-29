# Rust + Tauri + JavaScript: Backend/Frontend Communication Patterns

**Version:** 1
**Date:** June 2026
**Context:** Written from experience building Photyx, a Tauri v2 + Svelte 5 + Rust desktop astrophotography application. The patterns described here apply to any Tauri application regardless of which JavaScript framework (or none) is used on the frontend.

---

## 1. The Core Problem

Tauri's `invoke()` is synchronous from the frontend's perspective — the JavaScript thread blocks (awaits) until the Rust command returns. For fast operations this is fine. For long-running operations (stacking 128 astronomical images, analyzing frames, loading large files) it creates two problems:

1. **The UI freezes.** No progress can be displayed, no cancel button can be pressed, no other interaction is possible.
2. **Progress cannot be streamed.** The result arrives all at once when the operation completes.

The solution is an **async dispatch pattern**: the Tauri command returns immediately with an acknowledgment, the real work runs on a background thread, and the frontend polls for progress and completion independently.

---

## 2. The General Pattern

### 2.1 Overview

```
Frontend                          Rust Backend
--------                          ------------
invoke('run_command')  ────────►  Spawn background thread
◄──────────────────── accepted    Return immediately

setInterval(500ms):
  invoke('get_progress') ───────► Read atomics (label, current, total)
  ◄────────────────────────────── (string, u32, u32)
  Update progress display

  invoke('get_job_result') ─────► Check result slot
  ◄────────────────────────────── null | JobResult
  If result: handle completion
```

### 2.2 Backend Components

**1. Progress atomics** — written by the plugin, read by the poller. Atomics are used (not a Mutex) because they are lock-free and can be written from any thread without coordination.

```rust
pub static PROGRESS_CURRENT: AtomicU32 = AtomicU32::new(0);
pub static PROGRESS_TOTAL:   AtomicU32 = AtomicU32::new(0);
pub static PROGRESS_LABEL:   OnceCell<Mutex<String>> = OnceCell::new();
```

The label is a `Mutex<String>` rather than an atomic because strings are not fixed-size. The lock is held for nanoseconds so contention is negligible.

**2. A convenience setter** — plugins call this instead of touching the globals directly:

```rust
pub fn set_progress(label: &str, current: u32, total: u32) {
    if let Some(l) = PROGRESS_LABEL.get() {
        if let Ok(mut g) = l.lock() { *g = label.to_string(); }
    }
    PROGRESS_CURRENT.store(current, Ordering::Relaxed);
    PROGRESS_TOTAL.store(total, Ordering::Relaxed);
}
```

**3. A job result slot** — holds the completed result until the frontend retrieves it:

```rust
pub static JOB_RESULT: OnceCell<Mutex<Option<JobResult>>> = OnceCell::new();
```

`JobResult` mirrors whatever response shape the frontend expects. Initialize the slot at app startup:

```rust
let _ = JOB_RESULT.set(Mutex::new(None));
let _ = PROGRESS_LABEL.set(Mutex::new(String::new()));
```

**4. The dispatch command** — clears state, spawns the thread, returns immediately:

```rust
#[tauri::command]
fn run_command(script: String, state: State<Arc<AppState>>) -> AcceptedResponse {
    // Clear previous state
    PROGRESS_CURRENT.store(0, Ordering::Relaxed);
    PROGRESS_TOTAL.store(0, Ordering::Relaxed);
    if let Some(l) = PROGRESS_LABEL.get() {
        if let Ok(mut g) = l.lock() { g.clear(); }
    }
    if let Some(slot) = JOB_RESULT.get() {
        *slot.lock().expect("poisoned") = None;
    }

    let state = Arc::clone(&state);

    std::thread::spawn(move || {
        let mut ctx = state.context.lock().expect("poisoned");
        let result = do_long_running_work(&mut ctx, &script);
        if let Some(slot) = JOB_RESULT.get() {
            *slot.lock().expect("poisoned") = Some(result);
        }
    });

    AcceptedResponse { accepted: true }
}
```

**5. The polling commands** — fast, lock-free reads:

```rust
#[tauri::command]
fn get_progress() -> (String, u32, u32) {
    let label = PROGRESS_LABEL.get()
        .and_then(|m| m.lock().ok())
        .map(|g| g.clone())
        .unwrap_or_default();
    (
        label,
        PROGRESS_CURRENT.load(Ordering::Relaxed),
        PROGRESS_TOTAL.load(Ordering::Relaxed),
    )
}

#[tauri::command]
fn get_job_result() -> Option<JobResult> {
    JOB_RESULT.get().and_then(|slot| {
        slot.lock().expect("poisoned").take()
    })
}
```

Note: `take()` removes the value from the slot — the result is delivered exactly once.

### 2.3 Frontend Components

**1. The polling loop** — always running, low overhead:

```javascript
// progress.js
let progressCurrent = 0;
let progressTotal   = 0;
let progressLabel   = '';
let jobResult       = null;
let jobResultCallbacks = [];

setInterval(async () => {
    try {
        const [label, current, total] = await invoke('get_progress');
        progressLabel   = label;
        progressCurrent = current;
        progressTotal   = total;
        updateProgressDisplay();
    } catch { /* backend not ready */ }

    try {
        const result = await invoke('get_job_result');
        if (result !== null) {
            jobResult = result;
            notifyResultSubscribers(result);
        }
    } catch { /* backend not ready */ }
}, 500);

function onJobResult(callback) {
    jobResultCallbacks.push(callback);
}

function notifyResultSubscribers(result) {
    jobResultCallbacks.forEach(cb => cb(result));
    jobResultCallbacks = [];
}
```

**2. The dispatch call** — fire and forget:

```javascript
async function runLongOperation() {
    setStatusRunning('MyOperation');
    clearProgress();

    try {
        await invoke('run_command', { script: 'MyOperation' });
        // Result arrives via polling loop, not here
    } catch (err) {
        setStatusError(err);
    }
}
```

**3. Result handling** — subscribe before dispatching:

```javascript
onJobResult((result) => {
    if (result.success) {
        setStatusSuccess(result.message);
        handleResults(result.data);
    } else {
        setStatusError(result.error);
    }
});

runLongOperation();
```

### 2.4 Progress Display

The status bar combines the operation name (set by the caller) with the label and counters (set by the plugin):

```
"MyOperation: Registering — 38/128 frames"
"MyOperation: Analyzing frames…"     (label set, total = 0)
"MyOperation"                         (no progress yet)
```

Logic:
```javascript
function formatStatusMessage(operationName, label, current, total) {
    if (total > 0) {
        const labelPart = label ? `: ${label}` : '';
        return `${operationName}${labelPart} — ${current}/${total} frames`;
    }
    if (label) {
        return `${operationName}: ${label}…`;
    }
    return operationName;
}
```

---

## 3. Plugin-Side Convention

Every long-running plugin follows this contract:

```rust
fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
    // 1. Set initial label immediately (shows something even before counting starts)
    crate::set_progress("Preparing", 0, 0);

    // 2. Update progress at each meaningful unit of work
    let total = items.len() as u32;
    crate::set_progress("Processing", 0, total);
    for (i, item) in items.iter().enumerate() {
        process(item);
        crate::set_progress("Processing", (i + 1) as u32, total);
    }

    // 3. Clear progress before returning — plugin cleans up after itself
    crate::set_progress("", 0, 0);

    Ok(PluginOutput::Data(json!({ "message": "Done", ... })))
}
```

**The plugin is responsible for clearing its own progress.** Downstream operations should not need to know what the previous plugin wrote.

For parallel loops, use an atomic counter inside the closure:

```rust
let completed = std::sync::atomic::AtomicU32::new(0);
let total = items.len() as u32;
crate::set_progress("Analyzing", 0, total);

items.par_iter().for_each(|item| {
    process(item);
    let n = completed.fetch_add(1, Ordering::Relaxed) + 1;
    crate::set_progress("Analyzing", n, total);
});

crate::set_progress("", 0, 0);
```

---

## 4. One Active Operation at a Time

This pattern assumes at most one long-running operation is active simultaneously. This is enforced naturally by the backend: `AppContext` is held in a `Mutex`, so only one thread can hold it at a time. A second `invoke('run_command')` while the first is running will block waiting for the mutex.

On the frontend, a `jobOwner` string tracks which component fired the current job, so that only the owning component reacts to the result:

```javascript
let jobOwner = null;

async function runAsOwner(ownerName, script) {
    jobOwner = ownerName;
    await invoke('run_command', { script });
}

onJobResult((result) => {
    if (jobOwner !== 'myComponent') return;
    jobOwner = null;
    handleResult(result);
});
```

This prevents two UI components (e.g. a menu item and a Quick Launch button) from both reacting to the same job result.

---

## 5. Memory Management in Background Threads

When the background thread needs access to shared application state, it must work with `Arc<AppState>` — clone the Arc before spawning:

```rust
let state = Arc::clone(&state);  // clone before move
std::thread::spawn(move || {
    let mut ctx = state.context.lock().expect("poisoned");
    // do work
});
```

**Never try to pass `&mut AppContext` into a thread or Rayon closure.** The borrow checker will reject it. Extract all needed data into owned types before entering parallel sections:

```rust
// Wrong — cannot borrow ctx inside Rayon
items.par_iter().for_each(|item| {
    ctx.do_something(item);  // compile error
});

// Right — extract owned data first
let data: Vec<OwnedData> = ctx.items.iter().map(|i| i.clone()).collect();
data.par_iter().for_each(|item| {
    process(item);  // no ctx needed
});
// Write results back sequentially
for (i, result) in results.iter().enumerate() {
    ctx.results[i] = result.clone();
}
```

**Memory budgeting for large datasets:** Pre-allocating all frame data simultaneously before parallel processing causes catastrophic memory pressure. The correct pattern is batch processing with explicit drop boundaries:

```rust
for chunk in items.chunks(batch_size) {
    // Sequential: load one batch
    let chunk_data: Vec<Data> = chunk.iter().map(load).collect();
    // Parallel: process the batch
    let results: Vec<Result> = chunk_data.par_iter().map(process).collect();
    // Sequential: accumulate results
    for result in &results { accumulate(result); }
    // chunk_data and results drop here — memory released before next batch
}
```

Peak memory = one batch, not all frames.

---

## 6. Photyx Implementation Reference

### Files

| File | Role |
|---|---|
| `src-tauri/src/lib.rs` | `PROGRESS_*` globals, `set_progress()`, `JOB_RESULT`, `run_script` async dispatch, `get_progress`, `get_job_result` Tauri commands |
| `src-svelte/lib/stores/progress.ts` | 500ms polling loop; `progress`, `jobResult`, `jobOwner` Svelte stores |
| `src-svelte/lib/components/StatusBar.svelte` | Progress display derived from `progress` store |
| `src-svelte/lib/components/Console.svelte` | Sets `jobOwner = 'console'`, subscribes to `jobResult` |
| `src-svelte/lib/components/QuickLaunch.svelte` | Sets `jobOwner = 'quicklaunch'`, subscribes to `jobResult` |
| `src-svelte/lib/components/StackingWorkspace.svelte` | Sets `jobOwner = 'stackingworkspace'`, subscribes to `jobResult` |
| `src-tauri/src/plugins/stack_frames.rs` | `set_progress("Registering"/"Integrating", ...)` |
| `src-tauri/src/plugins/analyze_frames.rs` | `set_progress("Analyzing", ...)` with atomic counter in `par_iter` |
| `src-tauri/src/plugins/add_files.rs` | `set_progress("Loading", ...)` |
| `src-tauri/src/plugins/fake_progress.rs` | Test harness; faithful simulation of real plugin progress pattern |

### Key types

```rust
// Delivered to frontend on completion
pub struct JobResult {
    pub results:         Vec<ScriptResult>,
    pub session_changed: bool,
    pub display_changed: bool,
    pub client_actions:  Vec<String>,
}

pub struct ScriptResult {
    pub line_number:    usize,
    pub command:        String,
    pub success:        bool,
    pub message:        Option<String>,
    pub data:           Option<serde_json::Value>,
    pub trace_line:     Option<String>,
    pub client_actions: Vec<String>,
}
```

### Testing the pipeline

`FakeProgress` is a test plugin that faithfully simulates a real plugin:

```rust
// Usage: FakeProgress frames=64
// Shows "FakeProgress: Simulating — X/64 frames" in status bar
fn execute(&self, _ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
    let total = args.get("frames").and_then(|v| v.parse().ok()).unwrap_or(128);
    crate::set_progress("Simulating", 0, total);
    for i in 1..=total {
        std::thread::sleep(Duration::from_millis(50));
        crate::set_progress("Simulating", i, total);
    }
    crate::set_progress("", 0, 0);
    Ok(PluginOutput::Data(json!({ "message": format!("Done ({} frames)", total) })))
}
```

---

## 7. What Was Tried and Abandoned

**Tauri events (emit/listen):** Tauri has a built-in event system for backend→frontend communication. It was considered but rejected because it requires careful lifecycle management (listeners must be registered before events fire, and cleaned up on component unmount), adds complexity around event deduplication, and the polling approach is simpler and equally performant at 500ms intervals.

**Channel-based streaming:** Tauri v2 supports streaming responses via channels. Rejected for the same reason — the polling approach is simpler and the 500ms polling interval is fast enough for human-perceptible progress display.

**Blocking `invoke` with progress:** The original implementation used a standard blocking `invoke` for `run_script`. This worked for completion but prevented any progress display because the JavaScript event loop cannot run while awaiting the invoke. This is the fundamental reason the async dispatch pattern was adopted.

**Starting polling only when `running` notification fires:** The initial polling implementation started the interval when `notifications.running()` fired and stopped it on completion. This created a race condition where the interval might not start before progress was already written. Replaced with always-on polling.

---

## 8. Lessons Learned

**Keep the polling interval at 500ms.** Faster intervals (100ms) create noticeable invoke overhead visible in devtools. Slower intervals (1000ms) make progress feel sluggish. 500ms is imperceptible as lag and negligible as overhead.

**The plugin clears its own progress.** Do not rely on the framework or the next operation to clear stale progress labels. Every plugin that sets progress should call `set_progress("", 0, 0)` before returning. This prevents stale labels from a completed operation appearing during the next unrelated operation.

**`jobOwner` prevents double-handling.** Without it, two subscribed components both react to every job result. With it, each component checks ownership and ignores results not addressed to it. This is simple and reliable for apps with a small number of command sources.

**Fire-and-forget on the frontend means error handling moves to the result handler.** The `invoke('run_command')` call can still throw (network error, Tauri bridge failure) but application-level errors (plugin failed, file not found) arrive in `JobResult`. The calling component needs to handle both.

**`std::thread::spawn` not `tokio::spawn` for CPU-bound work.** Tauri commands run in a Tokio async context. `tokio::spawn` is for I/O-bound async tasks. CPU-bound work (image processing, stacking) should use `std::thread::spawn` or Rayon, not Tokio tasks, to avoid starving the Tokio runtime.

**`Arc::clone` before moving into a thread.** The `State<Arc<AppState>>` from Tauri cannot be moved directly into a `std::thread::spawn` closure. Clone the Arc first, then move the clone.

---

*Document version: 1 — June 2026*
