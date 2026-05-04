### Photyx Project Onboarding — Read This First

#### Who I Am

My name is Stan. I am the sole developer of Photyx. I will refer to myself in the first person throughout our collaboration.

### Purpose of documents

In addition to this onboarding document, I will also upload documents that define the architecture, the patterns we use for UI and internal code, and how you will collaborate with me. Read them carefully and follow what they say. However, don't be afraid to ask me questions or suggest improvements, particularly if I'm diverging from the documents.

#### What Photyx Is

Photyx is a high-performance desktop astrophotography application built with **Tauri v2 + Svelte + Rust**. It is emphatically **not** an Electron app and will never become one. The target stack is Tauri, period.

The authoritative requirements document is `photyx_spec.md` (currently v20). The implementation reference is `development_notes.md` (currently v20). The UI patterns reference is `photyx_ui_patterns.md`. I will upload these at the start of sessions where we are doing development work. **Read all three before writing any code.** Do not deviate from the spec or suggest technologies inconsistent with it. There may be other documents that I'll provide that may help you as well.

### Stack Summary

Tauri v2 + Svelte 5 + TypeScript frontend; Rust backend with plugin registry; SQLite via rusqlite for all persistence. Linux dev (Ubuntu), Windows 11 target. Build: `npm run tauri dev`. CSS in `static/css/. Backend in src-tauri. Frontend in src-svelte.

### Architecture Overview

- **Plugin host:** every operation is a `PhotonPlugin` — file I/O, analysis, keywords, stretch. Registered in `lib.rs`, dispatched via `commands/` Tauri commands or pcode interpreter.
- **AppContext** (`context/mod.rs`): session state — `file_list`, `image_buffers`, `analysis_results`, `is_imported_session`.
- **Display pipeline:** `image_buffers` = raw pixels (never modified); `display_cache`, `full_res_cache`, `blink_cache_12/25` = derived JPEG bytes. Caches rebuilt on demand.
- **Analysis layer:** pure Rust in `analysis/` — no Tauri deps. `session_stats.rs` owns `classify_frame()`, `categorize_rejection()`, `compute_session_stats_iterative()`. SNR is diagnostic only — excluded from rejection.
- **Rejection categories:** O=Optical (FWHM/Ecc), T=Transparency (StarCount), B=Sky Brightness (BgMedian). Ordering: O first, B before T. Stored as `Option<String>` on `AnalysisResult`.
- **Commit Results** is a terminal operation: write PXFLAG → WriteCurrent → move REJECTs to `rejected/<name>.<ext>.rejected` → re-key ctx → `ui.showView(null)` + `ui.clearViewer()` + `closeSession()`.
- **Imported sessions** (`is_imported_session=true`): populated by `load_analysis_json`; skip reclassification in `get_analysis_results`; Commit disabled.
- **View registry:** `ui.showView('analysisGraph' | 'analysisResults' | null)` — never boolean flags.
- **Settings:** `AppSettings` struct, populated from `defaults.rs` + SQLite `preferences` table. `ThresholdProfile` holds 5 thresholds; SNR and StarCount stored as negative values in DB.
- **Frontend constants:** `THRESHOLD_FIELDS` min/max for negative-direction fields use actual signed values (`min: -4.0, max: -0.5`). `SNR_SIGMA_DEFAULT=-2.5`, `STAR_COUNT_SIGMA_DEFAULT=-3.0`.
- **Session JSON:** export via `Session → Export Session JSON…`; import via `Session → Import Session JSON…`. Filenames stored as basenames for cross-platform portability.
- **Menu structure:** File (Load Single Image, Exit) | Session (Select Dir, Close Session, Export JSON, Import JSON) | Edit | View | Analyze (separator before Contour Plot) | Tools | Help.
- **Capabilities:** `fs:allow-read-text-file` and `fs:allow-write-text-file` needed for JSON I/O; scopes include `$HOME`, `$DESKTOP`, `$DOWNLOAD`, `$DOCUMENT`, `$APPDATA/Photyx/**`.

#### Development Environment

- **Platform:** Windows 11 and Ubuntu Linux
- **Frontend:** Svelte + TypeScript in `src-svelte/`
- **Backend:** Rust in `src-tauri/`
- **Build tool:** Vite (hot-reloads `.svelte` and `.ts` files instantly; CSS in `static/` requires manual browser refresh; Rust changes require a full recompile)
- **Version control:** GitHub Desktop, committing at milestones
- **vcpkg** installed on `J:\` for cfitsio

#### How I Want Code Changes Delivered

**Do not start coding until I explicitly say "proceed."** Discussion must be complete first.

Once I say proceed, deliver **one change at a time** using BEFORE/AFTER blocks:

- **BEFORE block** — contains enough surrounding context to uniquely locate the code. I will delete everything in the BEFORE block.
- **AFTER block** — contains the complete replacement. I will copy the entire AFTER block and paste it in.
- **Always state the full file path** before each BEFORE/AFTER pair.
- For large multi-file changes, recommend (or I will ask for) a **complete file replacement** that I can download.
- Never combine multiple file changes into a single BEFORE/AFTER block.
- Always deliver one BEFORE/AFTER block at a time. Don't proceed until I
  explictly tell you to do so.
- After a significant change or module has been done, pause and give me a
  test that I can do to verify that everything is working as expected.

#### When a Complete File Replacement Is Appropriate

- When the changes are extensive enough that incremental BEFORE/AFTER would be confusing or error-prone
- When I ask for it explicitly
- Recommend it proactively if more than ~30% of a file is changing

#### Style and Process Preferences

**Discussion first:**

- Never write code speculatively. If the design isn't settled, keep discussing.
- Ask clarifying questions one at a time — don't pile up multiple questions unless they're tightly related.
- When I give short answers, accept them and move on. Don't re-ask or over-explain.

**Spec adherence:**

- The spec is non-negotiable. If something I ask for conflicts with the spec, flag it before proceeding.
- If I ask for something that isn't in the spec but should be, suggest adding it to the spec first.
- Never suggest technologies not in the stack (no Electron, no alternative frameworks, no unapproved crates without discussion).

**Document maintenance:**

- `photyx_spec.md`, `development_notes.md`, and `photyx_ui_patterns.md` must be kept current.
- At natural milestones (before commits), I will ask for updated versions of these documents. Produce them as complete file replacements, not patch lists.

**Commit messages:**

- When I'm about to commit, I will ask for a suggested summary and description. Please give them to me in separate text boxes that I can directly copy from.
- Summary: one line, imperative tense, concise.
- Description: bullet points grouped by file or feature area, specific about what changed and why.

**Notifications pattern:**

- Use `notifications.running()` (not `notifications.info()`) for long-running operations. It triggers a pulse animation.
- Use `notifications.success()` on completion, `notifications.error()` on failure.

**View management:**

- All viewer-region components are managed via `ui.showView()` and the `VIEWS` registry in `ui.ts`.
- Never use individual boolean flags for viewer-region visibility.
- Close buttons always call `ui.showView(null)`.

**Confirmations:**

- Never use `window.confirm()` or native OS dialogs for destructive action confirmation.
- Use the inline confirmation bar pattern documented in `photyx_ui_patterns.md` Pattern 8.

**Console output:**

- Any action triggered from outside `Console.svelte` (menus, Quick Launch, panels) that produces output must write to the console via `consolePipe`.
- Important status updates also go to `notifications`.

**CSS variables:**

- Always use the theme variables: `--bg-color`, `--text-color`, `--primary-color`, `--border-color`, `--card-bg`, `--card-hover`, etc.
- Never invent variables like `--color-bg` or `--color-text` — these don't exist in the theme files and will break the Light and Dark themes.
- Never use CSS inline. I want all CSS design elements to be in their own separate CSS files, usually with each major module of the project that is user facing with its own CSS file.

**Summary responses:**

- When I give you a numbered list of decisions, respond with a concise acknowledgment and move on. Don't re-explain each point back to me at length.

#### What We Do Not Do

- No temporary localStorage hacks that need to be unwound later (Phase 9 will handle persistence properly via `tauri-plugin-store`)
- No `window.confirm()` or `window.prompt()` for anything destructive — inline UI patterns only
- No hardcoded fake data in panels (Macro Library, Plugin Manager, etc. — everything must come from real data sources)
- No individual boolean flags for viewer-region visibility — use `ui.showView()`

### Reference Table

| Topic                        | Document                          |
| ---------------------------- | --------------------------------- |
| Full requirements            | `photyx_spec.md`                  |
| Implementation details       | `photyx_development.md`           |
| UI patterns & rules          | `photyx_ui_patterns.md`           |
| Commands, keywords, settings | `photyx_reference.md`             |
| DB schema & persistence      | `photyx_persistence_inventory.md` |
| Plugin status table          | `photyx_reference.md`             |
| CSS variables                | `photyx_ui_patterns.md`           |
