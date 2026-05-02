# Photyx — Adding New UI Windows and Frontend-Triggered Actions

## Overview

Photyx uses Tauri + Svelte + Rust. The frontend is in `src-svelte/`, the backend in `src-tauri/src/`. This note describes the established patterns for adding new viewer-region components, new pcode console commands, sliding panels, and other recurring UI constructs.

**Important:** Photyx is a single-window SPA. There are no floating OS windows or draggable overlays. Components that need full screen real estate (like the Analysis Graph) replace the viewer region entirely, controlled by the view registry in the `ui` store. The user closes them to return to the image viewer.

---

## Pattern 1 — New Viewer-Region Component (e.g. Analysis Graph, Analysis Results)

A viewer-region component replaces the image viewer when active. It is controlled by the view registry in `ui.ts` (see Pattern 6) and rendered in `+page.svelte` using `{#if}/{:else if}/{:else}` alongside `<Viewer>`.

### Step 1 — Register in the VIEWS constant in `src-svelte/lib/stores/ui.ts`

```typescript
export const VIEWS = [
    'analysisGraph',
    'analysisResults',
    'myNewView',        // ← add here
] as const;
```

### Step 2 — Create the Svelte component

Place it in `src-svelte/lib/components/MyView.svelte`. The component fills 100% width and height, has a Close button calling `ui.showView(null)`, and uses flex column layout.

### Step 3 — Create the CSS file

Place it in `static/css/myview.css`. Import in `src-svelte/app.html`. Use theme CSS variable names (see CSS Variables below).

### Step 4 — Register in `+page.svelte`

Import the component and add an `{:else if}` branch in the viewer region.

### Step 5 — Trigger from the menu

In `MenuBar.svelte`, add a menu item and handle it in the `action()` switch:

```typescript
case 'my-new-view': ui.showView('myNewView'); break;
```

### Step 6 — Trigger from the pcode console

In `Console.svelte`, add to the `CLIENT_COMMANDS` map:

```typescript
showmynewview: () => { ui.showView('myNewView'); },
```

### Step 7 — Add to pcodeCommands.ts for tab completion

### Step 8 — Close button in the component

Always use `ui.showView(null)` — never individual boolean setters.

---

## Pattern 2 — New Tauri Command (backend data for a view)

Add a `#[tauri::command]` function in the appropriate `src-tauri/src/commands/` submodule. Register it in `lib.rs` invoke handler using fully qualified path (`commands::mymodule::my_command`).

---

## Pattern 3 — New pcode Plugin (Rust, registry-dispatched)

1. Create `src-tauri/src/plugins/my_plugin.rs` implementing `PhotonPlugin`
2. Add `pub mod my_plugin;` to `src-tauri/src/plugins/mod.rs`
3. Register in `lib.rs` `run()`: `registry.register(Arc::new(plugins::my_plugin::MyPlugin));`
4. Add command name to `pcodeCommands.ts`
5. Add a help entry to `pcodeHelp.ts`
6. Add an `ARG_HINTS` entry to `Console.svelte` for tab completion argument hints

---

## Pattern 4 — Sending Output to the Console from Outside Console.svelte

**Always use `pipeToConsole()` — never call `consolePipe.set()` directly.**

`consolePipe` is a queue (`ConsoleLine[]`). Direct `.set()` calls corrupt the queue by replacing the array with a single object. The next `pipeToConsole()` call tries to spread the object (`[...q, newLine]`) and throws a TypeError. This is a silent, hard-to-diagnose bug.

```typescript
import { pipeToConsole } from '../stores/consoleHistory';

pipeToConsole('Hello from QuickLaunch', 'success');

result.message.split('\n').forEach(line => {
    if (line) pipeToConsole(line, 'success');
});
```

This applies everywhere outside `Console.svelte`: `QuickLaunch.svelte`, `MacroLibrary.svelte`, `MenuBar.svelte`, and any future component that writes to the console.

### Line types

- `'success'` — green
- `'error'` — red
- `'warning'` — yellow
- `'info'` — dim
- `'output'` — neutral
- `'input-echo'` — shows `>` prompt prefix

---

## Pattern 5 — Post-Command Side Effects

**Do not use command-name matching for post-command side effects.** The established pattern is `client_actions` (Pattern 18). `syncSessionState` in `Console.svelte` is retained for session sync only.

---

## Pattern 6 — View Registry (showView)

All viewer-region components are managed through a central registry. Never use individual boolean flags for viewer-region visibility.

```typescript
ui.showView('analysisGraph');     // open Analysis Graph
ui.showView('analysisResults');   // open Analysis Results
ui.showView(null);                // return to image viewer
```

Adding a new view requires: one entry in `VIEWS`, one `{:else if}` in `+page.svelte`, one import. Nothing else changes.

---

## Pattern 7 — Wide Sliding Panel

```svelte
<div id="panel-container" class="open" class:wide={$ui.activePanel === 'keywords'}>
```

```css
#panel-container.wide { width: 75vw !important; }
```

---

## Pattern 8 — Inline Confirmation Bar

