# Tutorial 3: Multi-Session Project Workflow

**What you'll learn:** how to analyze and stack frames from multiple
imaging nights of the same target as a single combined pool, using
pcode.

**Before you begin:** complete Tutorials 1 and 2 first. This tutorial
is pcode-only — everything here is written as a script, with no UI-only
equivalent shown.

---

## 1. The Problem

You've imaged the same target across several nights. Each night is its
own session folder, with its own conditions — one night might be
clearer than another, seeing might vary, and so on. Tutorial 1 walked
through analyzing *one* session. This tutorial combines *all* of them
into a single pool before analyzing and stacking — different frames
compete against each other for PASS/REJECT purposes, not just against
their own night.

Real multi-session projects often mix filters, too — a target imaged
with a duo-narrowband filter on some nights and a broadband filter on
others. Frames shot through different filters shouldn't be combined
into one analysis or stacking pass: they have different threshold
profiles, and `StackFrames` will only combine frames that match its
reference frame's filter — anything else is silently excluded from the
stack rather than an error. This tutorial handles that by processing
each filter as its own pass throughout, matching the way you'd script
a real mixed-filter project.

## 2. The Folder Hierarchy This Tutorial Assumes

This tutorial assumes a project laid out as one parent folder
containing one subfolder per imaging session, each with its own
`data/lights/` folder inside:

```
NGC7380-Wizard-Nebula/
    NGC7380-duo-sess-20240611/
        data/
            lights/
                frame001.fit
                ...
    NGC7380-duo-sess-20240612/
        data/
            lights/
                frame001.fit
                ...
    NGC7380-duo-sess-20241005/
        data/
            lights/
                frame001.fit
                ...
    NGC7380-ircut-sess-20240911/
        data/
            lights/
                frame001.fit
                ...
```

Three nights were shot with a duo-narrowband filter, one with an ircut
(broadband) filter — the session folder names carry the filter and
date so a glob pattern can select all the duo-band sessions separate
from the broadband filter session. If your own folder structure looks
different, adjust the glob patterns in step 4 to match.

## 3. Set Project Variables

Rather than repeating the full project path and target name in every
command, set them once at the top of the script:

```
Set project = "J:/Projects/NGC7380-Wizard-Nebula"
# alternate Linux/MacOS path might be: "/Astro/Projects/NGC7380-Wizard-Nebula"
Set tgt = "NGC7380"
```

Any command from here on can reference `$project` or `$tgt`.

## 4. Load Each Filter Group with Its Own Glob

`AddFiles`/`ReadImages` accept glob patterns, including wildcards in
the middle of a path — not just at the end. This lets one command pull
frames from every session folder matching a given filter, across the
whole project, at once. Because this project mixes filters, load and
process the duo sessions and the ircut sessions as separate passes
rather than combining everything into one glob:

```
ClearSession
ReadImages path="$project/*-duo-*/data/lights"

CountFiles
Assert expression="$filecount > 0"
Print "Loaded " + $filecount + " duo frames across all duo sessions"
```

```
ClearSession
ReadImages path="$project/*-ircut-*/data/lights"

CountFiles
Assert expression="$filecount > 0"
Print "Loaded " + $filecount + " ircut frames across all ircut sessions"
```

`Assert` halts the script with a clear error if the glob matched
nothing — worth having in any script that depends on files actually
being found, rather than silently continuing with zero frames. Note
each pass starts with `ClearSession` — the duo pass and the ircut pass
never share a session pool.

Another way of approaching the problem of zero files is to put a
conditional check after the `CountFiles` command and process that set
of files only if there is more than one. This way you can handle cases
where files don't exist, but you you just skipping processing, rather
halting the macro.

```
ClearSession
ReadImages path="$project/*-ircut-*/data/lights"

CountFiles
if $filecount > 0
  Print "Loaded " + $filecount + " ircut frames across all ircut sessions"
EndIf

```

## 5. Run Analyze Frames on Each Combined Pool

```
AnalyzeFrames profile="Duo-band"
```

```
AnalyzeFrames profile="Broadband"
```

This is the same `AnalyzeFrames` command from Tutorials 1 and 2 — the
difference is entirely in what's loaded and which profile applies.
Because every frame from every session *of that filter* is in the
session pool together, PASS/REJECT is decided relative to that
filter's combined population, not any single night — and not against
frames shot through a different filter, which would skew the
statistics.

> Use (or create) threshold profiles named to reflect each filter —
> `"Duo-band"` and `"Broadband"` here are just examples; name your own
> profiles however makes sense for your workflow.

## 6. Review Results

