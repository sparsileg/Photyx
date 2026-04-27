# Photyx — Adding New UI Windows and Frontend-Triggered Actions

## Overview

Photyx uses Tauri + Svelte + Rust. The frontend is in `src-svelte/`, the backend in `src-tauri/src/`. This note describes the established patterns for adding new viewer-region components, new pcode console commands, sliding panels, and other recurring UI constructs.

**Important:** Photyx is a single-window SPA. There are no floating OS windows or draggable overlays. Components that need full screen real estate (like the Analysis Graph) replace the viewer region entirely, controlled by the view registry in the `ui` store. The user closes them to return to the image viewer.

---

## Pattern 1 — New Viewer-Region Component (e.g. Analysis Graph, Analysis Results)

A viewer-region component replaces the image viewer when active. It is controlled by the view registry in `ui.ts` (see Pattern 6) and rendered in `+page.svelte` using `{#if}/{:else if}/{:else}` alongside `<Viewer>`.

### Step 1 — Register in the VIEWS constant in `src-svelte/lib/stores/ui.ts`

Add one entry to the `VIEWS` array:

```typescript
export const VIEWS = [
    'analysisGraph',
    'analysisResults',
    'myNewView',        // ← add here
] as const;
```

This is the only place you need to register a new view. `ViewName` type and `showView()` update automatically.

### Step 2 — Create the Svelte component

Place it in `src-svelte/lib/components/MyView.svelte`.

The component:
- Fills `100%` width and height of the viewer region
- Has a Close button that calls `ui.showView(null)`
- Has a toolbar row at the top for controls
- Uses `display: flex; flex-direction: column` layout
- Does NOT use `position: fixed` or dragging — it lives in the normal document flow

### Step 3 — Create the CSS file

Place it in `static/css/myview.css`. Import it in `src-svelte/app.html` alongside the other CSS imports:

```html
<link rel="stylesheet" href="/css/myview.css" />
```

Use the correct theme CSS variable names (see §CSS Variables below).

### Step 4 — Register in `+page.svelte`

In `src-svelte/routes/+page.svelte`:

```svelte
import MyView from '../lib/components/MyView.svelte';
```

In the viewer region, add an `{:else if}` branch:

```svelte
<div id="viewer-region">
    {#if $ui.activeView === 'analysisGraph'}
        <AnalysisGraph />
    {:else if $ui.activeView === 'analysisResults'}
        <AnalysisResults />
    {:else if $ui.activeView === 'myNewView'}
        <MyView />
    {:else}
        <Viewer onMousePixel={onMousePixel} />
    {/if}
    ...
</div>
```

Also update the filename overlay condition to exclude the new view:

```svelte
{:else if !$ui.blinkTabActive && $ui.activeView === null && ...}
```

### Step 5 — Trigger from the menu

In `src-svelte/lib/components/MenuBar.svelte`, add a menu item and handle it in the `action()` switch:

```typescript
case 'my-new-view': ui.showView('myNewView'); break;
```

### Step 6 — Trigger from the pcode console

In `src-svelte/lib/components/Console.svelte`, add to the `CLIENT_COMMANDS` map:

```typescript
showmynewview: () => {
    ui.showView('myNewView');
    return true;
},
```

### Step 7 — Add to pcodeCommands.ts for tab completion

In `src-svelte/lib/pcodeCommands.ts`, add the command name to the Set:

```typescript
'ShowMyNewView',
```

### Step 8 — Close button in the component

Always use `ui.showView(null)` — never individual boolean setters:

```svelte
<button onclick={() => ui.showView(null)}>✕ Close</button>
```

---

## Pattern 2 — New Tauri Command (backend data for a view)

If the view needs data from Rust (like `get_analysis_results`), add a `#[tauri::command]` function to `src-tauri/src/lib.rs` and register it in the `invoke_handler`.

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

Only use this when the command needs to read or modify `AppContext` on the Rust side. Do NOT use it just to open a view — use `CLIENT_COMMANDS` instead (Pattern 1).

1. Create `src-tauri/src/plugins/my_plugin.rs` implementing `PhotonPlugin`
2. Add `pub mod my_plugin;` to `src-tauri/src/plugins/mod.rs`
3. Register in `lib.rs` `run()`: `registry.register(Arc::new(plugins::my_plugin::MyPlugin));`
4. Add command name to `pcodeCommands.ts`

---

## Pattern 4 — Sending Output to the Console from Outside Console.svelte

Components other than `Console.svelte` (e.g. `QuickLaunch.svelte`, `MacroLibrary.svelte`, `MenuBar.svelte`) can write lines to the console using the `consolePipe` store in `consoleHistory.ts`.

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

## Pattern 6 — View Registry (showView)

All viewer-region components are managed through a central registry. This is the established pattern — never use individual boolean flags for viewer-region visibility.

### The registry

In `src-svelte/lib/stores/ui.ts`:

```typescript
// To add a new view: add one entry here. showView() handles the rest.
export const VIEWS = [
    'analysisGraph',
    'analysisResults',
] as const;

export type ViewName = typeof VIEWS[number];
```

`UIState` contains `activeView: ViewName | null` (null = image viewer).

### Showing a view

```typescript
ui.showView('analysisGraph');     // open Analysis Graph
ui.showView('analysisResults');   // open Analysis Results
ui.showView(null);                // return to image viewer
```

### Rules

