# Tutorial 2: Automating with pcode

**What you'll learn:** how to turn the workflow from Tutorial 1 into a
reusable script, using pcode — Photyx's built-in macro language.

**Before you begin:** complete Tutorial 1 first. This tutorial assumes
you're comfortable with what each step *does*; here we're just
learning to write it instead of clicking it.

---

## 1. Open the Console

The console lives as a collapsible panel at the bottom of the viewer.
Click its header to expand it to a larger overlay if you want more
room to read output.

Notice the **Trace / No Trace** toggle. In Trace mode, every command
you run is echoed back with its fully resolved arguments before it
executes — useful while you're learning, since you can see exactly
what Photyx received. Turn it on for this tutorial.

## 2. Run Your First Command

Type this into the console and press Enter:

```
CountFiles
```

`CountFiles` counts the frames currently loaded and stores the result
in a variable called `$filecount`. Every pcode command follows this
same shape: a command name, followed by zero or more `name=value`
arguments. `CountFiles` happens to take none.

Try:

```
Print $filecount
```

## 3. Reproduce Tutorial 1 as a Script

Everything you did by hand in Tutorial 1 can be typed as one block:

```
ClearSession
AddFiles paths="<your session directory>/*.fit"

AddKeyword name=FILTER value=duo comment="Tutorial test tag" scope=all
WriteCurrent

AnalyzeFrames profile="Default"
ShowAnalysisResults
```

Run it, review the results the same way you did in Tutorial 1, then
continue the same script to commit and stack:

```
CommitAnalysis append=.reject
StackFrames
```

Notice `CommitAnalysis` is the scripted equivalent of clicking Commit
Results, and it takes the same `.reject` suffix you saw appear on
rejected filenames in Tutorial 1.

## 4. Introduce Variables

Variables are set with `Set` and read back with a `$` prefix:

```
Set targetName = "M31"
Set label = "Session: " + $targetName
Print $label
```

Note the double quotes around the string literal — `Set`'s right-hand
side requires them for any text value. The `+` operator concatenates
when either side isn't purely numeric, so `$targetName` slots
naturally into the larger string.

## 5. Introduce a Loop

Here's a per-frame FWHM report — measuring focus quality frame by
frame and printing each result:

```
ReadImages path="<your session directory>"

CountFiles
For i = 0 To $filecount - 1
  SetFrame index=$i
  GetKeyword name=DATE-OBS
  ComputeFWHM
  Print $DATE-OBS + "  FWHM=" + $fwhm
EndFor

Log path="<your log folder>/fwhm_report.log"
```

`For i = 0 To $filecount - 1` steps through every loaded frame by
index. `SetFrame` makes frame `i` the active one, so every command
after it (`GetKeyword`, `ComputeFWHM`) operates on that specific
frame. `Log` writes everything printed since the last `Log` call out
to a file — handy for keeping a record of a batch run.

## 6. Introduce a Conditional

Let's flag frames with poor focus. Inside the same loop, right after
`ComputeFWHM`:

```
If $fwhm > 3.0
  Print "Poor focus — skipping"
Else
  Print "Focus acceptable"
EndIf
```

`If`/`Else`/`EndIf` blocks can nest, and comparisons use `==`, `!=`,
`<`, `>`, `<=`, `>=` — a single `=` is reserved for `Set` and isn't a
valid condition operator.

## 7. Save It as a Macro

Open the Macro Editor from the Icon Sidebar. Paste in your FWHM report
script (with the conditional from step 6 added), give it a name — for
example `fwhm-report` — and save it. Saving keeps a version history,
so you can always roll back to an earlier draft later.

## 8. Run It Two More Ways

You've been running commands directly in the console. Now try the
other two ways to run a saved macro:

- **From the Macro Library** — open the panel, find `fwhm-report`, and
  run it from there.
- **Via `RunMacro`** — type this into the console:

  ```
  RunMacro name="fwhm-report"
  ```

  `RunMacro` is also how one macro calls another — useful once you
  start building a library of small, reusable scripts.

## 9. Pin It to Quick Launch

Back in the Macro Library, pin `fwhm-report` to the Quick Launch bar.
It now appears as a one-click button above the viewer — no console or
macro library needed to run it next time. Pin as many macros as you
like; buttons wrap to a new row automatically.

## 10. Wrap-Up

You've now written variables, a loop, a conditional, and a saved,
pinnable macro — the building blocks for automating almost anything in
Photyx. From here, the **pcode Scripting Guide** is your ongoing
reference: every command, its arguments, and more worked examples than
we could fit in this tutorial.

Next up, if you're managing more than one imaging session for the same
target: **Tutorial 3: Multi-Session Project Workflow**.