For destructive actions, use an inline confirmation bar within the component rather than native OS dialogs. Native dialogs are unreliable in Tauri WebView.

```svelte
{#if confirmingAction}
    <div class="confirm-bar" onclick={(e) => e.stopPropagation()}>
        <span>Are you sure? This cannot be undone.</span>
        <button onclick={(e) => { e.stopPropagation(); doAction(); }}>Confirm</button>
        <button onclick={(e) => { e.stopPropagation(); confirmingAction = false; }}>Cancel</button>
    </div>
{/if}
```

Always add `stopPropagation()` to prevent click-through to parent handlers.

---

## Pattern 9 — Running Notifications with Pulse Animation + Expanded Bar

```typescript
notifications.running('AnalyzeFrames running…');
// ... do work ...
notifications.success('AnalyzeFrames complete.');
```

The `running` type triggers:

1. CSS pulse animation on status bar text and icon
2. Status bar expands to 66px (3× normal) with 33px font, positioned as `absolute` overlay
3. Dark semi-transparent background (`rgba(0,0,0,0.85)`) for readability
4. Smooth transition back when next notification fires

`#app` must have `position: relative` for the expanded bar to anchor correctly. The normal 22px status bar remains in the document flow as a placeholder — only the expanded running state overlays.

Use `notifications.running()` for any operation that takes more than ~0.5 seconds and has a clear completion state.

---

## Pattern 10 — Reactive Store-Derived State ($effect)

When component state must stay in sync with a Svelte store that can change from outside the component, use a `$effect` that watches the store and recomputes local state.

---

## Pattern 11 — Displaying a Temporary Image Outside the Session

Use `ui.setDisplayImage(dataUrl)` to display an image without adding it to the session file list. `displayImageUrl` is cleared automatically by `requestFrameRefresh()` and `clearViewer()`.

---

## Pattern 12 — Plugin Output Data (DispatchResponse.data)

When a plugin returns `PluginOutput::Data(json)`, the full JSON is passed through `DispatchResponse.data`. Available via `dispatch_command` only, not `run_script`.

---

## Pattern 13 — Format-Agnostic Single File Reader

Use `read_image_file()` from `plugins/image_reader.rs` for format-agnostic loading. Never duplicate file reading logic.

---

## Pattern 14 — Plugin Output File Convention ($NEW_FILE)

```rust
ctx.variables.insert("NEW_FILE".to_string(), out_path.clone());
```

Allows pcode scripts to use `$NEW_FILE` in subsequent commands. Frontend retrieves via `get_variable` Tauri command.

---

## Pattern 15 — consolePipe Queue Rule

**Always use `pipeToConsole(text, type)` — never call `consolePipe.set()` directly.**

Direct `.set()` calls corrupt the queue. The next `pipeToConsole()` call attempts to spread a non-array and throws:

```
TypeError: Spread syntax requires ...iterable[Symbol.iterator] to be a function
```

This error surfaces as a "Quick Launch error" or similar in the notification bar. The fix is always `pipeToConsole()`.

---

## Pattern 16 — Help Modal

Command help entries live in `src-svelte/lib/pcodeHelp.ts`. Add an entry there for every new command.

---

## Pattern 17 — Fixed Overlays (z-index Hierarchy)

### z-index hierarchy

| Layer                          | z-index |
| ------------------------------ | ------- |
| Menu bar                       | 200     |
| Status bar                     | 200     |
| Status bar (running, expanded) | 400     |
| Toolbar                        | 190     |
| Quick Launch                   | 180     |
| Sliding panels                 | 185     |
| Icon sidebar                   | 150     |
| Console (expanded)             | 300     |
| Macro Editor                   | 400     |
| Modal dialogs                  | 450     |
| Help Modal                     | 500     |

### Top offset rule

```
menu bar:    28px
toolbar:     34px
quick launch: 34px
─────────────────
top:         96px
```

---

## Pattern 18 — Plugin Client Actions (client_actions)

When a plugin needs the frontend to perform a side effect after execution, it emits a `client_action` string in its `PluginOutput::Data` JSON.

### Rust side

```rust
Ok(PluginOutput::Data(serde_json::json!({
    "message":       "Operation complete",
    "client_action": "refresh_autostretch",
})))
```

### Registered actions

| Action                | Plugin       | Frontend effect                 |
| --------------------- | ------------ | ------------------------------- |
| `refresh_autostretch` | AutoStretch  | Calls `applyAutoStretch()`      |
| `refresh_annotations` | ComputeFWHM  | Calls `ui.refreshAnnotations()` |
| `open_keyword_modal`  | ListKeywords | Calls `ui.openKeywordModal()`   |

### Frontend dispatch — all three entry points

`QuickLaunch.svelte`, `MacroLibrary.svelte`, and `Console.svelte` all dispatch `client_actions` using:

```typescript
if (!Array.isArray(response.client_actions)) {
    console.warn('client_actions was not an array:', response.client_actions);
}
let autoStretched = false;
for (const action of response.client_actions ?? []) {
    if (action === 'refresh_autostretch') {
        await applyAutoStretch();
        autoStretched = true;
    }
    if (action === 'refresh_annotations') ui.refreshAnnotations();
    if (action === 'open_keyword_modal')  ui.openKeywordModal();
}
if (response.display_changed && !autoStretched) {
    ui.requestFrameRefresh();
}
```