```
ShowAnalysisResults
```

This opens the same Analysis Results table from Tutorial 1 — sort it,
inspect category badges, and confirm the classifications make sense
before committing. Run this after each filter's `AnalyzeFrames` call,
since the session pool (and therefore the results table) reflects only
whichever filter you most recently analyzed. Nothing has been written
to disk yet.

## 7. Commit Results

```
CommitAnalysis append=.project
```

Alternatively, if you don't use the `append` paremeter, the suffix
will default to `.reject`.

```
CommitAnalysis
```

Just like Tutorial 1, REJECT frames move to a `rejected/` subfolder
within their own original session directory (not a single combined
rejected folder) — each frame's rejection travels back to where it
came from. The `.project` suffix on the moved filename distinguishes
this pass from any other rejection pass you might run later.

Run this once per filter pass, right after that filter's own
`ShowAnalysisResults` review — committing the duo pass doesn't touch
the ircut frames, and vice versa, since each pass loaded its own
separate session pool.

## 8. Stack Each Filter Group

```
ClearSession
ReadImages path="$project/*-duo-*/data/lights"
StackFrames
```

```
ClearSession
ReadImages path="$project/*-ircut-*/data/lights"
StackFrames
```

All surviving frames *within a filter group* — regardless of which
night they came from — are registered and combined into one stack.
Photyx's rotational grouping handles frames from different nights
automatically, so you don't need to do anything special here even if
your equipment was rotated or re-polar-aligned between sessions. Keep
the duo and ircut stacks as two separate `StackFrames` runs — combining
frames from two different filters into a single stack isn't meaningful
data, and `StackFrames` will exclude the mismatched frames from the
stack anyway rather than blend them.

**REMEMBER**: A Photyx-stacked image is not useful for production
quality results. It is only to be used for visual validation.


## 9. Stretch and Commit the Stretch

If you want to preserve the non-linear stack, you can specify the
stretch parameters and save the stretch into the file. Normally we
don't recommend this - simply leave it linear for import back into
Photyx or other programs for viewing.

```
CommitStretch shadow_clip=-3.5 target_bg=0.10
```

Unlike Tutorial 1's default preview stretch, here we're specifying
exact values — useful once you have a stretch you like and want a
repeatable result rather than an auto-computed one each time. Run this
once per filter's stack, right after that filter's `StackFrames` call,
before moving on to the next filter's session load.

## 10. Write the Result

We don't want the stacked image to be inserted into the session so it
is tagged as a `stack`. To write it out to storage, you need to
specify specifically that is the stacked image.

```
WriteXISF destination="$project/stacked" stack=true
Print "Duo stack complete: " + $STACKED
```

```
WriteXISF destination="$project/stacked" stack=true
Print "Ircut stack complete: " + $STACKED
```

Each `WriteXISF stack=true` call exports whichever filter's stack is
currently held in `ctx.stack_result` — run it immediately after that
filter's own `CommitStretch`, before starting the next filter's
session load, or you'll overwrite the stack you meant to export.

## 11. Wrap-Up

This workflow is worth the extra setup specifically when you have
*multiple nights* of the same target and want them evaluated to reject
bad frames and combined together for visual inspection. It's worth the
filter-by-filter structure shown here as soon as a project spans more
than one filter. If you're only ever working with a single session at
a time, Tutorial 1's simpler workflow — run once per session — is all
you need, and is a good default until a project genuinely spans more
than one night.

## 12. Example Multi-Session, Multi-Filter Macro

The steps above walked through each piece individually. Here's a
complete, ready-to-adapt macro that combines analysis (with an
OBJECT-keyword safety check), export, and stacking for both filter
groups in one script — the kind of macro you'd save to the Macro
Library and pin to Quick Launch for a project you'll revisit as new
sessions are added. With a new project, simply edit the macro and
change the `project` and `tgt` variables.

