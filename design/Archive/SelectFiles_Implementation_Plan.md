# Photyx — SelectFiles Implementation Plan

## Phase A — Backend: Remove Active Directory, Add SelectFiles

### A1. AppContext

- Remove `active_directory` field
- Add `fn source_directories(&self) -> Vec<PathBuf>` — returns unique
  parent directories of all loaded files
- Add `fn common_parent(&self) -> Option<PathBuf>` — returns common
  parent if all files share one, else None

### A2. SelectFiles plugin (new)

- Replaces `select_directory.rs`
- Accepts a list of explicit file paths only — no directory expansion
- Clears current session state and loads the new file list into
  `ctx.file_list`
- Does not set `active_directory`
- Register as `SelectFiles`; retire `SelectDirectory`

### A3. Commit Results

- Update `commit_analysis_results` to create `rejected/` as a child of
  each rejected file's own source directory
- Remove any reference to `active_directory` in the commit path
- After moving rejected files, remove their paths from `ctx.file_list`
- Clear `ctx.analysis_results` (now stale)
- Do NOT close the session — pass frames remain loaded and ready
- Frontend commit sequence: drop `closeSession()`, keep `ui.showView(null)`
  and `ui.clearViewer()`, then trigger a frame refresh to the first remaining frame

### A4. Remaining active_directory references

- Audit all plugins and Tauri commands for `active_directory` reads
- Replace with `ctx.common_parent()` where a single path is needed
- Remove from `get_session` response or mark as Option<String>

---

## Phase B — Frontend: SelectFiles UI & Status Bar

### B1. commands.ts

- Replace `selectDirectory()` with `selectFiles()`
- Opens a multi-file picker (Tauri `open` with `multiple: true`,
  `directory: false`)
- User navigates to a folder and selects one or more files; repeat
  invocations add to the global context
- Calls `SelectFiles` pcode command with selected paths
- Removes `db.recordDirectoryVisit()` and `db.openSession()` directory
  argument (or passes null)

### B2. StatusBar.svelte

- Replace active directory display with file count + directory count
- Format: `157 files · 3 directories` (or `157 files · 1 directory`)
- Show nothing when no files are loaded

### B3. MenuBar.svelte

- Rename Session > Select Directory to Session > Select Files
- Action string: `select-files`

### B4. Session store

- Remove `activeDirectory` field entirely
- Add `directoryCount` derived from file list
- Session remains open after commit — do not reset session store on commit

---

## Phase C — pcode & Reference Document

### C1. pcodeCommands.ts

- Add `SelectFiles` to the command name list
- Retire `SelectDirectory` (remove or mark deprecated)

### C2. photyx_reference.md

- Add `SelectFiles` to command dictionary
- Mark `SelectDirectory` as deprecated alias

### C3. photyx_spec.md

- Update §13 phase table to reflect this work
- Remove active directory references from §8 UI descriptions

---

## Phase D — Cleanup & Testing

### D1. Remove dead code

- `select_directory.rs` plugin (or repurpose as alias)
- `active_directory` from DB schema if stored there
- `get_recent_directories` and `record_directory_visit` Tauri commands
  (already frontend-removed; now remove from preferences.rs)
- `recent_directories` table can be dropped via migration

### D2. Test scenarios

- Single directory selection — verify identical behavior to current
- Two-directory selection — verify both file sets load, AnalyzeFrames
  runs across full population, status bar shows correct counts
- Commit Results with mixed directories — verify each reject lands in
  its own source `rejected/` subfolder; verify rejected paths are
  removed from global context; verify pass frames remain loaded and
  viewer refreshes to first remaining frame
- Post-commit state — verify analysis results are cleared, session
  remains open, status bar reflects updated file and directory counts
- Close Session — verify full reset

---

## Sequencing Notes

- Phases A and B can proceed in parallel once A1 is complete
- Phase C is low-risk and can be done anytime after B3
- Phase D should be the final step before committing
- This work should be its own commit, separate from Phase 10 audit items
