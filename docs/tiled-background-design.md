# Design: Tiled (Local) Background & Noise Estimation for Star Detection

Status: Design / ready for implementation
Scope: `analysis/stars.rs`, `analysis/background.rs` (new tiler), `analysis/mod.rs` (config)
Target implementer: Sonnet 5
Related: interim `min_peak_significance` floor (already merged); this replaces the
global background/noise estimate that motivated that floor.

---

## 1. Problem being solved

`detect_stars` (`analysis/stars.rs`) currently thresholds every pixel against a
**single global** background and noise estimate for the whole frame:

```
detect_thresh = bg + detection_threshold * sigma_bg      // bg, sigma_bg are ONE pair per frame
```

`bg`/`sigma_bg` come from one sigma-clipped pass over a 1-in-4 subsample of the
entire image (`sigma_clipped_background`, stride `STAR_DETECTION_SUBSAMPLE_STEP`).

On frames whose background is spatially non-uniform (nebulosity, gradients,
vignetting), a single global estimate is wrong: sigma-clipping rejects the bright
star tail, so `sigma_bg` reflects the *quietest* region, and the threshold then
sits in the noise across busier regions. This causes:

- **Over-detection into the noise floor** — noise bumps just above the global
  line get counted as stars.
- **Threshold instability** — because the floor scales with a global `sigma_bg`,
  small transparency changes swing the count non-linearly.

### Evidence a static approach can't generalize

Calibrating an absolute-peak floor against PixInsight SFS ground truth on two
sessions gives materially different ideal values:

| Session | Type | Global bg | PI stars/frame | Floor to match PI |
|---|---|---|---|---|
| Sh2-101 | duo-band, 300s, gain 100, dense field | ~0.0455 | ~1000–1700 | **0.009** |
| M104 | broadband/uvir, 60s, gain 101, sparse field | ~0.057 | ~30–50 | **~0.033** |

A ~3.7× spread in the ideal floor between two real sessions. At 0.009 the M104
frame dumped only 130 detections total (vs 7566 on the duo frame) because M104's
higher global background raises its own 5σ line — direct proof the two sessions'
noise floors live at genuinely different absolute peaks. A **local** estimate
adapts to this automatically; a global estimate plus hand-tuned constant does not.

---

## 2. Goal

Replace the single global `bg`/`sigma_bg` used **for detection** with a **tiled,
bilinearly-interpolated local estimate**, so each pixel is thresholded against the
background and noise *in its own region* of the frame.

**In scope:** the detection threshold path inside `detect_stars`, and the flood
threshold (both currently derive from the global `bg`/`sigma_bg`).

**Explicitly out of scope:**
- The reported background **median** metric (`background_median`, used as a
  per-frame reject filter). It stays global — it's a whole-frame summary statistic
  and tiling it would only force a collapse back to one number.
