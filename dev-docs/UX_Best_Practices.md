# UX & Frontend Architecture — Best Practices

*A project-agnostic pattern library, refined across the Photyx
build. Assumes a native-backend + JS/HTML/CSS frontend
(e.g. Tauri). Frontend framework is left open — the patterns apply
whether the view layer is Svelte, vanilla JS, React, or otherwise;
examples below are framework-neutral pseudocode.*

## Index

1. Single-Region View Registry
2. Backend Command Registry
3. Extensible Plugin/Command Registry
4. Single Write Path for Shared State
5. Declarative Side-Effect Signaling
6. State-Driven Panel Sizing
7. Inline Confirmation Bar
8. Long-Running Operation Feedback
9. Sync Local State with an External Store
10. Centralize Format/File Parsing Logic
11. Self-Documenting Commands
12. Explicit Z-Index Hierarchy
13. Modal Dialogs — Draft-Copy Pattern
14. Portaled Dropdowns and Menus
15. Preview-as-Confirmation
16. Terminal Operation Sequences
17. Right-Click Context Menus
18. Two-Row Toolbar
19. Semantic Theme Variables

---

## 1. Single-Region View Registry

Full-screen or major UI components (a data view, a settings screen, an
analysis panel) should replace a single content region rather than
opening as floating windows or overlays. A native-feeling desktop SPA
has no draggable child windows.

Manage which component occupies that region through one central
registry — a single named enum/union of view states plus a
`showView(name | null)` function — rather than a scattered set of
boolean flags (`showSettings`, `showAnalysis`,
`showResults`...). Booleans multiply combinatorially and eventually
allow two "exclusive" views to be true at once; a single named slot
cannot.

```js
const VIEWS = ['settingsView', 'resultsView', 'myNewView'];
function showView(name) { activeView.set(name); } // null returns to the default/main view
```

Adding a new view costs one registry entry and one render-branch —
nothing else changes.

## 2. Backend Command Registry

Native/backend commands (Tauri commands, IPC handlers, etc.) should
live in small modules grouped by domain, and be registered in exactly
one place using fully-qualified names (`commands::files::read`, not a
bare `read`). This keeps the registration point readable as a manifest
of everything the frontend can call, and avoids name collisions as the
command surface grows.

## 3. Extensible Plugin/Command Registry

For apps with a scripting console, macro system, or plugin
architecture, define a common interface/trait that all commands
implement, and register instances into a single registry rather than
hand-writing a dispatch switch statement. Pair every registration
with:

- a help-text entry (for a help modal or `--help`)
- an argument-hint entry (for tab completion / autocomplete)

Treat "add a command" as a fixed checklist (implement → register →
document → hint) so nothing is silently under-documented.

## 4. Single Write Path for Shared State

Any shared, queue-like, or list-like piece of state (a console output
buffer, a notification queue, an undo stack) should have exactly one
function that mutates it. Every other part of the app calls that
function — nothing else calls the underlying store's raw setter
directly.

This isn't just tidiness: a raw `.set()` call that replaces an array
with a single object is a silent, hard-to-diagnose bug — the *next*
correct call (which assumes an array) throws deep inside a spread
operation, far from the actual mistake. A single append function makes
the invariant impossible to violate.

## 5. Declarative Side-Effect Signaling

When a backend command or action needs the frontend to do something
afterward (refresh a view, open a modal, re-render an overlay), never
infer that from matching on the *command's name* in the
frontend. Command names change; name-matching is fragile and
invisible.

Instead, have the backend return explicit action tokens as data, and
give the frontend one small, central table that maps each token to its
effect:

```js
// backend response
{ message: "done", client_actions: ["refresh_view"] }

// frontend, one place, for every entry point that can trigger commands
for (const action of response.client_actions ?? []) {
  if (action === "refresh_view") refreshView();
  if (action === "open_modal")   openModal();
}
```

Every entry point that can trigger a command (menu, console, macro
runner, quick-launch bar, etc.) dispatches through this same table, so
behavior is consistent no matter how the command was invoked.

