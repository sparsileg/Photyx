# Display & Navigation

Commands for moving between frames, caching blink thumbnails, and
opening the analysis views.

---

## `SetFrame`

Sets the current active frame by zero-based index.

```
SetFrame index=<integer>
```

```
SetFrame index=0
```

---

## `CacheFrames`

Pre-renders all loaded images to blink-resolution JPEGs, required
before using blink playback.

```
CacheFrames [resolution=<12|25>]
```

| Argument | Required | Default | Description |
| ------------ | -------- | ------- | ------------------------------------------------------------------------ |
| `resolution` | No | | `12` (12.5%) or `25` (25%). If omitted, both resolutions are cached. |

```
CacheFrames
CacheFrames resolution=25
```

---

## `ClearAnnotations`

Removes all star and analysis overlay annotations from the viewer.

```
ClearAnnotations
```

---

## `ShowAnalysisGraph`

Opens the Analysis Graph view.

```
AnalyzeFrames
ShowAnalysisGraph
```

---

## `ShowAnalysisResults`

Opens the Analysis Results table view.

```
AnalyzeFrames
ShowAnalysisResults
```
