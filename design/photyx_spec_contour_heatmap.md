# Photyx — Contour Heatmap Feature Spec (Draft)

*To be integrated into photyx_spec.md at a later milestone.*

---

## Feature: Contour Heatmap

### Overview

The Contour Heatmap is a full-frame focus quality visualization tool. It analyzes stars detected across a loaded FITS image, computes FWHM at each star's position, interpolates those values across a spatial grid, and renders the result as a heatmap image with contour lines overlaid. The output is treated as an image within Photyx — it can be displayed in the viewer region and saved to disk.

---

### Metric

**FWHM (Full Width at Half Maximum)** is the sole metric for the initial implementation. Smaller FWHM = sharper focus. The heatmap encodes FWHM spatially, making optical field curvature, tilt, and focus gradients immediately visible.

Eccentricity may be offered as an alternative rendering mode in a future phase, using the same heatmap infrastructure.

---

### Star Detection Prerequisite

The heatmap requires star detection to have been run on the loaded image prior to invocation. If no star detection data is available, the command aborts with an appropriate error message.

---

### Minimum Star Thresholds

| Condition                    | Behavior                                         |
| ---------------------------- | ------------------------------------------------ |
| Fewer than 25 stars detected | Abort with error — heatmap is not meaningful     |
| 25–74 stars detected         | Proceed with warning — results may be unreliable |
| 75+ stars detected           | Proceed normally                                 |

The hard abort threshold (25 stars) corresponds to fewer than 1 star per cell on a minimum 5×5 grid, rendering interpolation meaningless.

---

### Adaptive Grid Sizing

The grid resolution adapts to the number of detected stars, targeting approximately 10 stars per cell as the ideal density. Odd grid dimensions are preferred — they produce a true center cell, which is meaningful for symmetric optical systems.

| Stars detected | Grid size  |
| -------------- | ---------- |
| 25–74          | 5×5 (warn) |
| 75–399         | 5×5        |
| 400–899        | 7×7        |
| 900–1,599      | 9×9        |
| 1,600–2,499    | 11×11      |
| 2,500–3,599    | 13×13      |
| 3,600–5,000+   | 15×15      |

**Note:** These breakpoints are initial estimates. They will be tuned based on observational data from real images. The maximum grid size is capped at 15×15 for the initial implementation.

---

### Rendering

- The heatmap is rendered as a raster image at the same pixel dimensions as the source image.
- The color scale encodes FWHM value: cool colors (blue/green) represent sharp focus; warm colors (yellow/red) represent poor focus. Exact color ramp TBD.
- Contour lines are overlaid at regular FWHM intervals to make gradients legible.
- Cell FWHM values are computed as the mean FWHM of all stars within the cell.
- Interpolation between cell values produces a smooth surface. Interpolation method TBD (bilinear as baseline; bicubic or RBF if results are insufficiently smooth).

---

### Output

- The heatmap is generated as a Photyx image buffer that can be saved as a named file in any of the supported image formats.
- It is loaded into the Photyx session and displayed in the viewer region using the standard viewer component, identical to how a loaded FITS/XISF image is displayed.
- It can be saved to disk via the normal save/export mechanism.
- Metadata overlay (grid size used, star count, FWHM range, source filename) should be rendered into the image as FIT keywords.

---

### UI Integration

- **pcode command:** `ContourHeatmap`
- **Menu location:** Analyze menu
- **Quick Launch:** Can be pinned as a macro/script entry
- **Console output:** Progress and result written to the pcode console via `consolePipe`
- **Notifications:** `notifications.running()` during generation; `notifications.success()` or `notifications.error()` on completion
- **Viewer display:** Result image displayed in the standard viewer region (not a separate viewer-region component — the heatmap IS a named image)

---

### Implementation Location

| Component                                                 | Location                                    |
| --------------------------------------------------------- | ------------------------------------------- |
| Rust backend (star analysis, grid computation, rendering) | `src-tauri/src/plugins/contour_heatmap.rs`  |
| Plugin registration                                       | `src-tauri/src/plugins/mod.rs` and `lib.rs` |
| pcode command registration                                | `src-svelte/lib/pcodeCommands.ts`           |
| Console sync side effect                                  | `Console.svelte` → `syncSessionState()`     |

---

### Open Questions

1. Output image format: any supported file format
2. Output resolution: match source image dimensions
3. Color ramp: specific palette TBD (viridis, plasma, other). Let's try several and see what works best. Remember color-blind people when constructing palette.
4. Interpolation method: bilinear baseline
5. Contour interval: let's start at 10 contour levels, scaled to the range. make this easy to change as we experiment.
6. Metadata: save as FIT keywords
7. Final pcode command name: `ContourHeatmap` 
8. Menu placement: Analyze menu

---

*Draft — not yet integrated into photyx_spec.md*