## 6. State-Driven Panel Sizing

Sliding or expanding panels should size themselves from a class or
attribute driven by state, not from inline style manipulation:

```css
#panel.wide { width: 75vw !important; }
```
```js
panel.classList.toggle('wide', activePanelId === 'someWidePanel');
```

**Exception — genuinely dynamic per-instance values:** a value that's
runtime-computed and unique per element (a context menu's click
position, a user-adjustable font size) doesn't fit a class toggle,
since it isn't one of a fixed set of states. For these, set CSS custom
properties inline and reference them from the stylesheet, rather than
writing raw property values into `style=`:

```html


  Delete



```

```css
.context-menu { left: var(--menu-x); top: var(--menu-y); }
```

This keeps the *value* dynamic while keeping every other visual
property (background, border, shadow, radius) in the stylesheet where
it belongs — the inline `style=` attribute only ever carries
custom-property assignments, never literal CSS properties like `color`
or `padding`.

## 7. Inline Confirmation Bar

Never use native dialogs (`window.confirm`, `window.prompt`, OS alert
boxes) for destructive-action confirmation — they're inconsistent
across platforms and outright unreliable inside embedded webviews
(Tauri, Electron). Use an inline confirmation bar rendered within the
component instead:

```html


  Are you sure? This cannot be undone.
  Confirm
  Cancel


```

Always stop click-propagation on the bar itself, so a click inside it
doesn't bubble up to a parent "close on outside click" handler.

## 8. Long-Running Operation Feedback

Any operation that takes more than roughly half a second and has a
clear completion state should get a distinct "running" UI state, not
just a generic spinner buried in a corner:

- a persistent, visually distinct indicator (e.g. a pulsing status bar) for the duration
- a smooth transition to a "success" or "error" state on completion
- enough visual weight that the user doesn't wonder if the app has hung

A `running()` → `success()`/`error()` notification lifecycle, with a
CSS pulse animation and a temporarily expanded/overlaid status region,
works well and is easy to standardize across an app.

## 9. Sync Local State with an External Store

When a component's local state must track a store or global value that
can change from *outside* the component (not just from user
interaction inside it), use an explicit subscription/effect that
watches the store and recomputes local state — don't assume props or
one-time reads stay current. This applies equally whether the
mechanism is a framework's reactivity system or a plain `subscribe()`
callback in vanilla JS.

## 10. Centralize Format/File Parsing Logic

Any format-agnostic operation — reading a file that could be one of
several types, parsing a config, decoding a payload — should have
exactly one function that every call site uses. Never let two places
independently reimplement "read this kind of file," even slightly
differently; the two implementations will drift.

## 11. Self-Documenting Commands

Treat command documentation as part of the command's definition, not
an afterthought. When a new command/action is added, its help text and
(if applicable) its autocomplete hint should be added in the same
change — not left for "later," which in practice means never.

## 12. Explicit Z-Index Hierarchy

Maintain a single documented table of every stacking layer in the app
(menu bar, toolbar, panels, sidebar, modals, expanded/overlay states,
help) with a fixed z-index for each, rather than picking ad-hoc
numbers per component. This prevents the slow creep of `z-index: 9999`
fixes that eventually stop working. Example shape:

| Layer                    | z-index |
| -------------------------- | ------- |
| Chrome (menu/status bar)   | 200     |
| Panels                     | 185     |
| Sidebar                    | 150     |
| Expanded/overlay state     | 300–400 |
| Modal dialogs              | 450     |
| Help / top-most            | 500     |

Also worth fixing as a constant: the vertical offset where fixed
overlays begin, if the app has stacked chrome (menu bar + toolbar)
above the content area.

## 13. Modal Dialogs — Draft-Copy Pattern

For modal dialogs (preferences, parameter editors), edit a local draft
copy of the data — nothing is written to real state until the user
confirms (OK/Apply). Cancel simply discards the draft.

Rules that pair well with this:

