# Write & Export Commands

Commands for writing session frames or the transient stack result back
to disk, and for copying or moving files.

---

## `WriteCurrent`

Writes all buffered images back to their source paths. For
`.fit`/`.fits`/`.fts` files this rewrites **keywords only** — the
pixel data on disk is untouched, which makes this the standard way to
persist keyword changes across a whole session without a full
rewrite. For `.xisf` files it performs a full rewrite (pixels and
keywords together), since that format doesn't support in-place keyword
patching. Uses an atomic temp-rename for XISF.

```
WriteCurrent
```

---

## `WriteFrame`

Writes the currently active frame only back to its source path, using
an atomic temp-rename. Unlike `WriteCurrent`, this always performs a
full pixel + keyword rewrite regardless of format — including `.fit`
files.

```
WriteFrame
```

---

## `WriteFIT`

Writes all session files to a destination directory in FITS
format. Use `stack=true` to write the transient stack result as a
single file. The `.fit` extension is appended automatically for
session-frame output. When `stack=true`, stores the output path in
`$STACKED`.

```
WriteFIT destination=<path> [overwrite=<bool>] [stack=<bool>]
```

| Argument | Required | Default | Description |
| ------------- | -------- | ------- | -------------------------------------------------------------------------------------- |
| `destination` | Yes | | Output directory (session frames) or file path (stack=true) |
| `overwrite` | No | `false` | Overwrite existing files |
| `stack` | No | `false` | Write the transient stack result as a single FITS file instead of all session frames |

```
WriteFIT destination="/data/output" overwrite=true
WriteFIT destination="/data/masters/flat_master" stack=true
Print $STACKED
```

---

## `WriteXISF`

Writes all session files to a destination directory in XISF
format. Use `stack=true` to export the transient stack result instead,
using the auto-derived filename pattern
`Photyx_stack_OBJECT_FILTER_INTEGRATIONTIME_DTG.xisf`
(e.g. `Photyx_stack_M64_ircut_24000s_20260528113121Z.xisf`). When
`stack=true`, stores the output path in `$STACKED`.

```
WriteXISF destination=<path> [overwrite=<bool>] [compress=<bool>] [stack=<bool>]
```

| Argument | Required | Default | Description |
| ------------- | -------- | ------- | ---------------------------------------------------- |
| `destination` | Yes | | Directory to write files to |
| `overwrite` | No | `false` | Overwrite existing files |
| `compress` | No | `false` | Apply LZ4HC compression with byte shuffling |
| `stack` | No | `false` | Write the transient stack result instead of frames |

```
WriteXISF destination="/data/output" overwrite=true compress=false
WriteXISF destination="/data/output" stack=true
Print $STACKED
```

---

## `CopyFile`

Copies a file to a destination directory. Uses the current frame if no
source is specified. Stores the destination path in `$NEW_FILE`. The
source file and session are unchanged. Fails with an error if a file
already exists at the destination, unless `overwrite=true`.

```
CopyFile destination=<path> [source=<path>] [overwrite=<bool>]
```

| Argument | Required | Default | Description |
| ------------- | -------- | ------- | ------------------------------------------------------------ |
| `destination` | Yes | | Destination directory path (created automatically if needed) |
| `source` | No | | Source file path (default: current frame) |
| `overwrite` | No | `false` | Overwrite an existing file at the destination |

For example, to back up every frame in the session before processing:

```
CountFiles
For i = 0 To $filecount - 1
  SetFrame index=$i
  CopyFile destination="/data/Backups" overwrite=true
EndFor
```

---

## `MoveFile`

Moves a file to a destination. Uses the current frame if no source is
specified. If the destination is an existing directory (or ends with a
path separator), the file is moved into it preserving its
filename. Otherwise the destination is treated as a full file path,
allowing rename-during-move (`mv` semantics). The destination parent
directory is created automatically if needed. Stores the destination
path in `$NEW_FILE`. Removes the file from the session file list if it
was a session file. Fails with an error if a file already exists at
the destination, unless `overwrite=true`. Cross-filesystem moves
(e.g. external drive to local disk) use an atomic
copy-to-temp-then-rename sequence, so an interrupted move never leaves
a partial file at the destination name.

```
MoveFile destination=<path> [source=<path>] [overwrite=<bool>]
```

| Argument | Required | Default | Description |
| ------------- | -------- | ------- | ----------------------------------------------------------------------------- |
| `destination` | Yes | | Destination directory path, or full destination file path for rename-during-move |
| `source` | No | | Source file path (default: current frame). May be a file outside the session |
| `overwrite` | No | `false` | Overwrite an existing file at the destination |

```
MoveFile destination="/data/Rejects"
MoveFile source="$f" destination="/data/Rejects"
MoveFile source="$f" destination="/data/Rejects" overwrite=true
# Rename during move (mv semantics):
Set cleaned = stripext($f)
MoveFile source="$f" destination="$cleaned"
```
