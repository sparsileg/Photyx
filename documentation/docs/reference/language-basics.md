# Language Basics

pcode is line-oriented: each line is either a command, a variable
assignment, a flow-control statement, or a comment. Macros are saved
in the Photyx database and can be run from the console, the Quick
Launch bar, or via `RunMacro`.

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

Arguments are named. Argument names are case-insensitive. String
values containing spaces must be enclosed in double quotes. Boolean
arguments accept `true` or `false`.

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

- Variable names are case-insensitive when read (`$fwhm` and `$FWHM`
  refer to the same value).
- String literals on the right-hand side of `Set` must use **double
  quotes**.
- Variables persist for the lifetime of the script execution and are
  visible to any macro called via `RunMacro`.
- The bare `$name` form only matches `[A-Za-z0-9_]` — it stops at the
  first character outside that set. Keyword names containing other
  characters, most commonly a hyphen (e.g. `DATE-OBS`), need the
  braced form instead: `${DATE-OBS}`. Without braces, `$date-obs` is
  read as *subtract the identifier `obs` from `$date`*, not as a
  single variable reference, since `-` is also the subtraction
  operator.

### Arithmetic

`+`, `-`, `*`, `/`, `^` (exponentiation) are supported. Parentheses
group sub-expressions.

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

| Function | Description |
| ----------- | ------------------------ |
| `sqrt(x)` | Square root |
| `abs(x)` | Absolute value |
| `round(x)` | Round to nearest integer |
| `floor(x)` | Round down |
| `ceil(x)` | Round up |
| `min(x, y)` | Smaller of two values |
| `max(x, y)` | Larger of two values |

```
Set sigma = sqrt(($x - $mean) ^ 2)
Set clipped = min($value, 65535)
```

### Path functions

| Function | Description |
| ---------------- | ------------------------------------------------------------------- |
| `basename($path)` | Filename portion of a path, leading directories stripped |
| `dirof($path)` | Directory portion of a path, filename stripped |
| `stripext($path)` | Strips a trailing suffix appended after a known image extension (`.fit`, `.fits`, `.fts`, `.xisf`) — e.g. the `.session`/`.project` suffix added by `CommitAnalysis` |

```
Set name   = basename($f)
Set dir    = dirof($f)
Set parent = dirof(dirof($f))
Set clean  = stripext($f)
```

### System-set variables

Several commands automatically store their results in variables.

| Variable | Set by |
| ---------------- | --------------------------------------------- |
| `$fwhm` | `ComputeFWHM` |
| `$eccentricity` | `ComputeEccentricity` |
| `$starcount` | `CountStars` |
| `$backgroundmedian` | `BackgroundMedian` |
| `$filecount` | `CountFiles` |
| `$matchcount` | `CountMatches` |
| `$STACKED` | `WriteFIT stack=true`, `WriteXISF stack=true` |
| `$NEW_FILE` | `ContourHeatmap`, `CopyFile`, `MoveFile` |
| `$LOAD_FILE_PATH` | `LoadFile` |
| `$<KEYWORDNAME>` | `GetKeyword name=<KEYWORDNAME>` (uppercased; falls back to `default=` if given and the keyword is not found) |
| `$<name>` | `GetSystemPath name=<name>` (e.g. `name=downloads` stores `$downloads`) |

Example   reading a keyword into a variable:

```
GetKeyword name=FILTER
Print $FILTER
```

### GetSystemPath names

`GetSystemPath name=<name>` accepts one of seven values, each storing
the result in a variable named after it (`$<name>`):

| Name | Variable | Description |
| ----------- | ------------- | ----------------------------------------------------------------------------------------- |
| `downloads` | `$downloads` | System Downloads folder |
| `documents` | `$documents` | System Documents folder |
| `desktop` | `$desktop` | System Desktop folder |
| `temp` | `$temp` | System temp directory |
| `home` | `$home` | Current user's home directory |
| `log` | `$log` | Log directory   respects a configured log-directory override if one is set, otherwise the OS-default Photyx log directory |
| `db` | `$db` | Directory containing `photyx.db`, not the file itself |

```
GetSystemPath name=downloads
Print $downloads
ExportAnalysisReport path="$downloads/M82-Project-Analysis.json"

GetSystemPath name=log
Print $log
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

The `Else` branch is optional. `If` blocks may be nested. Supported
comparison operators: `==`, `!=`, `<`, `>`, `<=`, `>=`. String
comparisons are case-insensitive. Equality is always `==` — a single
`=` is assignment syntax used by `Set`, not a valid condition
operator.

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

The loop variable steps from N to M inclusive. Both bounds can be
variables or expressions.

```
Set frames = 10
For i = 1 To $frames
  Print "Processing frame " + $i
EndFor
```

### Loops — iterating over a glob pattern

`for <var> in "<pattern>"` expands a glob pattern and iterates over
each matched path, binding it to the loop variable. The variable holds
the full matched path as a string. Patterns may include wildcards in
any path segment.

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

Loops may be nested. Numeric and glob loops can be mixed. If a glob
pattern matches nothing, a warning is reported and the loop body
simply doesn't execute — the script continues normally rather than
halting.

```
for d in "J:/projects/M82/M82-ircut-sess-*"
  ClearSession
  AddFiles paths="$d/lights/*.fit"
  AnalyzeFrames profile="Broadband"
  CommitAnalysis append=.reject
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

By default, pcode halts on the first error. A failed command stops the
script and reports the error to the console.

Use `Assert` to add explicit checks:

```
Assert expression="$filecount > 0"
```

`Assert` halts execution with an `ASSERT_FAILED` error if the
condition is false. It is silent on pass in both Trace and No Trace
modes.

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

Writes all console output accumulated since the last `Log` call to a
file. Each `Log` call resets the accumulation point, so multiple `Log`
calls within a single macro can direct different segments of output to
different files. Useful for recording analysis results from batch
runs.

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

The **Trace / No Trace** toggle in the console header controls
verbosity. In Trace mode, each command and its resolved arguments are
echoed before execution. In No Trace mode, only output explicitly
produced by `Print` or a command's result message is shown.
