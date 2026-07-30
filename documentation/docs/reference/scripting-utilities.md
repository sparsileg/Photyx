# Scripting Utilities

General-purpose commands for variables, output, control flow, and
system paths. See [Language Basics](language-basics.md) for the
underlying syntax rules these build on.

---

## `Set`

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

## `Print`

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

## `Assert`

Halts execution with an error if the condition is false. Silent on
pass.

```
Assert expression=<condition>
```

```
Assert expression="$filecount > 0"
Assert expression="$fwhm < 5.0"
```

---

## `CountMatches`

Counts filesystem entries (files or directories) matching a glob
pattern and stores the result in `$matchcount`. Useful for
conditionally executing a block only when matching entries exist,
without loading them into the session.

```
CountMatches pattern=<glob>
```

| Argument | Required | Description |
| --------- | -------- | --------------------------------------------------------------------------------------- |
| `pattern` | Yes | Glob pattern to match. Supports `*`, `?`, and `[...]` wildcards anywhere in the path. |

```
CountMatches pattern="$project/*-duo-*"
If $matchcount > 0
  Print "Found " + $matchcount + " duo sessions"
EndIf
```

---

## `GetSystemPath`

Retrieves a well-known system directory path and stores it in a
variable named after the requested path.

```
GetSystemPath name=<downloads|documents|desktop|temp|home|log|db>
```

| Argument | Required | Description |
| -------- | -------- | ------------------------------------------------------------------------------------------------------------------ |
| `name` | Yes | System path to retrieve: `downloads`, `documents`, `desktop`, `temp`, `home`, `log`, or `db`. Result stored in `$<name>`. |

```
GetSystemPath name=downloads
Print $downloads
ExportAnalysisReport path="$downloads/M82-Project-Analysis.json"

GetSystemPath name=home
Print $home
```

---

## `RunMacro`

Executes a saved macro by name from the database. Inner command output
and `Print` statements appear in the console line by line.

```
RunMacro name=<string>
```

```
RunMacro name="my-workflow"
```

---

## `Log`

Writes all console output accumulated since the last `Log` call to a
file. This means you specify the Log file *after* the commands whose
output you want captured.

```
Log path=<path> [append=<bool>]
```

| Argument | Required | Default | Description |
| -------- | -------- | ------- | -------------------------------------------- |
| `path` | Yes | | Output file path |
| `append` | No | `false` | Append to existing file instead of erasing |

```
Log path="/logs/session.log" append=true
```

---

## `If` / `Else` / `EndIf`

Conditional execution. See [Flow Control](language-basics.md#flow-control).

---

## `For` / `EndFor`

Two loop forms — numeric range and glob iterator — both closed with
`EndFor`. Loops may be nested and mixed.

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

See [Flow Control](language-basics.md#flow-control) for full details
and examples.

---

## Console Built-ins

These commands are available in the interactive console but have no
effect inside a saved macro.

| Command | Description |
| ---------------- | ---------------------------------------------------------- |
| `Help` | Opens help for a specific command, or lists all commands |
| `Help <command>` | Shows syntax and examples for that command |
| `Clear` | Clears the console output buffer |
| `Version` | Prints Photyx and pcode version information |
| `pwd` | Lists unique source directories of all loaded files |
