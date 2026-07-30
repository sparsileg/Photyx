# Keyword Commands

Commands for reading and editing FITS header keywords, either on the
current frame or across the whole session.

---

## `AddKeyword`

Adds or replaces a FITS keyword on loaded images.

```
AddKeyword name=<string> value=<string> [comment=<string>] [scope=all|current]
```

| Argument | Required | Default | Description |
| --------- | -------- | ------- | -------------------------------- |
| `name` | Yes | | Keyword name (max 8 characters) |
| `value` | Yes | | Keyword value |
| `comment` | No | | FITS comment |
| `scope` | No | `all` | `all` frames or `current` only |

```
AddKeyword name=TELESCOP value="Celestron EdgeHD 8" comment="Telescope used"
AddKeyword name=PXFLAG value=PASS scope=current
```

---

## `ModifyKeyword`

Changes the value of an existing FITS keyword.

```
ModifyKeyword name=<string> value=<string> [comment=<string>] [scope=all|current]
```

| Argument | Required | Default | Description |
| --------- | -------- | ------- | -------------------------------- |
| `name` | Yes | | Keyword name to modify |
| `value` | Yes | | New keyword value |
| `comment` | No | | New comment |
| `scope` | No | `all` | `all` frames or `current` only |

```
ModifyKeyword name=OBJECT value="M31 Andromeda" scope=all
```

---

## `DeleteKeyword`

Removes a FITS keyword from loaded images.

```
DeleteKeyword name=<string> [scope=all|current]
```

| Argument | Required | Default | Description |
| -------- | -------- | ------- | -------------------------------- |
| `name` | Yes | | Keyword name to delete |
| `scope` | No | `all` | `all` frames or `current` only |

```
DeleteKeyword name=EXPTIME scope=all
```

---

## `CopyKeyword`

Copies a keyword value from one keyword name to another.

```
CopyKeyword from=<string> to=<string> [scope=all|current]
```

| Argument | Required | Default | Description |
| -------- | -------- | ------- | -------------------------------- |
| `from` | Yes | | Source keyword name |
| `to` | Yes | | Destination keyword name |
| `scope` | No | `all` | `all` frames or `current` only |

```
CopyKeyword from=EXPTIME to=EXPOSURE
CopyKeyword from=EXPTIME to=EXPOSURE scope=current
```

---

## `GetKeyword`

Retrieves a FITS keyword value from the current frame and stores it in
`$<NAME>` (uppercased). If the keyword is not found and `default=` is
given, the default value is stored instead of halting the script —
useful for optional keywords that may be missing on older or
third-party captures.

```
GetKeyword name=<string> [default=<string>]
```

| Argument | Required | Description |
| --------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `name` | Yes | Keyword name to retrieve |
| `default` | No | Fallback value if the keyword is not found on the current frame, instead of halting the script (e.g. `default=""` or `default="NULL"`). Does not apply to no-frame-loaded errors. |

**Side effect:** Stores result in `$<NAME>`. For example, `GetKeyword
name=FILTER` stores the value in `$FILTER`.

```
GetKeyword name=FILTER
Print $FILTER

GetKeyword name=OBJECT default=""
If $OBJECT == ""
  Print "OBJECT keyword not set"
EndIf
```

---

## `ListKeywords`

Lists all FITS header keywords for the current frame, sorted
alphabetically. Also opens the Keyword Editor panel.

```
ListKeywords
```
