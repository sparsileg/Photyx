# Session Commands

Commands for loading, clearing, and filtering the files in the current
session.

---

## `AddFiles`

Appends one or more files to the current session. Accepts explicit
file paths, glob patterns, or a mix of both in a comma-separated
list. Files already loaded are skipped. Use `ClearSession` first to
start a fresh session.

```
AddFiles paths=<path|glob>[,<path|glob>...]
```

| Argument | Required | Description |
| -------- | -------- | ------------------------------------------------------- |
| `paths` | Yes | Comma-separated list of file paths and/or glob patterns |

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

## `ReadImages`

Loads a single image file or all supported images in a directory into
the session. Files already loaded are skipped.

```
ReadImages path=<path>
```

| Argument | Required | Description |
| -------- | -------- | ---------------------------- |
| `path` | Yes | Path to a file or directory |

```
ReadImages path="/home/stan/lights"
ReadImages path="/home/stan/lights/frame001.xisf"
```

---

## `ClearSession`

Clears all files and state from the current session.

```
ClearSession
```

---

## `LoadFile`

Loads a single file for display, adding it to the session file list
(by design — this is not an isolated preview). Stores the path in
`$LOAD_FILE_PATH`. This command is used from `File > Load Single
Image`.

| Argument | Required | Description |
| -------- | -------- | ------------------ |
| `path` | Yes | Full path to file |

```
LoadFile path="/data/heatmaps/fwhm_heatmap.xisf"
```

---

## `CountFiles`

Stores the number of files currently loaded in the session in
`$filecount`.

```
CountFiles
Print $filecount
```

---

## `FilterByKeyword`

Filters the session file list to only those frames where the specified
keyword matches the given value. Non-matching frames are removed from
the session.

```
FilterByKeyword name=<string> value=<string>
```

| Argument | Required | Description |
| -------- | -------- | ---------------------------------- |
| `name` | Yes | Keyword name to filter on |
| `value` | Yes | Value to match (case-insensitive) |

```
FilterByKeyword name=FILTER value=Ha
FilterByKeyword name=OBJECT value="M31"
```

---

## `RejectFrame`

Moves a single frame to a `rejected/` subfolder within its own source
directory, removing it from the session and all caches. Defaults to
the current frame if `index` is omitted.

```
RejectFrame [index=<integer>] [append=<ext>]
```

| Argument | Required | Default | Description |
| -------- | -------- | ------- | ------------------------------------------------------------------------------------------ |
| `index` | No | current frame | Zero-based frame index to reject |
| `append` | No | | Suffix appended after the original filename extension (e.g. `append=cloudy` → `frame.fit.cloudy`). Leading dot is optional. |

Unlike `CopyFile`, `MoveFile`, and `ContourHeatmap`, this command does
**not** store its output path in `$NEW_FILE` — it has no system-set
variable side effect.

```
# Reject the current frame
RejectFrame

# Reject a specific frame by index, with a custom suffix
RejectFrame index=12 append=cloudy
```
