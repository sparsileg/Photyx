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
  - [Path functions](#path-functions)
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
    - [CountMatches](#countmatches)
    - [FilterByKeyword](#filterbykeyword)
    - [GetSystemPath](#getsystempath)
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
    - [MedianValue](#medianvalue)
    - [GetHistogram](#gethistogram)
    - [ContourHeatmap](#contourheatmap)
  - [Image Processing](#image-processing)
    - [AutoStretch](#autostretch)
    - [DebayerImage](#debayerimage)
    - [BinImage](#binimage)
  - [Stacking](#stacking)
    - [StackFrames](#stackframes)
    - [CommitStretch](#commitstretch)
    - [ClearStack](#clearstack)
  - [Display & Navigation](#display--navigation)
    - [SetFrame](#setframe)
    - [SetZoom](#setzoom)
    - [CacheFrames](#cacheframes)
    - [BlinkSequence](#blinksequence)
    - [ClearAnnotations](#clearannotations)
    - [ShowAnalysisGraph](#showanalysisgraph)
    - [ShowAnalysisResults](#showanalysisresults)
  - [Scripting Utilities](#scripting-utilities)
    - [Set](#set)
    - [Print](#print-1)
    - [Assert](#assert)
    - [RunMacro](#runmacro)
    - [Log](#log-1)
    - [If / Else / EndIf](#if--else--endif)
    - [For / EndFor](#for--endfor)
  - [Console Built-ins](#console-built-ins)
- [Deprecated Commands](#deprecated-commands)
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
  - [Restore rejected files](#restore-rejected-files)
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
| `abs(x)`    | Absolute value           |
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

These functions operate on path strings and are used in `Set` expressions and anywhere an expression is evaluated.

| Function       | Description                                                                 |
| -------------- | --------------------------------------------------------------------------- |
| `dirof($path)` | Returns the directory portion of a path, normalized to forward slashes      |
| `basename($path)` | Returns the filename portion of a path, stripping all directory components |
| `stripext($path)` | Strips any suffix after the last known image extension (`.fit`, `.fits`, `.fts`, `.xisf`) |

```
Set f = "/data/lights/rejected/frame001.fit.session"
Set dir  = dirof($f)        # /data/lights/rejected
Set name = basename($f)     # frame001.fit.session
Set clean = stripext($f)    # /data/lights/rejected/frame001.fit

# Walk up two directory levels:
Set parent = dirof(dirof($f))   # /data/lights

# Full restore pattern — rename and move in one step:
Set cleaned = stripext($f)
Set dest = dirof(dirof($f)) + "/" + basename($cleaned)
MoveFile source="$f" destination="$dest"
```

### System-set variables

Several commands automatically store their results in variables.

| Variable         | Set by                                                          |
| ---------------- | --------------------------------------------------------------- |
| `$fwhm`          | `ComputeFWHM`                                                   |
| `$eccentricity`  | `ComputeEccentricity`                                           |
| `$starcount`     | `CountStars`                                                    |
| `$filecount`     | `CountFiles`                                                    |
| `$matchcount`    | `CountMatches`                                                  |
| `$downloads`     | `GetSystemPath name=downloads`                                  |
| `$documents`     | `GetSystemPath name=documents`                                  |
| `$desktop`       | `GetSystemPath name=desktop`                                    |
| `$temp`          | `GetSystemPath name=temp`                                       |
| `$NEW_FILE`      | `ContourHeatmap`, `CopyFile`, `MoveFile`                        |
| `$<KEYWORDNAME>` | `GetKeyword name=<KEYWORDNAME>` (uppercased)                    |

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

The `Else` branch is optional. `If` blocks may be nested. Supported comparison operators: `==`, `!=`, `<`, `>`, `<=`, `>=`. String comparisons are case-insensitive.

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

Use `CountMatches` to conditionally execute a block only when matching filesystem entries exist:

```
CountMatches pattern="$project/*-duo-*"
If $matchcount > 0
  Print "Found " + $matchcount + " duo sessions"
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

Loops may be nested. Numeric and glob loops can be mixed.

```
for d in "J:/projects/M82/M82-ircut-sess-*"
  ClearSession
  AddFiles paths="$d/lights/*.fit"
  AnalyzeFrames profile="Session"
  CommitAnalysis append=.session
EndFor
```

If the glob pattern matches no entries, a warning is printed and the loop body does not execute. This makes glob loops safe to use without explicit existence checks.

### Loops — iterating over all session files

This is the standard way to process all frames in a session.

```
CountFiles
For i = 0 to $filecount - 1
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
  ComputeFWHM
  Print $fwhm
EndFor
Log path="/logs/fwhm.log"

# Second segment goes to the star count log
CountFiles
For i = 0 to $filecount - 1
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

Appends one or more files to the current session. Accepts explicit file paths, glob patterns, or a mix of both in a comma-separated list. Files already loaded are skipped. Use `ClearSession` first to start a fresh session.

```
AddFiles paths=<path|glob>[,<path|glob>...]
```

| Argument | Required | Description                                             |
| -------- | -------- | ------------------------------------------------------- |
| `paths`  | Yes      | Comma-separated list of file paths and/or glob patterns |

Glob wildcards: `*` matches any sequence of characters, `?` matches a single character, `[...]` matches a character class. Glob patterns can appear anywhere in the path, including intermediate directory segments. Unmatched patterns produce a warning rather than an error.

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
| -------- | -------- | --------------------------- |
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
| -------- | -------- | ----------------- |
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

#### `CountMatches`

Counts filesystem entries (files or directories) matching a glob pattern and stores the result in `$matchcount`. Useful for conditionally executing a block only when matching entries exist.

```
CountMatches pattern=<glob>
```

| Argument  | Required | Description                                                        |
| --------- | -------- | ------------------------------------------------------------------ |
| `pattern` | Yes      | Glob pattern to match. Supports `*`, `?`, and `[...]` wildcards.  |

```
CountMatches pattern="$project/*-duo-*"
If $matchcount > 0
  Print "Found " + $matchcount + " duo sessions"
EndIf
```

---

#### `FilterByKeyword`

Filters the session file list to only those frames where the specified keyword matches the given value. Non-matching frames are removed from the session.

```
FilterByKeyword name=<string> value=<string>
```

| Argument | Required | Description                       |
| -------- | -------- | --------------------------------- |
| `name`   | Yes      | Keyword name to filter on         |
| `value`  | Yes      | Value to match (case-insensitive) |

```
FilterByKeyword name=FILTER value=Ha
FilterByKeyword name=OBJECT value="M31"
```

---

#### `GetSystemPath`

Retrieves a well-known system directory path and stores it in a variable named after the requested path. Supported names: `downloads`, `documents`, `desktop`, `temp`.

```
GetSystemPath name=<downloads|documents|desktop|temp>
```

| Argument | Required | Description                                                                                    |
| -------- | -------- | ---------------------------------------------------------------------------------------------- |
| `name`   | Yes      | System path to retrieve. Result stored in `$<name>` (e.g. `name=downloads` → `$downloads`). |

```
GetSystemPath name=downloads
Print $downloads
ExportAnalysisReport path="$downloads/M82-Project-Analysis.json"

GetSystemPath name=temp
Print $temp
```

---

### Write / Export

#### `WriteCurrent`

Writes all buffered images back to their source paths in their original format using an atomic temp-rename. This is the standard way to persist keyword changes across all frames.

```
WriteCurrent
```

---

#### `WriteFrame`

Writes the currently active frame only back to its source path.

```
WriteFrame
```

---

#### `WriteFIT`

Writes all session files to a destination directory in FITS format. Use `stack=true` to write the transient stack result as a single file. The `.fit` extension is appended automatically if not specified. When `stack=true`, stores the output path in `$STACKED`.

```
WriteFIT destination=<path> [overwrite=<bool>] [stack=<bool>]
```

| Argument      | Required | Default | Description                                                                          |
| ------------- | -------- | ------- | ------------------------------------------------------------------------------------ |
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

---

#### `WriteXISF`

Writes all session files to a destination directory in XISF format. Use `stack=true` to export the transient stack result instead. When `stack=true`, stores the output path in `$STACKED`.

```
WriteXISF destination=<path> [overwrite=<bool>] [compress=<bool>] [stack=<bool>]
```

| Argument      | Required | Default | Description                                        |
| ------------- | -------- | ------- | -------------------------------------------------- |
| `destination` | Yes      |         | Directory to write files to                        |
| `overwrite`   | No       | `false` | Overwrite existing files                           |
| `compress`    | No       | `false` | Apply LZ4HC compression                            |
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

For example, to backup every frame in the session before processing:

```
CountFiles
For i = 0 To $filecount - 1
  SetFrame index=$i
  CopyFile destination="/data/Backups"
EndFor
```

---

#### `MoveFile`

Moves a file to a destination. Uses the current frame if no source is specified. If the destination is an existing directory, the file is moved into it preserving its filename. If the destination is a full file path (mv semantics), the file is moved and renamed in one step. The destination parent directory is created automatically if needed. Removes the file from the session if it was a session file.

```
MoveFile destination=<path> [source=<path>]
```

| Argument      | Required | Description                                                                                   |
| ------------- | -------- | --------------------------------------------------------------------------------------------- |
| `destination` | Yes      | Destination directory, or full destination file path for rename-during-move.                  |
| `source`      | No       | Source file path (default: current frame). May be a file outside the session.                 |

```
# Move to a directory (filename preserved):
MoveFile destination="/data/Rejects"
MoveFile source="$f" destination="/data/Rejects"

# Rename during move (mv semantics):
Set cleaned = stripext($f)
MoveFile source="$f" destination="$cleaned"

# Move and rename to parent directory:
Set cleaned = stripext($f)
Set dest = dirof(dirof($f)) + "/" + basename($cleaned)
MoveFile source="$f" destination="$dest"
```

---

### Keywords

#### `AddKeyword`

Adds or replaces a FITS keyword on loaded images.

```
AddKeyword name=<string> value=<string> [comment=<string>] [scope=all|current]
```

| Argument  | Required | Default | Description                     |
| --------- | -------- | ------- | ------------------------------- |
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

```
ModifyKeyword name=OBJECT value="M31 Andromeda" scope=all
```

---

#### `DeleteKeyword`

Removes a FITS keyword from loaded images.

```
DeleteKeyword name=<string> [scope=all|current]
```

```
DeleteKeyword name=EXPTIME scope=all
```

---

#### `CopyKeyword`

Copies a keyword value from one keyword name to another in the current frame.

```
CopyKeyword from=<string> to=<string>
```

```
CopyKeyword from=EXPTIME to=EXPOSURE
```

---

#### `GetKeyword`

Retrieves a FITS keyword value from the current frame and stores it in `$<NAME>` (uppercased).

```
GetKeyword name=<string>
```

**Side effect:** Stores result in `$<NAME>`. For example, `GetKeyword name=FILTER` stores the value in `$FILTER`.

```
GetKeyword name=FILTER
Print $FILTER
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

Computes five quality metrics for all loaded frames (FWHM, eccentricity, star count, signal weight, background median) and classifies each frame as PASS or REJECT using iterative sigma clipping against session statistics.

```
AnalyzeFrames [profile=<name>]
```

| Argument  | Required | Description                                                                                             |
| --------- | -------- | ------------------------------------------------------------------------------------------------------- |
| `profile` | No       | Threshold profile name (e.g. `Session`, `Project`). If omitted, uses the active profile from settings. |

```
AnalyzeFrames
AnalyzeFrames profile=Session
AnalyzeFrames profile=Project
```

Results are visible in the Analysis Results and Analysis Graph views. See `ShowAnalysisGraph` and `ShowAnalysisResults`.

---

#### `CommitAnalysis`

Moves all REJECT frames to a `rejected/` subfolder within each frame's source directory and removes them from the session. Pass frames remain loaded. Optionally appends a suffix to each moved filename.

```
CommitAnalysis [append=<ext>]
```

| Argument | Required | Default | Description                                                                                                                   |
| -------- | -------- | ------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `append` | No       |         | Suffix appended after the original filename extension (e.g. `append=.session` → `frame.fit.session`). Leading dot optional. |

```
CommitAnalysis
CommitAnalysis append=.session
CommitAnalysis append=.project
```

---

#### `ExportAnalysisReport`

Exports the current analysis results as a Photyx session JSON file. If `path` is omitted, a filename is derived from the first frame and written to the system Downloads folder.

```
ExportAnalysisReport [path=<path>]
```

| Argument | Required | Description                                                                                     |
| -------- | -------- | ----------------------------------------------------------------------------------------------- |
| `path`   | No       | Full destination path for the JSON file. If omitted, written to Downloads with auto-derived name. |

```
ExportAnalysisReport
ExportAnalysisReport path="$downloads/M82-Project-Duo-Analysis.json"
```

---

#### `ComputeFWHM`

Computes the Full Width at Half Maximum for detected stars in the current frame and displays per-star circle annotations on the viewer overlay.

```
ComputeFWHM
```

**Side effect:** Stores mean FWHM in `$fwhm`.

---

#### `ComputeEccentricity`

Computes mean star eccentricity for the current frame. Values near 0 = round stars; values near 1 = elongated stars.

```
ComputeEccentricity
```

**Side effect:** Stores result in `$eccentricity`.

---

#### `CountStars`

Counts the number of detected stars in the current frame.

```
CountStars
```

**Side effect:** Stores result in `$starcount`.

---

#### `MedianValue`

Returns the median pixel value per channel for the current frame.

```
MedianValue
```

---

#### `GetHistogram`

Computes the histogram and basic statistics (median, std dev, clipping %) for the current frame.

```
GetHistogram
```

---

#### `ContourHeatmap`

Generates a false-color spatial FWHM heatmap for the current frame. Writes the result as an XISF file to the source file's directory.

```
ContourHeatmap [palette=viridis|plasma|coolwarm] [contour_levels=<int>] [threshold=<float>] [saturation=<float>]
```

| Argument         | Required | Default   | Description                       |
| ---------------- | -------- | --------- | --------------------------------- |
| `palette`        | No       | `viridis` | Color palette                     |
| `contour_levels` | No       | `10`      | Number of contour levels          |
| `threshold`      | No       |           | Outlier pixel rejection threshold |
| `saturation`     | No       | `1.0`     | Color saturation multiplier       |

**Side effect:** Stores output file path in `$NEW_FILE`.

```
ContourHeatmap palette=plasma contour_levels=12
```

---

### Image Processing

#### `AutoStretch`

Applies an automatic stretch to the current frame for display using the PixInsight-compatible Auto-STF algorithm. The raw pixel buffer is not modified.

```
AutoStretch [shadowClip=<float>] [targetBackground=<float>]
```

| Argument           | Required | Default | Description                          |
| ------------------ | -------- | ------- | ------------------------------------ |
| `shadowClip`       | No       | `-2.8`  | Shadow clipping point in sigma units |
| `targetBackground` | No       | `0.15`  | Target background level (0.0–1.0)    |

```
AutoStretch shadowClip=-2.8 targetBackground=0.25
```

---

#### `DebayerImage`

Debayers a Bayer CFA image using bilinear interpolation. The Bayer pattern is read from the `BAYERPAT` keyword if present; `pattern=` overrides it.

```
DebayerImage [pattern=RGGB|BGGR|GRBG|GBRG] [method=bilinear]
```

```
DebayerImage pattern=RGGB
```

---

#### `BinImage`

Bins the current image by an integer factor, reducing resolution by averaging pixel blocks.

```
BinImage factor=<integer>
```

```
BinImage factor=2
```

---

### Stacking

#### `StackFrames`

Stacks all session frames into a single result image using reference frame selection, background normalization, FFT alignment, and sigma-clipped mean combination.

```
StackFrames [caldir=<path>]
```

| Argument  | Required | Description                             |
| --------- | -------- | --------------------------------------- |
| `caldir`  | No       | Directory containing calibration frames |

```
StackFrames
StackFrames caldir="/data/calibration"
```

---

#### `CommitStretch`

Permanently applies the Auto-STF stretch to the stack result pixel buffer. After committing, the stack buffer holds non-linear (stretched) data. Use `WriteXISF stack=true` to export.

```
CommitStretch [shadow_clip=<float>] [target_bg=<float>]
```

```
CommitStretch shadow_clip=-3.5 target_bg=0.10
```

---

#### `ClearStack`

Discards the transient stack result and closes the Stacking Workspace viewer.

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

#### `SetZoom`

Sets the viewer zoom level.

```
SetZoom level=<fit|25|50|100|200>
```

```
SetZoom level=fit
SetZoom level=100
```

---

#### `CacheFrames`

Pre-builds the blink cache for all session frames at both resolutions in the background. Required before using `BlinkSequence`.

```
CacheFrames
```

---

#### `BlinkSequence`

Starts blinking through all session frames for visual inspection. `CacheFrames` must have been run first.

```
BlinkSequence [fps=<float>]
```

| Argument | Required | Default | Description       |
| -------- | -------- | ------- | ----------------- |
| `fps`    | No       | `2.0`   | Frames per second |

```
CacheFrames
BlinkSequence fps=3
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

Assigns a value to a script variable. Supports arithmetic, string concatenation, math functions, and path functions.

```
Set <varname> = <expression>
```

```
Set x = 10
Set label = "Frame " + $x
Set sigma = sqrt(($x - $mean) ^ 2)
Set dir   = dirof($f)
Set name  = basename($f)
Set clean = stripext($f)
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
Print dirof($f) + "/" + basename($f)
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

#### `RunMacro`

Executes a saved macro by name from the database. Inner `Print` statements and command output appear in the console line by line.

```
RunMacro name=<string>
```

```
RunMacro name="my-workflow"
```

---

#### `Log`

Writes all console output accumulated since the last `Log` call to a file. This means that you specify the Log file *after* the commands you wish to include in the log.

```
Log path=<path> [append=<bool>]
```

| Argument | Required | Default | Description                                |
| -------- | -------- | ------- | ------------------------------------------ |
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
| ---------------- | -------------------------------------------------------- |
| `Help`           | Opens help for a specific command, or lists all commands |
| `Help <command>` | Shows syntax and examples for that command               |
| `Clear`          | Clears the console output buffer                         |
| `Version`        | Prints Photyx and pcode version information              |
| `pwd`            | Lists unique source directories of all loaded files      |

---

## Deprecated Commands

The following commands remain valid for script compatibility but are no longer used in analysis. They are no-ops or stubs.

| Command              | Notes                                                             |
| -------------------- | ----------------------------------------------------------------- |
| `BackgroundStdDev`   | Removed from analysis (r = 0.92–0.999 correlation with BgMedian) |
| `BackgroundGradient` | Removed from analysis (session-dependent sign reversal)           |

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
For i = 0 to $filecount - 1
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
For i = 0 to $filecount - 1
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

This example runs a two-pass analysis across a multi-session project. The first pass analyzes each session independently using forgiving session-level thresholds. The second pass loads all surviving frames and applies stricter project-level thresholds. Duo-band and broadband (ircut) sessions are never mixed — each filter type gets its own project-level analysis.

```
# ── Configuration ─────────────────────────────────────────────────────────────
Set project = "J:/Projects/M82-Cigar-Galaxy"

GetSystemPath name=downloads

# ── Pass 1a: session-level rejection — duo sessions ───────────────────────────

for d in "$project/*-duo-*"
  ClearSession
  AddFiles paths="$d/lights/*.fit"
  CountFiles
  Assert expression="$filecount > 0"
  Print "Analyzing duo session: " + $d + " (" + $filecount + " frames)"
  AnalyzeFrames profile="Session"
  ExportAnalysisReport
  CommitAnalysis append=.session
  Print "Done: " + $d
EndFor

# ── Pass 1b: session-level rejection — ircut sessions ─────────────────────────

for d in "$project/*-ircut-*"
  ClearSession
  AddFiles paths="$d/lights/*.fit"
  CountFiles
  Assert expression="$filecount > 0"
  Print "Analyzing ircut session: " + $d + " (" + $filecount + " frames)"
  AnalyzeFrames profile="Session"
  ExportAnalysisReport
  CommitAnalysis append=.session
  Print "Done: " + $d
EndFor

# ── Pass 2a: project-level rejection — duo sessions ───────────────────────────

CountMatches pattern="$project/*-duo-*"
If $matchcount > 0
  ClearSession
  AddFiles paths="$project/*-duo-*/lights/*.fit"
  CountFiles
  Assert expression="$filecount > 0"
  Print "Duo project pool: " + $filecount + " frames"
  AnalyzeFrames profile="Project"
  ExportAnalysisReport path="$downloads/Project-Duo-Analysis.json"
  CommitAnalysis append=.project
  Print "Duo project analysis complete."
EndIf

# ── Pass 2b: project-level rejection — ircut sessions ─────────────────────────

CountMatches pattern="$project/*-ircut-*"
If $matchcount > 0
  ClearSession
  AddFiles paths="$project/*-ircut-*/lights/*.fit"
  CountFiles
  Assert expression="$filecount > 0"
  Print "Ircut project pool: " + $filecount + " frames"
  AnalyzeFrames profile="Project"
  ExportAnalysisReport path="$downloads/Project-Ircut-Analysis.json"
  CommitAnalysis append=.project
  Print "Ircut project analysis complete."
EndIf

Print "All done."
```

---

### Restore rejected files

Reverses a previous `CommitAnalysis` run by stripping the `.session` or `.project` suffix from each rejected file and moving it back to its parent `lights/` directory. Useful during testing when you want to re-run analysis with different parameters.

```
# ── Configuration ─────────────────────────────────────────────────────────────
Set project = "J:/Projects/M82-Cigar-Galaxy"

# ── Restore session-level rejects ─────────────────────────────────────────────

for f in "$project/*/lights/rejected/*.fit.session"
  Set cleaned = stripext($f)
  Set dest = dirof(dirof($f)) + "/" + basename($cleaned)
  Print "Restoring: " + basename($f)
  MoveFile source="$f" destination="$dest"
EndFor

# ── Restore project-level rejects ─────────────────────────────────────────────

for f in "$project/*/lights/rejected/*.fit.project"
  Set cleaned = stripext($f)
  Set dest = dirof(dirof($f)) + "/" + basename($cleaned)
  Print "Restoring: " + basename($f)
  MoveFile source="$f" destination="$dest"
EndFor

Print "All rejected files restored."
```

---

### Calling a sub-macro

```
# Main workflow delegates to reusable sub-macros
RunMacro name="load-and-check"
RunMacro name="analyze-and-report"
```

This allows building libraries of composable macros pinned to the Quick Launch bar.
