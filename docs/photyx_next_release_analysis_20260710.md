# Photyx — Post-1.0 Code Analysis & Next-Release Plan

**Scope:** Full review of `src-tauri` (~17K lines Rust), `src-svelte` (~9K lines
Svelte 5 + TypeScript), and the shipped documentation suite, performed against
the 1.0-candidate source tree. Covers bugs, inefficiencies, algorithmic
weaknesses, architectural problems, and UX/code-standards violations.

**Exclusions (per agreement):** Already-tracked items are not re-covered here —
issues #69 (unused background metrics), #70 (Moffat enhancements),
#71 (centralize thresholds), the separate-RGB-channel-views bug, and
`TRI_MAX_STARS` sparse-field validation. Items already listed in Technical
Reference §14 are only raised where the code contradicts the documented
understanding.

**Companion deliverable:** `photyx_issues.zip` — 21 issue files in the standard
format, one per finding group (related causes bundled, divergent concerns
split).

---

## 1. Executive Summary

The codebase is in good overall shape for a solo project of this scope: the
plugin architecture is consistently applied, the memory-chunking discipline is
real and correctly propagated across `AnalyzeFrames`, `CacheFrames`,
`start_background_cache`, and `StackFrames` Pass 2, and the analysis pipeline's
statistical machinery (bimodal anchoring, two-pass clipping) matches its design
intent and is covered by meaningful unit tests.

That said, the review surfaced **five findings I would classify as
high-severity**, several of which sit directly on documented core workflows:

1. **The documented canonical pcode session loop cannot execute.**
   `For i = 0 To $filecount - 1` fails, because `For` bounds are
   variable-substituted but never expression-evaluated
   (`pcode/mod.rs` parses the bound with a bare `parse::<i64>()`). Every
   example in the pcode Guide that iterates session frames uses this exact
   form. Related interpreter defects: a broken `If` condition silently
   evaluates to `false` instead of halting, and a failed macro invoked via
   `RunMacro` reports success to the calling script. (Issue 01)

2. **`profile=` analysis runs are silently re-classified under the active
   profile.** `get_analysis_results` reclassifies every frame using
   `ctx.analysis_thresholds` (the active profile) and reports those as
   `applied_thresholds`, ignoring `ctx.last_analysis_thresholds` entirely.
   Run `AnalyzeFrames profile="Project"`, open Analysis Results, commit — and
   you have committed the *active* profile's classification, not Project's.
   This directly undermines the documented Session/Project two-pass workflow
   whenever the UI is involved between analyze and commit. (Issue 02)

3. **RGB FITS round-trip corruption.** The FITS reader re-interleaves planar
   color data only in the F32 branch; U8 and U16 RGB FITS load planar and are
   then treated as interleaved everywhere downstream. The FITS *writer*
   correctly deinterleaves all bit depths — so a U16 RGB FITS written by
   Photyx and re-read by Photyx comes back scrambled. Additionally, 32-bit
   integer FITS clamps to 65535 (solid white for real data) instead of the
   `>>16` downconversion the XISF/TIFF paths use. (Issue 03)

4. **`WriteCurrent` degrades keyword types.** `update_fits_keywords` deletes
   all non-structural keywords and rewrites every one as a *quoted string* —
   `EXPTIME`, `GAIN`, `FOCALLEN`, etc. become string-typed after one keyword
   persist. Tools that read these numerically downstream (WBPP/SFS read
   `EXPTIME`) may fail or misparse. This is the standard "persist keyword
   changes" path, so it touches your primary data. A one-byte-short unsafe
   buffer in the same function's `ffgkyn` loop is fixed in the same issue.
   (Issue 04)

5. **`MoveFile`/`CopyFile` silently destroy existing destination files.**
   No overwrite guard exists (unlike every Write plugin), rename semantics
   diverge between Linux and Windows, and the cross-device fallback is
   non-atomic. `MoveFile` also shifts `current_frame` off its file when the
   moved file precedes it in the list — the same index-adjustment bug class
   `remove_single_frame` was explicitly hardened against. (Issue 05)

Beyond these, three architectural findings shape the performance phase:

- **The display cache is dead code, and its build output is thrown away.**
  `ctx.display_cache` is never written by anything. `start_background_cache`
  computes stretched display-resolution JPEGs for the whole session — the
  most expensive pass it runs — uses them only as thumbnail sources, and
  discards them. Meanwhile every frame navigation re-renders from raw pixels
  and performs a five-round-trip IPC waterfall (`SetFrame` →
  `debug_buffer_info` → `get_session` → `get_keywords` → `get_current_frame`),
  all serialized on the context mutex. This is Photyx's equivalent of the
  Astryx viewfinder issue: the hot path has a cache architecture on paper and
  none in practice. (Issue 08)