- Reported background **stddev**/**gradient** metrics. Note: `background_gradient`
  is being removed separately (no longer used as a reject filter). Do NOT build the
  new tiler on top of `background_gradient`'s grid — build a purpose-specific tiler
  (see §4). If `background_gradient` is already gone when you implement this, ignore
  references to it.
- Shape metrics (FWHM, eccentricity). These validate well against PI already and
  are not touched.

---

## 3. Non-goals / what NOT to change

- Do not change the detection algorithm's later stages: local-maximum test,
  flood-fill, min-component-size (`MIN_STAR_COMPONENT_PIXELS = 5`),
  extended-source rejection, centroiding, patch extraction, sorting. Only the
  **threshold values** each pixel is compared against change.
- Do not remove the `min_peak_significance` floor. It becomes a **safety net**
  (see §7), not a per-session knob. Keep it in the pipeline.
- Do not change `StarDetectionConfig`'s existing fields' meanings. Add new fields
  only.
- Keep detection deterministic. No randomness in tile sampling.

---

## 4. Design: the tiler

### 4.1 Tile grid

Divide the frame into a grid of tiles. Tile size is chosen to be:
- **coarse enough** that real stars do not dominate a tile's sigma-clipped
  background estimate (sigma-clipping already rejects the bright tail, but a tile
  must contain enough background pixels for the clip to be stable), and
- **fine enough** to track nebula-scale background variation.

**Default: 256×256 px tiles.** On a 3008×3008 frame that's ~12×12 tiles. Make it a
config field (`background_tile_size: usize`, default 256), not a magic number.

Edge tiles (frame dims not divisible by tile size) are smaller remainder tiles;
handle by clamping the last tile's extent to the frame edge (same pattern as
`background_gradient`'s `.min(width)/.min(height)`).

### 4.2 Per-tile estimate

For each tile, compute a `BackgroundEstimate { median, stddev }` using the existing
`sigma_clipped_background` on the tile's pixels. Apply the existing subsample stride
*within each tile* for speed (a tile of 256×256 = 65 536 px, subsampled 1-in-4 =
~16 k px, ample for a stable sigma-clip).

Store results as two 2-D grids indexed by tile row/col:
```
tile_bg[row][col]     : f32   // sigma-clipped median for that tile
tile_sigma[row][col]  : f32   // sigma-clipped stddev for that tile
```

Guard: if a tile is pathologically empty or all-clipped, fall back to that tile's
raw median/stddev (same fallback `sigma_clipped_background` already implements
internally — so this mostly comes for free).

### 4.3 Bilinear interpolation

Hard per-tile thresholds create seams: a star straddling a tile boundary sees a
step change in its threshold. Avoid this by treating each tile's estimate as a
sample **at the tile center**, and bilinearly interpolating `bg`/`sigma` for every
pixel from the four nearest tile centers.

For a pixel at `(x, y)`:
1. Find the four surrounding tile centers (clamp at frame edges — pixels outside the
   outermost tile centers use the nearest edge tiles, i.e. clamp, not extrapolate).
2. Bilinearly interpolate `bg(x,y)` from the four `tile_bg` values and `sigma(x,y)`
   from the four `tile_sigma` values.

Result: two smooth per-pixel fields `bg(x,y)` and `sigma(x,y)`, no seams.

Implementation note: you do NOT need to materialize full-resolution `bg`/`sigma`
images (that's another 2× frame-size f32 buffers). Interpolate on demand inside the
detection loop, OR precompute per-pixel only if profiling shows the repeated
interpolation math dominates. Start with on-demand; measure.

### 4.4 Threshold per pixel

Replace the current global thresholds:

```rust
// BEFORE (global):
let detect_thresh = bg + config.detection_threshold * bg_sd;
let flood_thresh  = bg + config.flood_threshold     * bg_sd;
```

with per-pixel local thresholds computed from the interpolated fields:

```rust
// AFTER (local), evaluated per candidate pixel (x, y):
let bg_xy    = interp_bg(x, y);
let sigma_xy = interp_sigma(x, y).max(1e-6);      // keep the existing zero guard
let detect_thresh_xy = bg_xy + config.detection_threshold * sigma_xy;
```

The **flood threshold** also becomes local. Flood-fill grows from a seed pixel; use
the seed pixel's local `bg`/`sigma` for the whole component's flood threshold (do
NOT recompute per flooded pixel — that would make component membership depend on
traversal order). I.e. compute `flood_thresh_seed = bg_seed + flood_threshold *
sigma_seed` once per seed, pass it into `flood_fill` as today.

Background subtraction (`bgsub`) currently uses the global `bg`. It should use the
**local** `bg(x,y)`:

```rust
// per pixel:
let bgsub_xy = (luma[y*width + x] - interp_bg(x, y)).max(0.0);
```

Note this makes `bgsub` position-dependent, which is correct — a star in a bright
region should have its local background subtracted, not the frame minimum. The
`peak` stored on `StarCandidate` (used by `min_peak_significance` and
extended-source rejection) is therefore now a **locally** background-subtracted
peak, which is exactly what we want the floor to gate on.

---

## 5. Config changes (`analysis/mod.rs`)

Add to `StarDetectionConfig`:

```rust
/// Edge length in pixels of the square tiles used for local background/noise
/// estimation during detection. Coarse enough that real stars don't dominate a
/// tile's sigma-clipped background; fine enough to track nebula-scale variation.
/// Default 256 (~12x12 tiles on a 3008px frame).
pub background_tile_size: usize,
```

Default: `background_tile_size: 256`.

Keep `min_peak_significance` as-is (safety net, see §7). Its default may be lowered
once tiling is validated (see §9), but do not change it in this change.

---

## 6. Suggested code structure

New function in `background.rs`:

```rust
/// A tiled local background/noise estimate over a luminance image.
pub struct LocalBackgroundField {
    tile_size: usize,
    cols: usize,
    rows: usize,
    width: usize,
    height: usize,
    tile_bg: Vec<f32>,     // rows*cols, row-major
    tile_sigma: Vec<f32>,  // rows*cols, row-major
}

impl LocalBackgroundField {
    /// Build the tiled estimate. Subsamples within each tile for speed.
    pub fn estimate(
        luma: &[f32],
        width: usize,
        height: usize,
        tile_size: usize,
        sigma_clip: &SigmaClipConfig,
    ) -> Self { /* ... */ }

    /// Bilinearly-interpolated background median at pixel (x, y).
    pub fn bg_at(&self, x: usize, y: usize) -> f32 { /* ... */ }

    /// Bilinearly-interpolated noise sigma at pixel (x, y).
    pub fn sigma_at(&self, x: usize, y: usize) -> f32 { /* ... */ }
}
```

`detect_stars` (`stars.rs`) then:
1. Builds `let field = LocalBackgroundField::estimate(luma, width, height, config.background_tile_size, &config.sigma_clip);`
   in place of the current single `sigma_clipped_background` call.
2. In the peak-finding loop, computes `detect_thresh` per pixel via `field.bg_at` /
   `field.sigma_at`.
3. Computes `bgsub` per pixel via `field.bg_at`.
4. Per seed, computes the local flood threshold and passes it to `flood_fill`.

Keep the rest of `detect_stars` byte-for-byte.

---

## 7. `min_peak_significance` as safety net

The floor stays in the pipeline **after** the local-threshold detection, unchanged
in mechanism: reject candidates whose (now locally) background-subtracted peak is
below `config.min_peak_significance`.

Its role changes from "primary noise gate, session-tuned" to "regime-independent
safety net": if the local estimate is still too permissive somewhere, the floor
catches gross noise. It is NOT to be re-tuned per session. Once tiling is validated
(§9), its default may be lowered (possibly toward a small regime-independent value),
but that is a follow-up decision backed by the same PI cross-check, not part of this
change.

---

## 8. Performance

Per-tile sigma-clipping over a subsampled frame, ~12×12 tiles, is comparable in
total work to the current single global sigma-clip (same total pixels sampled, just
partitioned) plus grid bookkeeping. The interpolation adds a few multiplies per
candidate pixel.

Requirements:
- Measure a full-session `AnalyzeFrames` run (all frames) before/after on both the
  Sh2-101 and M104 sessions. Report wall-clock delta.
- If on-demand interpolation dominates, precompute per-pixel `bg`/`sigma` fields
  once per frame (two f32 buffers, frame-sized) and index them — trades memory for
  speed. Decide based on measurement, not assumption.
- No regression to the prefetch/reader pipeline; this is confined to `detect_stars`
  and its background estimation.

---

## 9. Validation / acceptance

This is the pass/fail bar. Validate against PixInsight SFS ground truth on BOTH
sessions in the same run — the cross-session pair IS the regression harness.

Baseline (current, global estimate + `min_peak_significance = 0.009`):
- Sh2-101 duo: full-session **median star-count ratio vs PI = 0.96**, classification
  55 PASS / 5 REJECT (rejects = frames 35,36,38,40,41). Residual per-frame tilt:
  ratio ~1.3 on good-seeing frames (1–12) to ~0.7 on degraded frames (43–60).
- M104 broadband: **median ratio 3.9** (3.3–9× range) — over-counts badly at the
  duo-calibrated floor.

Acceptance for the tiled implementation, using ONE configuration (no per-session
constants):
1. **Sh2-101 duo:** median star-count ratio vs PI stays in a comparable band to
   baseline (≈0.9–1.1). Classification unchanged: good frames pass, frames
   35/36/38/40/41 reject.
2. **M104 broadband:** median star-count ratio vs PI pulled from ~3.9 into the same
   stable band as the duo session (target ≈0.9–1.3 — exact parity with PI's
   wavelet detector is NOT required; regime-consistent ratio IS). Classification:
   good frames pass, the genuinely-bad cluster (M104 frames 59–64) rejects.
3. **Tilt flattened:** the within-session per-frame ratio tilt on Sh2-101 (good vs
   degraded frames) is measurably reduced vs the global-estimate baseline.
4. **No per-session tuning:** the SAME `StarDetectionConfig` (same tile size, same
   `detection_threshold`, same `min_peak_significance`) produces acceptance 1–3 on
   both sessions. This is the whole point — demonstrate the local estimate removes
   the session-dependent constant.
5. **Performance:** full-session runtime delta quantified and accepted; no
   pathological slowdown.

### Validation harness note

A temporary per-frame diagnostic dump (peak, pixel_count, raw luma per candidate)
was used during the interim-floor calibration and is the right tool here too. Re-add
an equivalent dump during tuning; **remove it before commit.** For per-frame ratio
comparison, join Photyx export star counts to the PI SFS CSV by frame sequence
number (parse the trailing `_NNNN` from the filename).

---

## 10. Methodology constraints (project conventions)

- Discussion-first confirmation of tile size and interpolation approach before
  coding is already done (this doc); if implementation surfaces a reason to deviate
  (e.g. tile size needs to change materially), raise it rather than silently
  choosing.
- Fresh uploads of `stars.rs`, `background.rs`, `mod.rs` before editing.
- One change at a time with `cargo check` between steps. Suggested order:
  (a) add config fields; (b) add `LocalBackgroundField` in `background.rs` with unit
  tests; (c) wire `detect_stars` onto it; (d) validate against both PI CSVs;
  (e) remove diagnostic; (f) docs; (g) commit message.
- Update the technical reference doc for the detection-pipeline change BEFORE the
  commit message.
- Source files are ground truth over this doc if they disagree at implementation
  time — treat this as the design intent, not a spec frozen against code drift.

---

## 11. Unit tests to add (`background.rs`)

- Flat image → tiled field returns ~uniform bg, ~uniform small sigma; `bg_at`
  interpolation is continuous (no seams): sample along a horizontal line crossing a
  tile boundary and assert no step discontinuity larger than a small epsilon.
- Ramp image (dark left half, bright right half) → `bg_at` on the left < `bg_at` on
  the right; interpolated values transition smoothly across the boundary.
- Single bright star in one tile does NOT materially inflate that tile's `bg`/`sigma`
  (sigma-clip rejects it) — assert the star's tile bg ≈ neighboring tiles' bg.
- Edge/remainder tiles (frame size not divisible by tile size) don't panic and
  produce sane values.

Keep the existing `detect_stars` tests passing unchanged — the local path must
reduce to correct behavior on the synthetic flat-with-stars fixtures (which have
uniform background, so local ≈ global there).