```
# ── Configuration ─────────────────────────────────────────────────────────────
# Set this to the top-level project directory before running.
Set project = "/Astro/Projects/NGC7380-Wizard-Nebula"
Set tgt = "NGC7380"
Set doCommit = "No"

# ── Resolve system paths ───────────────────────────────────────────────────────
GetSystemPath name=downloads

# ── Pass 1: filtering outliers — duo sessions ───────────────────────────

# are there any `duo` imaging sessions?
CountMatches pattern="$project/*-duo-*"

If $matchcount > 0
  ClearSession
  AddFiles paths="$project/*-duo-*/data/lights/*.fit"

  # insert FILTER keyword, if necessary, for follow-on processing
  GetKeyword name=FILTER default="NULL"
  if $FILTER == "NULL"
    print "Adding FILTER keyword to duo frames"
    AddKeyword name=FILTER value="duo" comment="insertion via macro"
    WriteCurrent
  EndIf

  # insert OBJECT keyword if necessary
  GetKeyword name=OBJECT default="NULL"
  if $OBJECT == "NULL"
    print "Adding OBJECT keyword to duo frames"
    AddKeyword name=OBJECT value=$tgt comment="insertion via macro"
    WriteCurrent
  endif

  CountFiles
  if $filecount > 0
    Print "Duo project pool: " + $filecount + " frames"
    AnalyzeFrames profile="Duo-band"
    ExportAnalysisReport path="$downloads/Project-Duo-Analysis.json"
    if $doCommit == "Yes"
      CommitAnalysis append=.project
    EndIf
    Print "Duo project analysis complete."
  EndIf
EndIf

# ── Stack duo sessions ────────────────────────────────────────────────────────

CountMatches pattern="$project/*-duo-*"
If $matchcount > 0
  CountFiles
  if $filecount > 0
    Print "Stacking duo frames: " + $filecount + " frames"
    StackFrames
    WriteXISF destination="$downloads" stack=true
    Print "Duo stack saved: " + $STACKED
    ClearStack
  Endif
EndIf

# ── Pass 2: filtering outliers — ircut sessions ─────────────────────────

# are there any 'ircut' imaging sessions?
CountMatches pattern="$project/*-ircut-*"

If $matchcount > 0
  ClearSession
  AddFiles paths="$project/*-ircut-*/data/lights/*.fit"

  # insert FILTER keyword, if necessary, for follow-on processing
  GetKeyword name=FILTER default="NULL"
  if $FILTER == "NULL"
    print "Adding FILTER keyword to ircut frames"
    AddKeyword name=FILTER value="ircut" comment="insertion via macro"
    WriteCurrent
  EndIf


  # insert OBJECT keyword if necessary
  GetKeyword name=OBJECT default="NULL"
  if $OBJECT == "NULL"
    print "Adding OBJECT keyword to ircut frames"
    AddKeyword name=OBJECT value=$tgt comment="insertion via macro"
    WriteCurrent
  endif

  CountFiles
  if $filecount > 0
    Print "Ircut project pool: " + $filecount + " frames"
    AnalyzeFrames profile="Broadband"
    ExportAnalysisReport path="$downloads/Project-Ircut-Analysis.json"
    if $doCommit == "Yes"
      CommitAnalysis append=.project
    EndIf
    Print "Ircut project analysis complete."
  EndIf
EndIf

# ── Stack ircut sessions ──────────────────────────────────────────────────────

CountMatches pattern="$project/*-ircut-*"
If $matchcount > 0
  CountFiles
  if $filecount > 0
    Print "Stacking ircut frames: " + $filecount + " frames"
    StackFrames
    WriteXISF destination="$downloads" stack=true
    Print "Ircut stack saved: " + $STACKED
    ClearStack
  EndIf
EndIf

ClearSession
Print "Done with rejection and stacking"
```

A few things worth noting about this macro compared to the
step-by-step version above:

- **`CountMatches` guards each pass.** If a project only has one
  filter (or a filter's sessions haven't been shot yet), the
  corresponding `If $matchcount > 0` block is skipped entirely rather
  than failing on an empty glob — useful for a macro you'll re-run as
  a project grows.
- **The OBJECT-keyword check is a one-time safety net.** `GetKeyword
  name=OBJECT default="NULL"` won't halt the script if the keyword is
  missing (unlike omitting `default=`); the `If $OBJECT == "NULL"`
  block then backfills it from `$tgt` and persists it with
  `WriteCurrent` — useful if some capture software didn't set OBJECT
  automatically. Once the keyword exists, this block is a no-op on
  future runs.
- **`CommitAnalysis` is guarded by a global variable.** As written,
  this macro exports an analysis report for review but stops short of
  actually moving REJECT frames unless the `$doCommit` variable is set
  to "Yes". This is a deliberate "look before you commit" default
  worth keeping until you've reviewed `Project-Duo-Analysis.json` /
  `Project-Ircut-Analysis.json` from a real run. Set `doCommit =
  "Yes"` once you're ready to commit automatically.
- **Stacking a large number of frames can result in a small cropped
  image.** The Photyx stacking algorithm tried to autocrop stacking
  artifacts from the edges, but is not a sophisticated
  algorithm. Stacking a large number of frames that might vary
  significantly in position might result in a stacked image that is
  small and not very useful.