- **`get_full_frame` renders a full-resolution JPEG while holding the context
  lock**, and `full_res_cache` grows without bound and — like all four JPEG
  caches — is invisible to the buffer-pool memory accounting
  (`total_memory_used()` counts raw buffers only). Browsing a 128-frame
  session at 100% zoom can quietly accumulate hundreds of MB the limit never
  sees. Given the effort just invested in memory behavior, this blind spot is
  worth closing. (Issue 09)

- **The command surface has ghosts.** `SetZoom`, `BlinkSequence`, and
  `MedianValue` are documented in the Guide and Technical Reference, appear in
  the frontend autocomplete and help database — and do not exist in the
  backend or the client-command dispatch. They return `UNKNOWN_COMMAND`.
  `BinImage` exists only in the docs. (Issue 14)

The remaining findings are consistency, robustness, and hygiene: concurrency
races in the job-result singleton, poisoned-mutex fragility, `ReadImages`
bypassing the memory limit, the Analysis Graph's incomplete commit flow, dead
DB schema (`frame_analysis_results` is created and indexed but never read or
written), shipped dev scaffolding (`FakeProgress`, `get_stack_frame`,
`console.trace` in the frame-navigation hot path), fourteen inline `style=`
attributes, raw `consolePipe.update` calls that violate the `pipeToConsole`
rule, magic-number drift against `defaults.rs`'s single-source-of-truth
mandate, and a documentation layer that has fallen behind the implementation
in specific, enumerable ways (§6.2 still describes Moffat-based metrics; the
metrics are moment-based).

## 2. What Is in Good Shape

Worth stating explicitly, because it constrains how aggressive the plan needs
to be. The pixel-chunking memory pattern is correctly and uniformly applied.
The two-pass sigma clipping and bimodal Star Count anchoring implement the
documented algorithm faithfully, including the deliberate anchor-freeze
between passes, and carry real unit tests. The migration framework
(`PRAGMA user_version`, append-only closures) is sound. `WriteFrame` and
`WriteCurrent`'s XISF/TIFF paths honor the atomic temp-rename contract. The
client_actions dispatch pattern is consistently used by the plugins that need
it. Frontend view management goes through `ui.showView()` and the `VIEWS`
registry without exception, and no `window.confirm()` usage exists anywhere.
The classification, categorization, and commit logic all agree with the spec.
Nothing in this review suggests the 1.0 analysis results you have validated
against SFS are wrong — the metric computations themselves are solid; the
defects cluster in the *plumbing around* them.

**Issue 1 is Photyx 77.**

## 3. Findings by Area

### 3.1 pcode Interpreter (Issues 01, 14, 20)

The interpreter's expression evaluator (`expr.rs`) is competent — proper
recursive descent, sensible precedence, good function library. The defects are
at the seams where the evaluator *isn't* invoked:

`For` bounds (`pcode/mod.rs` ~line 329/345) run `substitute_vars` then
`parse::<i64>()`. `"$filecount - 1"` becomes `"128 - 1"`, which is not an
integer. The Guide states "Both bounds can be variables or expressions" and
every session-iteration example depends on it. The fix is one call site:
evaluate bounds through `expr::evaluate_expr` before parsing.

`If` condition evaluation errors are swallowed (`evaluate_condition` wrapper
logs a warning and returns `false`), so a typo'd condition silently takes the
`Else` branch in a batch job — the opposite of the documented halt-on-error
contract. A `For` bound parse failure `return`s out of the enclosing block
even when `halt_on_error` is false, skipping sibling statements
inconsistently.

`RunMacro` returns `Ok(PluginOutput::Message("...halted..."))` when the inner
macro fails, so the *outer* script records success and continues past a failed
sub-macro. For composed macro libraries pinned to Quick Launch, this means a
failed load step doesn't stop the analysis step that follows it.

Two commands have divergent duplicate implementations: the registered `Assert`
plugin evaluates against an **empty** variables map (so
`dispatch_command`-path asserts always resolve `$vars` to empty strings) while
the interpreter's internal Assert uses real variables; `Print` is similarly
duplicated. The plugin copies should delegate to `ctx.variables` or be
removed from the interactive path.

Separately (Issue 20), §13's path conventions are only partially implemented:
`~` home expansion exists nowhere; `Log` resolves relative paths against
`None` rather than `common_parent()`; `AddFiles` doesn't resolve relative
paths at all.