- The backdrop does **not** dismiss on click — only Cancel, OK, or an explicit close button do. This protects users from losing work to a stray click, especially for anything more complex than a single toggle.
- Escape triggers cancel.
- The dialog itself stops click-propagation so its own clicks don't bubble to the backdrop.

## 14. Portaled Dropdowns and Menus

Dropdown menus and similar floating UI should render (or "portal") to
the document body rather than nesting inside their triggering
component, so they can escape any `overflow: hidden` or
stacking-context clipping from parent panels.

This has a specific, easy-to-miss consequence: any "close this panel
on outside click" handler on an *ancestor* of the trigger must
explicitly exclude the portaled menu's class from its outside-click
check — otherwise clicking a menu item (which is technically outside
the panel's DOM subtree) closes the parent panel out from under the
click.

```js
if (target.closest('#panel') || target.closest('.portaled-menu')) return; // don't close
```

## 15. Preview-as-Confirmation

Not every consequential action needs pattern 7's confirmation
bar. When the user has already reviewed the exact result in the UI
before triggering the action — a results table they've inspected, a
diff they've read — the preview itself functions as the confirmation,
and an extra dialog is friction rather than protection.

Reserve the inline confirmation bar (7) for actions that are
destructive *without* a visible preview; skip it when the UI already
showed the user precisely what they're about to commit.

## 16. Terminal Operation Sequences

For an action that concludes a workflow (a "commit," "finish," or
"submit" step), define the post-success sequence explicitly and keep
it in one place: sync any locally-pending state to the backend,
perform the write, then reset the UI in a fixed order (close the view,
clear transient state, return to the default screen). Treat it as a
single documented sequence rather than something each call site
reimplements slightly differently.

## 17. Right-Click Context Menus

For per-row or per-item actions, a floating context menu triggered by
right-click (`contextmenu` event, with `preventDefault()` to suppress
the native menu) works well:

```js
function onContextMenu(e, item) {
  e.preventDefault();
  contextMenu = { x: e.clientX, y: e.clientY, item };
}
```

Close it on any outside click via a window-level click listener, and
stop propagation on the menu itself so clicking inside it doesn't
immediately close it. Menu labels should reflect the item's current
state ("Mark as read" vs "Mark as unread"), not a fixed label.

## 18. Two-Row Toolbar

For a component that needs both action buttons and contextual metadata
(a file path, an imported/read-only badge, a status indicator), split
the toolbar into two rows rather than cramming both into one: a
primary row for title and actions, a secondary row — visually
subordinate (smaller text, muted color), separated by a border — for
context. Keep both rows `flex-shrink: 0` so a scrollable body between
them doesn't compress them.

## 19. Semantic Theme Variables

All CSS should reference a fixed, named set of semantic theme
variables (`--bg-color`, `--text-secondary`, `--border-color`,
`--primary-color`, etc.) rather than literal hex values or ad-hoc
variable names invented per file. Pick the variable set once, document
it, and never introduce a parallel naming scheme (`--color-bg`
alongside `--bg-color`) — that fork is how theming quietly breaks.

## 20. Multiple Switchable Themes

Ship at least two themes — a dark theme and a light theme — switchable
by the user at runtime, without a reload, and remembered across
sessions as a stored preference. This is largely free if Pattern 19
(Semantic Theme Variables) is already in place: a theme is nothing
more than a different set of values bound to the same variable
names. Switching theme means changing a single attribute or class on
the root element and letting every component re-resolve its variables;
it should never require touching individual component CSS.

Red/green colorblindness (deuteranopia and protanopia together affect
roughly 1 in 12 men) must be accounted for whenever color choices
carry meaning. Never let red-vs-green be the *only* signal for a
semantic distinction — pass/fail, success/error, reject/accept. Pair
every such color with a redundant channel: an icon, a text label, a
position, a shape. When choosing the actual hues for semantic roles
(danger, success, warning), check them against a colorblindness
simulator before finalizing the palette — colors that look obviously
distinct to a majority of engineers can be nearly indistinguishable to
a meaningful fraction of users.

Apply the chosen theme before first paint (read the stored preference
synchronously at startup) to avoid a flash of the wrong theme on load.

## 21. Scalable, Deformation-Safe Typography

Define font sizes in relative units (`rem`) anchored to a single root
font-size, not fixed pixel values scattered per component. Expose an
accessible, discoverable control — a stepper, slider, or +/- toggle in
settings — that scales a global multiplier within a practical range
(roughly 80%–150% is a reasonable default band). Line-height and
letter-spacing should scale proportionally alongside font-size (also
in relative units), not stay fixed while the font grows — a large font
with an unchanged line-height reads as cramped and undermines the
point of the accessibility control.

The harder half of this pattern is layout, not typography:
fixed-height containers, fixed-pixel line-heights, and text truncation
(`overflow: hidden` + ellipsis) that assumes a specific rendered width
all break at the extremes of the scale range. Test the UI at both the
minimum and maximum supported text size, not just the default — a
layout that looks fine at 100% can visibly overlap or clip at
150%. Prefer flexible layouts (flex/grid that can grow) over
fixed-pixel containers for anything that holds user-facing text, and
reserve truncation for places where the full content remains reachable
on hover, click, or expand — never truncate to the point of
permanently losing information as text scales up.

Defining the type scale itself as a small set of CSS variables (a body
size, a heading size, etc.) computed from one root multiplier keeps
this a single settings change that cascades everywhere, rather than a
per-component override problem.

## 22. Collapsible Panels to Maximize Workspace

Side panels, toolbars, and secondary bars that support the primary
work surface — but aren't themselves the primary work surface — should
be collapsible. Once a user has finished configuring an options
sidebar or inspecting a properties panel, that screen space should be
reclaimable for the actual content (the canvas, the document, the
viewer). Not every UI element needs this: a persistent menu bar or
status bar usually shouldn't collapse. The judgment call for the
engineer is simply to ask, for each auxiliary panel, "does this need
to stay visible right now" — and if the honest answer is often "no,"
make it collapsible.

Implement this with the same state-driven approach as Patterns 1 and
6: a collapsed flag or width value living in view/panel state,
animated via a CSS transition on width or transform rather than an
abrupt `display: none`, so collapsing feels smooth rather than
jarring. Leave a thin persistent strip, edge, or icon as the re-expand
affordance after collapse — the control to bring a panel back should
never disappear along with the panel itself. If the collapsed/expanded
state reflects a workspace layout preference rather than a momentary
per-task toggle, persist it across sessions the same way a theme
choice would be.

## 23. Responsive Design Across Screen Sizes

Not every application needs to run on a phone. But whenever there's a
plausible chance it will — a companion mobile view, tablet field use,
or simply an audience that might resize a window very small — design
the layout to adapt rather than assuming a fixed large desktop
canvas. Multi-column layouts, side-by-side panels, and wide toolbars
are desktop assumptions that stop working well under roughly 600–700px
of width.

Define a small, named set of breakpoints (mobile / tablet / desktop is
usually enough) tied to screen width, and switch actual UI *structure*
at those thresholds — not just font size. Multi-column layouts
collapse to single column; a sidebar becomes a bottom sheet or a
drawer triggered from a menu icon; interactions that assume hover are
replaced with tap-friendly equivalents, since touch has no hover state
at all. Either CSS media queries or a JS-based breakpoint check
(consistent with the state-driven registry pattern used elsewhere in
this doc) work fine as the mechanism — consistency across the app
matters more than which one is chosen.

Treat this as a design-time consideration even for apps that start out
desktop-only. The earlier layout decisions account for reflow, the
cheaper it is to add a mobile mode later. At minimum, define the
breakpoint values as named constants up front — the same way Pattern
12 fixes the z-index hierarchy — so a future engineer adding mobile
support has one place to plug into, rather than reverse-engineering ad
hoc pixel checks scattered through the CSS.