Note the defensive guard — `response.client_actions` should always be an array but a warning log is included for debugging.

### Rules

- **Never** use command-name matching for display or UI side effects — use `client_actions`
- **Every new plugin** that affects the display or triggers a UI side effect must emit the appropriate `client_action`
- `display_changed` remains in `ScriptResponse` for the non-macro direct command path

---

## Pattern 19 — Modal Dialogs (Preferences, Analysis Parameters)

Modal dialogs use the `pref-backdrop` / `pref-dialog` CSS class pattern from `preferences.css`.

**Key rules:**

- The backdrop div does NOT have `onclick={cancel}` — clicking outside the dialog does not dismiss it. The user must use Cancel, OK, or the ✕ button.
- The dialog div has `onclick={(e) => e.stopPropagation()}` to prevent clicks from bubbling to the backdrop.
- Escape key triggers cancel.
- Draft-copy pattern: nothing is written until OK or Apply. Cancel discards all changes.

**Width:** Preferences dialog is 540px. Analysis Parameters dialog is 400px (set via `.tp-dialog { width: 400px; }` override of `.pref-dialog`).

---

## Pattern 20 — Dropdown Component Usage

`Dropdown.svelte` is a custom CSS-friendly select component. It appends its menu to `document.body` to escape stacking contexts.

**Critical usage rule:** Use `value={x} on:change={(e) => { x = e.detail; }}` — NOT `bind:value={x}`. The Svelte 4/5 boundary means `bind:value` does not reliably propagate changes back to Svelte 5 parent `$state` variables.

```svelte
<!-- CORRECT -->
<Dropdown
    value={formatFilter}
    options={FORMAT_FILTERS.map(f => ({ value: f.id, label: f.label }))}
    on:change={(e) => { formatFilter = e.detail as FormatFilter; }}
/>

<!-- WRONG — bind:value does not work reliably across Svelte 4/5 boundary -->
<Dropdown bind:value={formatFilter} ... />
```

**Panel close guard:** `IconSidebar.svelte`'s outside-click handler includes `.dropdown-menu` in its exclusion list:

```typescript
if (target.closest('#panel-container') || target.closest('#icon-sidebar') ||
    target.closest('.macro-editor-panel') || target.closest('.dropdown-menu')) return;
```

Without this, clicking a dropdown menu item (which is appended to `document.body`, outside the panel) closes the sliding panel.

**Blink resolution:** After changing `blinkResolution` via the dropdown, call `ui.setBlinkResolution(blinkResolution)` explicitly so `Viewer.svelte` uses the correct scale factor for blink frame rendering.

---

## CSS Variables Reference

All CSS files must use these theme variables:

| Variable               | Purpose                             |
| ---------------------- | ----------------------------------- |
| `--bg-color`           | Main background                     |
| `--sidebar-bg`         | Sidebar and panel background        |
| `--card-bg`            | Card/header background              |
| `--card-hover`         | Hover state background              |
| `--input-bg`           | Input field background              |
| `--text-color`         | Primary text                        |
| `--text-secondary`     | Secondary/dim text                  |
| `--primary-color`      | Accent color (green in Matrix)      |
| `--primary-hover`      | Accent hover                        |
| `--primary-text`       | Text on primary-colored backgrounds |
| `--border-color`       | Primary border                      |
| `--border-color-light` | Subtle border                       |
| `--success-color`      | Success state                       |
| `--warning-color`      | Warning state                       |
| `--error-color`        | Error state                         |

**Never** use variables like `--color-bg`, `--color-text`, `--color-border`.

---

## Key Principles

**If the action only affects the frontend, handle it entirely in `CLIENT_COMMANDS` and the `ui` store. Only go to Rust when you need `AppContext` data.**

**All viewer-region views are managed exclusively through `ui.showView()`. Individual boolean flags for view visibility are not permitted.**

**Native OS dialogs (`window.confirm`, `window.prompt`) are unreliable in Tauri. Use inline confirmation bars (Pattern 8) for destructive actions.**

**CSS changes in `static/` require a manual browser refresh — they are not hot-reloaded by Vite.**

**Rust recompilation is required only when files in `src-tauri/` change. Svelte and TypeScript changes hot-reload instantly.**

**Always use `pipeToConsole()` to write to the console from outside `Console.svelte` — never call `consolePipe.set()` directly. Direct `.set()` calls corrupt the queue and cause spread TypeErrors. See Pattern 15.**

**Dropdown components must use `value` + `on:change` — not `bind:value`. The Svelte 4/5 boundary breaks two-way binding. See Pattern 20.**

**Modal dialogs do not close on backdrop click. Users must use Cancel, OK, or ✕. See Pattern 19.**

**The status bar expands to 3× height when `notifications.running()` is active. `#app` must have `position: relative`. See Pattern 9.**

**Fixed overlays use `position: fixed`, start at `top: 96px`, and must respect the z-index hierarchy. See Pattern 17.**
