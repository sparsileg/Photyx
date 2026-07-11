# Photyx pcode Scripting Guide

pcode is the macro language built into Photyx. It is line-oriented: each line is either a command, a variable assignment, a flow-control statement, or a comment. Macros are saved in the Photyx database and can be run from the console, the Quick Launch bar, or via `RunMacro`.

---

## Table of Contents

- [Basics](#basics)
  - [Comments](#comments)
  - [Command syntax](#command-syntax)
  - [Running a macro from the console](#running-a-macro-from-the-console)
- [Variables](#variables)
  - [Arithmetic](#arithmetic)
  - [String concatenation](#string-concatenation)
  - [Math functions](#math-functions)
  - [System-set variables](#system-set-variables)
- [Flow Control](#flow-control)
    - [Conditionals](#conditionals)
    - [Loops — iterating over a numeric range](#loops--iterating-over-a-numeric-range)
    - [Loops — iterating over a glob pattern](#loops--iterating-over-a-glob-pattern)
    - [Loops — iterating over all session files](#loops--iterating-over-all-session-files)
- [Error Handling](#error-handling)
- [Console Output](#console-output)
  - [Print](#print)
  - [Log](#log)
- [Trace Mode](#trace-mode)
- [Command Reference](#command-reference)
  - [Session](#session)
    - [AddFiles](#addfiles)
    - [ReadImages](#readimages)
    - [ClearSession](#clearsession)
    - [LoadFile](#loadfile)
    - [CountFiles](#countfiles)
    - [FilterByKeyword](#filterbykeyword)
    - [RejectCurrentFrame](#rejectcurrentframe)
  - [Write / Export](#write--export)
    - [WriteCurrent](#writecurrent)
    - [WriteFrame](#writeframe)
    - [WriteFIT](#writefit)
    - [WriteTIFF](#writetiff)
    - [WriteXISF](#writexisf)
    - [CopyFile](#copyfile)
    - [MoveFile](#movefile)
  - [Keywords](#keywords)
    - [AddKeyword](#addkeyword)
    - [ModifyKeyword](#modifykeyword)
    - [DeleteKeyword](#deletekeyword)
    - [CopyKeyword](#copykeyword)
    - [GetKeyword](#getkeyword)
    - [ListKeywords](#listkeywords)
  - [Analysis](#analysis)
    - [AnalyzeFrames](#analyzeframes)
    - [CommitAnalysis](#commitanalysis)
    - [ExportAnalysisReport](#exportanalysisreport)
    - [ComputeFWHM](#computefwhm)
    - [ComputeEccentricity](#computeeccentricity)
    - [CountStars](#countstars)
    - [GetHistogram](#gethistogram)
    - [ContourHeatmap](#contourheatmap)
    - [BackgroundMedian](#backgroundmedian)
    - [BackgroundStdDev (deprecated)](#backgroundstddev-deprecated)
    - [BackgroundGradient (deprecated)](#backgroundgradient-deprecated)
  - [Image Processing](#image-processing)
    - [AutoStretch](#autostretch)
    - [DebayerImage](#debayerimage)
  - [Stacking](#stacking)
    - [StackFrames](#stackframes)
    - [CommitStretch](#commitstretch)
    - [ClearStack](#clearstack)
  - [Display & Navigation](#display--navigation)
    - [SetFrame](#setframe)
    - [CacheFrames](#cacheframes)
    - [ClearAnnotations](#clearannotations)
    - [ShowAnalysisGraph](#showanalysisgraph)
    - [ShowAnalysisResults](#showanalysisresults)
  - [Scripting Utilities](#scripting-utilities)
    - [Set](#set)
    - [Print](#print-1)
    - [Assert](#assert)
    - [CountMatches](#countmatches)
    - [GetSystemPath](#getsystempath)
    - [RunMacro](#runmacro)
    - [Log](#log-1)
    - [If / Else / EndIf](#if--else--endif)
    - [For / EndFor](#for--endfor)
  - [Console Built-ins](#console-built-ins)
- [Complete Examples](#complete-examples)
  - [Batch format conversion: FITS → XISF](#batch-format-conversion-fits--xisf)
  - [Quality analysis and review workflow](#quality-analysis-and-review-workflow)
  - [Filter session by keyword then write](#filter-session-by-keyword-then-write)
  - [Per-frame FWHM report with log](#per-frame-fwhm-report-with-log)
  - [Numeric loop: step through frames by index](#numeric-loop-step-through-frames-by-index)
  - [Conditional processing based on keyword](#conditional-processing-based-on-keyword)
  - [Heatmap generation with file capture](#heatmap-generation-with-file-capture)
  - [Full stack pipeline](#full-stack-pipeline)
  - [Session and project analysis workflow](#session-and-project-analysis-workflow)
  - [Calling a sub-macro](#calling-a-sub-macro)

---

## Basics

### Comments

Any line beginning with `#` is ignored.

```
# This is a comment
AddFiles paths="/data/lights/frame001.fit"   # inline comments are not supported
```

### Command syntax

```
CommandName arg1=value arg2="string value"
```

Arguments are named. Argument names are case-insensitive. String values containing spaces must be enclosed in double quotes. Boolean arguments accept `true` or `false`.

### Running a macro from the console

Type the macro name directly after `RunMacro`:

```
RunMacro name="my-workflow"
```

Or open it in the Macro Editor and click **Run**.

---

## Variables

Variables are set with `Set` and referenced with a `$` prefix.

```
Set count = 10
Set label = "Frame " + $count
Print $label
```

- Variable names are case-insensitive when read (`$fwhm` and `$FWHM` refer to the same value).
- String literals on the right-hand side of `Set` must use **double quotes**.
- Variables persist for the lifetime of the script execution and are visible to any macro called via `RunMacro`.

### Arithmetic

`+`, `-`, `*`, `/`, `^` (exponentiation) are supported. Parentheses group sub-expressions.

```
Set area   = 3.14159 * $r ^ 2
Set scaled = ($raw - $min) / ($max - $min)
```

### String concatenation

The `+` operator concatenates when either operand is non-numeric.

```
Set path = "/data/" + $target + "/lights"
```

### Math functions

| Function    | Description              |
| ----------- | ------------------------ |
| `sqrt(x)`   | Square root              |
| `abs(x)`    | Absolute value            |
| `round(x)`  | Round to nearest integer |
| `floor(x)`  | Round down               |
| `ceil(x)`   | Round up                 |
| `min(x, y)` | Smaller of two values    |
| `max(x, y)` | Larger of two values     |

```
Set sigma = sqrt(($x - $mean) ^ 2)
Set clipped = min($value, 65535)
```

### Path functions

| Function        | Description                                                        |
| ---------------- | ------------------------------------------------------------------- |
| `basename($path)` | Filename portion of a path, leading directories stripped          |
| `dirof($path)`     | Directory portion of a path, filename stripped                    |
| `stripext($path)`  | Strips a trailing suffix appended after a known image extension (`.fit`, `.fits`, `.fts`, `.xisf`) — e.g. the `.session`/`.project` suffix added by `CommitAnalysis` |

```
Set name   = basename($f)
Set dir    = dirof($f)
Set parent = dirof(dirof($f))
Set clean  = stripext($f)
```

### System-set variables

Several commands automatically store their results in variables.

| Variable         | Set by                                       |
| ---------------- | --------------------------------------------- |
| `$fwhm`          | `ComputeFWHM`                                |
| `$eccentricity`  | `ComputeEccentricity`                        |
| `$starcount`     | `CountStars`                                 |
| `$filecount`     | `CountFiles`                                 |
| `$matchcount`    | `CountMatches`                               |
| `$STACKED`       | `WriteFIT stack=true`, `WriteXISF stack=true` |
| `$NEW_FILE`      | `ContourHeatmap`, `CopyFile`, `MoveFile`     |
| `$LOAD_FILE_PATH` | `LoadFile`                                  |
| `$<KEYWORDNAME>` | `GetKeyword name=<KEYWORDNAME>` (uppercased; falls back to `default=` if given and the keyword is not found) |
| `$<name>`        | `GetSystemPath name=<name>` (e.g. `name=downloads` stores `$downloads`) |

Example — reading a keyword into a variable:

```
GetKeyword name=FILTER
Print $FILTER
```

---

## Flow Control

### Conditionals

```
If <expression>
  ...
Else
  ...
EndIf
```

The `Else` branch is optional. `If` blocks may be nested. Supported comparison operators: `==`, `!=`, `<`, `>`, `<=`, `>=`. String comparisons are case-insensitive. Equality is always `==` — a single `=` is assignment syntax used by `Set`, not a valid condition operator.

```
If $fwhm > 3.0
  Print "Poor focus — skipping"
Else
  Print "Focus acceptable"
EndIf
```

```
If $FILTER == "Ha"
  Print "Narrowband session"
EndIf
```

### Loops — iterating over a numeric range

```
For varname = N To M
  ...
EndFor
```

The loop variable steps from N to M inclusive. Both bounds can be variables or expressions.

```
Set frames = 10
For i = 1 To $frames
  Print "Processing frame " + $i
EndFor
```

### Loops — iterating over a glob pattern

`for <var> in "<pattern>"` expands a glob pattern and iterates over each matched path, binding it to the loop variable. The variable holds the full matched path as a string. Patterns may include wildcards in any path segment.

```
for <var> in "<glob_pattern>"
  ...
EndFor
```

```
for d in "J:/projects/M82/M82-*-sess-*"
  Print $d
EndFor
```

Loops may be nested. Numeric and glob loops can be mixed. If a glob pattern matches nothing, a warning is reported and the loop body simply doesn't execute — the script continues normally rather than halting.

```
for d in "J:/projects/M82/M82-ircut-sess-*"
  ClearSession
  AddFiles paths="$d/lights/*.fit"
  AnalyzeFrames profile="Session"
  CommitAnalysis append=.session
EndFor
```

### Loops — iterating over all session files

This is the standard way to process all frames in a session.

```
CountFiles
For i = 0 to $filecount - 1
  SetFrame index=$i
  ComputeFWHM
  Print $fwhm
EndFor
```

---

## Error Handling

By default, pcode halts on the first error. A failed command stops the script and reports the error to the console.

Use `Assert` to add explicit checks:

```
Assert expression="$filecount > 0"
```

`Assert` halts execution with an `ASSERT_FAILED` error if the condition is false. It is silent on pass in both Trace and No Trace modes.

---

## Console Output

### Print

Outputs an evaluated expression to the console:

```
Print "Hello world"
Print $fwhm
Print "FWHM: " + $fwhm
Print $x + 1
```

### Log

Writes all console output accumulated since the last `Log` call to a file. Each `Log` call resets the accumulation point, so multiple `Log` calls within a single macro can direct different segments of output to different files. Useful for recording analysis results from batch runs.

```
Log path="/logs/session.log"
Log path="/logs/session.log" append=true
```

```
# First segment goes to the FWHM log
CountFiles
For i = 0 to $filecount - 1
  SetFrame index=$i
  ComputeFWHM
  Print $fwhm
EndFor
Log path="/logs/fwhm.log"


# Second segment goes to the star count log
CountFiles
For i = 0 to $filecount - 1
  SetFrame index=$i
  CountStars
  Print $starcount
EndFor
Log path="/logs/starcounts.log"
```

---

## Trace Mode

The **Trace / No Trace** toggle in the console header controls verbosity. In Trace mode, each command and its resolved arguments are echoed before execution. In No Trace mode, only output explicitly produced by `Print` or a command's result message is shown.

---

## Command Reference

Commands are grouped by function. Arguments in `[brackets]` are optional.

---

### Session

#### `AddFiles`

Appends one or more files to the current session. Accepts explicit file
paths, glob patterns, or a mix of both in a comma-separated list. Files
already loaded are skipped. Use `ClearSession` first to start a fresh
session.

```
AddFiles paths=<path|glob>[,<path|glob>...]
```

| Argument | Required | Description                                             |
| -------- | -------- | ------------------------------------------------------- |
| `paths`  | Yes      | Comma-separated list of file paths and/or glob patterns |

Glob wildcards: `*` matches any sequence of characters, `?` matches a
single character, `[...]` matches a character class. Glob patterns can
appear anywhere in the path, including intermediate directory
segments. Unmatched patterns produce a warning rather than an error.

```
AddFiles paths="/data/M31/frame001.fit,/data/M31/frame002.fit"
AddFiles paths="/data/M31/lights/*.fit"
AddFiles paths="J:/projects/M82/M82-*-sess-*/lights/*.fit"
AddFiles paths="/data/M31/lights/*.fit,/data/M31/extra/frame099.fit"
```

---

#### `ReadImages`

Loads a single image file or all supported images in a directory into the session. Files already loaded are skipped.

```
ReadImages path=<path>
```

| Argument | Required | Description                 |
| -------- | -------- | ---------------------------- |
| `path`   | Yes      | Path to a file or directory |

```
ReadImages path="/home/stan/lights"
ReadImages path="/home/stan/lights/frame001.xisf"
```

---

#### `ClearSession`

Clears all files and state from the current session.

```
ClearSession
```

---

#### `LoadFile`

Loads a single file for temporary display without adding it to the session file list. Stores the path in `$LOAD_FILE_PATH`. This command is used from `File > Load Single Image`.

| Argument | Required | Description       |
| -------- | -------- | ------------------ |
| `path`   | Yes      | Full path to file |

```
LoadFile path="/data/heatmaps/fwhm_heatmap.xisf"
```

---

#### `CountFiles`

Stores the number of files currently loaded in the session in `$filecount`.

```
CountFiles
Print $filecount
```

---

#### `FilterByKeyword`

Filters the session file list to only those frames where the specified keyword matches the given value. Non-matching frames are removed from the session.

```
FilterByKeyword name=<string> value=<string>
```

| Argument | Required | Description                       |
| -------- | -------- | ---------------------------------- |
| `name`   | Yes      | Keyword name to filter on         |
| `value`  | Yes      | Value to match (case-insensitive) |

```
FilterByKeyword name=FILTER value=Ha
FilterByKeyword name=OBJECT value="M31"
```

---

#### `RejectCurrentFrame`

Moves a single frame to a `rejected/` subfolder within its own source
directory, removing it from the session and all caches. Defaults to
the current frame if `index` is omitted.

```
RejectCurrentFrame [index=<integer>] [append=<ext>]
```

| Argument | Required | Default | Description                                                                              |
| -------- | -------- | ------- | ------------------------------------------------------------------------------------------ |
| `index`  | No       | current frame | Zero-based frame index to reject                                                    |
| `append` | No       |         | Suffix appended after the original filename extension (e.g. `append=cloudy` → `frame.fit.cloudy`). Leading dot is optional. |

Unlike `CopyFile`, `MoveFile`, and `ContourHeatmap`, this command does
**not** store its output path in `$NEW_FILE` — it has no system-set
variable side effect.

```
# Reject the current frame
RejectCurrentFrame

# Reject a specific frame by index, with a custom suffix
RejectCurrentFrame index=12 append=cloudy
```

---

### Write / Export

#### `WriteCurrent`

Writes all buffered images back to their source paths. For `.fit`/`.fits`/`.fts` files this rewrites **keywords only** — the pixel data on disk is untouched, which makes this the standard way to persist keyword changes across a whole session without a full rewrite. For `.xisf` and `.tiff` files it performs a full rewrite (pixels and keywords together), since those formats don't support in-place keyword patching. Uses an atomic temp-rename for XISF/TIFF.

```
WriteCurrent
```

---

#### `WriteFrame`

Writes the currently active frame only back to its source path, using an atomic temp-rename. Unlike `WriteCurrent`, this always performs a full pixel + keyword rewrite regardless of format — including `.fit` files.

```
WriteFrame
```

---

#### `WriteFIT`

Writes all session files to a destination directory in FITS format. Use `stack=true` to write the transient stack result as a single file. The `.fit` extension is appended automatically for session-frame output. When `stack=true`, stores the output path in `$STACKED`.

```
WriteFIT destination=<path> [overwrite=<bool>] [stack=<bool>]
```

| Argument      | Required | Default | Description                                                                          |
| ------------- | -------- | ------- | -------------------------------------------------------------------------------------- |
| `destination` | Yes      |         | Output directory (session frames) or file path (stack=true)                          |
| `overwrite`   | No       | `false` | Overwrite existing files                                                             |
| `stack`       | No       | `false` | Write the transient stack result as a single FITS file instead of all session frames |

```
WriteFIT destination="/data/output" overwrite=true
WriteFIT destination="/data/masters/flat_master" stack=true
Print $STACKED
```

---

#### `WriteTIFF`

Writes all session files to a destination directory in TIFF format with AstroTIFF keyword embedding.

```
WriteTIFF destination=<path> [overwrite=<bool>]
```

| Argument      | Required | Default | Description                  |
| ------------- | -------- | ------- | ------------------------------ |
| `destination` | Yes      |         | Directory to write files to  |
| `overwrite`   | No       | `false` | Overwrite existing files     |

```
WriteTIFF destination="/data/Output" overwrite=true
```

---

#### `WriteXISF`

Writes all session files to a destination directory in XISF format. Use `stack=true` to export the transient stack result instead, using the auto-derived filename pattern `Photyx_stack_OBJECT_FILTER_INTEGRATIONTIME_DTG.xisf` (e.g. `Photyx_stack_M64_ircut_24000s_20260528113121Z.xisf`). When `stack=true`, stores the output path in `$STACKED`.

```
WriteXISF destination=<path> [overwrite=<bool>] [compress=<bool>] [stack=<bool>]
```

| Argument      | Required | Default | Description                                        |
| ------------- | -------- | ------- | ---------------------------------------------------- |
| `destination` | Yes      |         | Directory to write files to                        |
| `overwrite`   | No       | `false` | Overwrite existing files                           |
| `compress`    | No       | `false` | Apply LZ4HC compression with byte shuffling        |
| `stack`       | No       | `false` | Write the transient stack result instead of frames |

```
WriteXISF destination="/data/output" overwrite=true compress=false
WriteXISF destination="/data/output" stack=true
Print $STACKED
```

---

#### `CopyFile`

Copies a file to a destination directory. Uses the current frame if no source is specified. Stores the destination path in `$NEW_FILE`. The source file and session are unchanged.

```
CopyFile destination=<path> [source=<path>]
```

| Argument      | Required | Description                                                |
| ------------- | -------- | ------------------------------------------------------------ |
| `destination` | Yes      | Destination directory path (created automatically if needed) |
| `source`      | No       | Source file path (default: current frame)                  |

For example, to back up every frame in the session before processing:

```
CountFiles
For i = 0 To $filecount - 1
  SetFrame index=$i
  CopyFile destination="/data/Backups"
EndFor
```

---

#### `MoveFile`

Moves a file to a destination. Uses the current frame if no source is specified. If the destination is an existing directory (or ends with a path separator), the file is moved into it preserving its filename. Otherwise the destination is treated as a full file path, allowing rename-during-move (`mv` semantics). The destination parent directory is created automatically if needed. Stores the destination path in `$NEW_FILE`. Removes the file from the session file list if it was a session file.

```
MoveFile destination=<path> [source=<path>]
```

| Argument      | Required | Description                                                                |
| ------------- | -------- | ----------------------------------------------------------------------------- |
| `destination` | Yes      | Destination directory path, or full destination file path for rename-during-move |
| `source`      | No       | Source file path (default: current frame). May be a file outside the session |

```
MoveFile destination="/data/Rejects"
MoveFile source="$f" destination="/data/Rejects"
# Rename during move (mv semantics):
Set cleaned = stripext($f)
MoveFile source="$f" destination="$cleaned"
```

---

### Keywords

#### `AddKeyword`

Adds or replaces a FITS keyword on loaded images.

```
AddKeyword name=<string> value=<string> [comment=<string>] [scope=all|current]
```

| Argument  | Required | Default | Description                     |
| --------- | -------- | ------- | -------------------------------- |
| `name`    | Yes      |         | Keyword name (max 8 characters) |
| `value`   | Yes      |         | Keyword value                   |
| `comment` | No       |         | FITS comment                    |
| `scope`   | No       | `all`   | `all` frames or `current` only  |

```
AddKeyword name=TELESCOP value="Celestron EdgeHD 8" comment="Telescope used"
AddKeyword name=PXFLAG value=PASS scope=current
```

---

#### `ModifyKeyword`

Changes the value of an existing FITS keyword.

```
ModifyKeyword name=<string> value=<string> [comment=<string>] [scope=all|current]
```

| Argument  | Required | Default | Description                     |
| --------- | -------- | ------- | -------------------------------- |
| `name`    | Yes      |         | Keyword name to modify          |
| `value`   | Yes      |         | New keyword value               |
| `comment` | No       |         | New comment                     |
| `scope`   | No       | `all`   | `all` frames or `current` only  |

```
ModifyKeyword name=OBJECT value="M31 Andromeda" scope=all
```

---

#### `DeleteKeyword`

Removes a FITS keyword from loaded images.

```
DeleteKeyword name=<string> [scope=all|current]
```

| Argument | Required | Default | Description                    |
| -------- | -------- | ------- | -------------------------------- |
| `name`   | Yes      |         | Keyword name to delete         |
| `scope`  | No       | `all`   | `all` frames or `current` only |

```
DeleteKeyword name=EXPTIME scope=all
```

---

#### `CopyKeyword`

Copies a keyword value from one keyword name to another.

```
CopyKeyword from=<string> to=<string> [scope=all|current]
```

| Argument | Required | Default | Description                    |
| -------- | -------- | ------- | -------------------------------- |
| `from`   | Yes      |         | Source keyword name            |
| `to`     | Yes      |         | Destination keyword name       |
| `scope`  | No       | `all`   | `all` frames or `current` only |

```
CopyKeyword from=EXPTIME to=EXPOSURE
CopyKeyword from=EXPTIME to=EXPOSURE scope=current
```

---

#### `GetKeyword`

Retrieves a FITS keyword value from the current frame and stores it in `$<NAME>` (uppercased). If the keyword is not found and `default=` is given, the default value is stored instead of halting the script — useful for optional keywords that may be missing on older or third-party captures.

```
GetKeyword name=<string> [default=<string>]
```

| Argument  | Required | Description                                                                                                                                                |
| --------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name`    | Yes      | Keyword name to retrieve                                                                                                                                  |
| `default` | No       | Fallback value if the keyword is not found on the current frame, instead of halting the script (e.g. `default=""` or `default="NULL"`). Does not apply to no-frame-loaded errors. |

**Side effect:** Stores result in `$<NAME>`. For example, `GetKeyword name=FILTER` stores the value in `$FILTER`.

```
GetKeyword name=FILTER
Print $FILTER

GetKeyword name=OBJECT default=""
If $OBJECT == ""
  Print "OBJECT keyword not set"
EndIf
```

---

#### `ListKeywords`

Lists all FITS header keywords for the current frame, sorted alphabetically. Also opens the Keyword Editor panel.

```
ListKeywords
```

---

### Analysis

#### `AnalyzeFrames`

Computes five quality metrics for loaded frames (FWHM, eccentricity, star
count, signal weight, background median) and classifies each frame as
PASS or REJECT using iterative sigma clipping against session statistics.

```
AnalyzeFrames [profile=<string>] [scope=all|current] [threshold=<float>] [saturation=<float>]
```

| Argument     | Required | Default | Description                                                                                                      |
| ------------ | -------- | ------- | ------------------------------------------------------------------------------------------------------------------ |
| `profile`    | No       |         | Threshold profile name to use for this run. If omitted, uses the active profile set in Edit > Analysis Parameters. The active profile is not permanently changed. |
| `scope`      | No       | `all`   | `all` runs the full two-pass session analysis (session stats, PASS/REJECT classification, reference-frame selection). `current` runs the same five metrics on only the current frame and prints raw values — no session stats or classification. |
| `threshold`  | No       | `5.0`   | Star detection threshold in units of background std dev                                                          |
| `saturation` | No       | `0.98`  | Saturation threshold — stars at or above this value are rejected from detection                                  |

```
AnalyzeFrames
AnalyzeFrames profile="Session"
AnalyzeFrames profile="Project"
AnalyzeFrames scope=current
```

Results are visible in the Analysis Results and Analysis Graph views. See
`ShowAnalysisGraph` and `ShowAnalysisResults`.

---

#### `CommitAnalysis`

Moves all REJECT frames to a `rejected/` subfolder within each frame's source directory and removes them from the session. Pass frames remain loaded. Optionally appends a suffix to each moved filename.

```
CommitAnalysis [append=<ext>]
```

| Argument | Required | Default | Description |
| -------- | -------- | ------- | ----------- |
| `append` | No | | Suffix appended after the original filename extension (e.g. `append=.session` → `frame.fit.session`). Leading dot is optional. Defaults to no suffix. |

```
CommitAnalysis
CommitAnalysis append=.session
```

---

#### `ExportAnalysisReport`

Exports the current analysis results as a Photyx session JSON file. If `path` is omitted, a filename is derived from the first frame and written to the system Downloads folder.

```
ExportAnalysisReport [path=<path>]
```

| Argument | Required | Description |
| -------- | -------- | ----------- |
| `path` | No | Full destination path for the JSON file. If omitted, written to the Downloads folder with an auto-derived filename. |

```
ExportAnalysisReport
ExportAnalysisReport path="D:/projects/M64/M64_sess_20241112_analysis.json"
```

---

#### `ComputeFWHM`

Computes the median Full Width at Half Maximum for detected stars in the current frame, reported in pixels (and arcseconds when `FOCALLEN`, `INSTRUME`, and `XBINNING` keywords are present) and displays per-star circle annotations on the viewer overlay.

```
ComputeFWHM [threshold=<float>] [peak_radius=<int>] [saturation=<float>]
```

| Argument      | Required | Default | Description                                                |
| ------------- | -------- | ------- | -------------------------------------------------------------- |
| `threshold`   | No       | `5.0`   | Star detection threshold in units of background std dev       |
| `peak_radius` | No       | `3`     | Radius in pixels for the local-maximum test                   |
| `saturation`  | No       | `0.98`  | Stars at or above this peak value are rejected as saturated   |

**Side effect:** Stores mean FWHM in `$fwhm`.

```
ComputeFWHM
Print $fwhm
```

---

#### `ComputeEccentricity`

Computes mean star eccentricity for the current frame. Values near 0 = round stars; values near 1 = elongated stars.

```
ComputeEccentricity [threshold=<float>] [peak_radius=<int>] [saturation=<float>]
```

| Argument      | Required | Default | Description                                              |
| ------------- | -------- | ------- | ------------------------------------------------------------ |
| `threshold`   | No       | `5.0`   | Star detection threshold in units of background std dev     |
| `peak_radius` | No       | `3`     | Radius in pixels for the local-maximum test                 |
| `saturation`  | No       | `0.98`  | Stars at or above this peak value are rejected as saturated |

**Side effect:** Stores result in `$eccentricity`.

```
ComputeEccentricity
Print $eccentricity
```

---

#### `CountStars`

Counts the number of detected stars in the current frame using peak-finding on a sigma-clipped, background-subtracted image.

```
CountStars [threshold=<float>] [peak_radius=<int>] [flood_threshold=<float>] [saturation=<float>] [sigma=<float>] [iterations=<int>]
```

| Argument          | Required | Default | Description                                                    |
| ----------------- | -------- | ------- | ------------------------------------------------------------------ |
| `threshold`       | No       | `5.0`   | Detection threshold in units of background std dev                |
| `peak_radius`     | No       | `3`     | Radius in pixels for the local-maximum test                       |
| `flood_threshold` | No       | `2.0`   | Flood-fill lower bound in units of background std dev             |
| `saturation`      | No       | `0.98`  | Stars at or above this peak value are rejected as saturated       |
| `sigma`           | No       | `3.0`   | Sigma-clipping threshold used for background estimation           |
| `iterations`      | No       | `5`     | Maximum sigma-clipping iterations for background estimation       |

**Side effect:** Stores result in `$starcount`.

```
CountStars
Print $starcount
```

---

#### `GetHistogram`

Computes the histogram and basic statistics (median, std dev, clipping %) for the current frame. RGB frames get per-channel statistics.

```
GetHistogram
```

---

#### `ContourHeatmap`

Generates a false-color spatial FWHM heatmap for the current frame: stars are detected, per-star FWHM is measured, values are interpolated across an adaptive grid, and the result is rendered with contour lines. Writes the result as an XISF file named `<source_stem>_heatmap.xisf` in the source file's directory.

```
ContourHeatmap [palette=viridis|plasma|coolwarm] [contour_levels=<int>] [threshold=<float>] [saturation=<float>]
```

| Argument         | Required | Default   | Description                                          |
| ---------------- | -------- | --------- | ------------------------------------------------------- |
| `palette`        | No       | `viridis` | Color palette                                        |
| `contour_levels` | No       | `10`      | Number of contour levels (minimum 2)                 |
| `threshold`      | No       | `5.0`     | Star detection threshold in units of background std dev |
| `saturation`     | No       | `0.98`    | Stars at or above this peak value are rejected as saturated |

**Side effect:** Stores output file path in `$NEW_FILE`.

```
ContourHeatmap palette=plasma contour_levels=12
```

---

#### `BackgroundMedian`

Computes the sigma-clipped background median for the current frame. This is one of the five metrics `AnalyzeFrames` computes internally for every frame; running it standalone is useful for inspecting or tuning background estimation on a single frame.

```
BackgroundMedian [sigma=<float>] [iterations=<int>] [grid=<int>]
```

| Argument     | Required | Default | Description                                        |
| ------------ | -------- | ------- | ------------------------------------------------------ |
| `sigma`      | No       | `3.0`   | Sigma-clipping threshold in std dev units              |
| `iterations` | No       | `5`     | Maximum sigma-clipping iterations                      |
| `grid`       | No       | `4`     | Grid divisions per axis used internally for gradient estimation |

```
BackgroundMedian
BackgroundMedian sigma=2.5 iterations=8
```

---

#### `BackgroundStdDev` (deprecated)

**Deprecated but fully operational.** Computes the sigma-clipped background standard deviation for the current frame. No longer used by `AnalyzeFrames` — dropped because it correlated 0.92–0.999 with `BackgroundMedian` and added no discriminating signal. Retained for pcode script compatibility and standalone use; it still runs the full computation and returns real results, it just isn't part of the standard analysis pipeline.

```
BackgroundStdDev [sigma=<float>] [iterations=<int>] [grid=<int>]
```

Same arguments as `BackgroundMedian`.

```
BackgroundStdDev
```

---

#### `BackgroundGradient` (deprecated)

**Deprecated but fully operational.** Computes a background gradient estimate for the current frame. No longer used by `AnalyzeFrames` — dropped due to session-dependent sign reversal that made it unreliable as a rejection criterion. Retained for pcode script compatibility and standalone use.

```
BackgroundGradient [sigma=<float>] [iterations=<int>] [grid=<int>]
```

Same arguments as `BackgroundMedian`.

```
BackgroundGradient
```

---

### Image Processing

#### `AutoStretch`

Applies an automatic stretch to the current frame for display using the PixInsight-compatible Auto-STF algorithm. The raw pixel buffer is not modified.

```
AutoStretch [shadowClip=<float>] [targetBackground=<float>]
```

| Argument           | Required | Default | Description                          |
| ------------------ | -------- | ------- | -------------------------------------- |
| `shadowClip`       | No       | `-2.8`  | Shadow clipping point in sigma units |
| `targetBackground` | No       | `0.15`  | Target background level (0.0–1.0)    |

```
AutoStretch shadowClip=-2.8 targetBackground=0.25
```

---

#### `DebayerImage`

Debayers a Bayer CFA image to interleaved RGB using bilinear interpolation. Operates on the transient stack result if one exists; otherwise operates on the current session frame. The Bayer pattern is always read from the `BAYERPAT` (or `BAYER_PATTERN`) keyword, defaulting to RGGB if neither is present — there is currently no way to override the pattern or interpolation method from pcode.

```
DebayerImage
```

Takes no arguments. Frames that are already RGB are left unchanged (reported, not an error).

```
DebayerImage
```

> **Note:** Earlier documentation described `pattern=` and `method=` arguments; these do not exist in the current implementation. See the open issue tracking whether pattern override support is worth adding.

---

### Stacking

#### `StackFrames`

Stacks all session frames into a single result image using meridian-flip-aware group reference selection, FFT phase-correlation + triangle rigid alignment, and two-pass sigma-clipped mean combination. Color-aware: if the reference frame is Bayer or RGB, the stack accumulates all three channels.

```
StackFrames
```

Takes no arguments — calibration is applied separately before frames are loaded into the session, not as part of this command.

```
StackFrames
```

---

#### `CommitStretch`

Permanently applies the Auto-STF stretch to the stack result pixel buffer. After committing, the stack buffer holds non-linear (stretched) data. Use `WriteXISF stack=true` to export.

```
CommitStretch [shadow_clip=<float>] [target_bg=<float>]
```

| Argument      | Required | Description                                          |
| ------------- | -------- | ------------------------------------------------------- |
| `shadow_clip` | No       | Shadow clipping factor (default: current context value) |
| `target_bg`   | No       | Target background value 0.0–1.0 (default: current context value) |

```
CommitStretch shadow_clip=-3.5 target_bg=0.10
```

---

#### `ClearStack`

Discards the transient stack result and per-frame contribution data, returning the viewer to the normal session image.

```
ClearStack
```

---

### Display & Navigation

#### `SetFrame`

Sets the current active frame by zero-based index.

```
SetFrame index=<integer>
```

```
SetFrame index=0
```

---


#### `CacheFrames`

Pre-renders all loaded images to blink-resolution JPEGs, required before using `BlinkSequence`.

```
CacheFrames [resolution=<12|25>]
```

| Argument     | Required | Default | Description                                                          |
| ------------ | -------- | ------- | ------------------------------------------------------------------------ |
| `resolution` | No       |         | `12` (12.5%) or `25` (25%). If omitted, both resolutions are cached. |

```
CacheFrames
CacheFrames resolution=25
```

---


#### `ClearAnnotations`

Removes all star and analysis overlay annotations from the viewer.

```
ClearAnnotations
```

---

#### `ShowAnalysisGraph`

Opens the Analysis Graph view.

```
AnalyzeFrames
ShowAnalysisGraph
```

---

#### `ShowAnalysisResults`

Opens the Analysis Results table view.

```
AnalyzeFrames
ShowAnalysisResults
```

---

### Scripting Utilities

#### `Set`

Assigns a value to a script variable.

```
Set <varname> = <expression>
```

```
Set x = 10
Set label = "Frame " + $x
Set sigma = sqrt(($x - $mean) ^ 2)
```

---

#### `Print`

Outputs an evaluated expression to the console.

```
Print <expression>
```

```
Print "Hello world"
Print $fwhm
Print "FWHM: " + $fwhm
```

---

#### `Assert`

Halts execution with an error if the condition is false. Silent on pass.

```
Assert expression=<condition>
```

```
Assert expression="$filecount > 0"
Assert expression="$fwhm < 5.0"
```

---

#### `CountMatches`

Counts filesystem entries (files or directories) matching a glob pattern and stores the result in `$matchcount`. Useful for conditionally executing a block only when matching entries exist, without loading them into the session.

```
CountMatches pattern=<glob>
```

| Argument  | Required | Description                                                                            |
| --------- | -------- | --------------------------------------------------------------------------------------- |
| `pattern` | Yes      | Glob pattern to match. Supports `*`, `?`, and `[...]` wildcards anywhere in the path. |

```
CountMatches pattern="$project/*-duo-*"
If $matchcount > 0
  Print "Found " + $matchcount + " duo sessions"
EndIf
```

---

#### `GetSystemPath`

Retrieves a well-known system directory path and stores it in a variable named after the requested path.

```
GetSystemPath name=<downloads|documents|desktop|temp>
```

| Argument | Required | Description                                                                                        |
| -------- | -------- | --------------------------------------------------------------------------------------------------- |
| `name`   | Yes      | System path to retrieve: `downloads`, `documents`, `desktop`, or `temp`. Result stored in `$<name>`. |

```
GetSystemPath name=downloads
Print $downloads
ExportAnalysisReport path="$downloads/M82-Project-Analysis.json"
```

---

#### `RunMacro`

Executes a saved macro by name from the database. Inner command output and `Print` statements appear in the console line by line.

```
RunMacro name=<string>
```

```
RunMacro name="my-workflow"
```

---

#### `Log`

Writes all console output accumulated since the last `Log` call to a file. This means you specify the Log file *after* the commands whose output you want captured.

```
Log path=<path> [append=<bool>]
```

| Argument | Required | Default | Description                                |
| -------- | -------- | ------- | -------------------------------------------- |
| `path`   | Yes      |         | Output file path                           |
| `append` | No       | `false` | Append to existing file instead of erasing |

```
Log path="/logs/session.log" append=true
```

---

#### `If / Else / EndIf`

Conditional execution. See [Flow Control](#flow-control).

---

#### `For / EndFor`

Two loop forms — numeric range and glob iterator — both closed with `EndFor`. Loops may be nested and mixed.

**Numeric range:**
```
For <var> = N To M
  ...
EndFor
```

**Glob iterator:**
```
for <var> in "<glob_pattern>"
  ...
EndFor
```

See [Flow Control](#flow-control) for full details and examples.

---

### Console Built-ins

These commands are available in the interactive console but have no effect inside a saved macro.

| Command          | Description                                              |
| ---------------- | ---------------------------------------------------------- |
| `Help`           | Opens help for a specific command, or lists all commands |
| `Help <command>` | Shows syntax and examples for that command               |
| `Clear`          | Clears the console output buffer                         |
| `Version`        | Prints Photyx and pcode version information               |
| `pwd`            | Lists unique source directories of all loaded files       |

---

## Complete Examples

### Batch format conversion: FITS → XISF

```
# Convert all lights in a directory from FITS to XISF
ClearSession
ReadImages path="/data/M31/lights"
WriteXISF destination="/data/M31/xisf" overwrite=false compress=false
Print "Conversion complete."
```

---

### Quality analysis and review workflow

```
# Standard analysis workflow
ClearSession
ReadImages path="/data/NGC7331/lights"

CountFiles
Assert expression="$filecount > 0"
Print "Loaded " + $filecount + " frames"

AnalyzeFrames
ShowAnalysisResults
```

After reviewing results and committing, pass frames remain loaded and are ready to stack.

---

### Filter session by keyword then write

```
# Keep only Ha frames, write to a separate directory
FilterByKeyword name=FILTER value=Ha
CountFiles
Print "Ha frames: " + $filecount
WriteFIT destination="/data/Ha-only" overwrite=true
```

---

### Per-frame FWHM report with log

```
# Measure FWHM on every frame and write results to a log file
ReadImages path="/data/lights"

CountFiles
For i = 0 To $filecount - 1
  SetFrame index=$i
  GetKeyword name=DATE-OBS
  ComputeFWHM
  Print $DATE-OBS + "  FWHM=" + $fwhm
EndFor

Log path="/logs/fwhm_report.log"
```

---

### Numeric loop: step through frames by index

```
# Visit the first five frames by index
For i = 0 To 4
  SetFrame index=$i
  ComputeFWHM
  Print "Frame " + $i + ": FWHM=" + $fwhm
EndFor
```

---

### Conditional processing based on keyword

```
# Apply different stretch depending on filter
ReadImages path="/data/session"

CountFiles
For i = 0 To $filecount - 1
  SetFrame index=$i
  GetKeyword name=FILTER
  If $FILTER == "Ha"
    AutoStretch shadowClip=-2.4 targetBackground=0.10
  Else
    AutoStretch shadowClip=-2.8 targetBackground=0.20
  EndIf
EndFor
```

---

### Heatmap generation with file capture

```
# Generate a contour heatmap and copy it to a review folder
SetFrame index=0
ContourHeatmap palette=plasma contour_levels=12
Print "Heatmap written to: " + $NEW_FILE
CopyFile source=$NEW_FILE destination="/data/review"
```

---

### Full stack pipeline

```
# Load, analyze, stack, stretch, and export
ClearSession
ReadImages path="/data/M31/lights"
AnalyzeFrames
ShowAnalysisResults
# After manually committing rejects in the UI, continue:
StackFrames
CommitStretch shadow_clip=-3.5 target_bg=0.10
WriteXISF destination="/data/M31/stacked" stack=true
Print "Stack complete."
```

---

### Session and project analysis workflow

This example runs a two-pass analysis across a multi-session project. The
first pass analyzes each session independently using forgiving
session-level thresholds, moving the worst outliers to `rejected/`
subfolders. The second pass loads all surviving frames together and applies
stricter project-level thresholds to select the best material for
stacking.

```
# ── Pass 1: session-level rejection ─────────────────────────────────────────
# Process each session directory independently.
# Rejects are moved to <session>/lights/rejected/*.fit.session

for d in "J:/projects/M82/M82-ircut-sess-*"
  ClearSession
  AddFiles paths="$d/lights/*.fit"
  CountFiles
  Assert expression="$filecount > 0"
  Print "Session: " + $d + " — " + $filecount + " frames"
  AnalyzeFrames profile="Session"
  CommitAnalysis append=.session
EndFor

# ── Pass 2: project-level rejection ──────────────────────────────────────────
# Load surviving frames from all sessions together.
# Rejects from this pass are moved to rejected/*.fit (no suffix).

ClearSession
AddFiles paths="J:/projects/M82/M82-ircut-sess-*/lights/*.fit"
CountFiles
Assert expression="$filecount > 0"
Print "Project pool: " + $filecount + " frames"
AnalyzeFrames profile="Project"
CommitAnalysis append=.project
ShowAnalysisResults
```

After reviewing the Analysis Results table, click **Commit Results** to
finalize project-level rejections. Pass frames remain loaded and are ready
for stacking.

---

### Calling a sub-macro

```
# Main workflow delegates to reusable sub-macros
RunMacro name="load-and-check"
RunMacro name="analyze-and-report"
```

This allows building libraries of composable macros pinned to the Quick Launch bar.
