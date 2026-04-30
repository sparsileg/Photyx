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
5. Add a help entry to `pcodeHelp.ts` — every command must have a help entry for the `help <command>` console feature to work
6. Add an `ARG_HINTS` entry to `Console.svelte` for tab completion argument hints

## Pattern 4 — Sending Output to the Console from Outside Console.svelte

Components other than `Console.svelte` (e.g. `QuickLaunch.svelte`, `MacroLibrary.svelte`, `MenuBar.svelte`) can write lines to the console using the `consolePipe` store in `consoleHistory.ts`.

### How it works

`consoleHistory.ts` exports a `consolePipe` writable store:

```typescript
export const consolePipe = writable<ConsoleLine | null>(null);
```

`Console.svelte` watches it via `$effect` and appends any non-null value to the console output, then resets it to null.

### Usage in an external component

**Always use `pipeToConsole()` — never call `consolePipe.set()` directly.** `consolePipe` is a queue; direct `.set()` calls overwrite each other on rapid successive writes. See Pattern 15 for the full queue rule.

```typescript
import { pipeToConsole } from '../stores/consoleHistory';

// Send a single line
pipeToConsole('Hello from QuickLaunch', 'success');

// Send multiple lines
result.message.split('\n').forEach(line => {
    if (line) pipeToConsole(line, 'success');
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

## Pattern 5 — Post-Command Side Effects

**Do not use command-name matching for post-command side effects.** The established pattern is `client_actions` (Pattern 18). Command-name matching in `syncSessionState` and result loops is a legacy approach that breaks when commands are wrapped in macros.

`syncSessionState` in `Console.svelte` is retained for session sync (directory, file list) and for handling `ContourHeatmap` and `LoadFile` which use the `dispatch_command` path rather than `run_script`. Do not add new command-name cases there for display or UI side effects — use `client_actions` instead.

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

### Variant — Pinned Warning (MacroLibrary)

When a macro cannot be deleted because it is currently pinned to Quick Launch, show a **warning bar** instead of a confirmation bar — no Confirm button, just a message and Cancel:

```svelte
{#if pinnedWarning}
    <div class="confirm-bar" onclick={(e) => e.stopPropagation()}>
        <span>Remove from Quick Launch first.</span>
        <button onclick={(e) => { e.stopPropagation(); pinnedWarning = false; }}>OK</button>
    </div>
{/if}
```

The warning bar uses the same CSS as the confirmation bar. It is not a confirmation — the destructive action does not proceed. See `MacroLibrary.svelte` for the reference implementation.

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

## Pattern 11 — Displaying a Temporary Image Outside the Session

Use `ui.setDisplayImage(dataUrl)` to display an image in the viewer without adding it to the session file list. This is used for heatmaps generated by `ContourHeatmap` and files loaded via `File > Load Single Image`.

```typescript
ui.setDisplayImage(dataUrl);  // display image
ui.setDisplayImage(null);     // clear — viewer returns to session frame
```

`Viewer.svelte` watches `$ui.displayImageUrl` via a `$effect` and calls `drawImageFromUrl()` when set. The `needsFullRes` effect checks `$ui.displayImageUrl` and skips loading the session frame if set, preventing zoom changes from reverting to the source image.

`displayImageUrl` is cleared automatically by `requestFrameRefresh()` and `clearViewer()`.

**Rules:**

- Never use `requestFrameRefresh()` to display a temporary image — use `setDisplayImage()` instead
- Plugins that create output files should store the output path in `ctx.variables["NEW_FILE"]` so the frontend can retrieve it via `get_variable` and call `loadFile(path)`
- Scripts (`run_script` path) never trigger `setDisplayImage()` — only interactive paths (console, menu, Quick Launch) do

---

## Pattern 12 — Plugin Output Data (DispatchResponse.data)

When a plugin returns `PluginOutput::Data(json)`, the full JSON value is now passed through `DispatchResponse.data` to the frontend. This allows plugins to return rich structured data beyond just a message string.

### Rust side

```rust
Ok(PluginOutput::Data(json!({
    "plugin":   "MyPlugin",
    "output":   out_path,
    "message":  message,
})))
```

### Frontend side (Console.svelte)

The `dispatch` invoke type includes `data`:

```typescript
const response = await invoke<{
    success: boolean;
    output: string | null;
    error: string | null;
    data: Record<string, unknown> | null;
}>('dispatch_command', { request: { command, args } });
```

`syncSessionState` receives `data` as a fourth parameter:

```typescript
async function syncSessionState(
    cmd: string,
    args: Record<string, string>,
    output: string | null,
    data: Record<string, unknown> | null = null
) {
    if (cmd === 'myplugin') {
        const filePath = data?.output as string | null;
        if (filePath) await loadFile(filePath);
    }
}
```

**Rules:**

- `data` is only available via `dispatch_command` (interactive path), not `run_script` (scripted path)
- Use `data` for file paths, URLs, and structured results — use `output` (message string) for console display

---

## Pattern 13 — Format-Agnostic Single File Reader

To read any supported image format from disk into an `ImageBuffer`, use `read_image_file()` from `plugins/image_reader.rs`. It dispatches by file extension to the appropriate format-specific reader.

```rust
use crate::plugins::image_reader::read_image_file;

let buffer = read_image_file(&path)
    .map_err(|e| format!("Failed to load '{}': {}", path, e))?;
```

Supported extensions: `fit`, `fits`, `fts`, `xisf`, `tif`, `tiff`.

The per-format helpers remain the single source of truth:

- `plugins/read_fits.rs` → `read_fits_file(path)`
- `plugins/read_xisf.rs` → `read_xisf_file(path)`
- `plugins/read_tiff.rs` → `read_tiff_file(path)`

**Never duplicate file reading logic** — always call the appropriate helper or `read_image_file()`.

---

## Pattern 14 — Plugin Output File Convention ($NEW_FILE)

When a plugin creates a new file on disk, store its path in `ctx.variables["NEW_FILE"]`:

```rust
ctx.variables.insert("NEW_FILE".to_string(), out_path.clone());
```

This allows pcode scripts to use `$NEW_FILE` in subsequent commands:

```ContourHeatmap
ContourHeatmap
MoveFile source="$NEW_FILE" destination="D:/heatmaps/"
```

The frontend can retrieve it via the `get_variable` Tauri command:

```typescript
const path = await invoke<string | null>('get_variable', { name: 'NEW_FILE' });
if (path) await loadFile(path);
```

**Rules:**

- Any plugin that creates a new file should set `$NEW_FILE`
- `$NEW_FILE` is overwritten each time a plugin runs — scripts should use it immediately after the generating command
- Interactive paths (console, menu, Quick Launch) use `data.output` from `DispatchResponse` in preference to `get_variable` when available

---

## Pattern 15 — consolePipe Queue Rule

`consolePipe` in `consoleHistory.ts` is a **queue**, not a signal. `Console.svelte` watches it via `$effect` and appends each non-null value then resets to null — but if two `.set()` calls fire in the same microtask tick, the second overwrites the first before the effect runs, silently dropping a line.

**Always use `pipeToConsole(text, type)` — never call `consolePipe.set()` directly.**

```typescript
import { pipeToConsole } from '../stores/consoleHistory';

pipeToConsole('Operation complete.', 'success');
pipeToConsole('Warning: file already exists.', 'warning');
```

`pipeToConsole()` internally enqueues lines so rapid successive calls are all delivered in order. Direct `.set()` calls bypass the queue and are not safe for multi-line output.

This rule applies everywhere outside `Console.svelte`: `QuickLaunch.svelte`, `MacroLibrary.svelte`, `MenuBar.svelte`, and any future component that writes to the console.

---

## Pattern 16 — Help Modal

The Help Modal displays contextual help for a pcode command. It is triggered from the console by typing `help <command>`.

### Data source

Command help entries live in `src-svelte/lib/pcodeHelp.ts`. Add an entry there for every new command:

```typescript
export const PCODE_HELP: Record<string, HelpEntry> = {
    AutoStretch: {
        syntax: 'AutoStretch',
        description: 'Applies Auto-STF stretch to the current frame.',
        params: [],
    },
    MyCommand: {
        syntax: 'MyCommand param="value"',
        description: 'Does something useful.',
        params: [
            { name: 'param', required: false, description: 'Controls the thing.' },
        ],
    },
};
```

### Prop wiring

`Console.svelte` exposes an `onhelp` callback prop:

```typescript
let { onhelp }: { onhelp?: (entry: HelpEntry | null) => void } = $props();
```

When the user types `help <command>`, `Console.svelte` looks up the entry in `PCODE_HELP` and calls `onhelp(entry)` (or `onhelp(null)` if not found, which Console handles by printing an error itself).

In `+page.svelte`, `helpEntry` state receives the callback and passes it to `HelpModal`:

```svelte
<Console onhelp={(entry) => helpEntry = entry} />

{#if helpEntry}
    <HelpModal entry={helpEntry} onclose={() => helpEntry = null} />
{/if}
```

### Positioning and z-index

```css
.help-modal {
    position: fixed;
    top: 96px;          /* clears menu (28px) + toolbar (34px) + quick launch (34px) */
    right: 16px;
    z-index: 500;
    max-width: 420px;
}
```

The modal is dismissable via its close button or the `Escape` key. The `Escape` handler is attached at the document level while the modal is open, and removed on close.

---

## Pattern 17 — Fixed Overlays (z-index Hierarchy)

Components that need to cover the full viewer region (expanded console, macro editor, help modal) use `position: fixed` to break out of any parent stacking context created by `transform`, `opacity`, or `will-change`.

### z-index hierarchy

| Layer              | z-index |
| ------------------ | ------- |
| Menu bar           | 200     |
| Status bar         | 200     |
| Toolbar            | 190     |
| Quick Launch       | 180     |
| Sliding panels     | 185     |
| Icon sidebar       | 150     |
| Console (expanded) | 300     |
| Macro Editor       | 400     |
| Help Modal         | 500     |

Higher values appear on top. Do not assign z-index values outside this table without updating it.

### Top offset rule

All fixed overlays must start below the top bars to avoid covering the menu and toolbar chrome:

```
menu bar:    28px
toolbar:     34px
quick launch: 34px
─────────────────
top:         96px
```

```css
.my-overlay {
    position: fixed;
    top: 96px;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 300;   /* pick from hierarchy table */
}
```

Overlays that should not extend to the bottom (e.g. the Help Modal in the upper-right corner) set `top: 96px` and `right: 16px` with an explicit `max-height` and `overflow-y: auto` rather than `bottom: 0`.

---

## Pattern 18 — Plugin Client Actions (client_actions)

When a plugin needs the frontend to perform a side effect after execution — refreshing the display, drawing an overlay, opening a modal — it declares this by emitting a `client_action` string in its `PluginOutput::Data` JSON. The frontend dispatches these actions after `run_script` returns, with no command-name matching required.

### Rust side — emitting an action

```rust
Ok(PluginOutput::Data(serde_json::json!({
    "message":       "Operation complete",
    "client_action": "refresh_autostretch",
})))
```

### Registered actions

| Action                | Plugin         | Frontend effect                 |
| --------------------- | -------------- | ------------------------------- |
| `refresh_autostretch` | AutoStretch    | Calls `applyAutoStretch()`      |
| `refresh_annotations` | ComputeFWHM    | Calls `ui.refreshAnnotations()` |
| `open_keyword_modal`  | ListKeywords   | Calls `ui.openKeywordModal()`   |

### Frontend dispatch — all three entry points

`QuickLaunch.svelte`, `MacroLibrary.svelte`, and `Console.svelte` all dispatch `client_actions` using the same pattern:

```typescript
let autoStretched = false;
for (const action of response.client_actions) {
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

### RunMacro propagation

`RunMacro` automatically collects `client_actions` from all inner plugin results and re-emits them in its own response. A macro wrapping `AutoStretch` correctly delivers `refresh_autostretch` to the frontend — no special handling required.

### Rules

- **Never** use command-name matching for display or UI side effects — use `client_actions`
- **Every new plugin** that affects the display or triggers a UI side effect must emit the appropriate `client_action` — this is part of the plugin contract
- **The frontend dispatch table** (above) must be updated when a new action is registered
- `display_changed` remains in `ScriptResponse` for the non-macro direct command path — always guard `requestFrameRefresh()` with `!autoStretched` to prevent overwriting a stretched display


---

## CSS Variables Reference

All CSS files must use these theme variables (defined in `static/themes/dark.css`, `light.css`, `matrix.css`):

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

**Never** use variables like `--color-bg`, `--color-text`, `--color-border` — these do not exist in the theme files and will produce invisible/broken UI in Light and Dark themes.

---

## Key Principles
pp
**If the action only affects the frontend (opening a view, changing a setting), handle it entirely in `CLIENT_COMMANDS` and the `ui` store. Only go to Rust when you need `AppContext` data.**

**All viewer-region views are managed exclusively through `ui.showView()`. Individual boolean flags for view visibility are not permitted.**

**Native OS dialogs (`window.confirm`, `window.prompt` for confirmation) are unreliable in Tauri. Use inline confirmation bars (Pattern 8) for destructive actions.**

**CSS changes in `static/` require a manual browser refresh — they are not hot-reloaded by Vite.**

**Rust recompilation is required only when files in `src-tauri/` change. Svelte and TypeScript changes hot-reload instantly.**

**Temporary images (heatmaps, single loaded files) are displayed via `ui.setDisplayImage()` and never added to the session file list directly. See Pattern 11.**

**Plugins that create output files always store the path in `ctx.variables["NEW_FILE"]`. See Pattern 14.**

**Never duplicate file reading logic — use `read_image_file()` from `image_reader.rs` for format-agnostic loading. See Pattern 13.**

**`DispatchResponse.data` carries the full plugin JSON response to the frontend for interactive calls only. Scripts use `run_script` which only exposes the message string. See Pattern 12.**

**Always use `pipeToConsole()` to write to the console from outside `Console.svelte` — never call `consolePipe.set()` directly. Rapid direct `.set()` calls silently drop lines. See Pattern 15.**

**Help Modal data lives exclusively in `pcodeHelp.ts`. Add an entry there for every new pcode command. See Pattern 16.**

**Fixed overlays use `position: fixed`, start at `top: 96px`, and must respect the z-index hierarchy (panels 140, console expanded 300, macro editor 400, help modal 500). See Pattern 17.**
