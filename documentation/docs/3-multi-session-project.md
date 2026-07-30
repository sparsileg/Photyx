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

## 2. The Folder Hierarchy This Tutorial Assumes

This tutorial assumes a project laid out as one parent folder
containing one subfolder per imaging session, each with its own
`lights/` folder inside:

```
M82/
├── M82-ircut-sess-01/
│   └── lights/
│       ├── frame001.fit
│       └── ...
├── M82-ircut-sess-02/
│   └── lights/
│       └── ...
└── M82-ircut-sess-03/
    └── lights/
        └── ...
```

If your own folder structure looks different, adjust the glob pattern
in step 4 to match.

## 3. Set a Project Variable

Rather than repeating the full project path in every command, set it
once at the top of the script:

```
Set project = "J:/projects/M82"
# Linux/Mac equivalent: Set project = "/data/projects/M82"
```

Every path from here on can reference `$project` instead of the full
string.

## 4. Load All Sessions with One Glob

`AddFiles` accepts glob patterns, including wildcards in the middle of
a path — not just at the end. This lets one command pull frames from
every session folder in the project at once:

```
ClearSession
AddFiles paths="$project/M82-*-sess-*/lights/*.fit"

CountFiles
Assert expression="$filecount > 0"
Print "Loaded " + $filecount + " frames across all sessions"
```

`Assert` halts the script with a clear error if the glob matched
nothing — worth having in any script that depends on files actually
being found, rather than silently continuing with zero frames.

## 5. Run Analyze Frames on the Combined Pool

```
AnalyzeFrames profile="Project"
```

This is the same `AnalyzeFrames` command from Tutorials 1 and 2 — the
difference is entirely in what's loaded. Because every frame from
every session is in the session pool together, PASS/REJECT is decided
relative to the combined population, not any single night.

> Use (or create) a threshold profile named to reflect this run — the
> `"Project"` name here is just an example; name your own profile
> however makes sense for your workflow.

## 6. Review Results

```
ShowAnalysisResults
```

This opens the same Analysis Results table from Tutorial 1 — sort it,
inspect category badges, and confirm the classifications make sense
before committing. Nothing has been written to disk yet.

## 7. Commit Results

```
CommitAnalysis append=.project
```

Just like Tutorial 1, REJECT frames move to a `rejected/` subfolder
within their own original session directory (not a single combined
rejected folder) — each frame's rejection travels back to where it
came from. The `.project` suffix on the moved filename distinguishes
this pass from any other rejection pass you might run later.

## 8. Stack the Combined Set

```
StackFrames
```

All surviving frames — regardless of which night they came from — are
aligned and combined into one stack. Photyx's rotational grouping
handles frames from different nights automatically, so you don't need
to do anything special here even if your equipment was rotated or
re-polar-aligned between sessions.

## 9. Stretch and Commit the Stretch

```
CommitStretch shadow_clip=-3.5 target_bg=0.10
```

Unlike Tutorial 1's default preview stretch, here we're specifying
exact values — useful once you have a stretch you like and want a
repeatable result rather than an auto-computed one each time.

## 10. Write the Result

```
WriteXISF destination="$project/stacked" stack=true
Print "Stack complete: " + $STACKED
```

## 11. Wrap-Up

This workflow is worth the extra setup specifically when you have
*multiple nights* of the same target and want them evaluated and
combined together. If you're only ever working with a single session
at a time, Tutorial 1's simpler workflow — run once per session — is
all you need, and is a good default until a project genuinely spans
more than one night.
