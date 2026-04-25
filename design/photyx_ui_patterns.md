# Photyx — Adding New UI Windows and Frontend-Triggered Actions

## Overview

Photyx uses Tauri + Svelte + Rust. The frontend is in `src-svelte/`, the backend in `src-tauri/src/`. This note describes the established pattern for adding new floating windows (like the Analysis Graph) and new pcode console commands that trigger frontend actions.

---

## Pattern 1 — New Floating Window (e.g. Analysis Graph)

A floating window is a Svelte component that renders conditionally based on a boolean in the `ui` store. It is **not** a modal with a backdrop — it is `position: fixed` and draggable.

### Step 1 — Add a boolean to `src-svelte/lib/stores/ui.ts`

Add to the `UIState` interface:
```typescript
showMyWindow: boolean;
```

Add to the `initial` object:
```typescript
showMyWindow: false,
```

Add a setter to the store's returned object:
```typescript
setShowMyWindow: (v: boolean) => update(s => ({ ...s, showMyWindow: v })),
```

### Step 2 — Create the Svelte component

Place it in `src-svelte/lib/components/MyWindow.svelte`.

The component:
- Reads `$ui.showMyWindow` and wraps everything in `{#if $ui.showMyWindow}`
- Has a drag handle div with `onmousedown` that adds `mousemove`/`mouseup` listeners to `window`
- Cleans up listeners in `onDestroy`
- Has a close button that calls `ui.setShowMyWindow(false)`
- Uses `position: fixed` with `left` and `top` bound to `$state` variables

### Step 3 — Create the CSS file

Place it in your styles directory (e.g. `src-svelte/styles/MyWindow.css`). Import it wherever other component CSS files are imported.

### Step 4 — Register in `+page.svelte`

In `src-svelte/routes/+page.svelte`:
```svelte
import MyWindow from '../lib/components/MyWindow.svelte';
```

Add to the template (outside `#app`, alongside `KeywordModal`):
```svelte
<MyWindow />
```

### Step 5 — Trigger from the menu

In `src-svelte/lib/components/MenuBar.svelte`, add a menu item to the appropriate menu and handle it in the `action()` switch:
```typescript
case 'my-window': ui.setShowMyWindow(true); break;
```

### Step 6 — Trigger from the pcode console

In `src-svelte/lib/components/Console.svelte`, add to the `CLIENT_COMMANDS` map:
```typescript
showmywindow: () => {
    ui.setShowMyWindow(true);
    return true;
},
```

`CLIENT_COMMANDS` handles commands entirely on the frontend — no Rust plugin needed. The key is the command name lowercased.

### Step 7 — Add to pcodeCommands.ts

In `src-svelte/lib/pcodeCommands.ts`, add the command name to the Set so tab completion works:
```typescript
'ShowMyWindow',
```

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

## Key Principle

**If the action only affects the frontend (opening a window, changing a view), handle it entirely in `CLIENT_COMMANDS` and the `ui` store. Only go to Rust when you need `AppContext` data.**

The `DispatchResponse`, `PcodeResult`, and `ScriptResult` structs do NOT need modification to support new windows. Do not add event fields or event buses — the `ui` store boolean pattern is sufficient and established.
