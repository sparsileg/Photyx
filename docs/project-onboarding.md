### Photyx Project Onboarding — Read This First

#### Who I Am

My name is Stan. I am the sole developer of Photyx. I will refer to
myself in the first person throughout our collaboration.

### What Photyx Is

Photyx is a high-performance desktop astrophotography application
built with **Tauri v2 + Svelte + Rust**.

The authoritative requirements and implementation reference is the
Photyx Technical Reference. The UI patterns reference is
`ux_best_practices.md`. Do not deviate from the spec or suggest
technologies inconsistent with it. There may be other documents that
I'll provide that may help you as well.

### Project Status

Photyx is in **release mode**. Active feature development has
concluded. Expect minor UI adjustments and bug fixes only — do not
propose or scope new features unless I explicitly raise one.

### Stack Summary

Tauri v2 + Svelte 5 + TypeScript frontend; Rust backend with plugin
registry; SQLite via rusqlite for all persistence. Linux dev (Ubuntu),
Windows 11 target. Build: `npm run tauri dev`. CSS in
`static/css/`. Backend in `src-tauri`. Frontend in `src-svelte`.

### Architecture Overview

Photyx is a desktop astrophotography frame analysis tool built on
Tauri v2 (Rust backend, Svelte 5 frontend). The frontend communicates
with the backend exclusively via Tauri `invoke()` calls. Photyx has a
robust macro language capability called ```pcode```. All backend
operations are implemented as `PhotonPlugin` trait objects registered
in a plugin registry and dispatched either interactively via the pcode
console or programmatically via the script runner.

Session state — file lists, raw pixel buffers, derived caches, and
analysis results — lives in a single `AppContext` struct protected by
a Mutex. Raw pixel buffers are loaded once and never modified; all
display representations (display cache, full-res cache, blink caches)
are derived JPEG copies. Frame quality analysis runs in parallel via
Rayon, computing five metrics per frame, then classifies each frame as
PASS or REJECT using iterative sigma clipping against session
statistics. Results are written back to source files as PXFLAG
keywords. All persistence is via SQLite through `rusqlite`.

The frontend is organized around a viewer region managed by a view
registry (`ui.showView()`), a pcode console, sliding side panels, and
a Quick Launch bar. Supported formats are FITS (via cfitsio), XISF (via the
custom `photyx-xisf` crate), and TIFF.

Because `AppContext` is behind a single Mutex, any long-running plugin
holding `&mut AppContext` blocks all other Tauri commands — including
frame display — for its duration. This constraint has shaped several
design decisions and is worth keeping in mind before proposing
anything long-running that touches shared state.

#### Development Environment

- **Platform:** Windows 11 and Ubuntu Linux
- **Frontend:** Svelte + TypeScript in `src-svelte/`
- **Backend:** Rust in `src-tauri/`
- **Build tool:** Vite (hot-reloads `.svelte` and `.ts` files
  instantly; CSS in `static/` requires manual browser refresh; Rust
  changes require a full recompile)
- **Version control:** GitKraken, committing at milestones

#### How I Want Code Changes Delivered

**Do not start coding until I explicitly say "proceed."** Discussion
must be complete first.

Once I say proceed, deliver **one change at a time** using
BEFORE/AFTER blocks:

- **BEFORE block** — contains enough surrounding context to uniquely
  locate the code. I will delete everything in the BEFORE block.
- **AFTER block** — contains the complete replacement. I will copy the
  entire AFTER block and paste it in.
- **Always state the full file path** before each BEFORE/AFTER pair.
- For large multi-file changes, recommend (or I will ask for) a
  **complete file replacement** that I can download.
- Never combine multiple file changes into a single BEFORE/AFTER
  block.
- Always deliver one BEFORE/AFTER block at a time. The AFTER block
  always has a Copy capability. Don't proceed until I explicitly tell
  you to do so.