- **Never** call `ui.setShowAnalysisGraph(true)` or any individual boolean setter for viewer-region visibility — those do not exist in the current implementation
- **Always** use `ui.showView()` — it is the single point of control
- **Close buttons** in viewer-region components always call `ui.showView(null)`
- **Menu items** and **console commands** that open views always call `ui.showView('viewName')`
- Adding a new view requires: one entry in `VIEWS`, one `{:else if}` in `+page.svelte`, one import in `+page.svelte`. Nothing else changes.

---

## Pattern 7 — Wide Sliding Panel

Some panels (e.g. Keyword Editor) need more horizontal space than the default 280px panel width. This is achieved by adding a `wide` CSS class to `#panel-container` based on which panel is active.

### In `IconSidebar.svelte`

```svelte
<div id="panel-container" class="open" class:wide={$ui.activePanel === 'keywords'}>
```

### In `layout.css`

```css
#panel-container.wide {
    width: 75vw !important;
}
```

Add additional `activePanel` values to the `class:wide` condition if other panels also need extra width.

---

## Pattern 8 — Inline Confirmation Bar

For destructive actions (delete, discard unsaved changes) use an inline confirmation bar within the component rather than native OS dialogs. Native dialogs (`window.confirm`, Tauri `dialog.confirm`) have permission and reliability issues in Tauri WebView.

### Pattern

Add a boolean state variable:

```typescript
let confirmingAction = $state(false);
```

Show the bar conditionally in the template:

```svelte
{#if confirmingAction}
    <div class="confirm-bar" onclick={(e) => e.stopPropagation()}>
        <span>Are you sure? This cannot be undone.</span>
        <button onclick={doAction}>Confirm</button>
        <button onclick={() => confirmingAction = false}>Cancel</button>
    </div>
{/if}
```

### Rules

- Always add `onclick={(e) => e.stopPropagation()}` to the bar div to prevent click-through
- Always add `onclick={(e) => { e.stopPropagation(); handler(); }}` to the buttons
- Cancel always sets `confirmingAction = false` and does nothing else
- The bar appears inline within the item it applies to (e.g. inside a macro list entry), not as a full-width overlay

### CSS

Style the bar using theme variables — see `macroeditor.css` (`.me-confirm-bar`) and `sidebar.css` (`.ml-confirm-bar`) for reference implementations.

---

## Pattern 9 — Running Notifications with Pulse Animation

For long-running operations, use `notifications.running()` instead of `notifications.info()`. This triggers a pulse animation on the status bar text, giving the user a clear visual indication that work is in progress.

```typescript
notifications.running('AnalyzeFrames running…');
// ... do work ...
notifications.success('AnalyzeFrames complete.');
```

The `running` notification type is defined in `notifications.ts` and styled in `statusbar.css` with a CSS `@keyframes status-pulse` animation. The pulse stops automatically when the next notification (success, error, info) replaces it.

Use `notifications.running()` for any operation that:
- Takes more than ~0.5 seconds
- Blocks the UI
- Has a clear completion state

Do not use it for instantaneous operations.

---

## Pattern 10 — Reactive Store-Derived State ($effect)

When component state must stay in sync with a Svelte store that can change from outside the component, use a `$effect` that watches the store and recomputes local state. This is the established pattern for the Macro Library's pinned state.

```typescript
// Automatically keep pinned state in sync with Quick Launch store
$effect(() => {
    const ql = $quickLaunch;
    const pinnedPaths = new Set<string>();
    for (const entry of ql) {
        const match = entry.script.match(/RunMacro path="([^"]+)"/);
        if (match) pinnedPaths.add(match[1].replace(/\\/g, '/'));
    }
    pinned = new Set(
        macros
            .filter(m => pinnedPaths.has(m.path.replace(/\\/g, '/')))
            .map(m => m.path)
    );
});
```

This pattern ensures that when the Quick Launch store changes (e.g. a button is removed via right-click context menu), the Macro Library's Pin/Pinned buttons update automatically without any manual coordination.

---

## CSS Variables Reference

All CSS files must use these theme variables (defined in `static/themes/dark.css`, `light.css`, `matrix.css`):

| Variable | Purpose |
|---|---|
| `--bg-color` | Main background |
| `--sidebar-bg` | Sidebar and panel background |
| `--card-bg` | Card/header background |
| `--card-hover` | Hover state background |
| `--input-bg` | Input field background |
| `--text-color` | Primary text |
| `--text-secondary` | Secondary/dim text |
| `--primary-color` | Accent color (green in Matrix) |
| `--primary-hover` | Accent hover |
| `--primary-text` | Text on primary-colored backgrounds |
| `--border-color` | Primary border |
| `--border-color-light` | Subtle border |
| `--success-color` | Success state |
| `--warning-color` | Warning state |
| `--error-color` | Error state |

**Never** use variables like `--color-bg`, `--color-text`, `--color-border` — these do not exist in the theme files and will produce invisible/broken UI in Light and Dark themes.

---

## Key Principles

**If the action only affects the frontend (opening a view, changing a setting), handle it entirely in `CLIENT_COMMANDS` and the `ui` store. Only go to Rust when you need `AppContext` data.**

**All viewer-region views are managed exclusively through `ui.showView()`. Individual boolean flags for view visibility are not permitted.**

**Native OS dialogs (`window.confirm`, `window.prompt` for confirmation) are unreliable in Tauri. Use inline confirmation bars (Pattern 8) for destructive actions.**

**CSS changes in `static/` require a manual browser refresh — they are not hot-reloaded by Vite.**

**Rust recompilation is required only when files in `src-tauri/` change. Svelte and TypeScript changes hot-reload instantly.**
