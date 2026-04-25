# Photyx — Adding New UI Windows and Frontend-Triggered Actions

## Overview

Photyx uses Tauri + Svelte + Rust. The frontend is in `src-svelte/`, the backend in `src-tauri/src/`. This note describes the established pattern for adding new viewer-region components (like the Analysis Graph) and new pcode console commands that trigger frontend actions.

**Important:** Photyx is a single-window SPA. There are no floating OS windows or draggable overlays. Components that need full screen real estate (like the Analysis Graph) replace the viewer region entirely, controlled by a boolean in the `ui` store. The user closes them to return to the image viewer.

---

## Pattern 1 — New Viewer-Region Component (e.g. Analysis Graph)

A viewer-region component replaces the image viewer when active. It is toggled by a boolean in the `ui` store and rendered in `+page.svelte` using `{#if}/{:else}` alongside `<Viewer>`.

### Step 1 — Add a boolean to `src-svelte/lib/stores/ui.ts`

Add to the `UIState` interface:
```typescript
showMyComponent: boolean;
```

Add to the `initial` object:
```typescript
showMyComponent: false,
```

Add a setter to the store's returned object:
```typescript
setShowMyComponent: (v: boolean) => update(s => ({ ...s, showMyComponent: v })),
```

### Step 2 — Create the Svelte component

Place it in `src-svelte/lib/components/MyComponent.svelte`.

The component:
- Fills `100%` width and height of the viewer region
- Has a Close button that calls `ui.setShowMyComponent(false)`
- Has a toolbar row at the top for controls
- Uses `display: flex; flex-direction: column` layout
- Does NOT use `position: fixed` or dragging — it lives in the normal document flow

### Step 3 — Create the CSS file

Place it in `src-svelte/static/css/mycomponent.css`. Import it in `src-svelte/app.html` alongside the other CSS imports:
```html
<link rel="stylesheet" href="/css/mycomponent.css" />
```

### Step 4 — Register in `+page.svelte`

In `src-svelte/routes/+page.svelte`:
```svelte
import MyComponent from '../lib/components/MyComponent.svelte';
```

In the viewer region, replace `<Viewer>` with an if/else:
```svelte
<div id="viewer-region">
    {#if $ui.showMyComponent}
        <MyComponent />
    {:else}
        <Viewer onMousePixel={onMousePixel} />
    {/if}
    ...
</div>
```

### Step 5 — Trigger from the menu

In `src-svelte/lib/components/MenuBar.svelte`, add a menu item and handle it in the `action()` switch:
```typescript
case 'my-component': ui.setShowMyComponent(true); break;
```

### Step 6 — Trigger from the pcode console

In `src-svelte/lib/components/Console.svelte`, add to the `CLIENT_COMMANDS` map:
```typescript
showmycomponent: () => {
    ui.setShowMyComponent(true);
    return true;
},
```

`CLIENT_COMMANDS` handles commands entirely on the frontend — no Rust plugin needed. The key is the command name lowercased.

### Step 7 — Add to pcodeCommands.ts for tab completion

In `src-svelte/lib/pcodeCommands.ts`, add the command name to the Set so tab completion works:
```typescript
'ShowMyComponent',
```

**Tab completion applies to ALL commands** — both client-side (`CLIENT_COMMANDS`) and Rust plugin commands. Any command that should be discoverable via Tab must be added to `pcodeCommands.ts`. The key is the PascalCase command name exactly as the user would type it.

---

## Pattern 2 — New Tauri Command (backend data for a window)

If the window needs data from Rust (like `get_analysis_results`), add a `#[tauri::command]` function to `src-tauri/src/lib.rs` and register it in the `invoke_handler`.

```rust
#[tauri::command]
fn get_my_data(state: State<PhotoxState>) -> serde_json::Value {
    let ctx = state.context.lock().expect("context lock poisoned");
    serde_json::json!({ ... })
}
```

Add to the `tauri::generate_handler![]` macro in `run()`.

Invoke from Svelte:
```typescript
const data = await invoke<MyType>('get_my_data');
```

---

## Pattern 3 — New pcode Plugin (Rust, registry-dispatched)

Only use this when the command needs to read or modify `AppContext` on the Rust side. Do NOT use it just to open a window — use `CLIENT_COMMANDS` instead (Pattern 1).

1. Create `src-tauri/src/plugins/my_plugin.rs` implementing `PhotonPlugin`
2. Add `pub mod my_plugin;` to `src-tauri/src/plugins/mod.rs`
3. Register in `lib.rs` `run()`: `registry.register(Arc::new(plugins::my_plugin::MyPlugin));`
4. Add command name to `pcodeCommands.ts`

---

## Pattern 4 — Sending Output to the Console from Outside Console.svelte

Components other than `Console.svelte` (e.g. `QuickLaunch.svelte`) can write lines to the console using the `consolePipe` store in `consoleHistory.ts`.

### How it works

`consoleHistory.ts` exports a `consolePipe` writable store:
```typescript
export const consolePipe = writable<ConsoleLine | null>(null);
```

`Console.svelte` watches it via `$effect` and appends any non-null value to the console output, then resets it to null.

### Usage in an external component

```typescript
import { consolePipe } from '../stores/consoleHistory';

// Send a single line
consolePipe.set({ id: Date.now(), text: 'Hello from QuickLaunch', type: 'success' });

// Send multiple lines (e.g. from a plugin result)
result.message.split('\n').forEach(line => {
    if (line) consolePipe.set({ id: Date.now(), text: line, type: 'success' });
});
```

### Line types
- `'success'` — green, for successful plugin output
- `'error'`   — red, for errors
- `'warning'` — yellow, for warnings
- `'info'`    — dim, for informational messages
- `'output'`  — neutral, for general output
- `'input-echo'` — shows the `>` prompt prefix, for echoing user input

---

## Pattern 5 — Post-Command Side Effects in Console.svelte

When a Rust plugin command succeeds, `Console.svelte` runs `syncSessionState()` to handle any side effects. Add cases here for commands that need to trigger frontend state changes after execution.

```typescript
async function syncSessionState(cmd: string, args: Record<string, string>, output: string | null) {
    // existing cases...
    if (cmd === 'mycommand') {
        ui.doSomething();
    }
}
```

The same pattern applies in `QuickLaunch.svelte` — inspect `response.results` after `run_script` completes:

```typescript
for (const r of response.results) {
    if (r.command.toLowerCase() === 'mycommand' && r.success) {
        ui.doSomething();
    }
}
```

Both places must be updated if a command can be triggered from both the console and the Quick Launch bar.

---

## Key Principle

**If the action only affects the frontend (opening a window, changing a view), handle it entirely in `CLIENT_COMMANDS` and the `ui` store. Only go to Rust when you need `AppContext` data.**

The `DispatchResponse`, `PcodeResult`, and `ScriptResult` structs do NOT need modification to support new windows. Do not add event fields or event buses — the `ui` store boolean pattern is sufficient and established.
