# Analysis Commands

Commands for computing per-frame quality metrics, classifying frames
as PASS/REJECT, and exporting or committing the results.

---

## `AnalyzeFrames`

Computes four quality metrics for loaded frames (FWHM, eccentricity,
star count, background median) and classifies each frame as PASS or
REJECT using iterative sigma clipping against session statistics.

```
AnalyzeFrames [profile=<string>] [scope=all|current] [threshold=<float>] [saturation=<float>]
```

| Argument | Required | Default | Description |
| ------------ | -------- | ------- | ------------------------------------------------------------------------------------------------------------------ |
| `profile` | No | | Threshold profile name to use for this run. If omitted, uses the active profile set in Edit > Analysis Parameters. The active profile is not permanently changed. |
| `scope` | No | `all` | `all` runs the full two-pass session analysis (session stats, PASS/REJECT classification, reference-frame selection). `current` runs the same four metrics on only the current frame and prints raw values — no session stats or classification. |
| `threshold` | No | `5.0` | Star detection threshold in units of background std dev |
| `saturation` | No | `0.98` | Saturation threshold — stars at or above this value are rejected from detection |

```
AnalyzeFrames
AnalyzeFrames profile="Session"
AnalyzeFrames profile="Project"
AnalyzeFrames scope=current
```

Results are visible in the Analysis Results and Analysis Graph
views. See [`ShowAnalysisGraph`](display-navigation.md#showanalysisgraph)
and [`ShowAnalysisResults`](display-navigation.md#showanalysisresults).

---

## `CommitAnalysis`

Moves all REJECT frames to a `rejected/` subfolder within each frame's
source directory and removes them from the session. Pass frames remain
loaded. Optionally appends a suffix to each moved filename.

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

## `ExportAnalysisReport`

Exports the current analysis results as a Photyx session JSON file. If
`path` is omitted, a filename is derived from the first frame and
written to the system Downloads folder.

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

## `ComputeFWHM`

Computes the median Full Width at Half Maximum for detected stars in
the current frame, reported in pixels (and arcseconds when `FOCALLEN`,
`INSTRUME`, and `XBINNING` keywords are present) and displays per-star
circle annotations on the viewer overlay.

```
ComputeFWHM [threshold=<float>] [peak_radius=<int>] [saturation=<float>]
```

| Argument | Required | Default | Description |
| ------------- | -------- | ------- | -------------------------------------------------------------- |
| `threshold` | No | `5.0` | Star detection threshold in units of background std dev |
| `peak_radius` | No | `3` | Radius in pixels for the local-maximum test |
| `saturation` | No | `0.98` | Stars at or above this peak value are rejected as saturated |

**Side effect:** Stores mean FWHM in `$fwhm`.

```
ComputeFWHM
Print $fwhm
```

---

## `ComputeEccentricity`

Computes mean star eccentricity for the current frame. Values near 0 =
round stars; values near 1 = elongated stars.

```
ComputeEccentricity [threshold=<float>] [peak_radius=<int>] [saturation=<float>]
```

| Argument | Required | Default | Description |
| ------------- | -------- | ------- | ------------------------------------------------------------ |
| `threshold` | No | `5.0` | Star detection threshold in units of background std dev |
| `peak_radius` | No | `3` | Radius in pixels for the local-maximum test |
| `saturation` | No | `0.98` | Stars at or above this peak value are rejected as saturated |

**Side effect:** Stores result in `$eccentricity`.

```
ComputeEccentricity
Print $eccentricity
```

---

## `CountStars`

Counts the number of detected stars in the current frame using
peak-finding on a sigma-clipped, background-subtracted image.

```
CountStars [threshold=<float>] [peak_radius=<int>] [flood_threshold=<float>] [saturation=<float>] [sigma=<float>] [iterations=<int>]
```

| Argument | Required | Default | Description |
| ----------------- | -------- | ------- | ------------------------------------------------------------------ |
| `threshold` | No | `5.0` | Detection threshold in units of background std dev |
| `peak_radius` | No | `3` | Radius in pixels for the local-maximum test |
| `flood_threshold` | No | `2.0` | Flood-fill lower bound in units of background std dev |
| `saturation` | No | `0.98` | Stars at or above this peak value are rejected as saturated |
| `sigma` | No | `3.0` | Sigma-clipping threshold used for background estimation |
| `iterations` | No | `5` | Maximum sigma-clipping iterations for background estimation |

**Side effect:** Stores result in `$starcount`.

```
CountStars
Print $starcount
```

---

## `GetHistogram`

Computes the histogram and basic statistics (median, std dev, clipping
%) for the current frame. RGB frames get per-channel statistics.

```
GetHistogram
```

---

## `ContourHeatmap`

Generates a false-color spatial FWHM heatmap for the current frame:
stars are detected, per-star FWHM is measured, values are interpolated
across an adaptive grid, and the result is rendered with contour
lines. Writes the result as an XISF file named
`<source_stem>_heatmap.xisf` in the source file's directory.

```
ContourHeatmap [palette=viridis|plasma|coolwarm] [contour_levels=<int>] [threshold=<float>] [saturation=<float>]
```

| Argument | Required | Default | Description |
| ---------------- | -------- | --------- | ------------------------------------------------------- |
| `palette` | No | `viridis` | Color palette |
| `contour_levels` | No | `10` | Number of contour levels (minimum 2) |
| `threshold` | No | `5.0` | Star detection threshold in units of background std dev |
| `saturation` | No | `0.98` | Stars at or above this peak value are rejected as saturated |

**Side effect:** Stores output file path in `$NEW_FILE`.

```
ContourHeatmap palette=plasma contour_levels=12
```

---

## `BackgroundMedian`

Computes the sigma-clipped background median for the current
frame. This is one of the four metrics `AnalyzeFrames` computes
internally for every frame; running it standalone is useful for
inspecting or tuning background estimation on a single frame.

```
BackgroundMedian [sigma=<float>] [iterations=<int>] [grid=<int>]
```

| Argument | Required | Default | Description |
| ------------ | -------- | ------- | ------------------------------------------------------ |
| `sigma` | No | `3.0` | Sigma-clipping threshold in std dev units |
| `iterations` | No | `5` | Maximum sigma-clipping iterations |
| `grid` | No | `4` | Grid divisions per axis used internally for gradient estimation |

**Side effect:** Stores result in `$backgroundmedian`.

```
BackgroundMedian
BackgroundMedian sigma=2.5 iterations=8
```