### 3.2 Analysis Workflow (Issues 02, 17, 19)

Issue 02 is the significant one and is described above. The design question it
raises: reclassification-on-refresh (§6.8) and thresholds-pinned-to-run
(§8.5) are fundamentally in tension, and the code resolved the tension in the
wrong direction silently. My recommendation in the issue is to make results
own their thresholds: reclassify with `last_analysis_thresholds` when present,
and make an *explicit* active-profile change (via the dialog's OK/Apply) the
only thing that re-baselines them.

Issue 17: `AnalysisGraph.commitResults()` is a four-line function — no
imported-session guard (the Results view has one), no toggled-flag sync, no
post-commit session sync, and the graph stays open rendering stale data over a
mutated session. Both views also hardcode the `'.reject'` suffix as string
literals while `REJECT_FILE_SUFFIX` exists in `constants.rs` and the docs say
`.rejected` — three conventions for one suffix.

Issue 19: reference-frame selection in `AnalyzeFrames` minimizes
`fwhm × eccentricity`, which degenerates as eccentricity → 0 — a bloated
5-px round star (product ≈ 0.05) beats a sharp 2-px star at ecc 0.3
(product 0.6). `StackFrames` uses `1/FWHM + (1 − ecc)` for the same concept.
One formula should win (the stacking one is better behaved), and the `"REF"`
flag string currently *replaces* the frame's PASS/REJECT in the results
payload, hiding its actual classification in the UI.

### 3.3 File I/O and Data Integrity (Issues 03, 04, 05, 06)

Covered in the summary. One addition on Issue 03: the F32 path assumes
normalized 0–1 data throughout (display clamps, `normalize_value` is a no-op
for F32). Float FITS/TIFF from other software with 0–65535 ranges will render
solid white and produce nonsense metrics. A range probe on load (or a
documented rejection) closes a real-world compatibility gap. The FITS header
parser also mis-splits string values containing `" /"` and doesn't handle the
CONTINUE long-string convention — folded into the same issue as robustness
notes.

On Issue 06: `WriteFIT`/`WriteXISF`/`WriteTIFF` batch exporters
delete-then-write with no temp-rename, while `WriteFrame`/`WriteCurrent` do it
correctly — and §9.3 claims all writes are atomic. With `overwrite=true`, a
crash mid-write destroys the pre-existing file.

### 3.4 Concurrency & Robustness (Issues 07, 15, 16)

`run_script` spawns a raw thread per invocation with no in-flight guard. Two
overlapping scripts (console + Quick Launch) interleave writes to the global
progress atomics, and because `get_job_result` is a single `take()` slot
polled at 500 ms, two jobs finishing within one poll window lose the first
result entirely — the initiating component never hears back. `DISPLAY_COMMANDS`
also lists `linearstretch`/`histogramequalization`, which don't exist.

Every mutex acquisition in the codebase is
`.expect("... lock poisoned")` — so a single panic inside any plugin
permanently bricks the app until restart. And panics are reachable:
`stack_frames.rs` has bare `unwrap()`s on `image_buffers.get()` in Pass 2 that
fire if the file list changed between passes. Recommendation: catch panics at
the dispatch boundary (`catch_unwind` around `plugin.execute`) and convert to
`PluginError`, which also protects the mutex.

`ReadImages` (Issue 15) is the second load path and enforces none of what
`AddFiles` does: no buffer-pool limit check, no progress reporting, no
basename re-sort. A `ReadImages` on a 300-frame directory sails straight past
the memory limit. `CacheFrames` (Issue 16) reports no progress at all while
holding the context mutex for the entire multi-second build, and its
completion message claims "`{total}/{total}` at both resolutions" regardless
of what was requested or what failed.

### 3.5 Display Pipeline Performance (Issues 08, 09, 10)

Described in the summary. Issue 10 collects the refactor work that Issue 08's
architecture decision unlocks: the ~150-line box-filter downsample loop exists
in four near-identical copies (`get_current_frame`, `load_file`,
`start_background_cache`, `cache_frames`), `get_current_frame`'s U16-RGB
branch computes a full mono box-filter pass and discards it, and the
sigma-clip estimator clones and re-sorts its working vector every iteration
(the retain step doesn't care about order — one in-place sort suffices). The
subsample constants also disagree (`step_by(4)` in stars.rs,
`SUBSAMPLE_STEP: 8` in background.rs, both under comments claiming "every 4th
pixel … ~1/16").

### 3.6 Dead Code, Schema, and Scaffolding (Issues 12, 13)

Shipped dev scaffolding: the `FakeProgress` plugin is registered in production
(and present in the frontend autocomplete — invoking it blocks the context
mutex for 6.4 s by default); `get_stack_frame` is commented "Temporary
diagnostic command" and registered; `commands.ts` fires `console.trace()` on
every frame navigation and `console.log` on every Add Files; `MetricWeights`
in `session_stats.rs` is a dead scoring system still carrying
`signal_weight`/`background_stddev`/`background_gradient` fields.

Dead schema: `frame_analysis_results` is created and indexed but has zero
reads or writes anywhere — the entire per-frame DB persistence layer §8.2
describes does not exist at runtime. `algorithm_sets` is seeded and never
queried. `console_history` is never inserted into (the console-history-size
preference bounds only the in-memory store). `session_history` rows are never
created (`open_session` has no callers) yet `close_session` is called on
exit. Fresh installs still create `signal_weight_reject_sigma`. Issue 13 is a
decision issue: implement the persistence layer or drop the tables in a v5
migration — either is fine, but the schema and the docs must match reality.

### 3.7 Frontend Standards & UX (Issue 18)

Fourteen inline `style="` attributes across seven components violate the
no-inline-CSS rule (two are legitimately dynamic — the Quick Launch context
menu position and the Macro Editor font custom property — the rest are static
styling that belongs in the module CSS files). `StackResult.svelte` and
`StackingWorkspace.svelte` call `consolePipe.update` directly with objects
missing the required `id` field — precisely the misuse `pipeToConsole()`
exists to prevent — and it ships because Vite performs no type-checking;
adding a `svelte-check` gate to the build would have caught it. The `Version`
console command hardcodes "Photyx 1.0.0-dev".

### 3.8 Documentation Reconciliation (Issue 21)

Enumerated doc-vs-code deltas, the largest being: §6.2 still attributes FWHM,
eccentricity, and star count to Moffat PSF fitting with Moffat acceptance
gating — the implementation is moment-based (`fwhm.rs` comments even describe
replacing the prior approach), star count is the count of stars with valid
moment-FWHM in 0.5–50 px, and `moffat.rs` is entirely `dead_code`. The SFS
cross-validation conclusions are unaffected (they were empirical against
actual outputs), but the documented rationale is wrong. Also: §2.2's constant
names/values don't match `defaults.rs` (`DISPLAY_JPEG_QUALITY = 92` vs actual
`DETAIL_JPEG_QUALITY = 90`; `BLINK_JPEG_QUALITY = 85` vs
`THUMBNAIL_JPEG_QUALITY = 75`); §14 still lists the `AnalysisThresholds` 3.0
bug (fixed) and claims `AnalyzeFrames` lacks progress reporting (it has it —
`CacheFrames` is the one that doesn't); §6.7 says the UI commit suffix is
`.rejected` (code: `.reject`); the command dictionary and plugin table list
four commands that don't exist; `quick_launch_visible` is a persisted
preference absent from §8.4; the onboarding reference table still points at
retired document names.

## 4. Phased Implementation Plan

Ordering: correctness and data-integrity first, then workflow correctness,
then robustness, then performance (which depends on architecture decisions),
then cleanup and documentation last so the docs describe the post-fix state
once. Within phases, issues are independent unless noted. Every phase follows
the standing methodology: discussion before code, fresh source uploads before
any change, one BEFORE/AFTER at a time, a verification test after each
significant change, and both-platform testing where I/O or paths are touched.

| Phase | Theme                              | Issues             | Est. sizes    |
| ----- | ---------------------------------- | ------------------ | ------------- |
| 1     | Data integrity & pcode correctness | 01, 03, 04, 05, 06 | M, M, M, M, S |
| 2     | Analysis workflow correctness      | 02, 19, 17, 14     | M, S, S, M    |
| 3     | Robustness & consistency           | 07, 15, 16, 20     | M, S, S, S    |
| 4     | Performance & display architecture | 08, 09, 10         | L, M, M       |
| 5     | Cleanup, schema, standards, docs   | 11, 12, 13, 18, 21 | S, S, M, S, M |

**Phase 1 — Data integrity & pcode correctness.** These are the findings that
can corrupt files or silently mis-execute scripts. Issue 01 first (smallest
blast radius, immediately restores the documented scripting contract), then
03 and 04 together (same file cluster, both need round-trip verification on
real FITS data), then 05 and 06. Exit test: a scripted end-to-end run — load,
loop all frames with `For i = 0 To $filecount - 1`, keyword edit,
`WriteCurrent`, re-read in both Photyx and PixInsight, verify keyword types
and RGB integrity survive.

**Phase 2 — Analysis workflow correctness.** Issue 02 requires a design
discussion before code (thresholds-own-results vs. reclassify-on-active — the
issue lays out the options); 19 and 17 are mechanical once 02's decision
lands. Issue 14 is a scoping decision: implement `SetZoom`/`BlinkSequence` as
client commands and `MedianValue` as a trivial plugin, or purge all four from
docs and frontend — the issue recommends implementing the first three since
the Guide's blink examples depend on them. Exit test: the documented
Session/Project two-pass workflow, driven both by pure pcode and by the
UI-review path, produces identical rejected/ contents.
p
**Phase 3 — Robustness & consistency.** Issue 07's panic-boundary work
meaningfully de-risks everything after it (a bug during Phase 4 perf work no
longer bricks the app). 15/16/20 are small alignment fixes. Exit test:
deliberately overlapping console + Quick Launch scripts both report results;
a forced plugin panic surfaces as a console error and the app keeps working.

**Phase 4 — Performance & display architecture.** Issue 08 is the big one and
needs a design discussion first (raw-vs-stretched cache semantics decide what
`display_cache` stores). Issue 10 is explicitly sequenced after 08 so the
consolidation targets the surviving code paths. Issue 09 can proceed in
parallel. Exit test: frame stepping through the 128-frame M82 benchmark
session before/after (IPC count, wall time per step, RSS after a full
100%-zoom browse).

**Phase 5 — Cleanup, schema, standards, docs.** Issue 13's schema decision
should happen early in the phase since Issue 21's doc updates depend on it.
Issue 21 last, capturing the post-fix state of everything above in one doc
pass.

## 5. Issue Index

| #   | Title                                                                               | Size | Labels                        |
| --- | ----------------------------------------------------------------------------------- | ---- | ----------------------------- |
| 01  | pcode interpreter: For-loop bounds and error-propagation defects                    | M    | bug, pcode                    |
| 02  | Analysis results silently reclassified under the active profile                     | M    | bug, analysis, workflow       |
| 03  | Image reader correctness: FITS RGB interleave, 32-bit scaling, F32 range            | M    | bug, io, data-integrity       |
| 04  | FITS keyword type fidelity lost on WriteCurrent; unsafe ffgkyn buffers              | M    | bug, io, data-integrity       |
| 05  | MoveFile/CopyFile: silent overwrite, non-atomic fallback, frame-index shift         | M    | bug, data-integrity, pcode    |
| 06  | Batch writers (WriteFIT/WriteXISF/WriteTIFF) are not atomic                         | S    | bug, io                       |
| 07  | Job dispatch: concurrency races, lost JobResults, poisoned-mutex fragility          | M    | bug, architecture             |
| 08  | Display cache is dead; frame navigation re-renders raw via 5-IPC waterfall          | L    | performance, architecture, ux |
| 09  | get_full_frame renders under lock; JPEG caches unbounded and unaccounted            | M    | performance, memory           |
| 10  | Consolidate duplicated downsample/render code; estimator inefficiencies             | M    | refactor, performance         |
| 11  | Constants drift: duplicated magic numbers vs defaults.rs mandate                    | S    | code-quality                  |
| 12  | Remove shipped dev scaffolding (FakeProgress, get_stack_frame, console.trace)       | S    | code-quality                  |
| 13  | Dead persistence schema: implement or drop frame_analysis_results et al.            | M    | architecture, database        |
| 14  | Documented commands that don't exist: SetZoom, BlinkSequence, MedianValue, BinImage | M    | bug, pcode, docs              |
| 15  | ReadImages bypasses memory limit; no progress; no session re-sort                   | S    | bug, consistency              |
| 16  | CacheFrames: no progress reporting; inaccurate completion message                   | S    | bug, ux                       |
| 17  | AnalysisGraph commit flow diverges from AnalysisResults                             | S    | bug, ux                       |
| 18  | Frontend standards: inline styles, consolePipe misuse, no type-check gate           | S    | code-quality, ux              |
| 19  | Reference-frame formula degenerate; REF flag masks PASS/REJECT                      | S    | bug, analysis                 |
| 20  | Path convention gaps: ~ expansion, Log/AddFiles relative resolution                 | S    | bug, consistency              |
| 21  | Documentation reconciliation: Moffat claim, constants, §14 staleness, ghosts        | M    | docs                          |

---

*Prepared by Claude from full-source review, July 2026. All file/line
references are against the uploaded tree and should be re-verified against
fresh uploads before any change, per standing methodology.*