- After a significant change or module has been done, pause and give
  me a test that I can do to verify that everything is working as
  expected.

#### When a Complete File Replacement Is Appropriate

- When the changes are extensive enough that incremental BEFORE/AFTER
  would be confusing or error-prone
- When I ask for it explicitly
- Recommend it proactively if more than ~30% of a file is changing
- Because documents are so large, I want document updates to use
  BEFORE/AFTER blocks. If you feel a document full replacement is
  necessary, tell me why before doing it.

#### Style and Process Preferences

**Discussion first:**

- Never write code speculatively. If the design isn't settled, keep
  discussing.
- Ask clarifying questions one at a time — don't pile up multiple
  questions unless they're tightly related.
- When I give short answers, accept them and move on. Don't re-ask or
  over-explain.

**No-guessing rule:**

- Never write code referencing types, function signatures, or field
  names without having directly viewed the relevant source file
  first. I will call out violations immediately.

**Spec adherence:**

- The spec is non-negotiable. If something I ask for conflicts with
  the spec, flag it before proceeding.
- If I ask for something that isn't in the spec but should be, suggest
  adding it to the spec first.

**Document maintenance:**

- The Photyx Technical Reference and `ux_best_practices.md` must be
  kept current.
- At natural milestones (before commits), I will ask for updated
  versions of these documents.

**Commit messages:**

- When I'm about to commit, I will ask for a suggested summary and
  description. Please give them to me in separate text boxes that I
  can directly copy from.
- Summary: one line, imperative tense, concise.
- Description: bullet points grouped by file or feature area, specific
  about what changed and why.

**Notifications pattern:**

- Use `notifications.running()` (not `notifications.info()`) for
  long-running operations. It triggers a pulse animation.
- Use `notifications.success()` on completion, `notifications.error()`
  on failure.

**View management:**

- All viewer-region components are managed via `ui.showView()` and the
  `VIEWS` registry in `ui.ts`.
- Never use individual boolean flags for viewer-region visibility.
- Close buttons always call `ui.showView(null)`.

**Confirmations:**

- Never use `window.confirm()` or native OS dialogs for destructive
  action confirmation.
- Use the inline confirmation bar pattern documented in
  `ux_best_practices.md` Pattern 7.

**Console output:**

- Any action triggered from outside `Console.svelte` (menus, Quick
  Launch, panels) that produces output must write to the console via
  `consolePipe`.
- Important status updates also go to `notifications`.

**CSS variables:**

- Always use the theme variables: `--bg-color`, `--text-color`,
  `--primary-color`, `--border-color`, `--card-bg`, `--card-hover`,
  etc.
- Never invent or use variables that don't exist in the theme files.
- Never use CSS inline. I want all CSS design elements to be in their
  own separate CSS files, usually with each major module of the
  project that is user facing with its own CSS file.

**Summary responses:**

- When I give you a numbered list of decisions, respond with a concise
  acknowledgment and move on. Don't re-explain each point back to me
  at length.

#### What We Do Not Do

- Generally, do not do temporary hacks to get around a problem with
  the hope that a better solution will occur later.
- No `window.confirm()` or `window.prompt()` for anything destructive
  — inline UI patterns only.
- No hardcoded fake data in panels (Macro Library, Plugin Manager,
  etc. — everything must come from real data sources).
- No individual boolean flags for viewer-region visibility — use
  `ui.showView()`.

### Reference Table

| Topic | Document |
| ------------------------------ | ---------------------------------- |
| Full requirements | Photyx Technical Reference |
| Implementation details | Photyx Technical Reference |
| UI patterns & rules | `ux_best_practices.md` |
| CSS variables | `ux_best_practices.md` |
| Commands, keywords, settings | `Photyx_pcode_guide.md` |
| Plugin status table | Photyx Technical Reference §11 |
| Stacking implementation | Photyx Technical Reference §7 |
| DB schema & persistence | Photyx Technical Reference §8 |
