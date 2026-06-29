# Photyx — Stacking Pipeline: Implementation Reference

**Version:** 5
**Date:** June 2026
**Status:** Implemented and working. Batch stacking with meridian-flip support complete. Live stacking deferred.

---

## 1. Purpose and Scope

Photyx stacking is a **diagnostic preview tool**, not a production stacker. The goal is to answer a single question quickly:

> "Did my imaging session go off the rails?"

It assesses framing, focus, tracking quality, cloud interference, and major gradients — fast enough to evaluate while still at the telescope. It is explicitly not a replacement for PixInsight SubframeSelector + stacking. PixInsight handles quality weighting during final integration; Photyx handles binary frame rejection and quick preview.

### Two Modes

| Mode | Status |
|---|---|
| **Batch diagnostic** — stack all loaded frames on demand | Implemented |
| **Live stack** — incrementally stack frames as they arrive | Deferred to second release |

Live stacking requires three components that must ship together: file system watching (`notify` crate), a background accumulation thread, and incremental display update. Do not implement any one without the others.

---

## 2. Architecture Overview

The stacking pipeline lives in `src-tauri/src/plugins/stack_frames.rs`. It is invoked via the `StackFrames` pcode command and runs asynchronously on a background thread (see §9 for the async architecture).

### High-level pipeline

```
Collect snapshots (star detection, FWHM, eccentricity, group assignment)
    ↓
Assign rotational groups (meridian flip detection)
    ↓
Select reference frames (per group + master)
    ↓
Solve M_cross for each non-master group (cross-group transform)
    ↓
Pass 1: Per-frame FFT + triangle alignment → Welford online mean + M2
    ↓
Pass 2: Sigma-clipped accumulation (batched parallel)
    ↓
Normalize output → write to AppContext stack slot
```

The stack result is held in a transient `ImageBuffer` in `AppContext`. It does not replace or disturb the loaded file list. The user explicitly writes it via `WriteXISF`, `WriteFIT`, or `WriteTIFF`.

---

## 3. Color Handling

One of the most important design decisions: **debayer first, then work in RGB throughout**.

- If the input frames are `ColorSpace::Bayer`, each frame is debayered to RGB before luma extraction
- Luma is then extracted from the RGB result for star detection and FFT alignment
- This eliminates the Bayer pattern mismatch caused by reversing raw Bayer data (`RGGB → BGGR`) which was causing systematic ~3–4px translation errors on all post-flip frames
- The stack accumulates all three RGB channels and outputs `ColorSpace::RGB`
- Mono input produces a mono stack output

**Never reverse raw Bayer pixel buffers.** Always debayer first.

---

## 4. Rotational Group Detection and Meridian Flip Handling

### Why groups are needed

A meridian flip rotates the camera 180° relative to the sky. Raw pixel buffers from post-flip frames are rotated 180° relative to pre-flip frames. FFT phase correlation completely fails across this boundary — the frame contents are too different. A cross-group transform (`M_cross`) must be solved once per non-master group to bridge the two orientations.

### Group assignment

Frames are sorted by `DATE-OBS` and assigned to groups based on two criteria:

1. **Rotator change > 90°** — always triggers a new group (meridian flip), regardless of time gap
2. **Time gap > 120 minutes AND rotator change > 10°** — splits same-rotator frames from different imaging nights into separate groups

Key constants:
```rust
const ROTATOR_GROUP_TOLERANCE: f32 = 10.0;   // degrees
const SESSION_GAP_MINUTES: f32     = 120.0;
const MERIDIAN_FLIP_THRESHOLD: f32 = 90.0;   // degrees
```

The larger group is the **master group**. Its best-quality frame (lowest FWHM, ties broken by eccentricity) becomes the master reference for the entire output coordinate space.

### M_cross solve

For each non-master group:

1. Load the group reference luma (debayered)
2. `reverse()` it — safe on debayered luma, no Bayer mismatch
3. FFT phase correlation against master reference luma → `(fft_dx, fft_dy)`
4. Detect stars in the flipped luma
5. Triangle-based rigid transform estimation against master reference stars
6. Compose: `M_cross = compose(post_flip, flip_180(width, height))`

The `flip_180` pre-rotation is applied first (inner), then the triangle match result (outer).

### Per-frame final transform

```
T = compose(M_cross, G)
```

Where `G` is the within-group FFT + triangle transform. For master-group frames, `M_cross = identity`, so `T = G`.

Frames are resampled using the full affine resampler (`resample_frame_affine`) when `|θ| ≥ 0.001 rad` or `a < 0.5` (indicating a flip is encoded). Otherwise the faster translation-only resampler is used.

---

## 5. Alignment Pipeline

### Stage 1: FFT phase correlation

`compute_translation(reference, target)` returns `(dx, dy)` where **positive dx means the target is shifted right** relative to the reference.

**Critical sign conventions** (these were a major source of bugs):
- Resampler: `src_x = out_x - dx` (subtracts dx to find source pixel)
- Star pre-translation for matching: `(cx + fft_dx, cy + fft_dy)` (adds dx to bring frame star into reference coordinate space)
- RANSAC residual reconstruction: `tx_full = aft_x + residual.tx` (positive, not negated)

FFT alignment must be **downsampled to ~1024px** before processing. Full 3008×3008 resolution causes multi-minute runtimes. The downsampling is handled inside `fft_align.rs`.

`fft_align.rs` is still needed for within-group per-frame translation even though triangle matching is used for rotation. Do not remove it.

### Stage 2: Triangle-based rigid refinement

After FFT gives a translation estimate, triangle matching (`estimate_rigid_transform_triangles`) refines the full rigid transform (translation + rotation).

Key finding: **least-squares refinement of the triangle vote result is numerically unstable** when star centroids are far from the image origin (which they always are for a 3008×3008 image). The fix is to skip refinement and return the winning voted transform directly.

`TRI_MAX_STARS = 40` (reduced from 60) — a 71% reduction in triangle pairs with no observed alignment quality loss. Empirically validate at 30 on sparse-star sessions before reducing further.

Triangle matching is parallelized via `par_iter().fold().reduce()` with per-thread VoteMap accumulation.

### What was tried and failed

| Approach | Result |
|---|---|
| `Vec::reverse()` on raw Bayer luma | Systematic ~3–4px translation error on flip frames |
| Rotation around image center | Wrong rotation center; corner doubling |
| RANSAC scale variation ±20% | Corrupted stack |
| Flip frame chaining (normalized luma anchor) | Drift from variable background normalization |
| Flip frame chaining (raw luma anchor) | Drift still present — turned out to be real dithering offsets |
| Direct FFT of flip frames against master reference | Only 9/80 frames stacked; content too different |
| Centroid remapping `(W-1-cx, H-1-cy)` | Made things worse; broke validation |
| FFT+RANSAC for M_cross | RANSAC returned wrong rotation (~0.112° vs true ~0.37°); ghost stars visible |
| Zeroing M_cross rotation (b=0) diagnostic | Ghost persisted — rotation was real, not RANSAC artifact |
| FFT-only M_cross | 6.5px mean verification residual |
| Least-squares triangle refinement | Numerically unstable; collapsed to wrong solution |
| **Triangle-based M_cross (current)** | **0.26–0.58px mean residual; round stars in multi-night stacks** |
| **DATE-OBS session boundary detection** | **Splits same-ROTATOR frames from different nights; eliminates elongation from polar alignment differences** |

---

## 6. Stacking Passes

### Pass 1: Welford online mean + M2

Sequential frame-by-frame accumulation using Welford's online algorithm for numerically stable mean and variance computation. This pass also:
- Validates filter keywords (excludes filter-mismatched frames)
- Estimates background level per frame for normalization
- Caches the final transform for each frame for use in Pass 2

**f64 accumulation is required** for per-pixel mean computation over large images. f32 precision loss is significant at 128+ frames.

Background normalization: frames are divided by their sigma-clipped background median before accumulation. This handles variable sky background across a session. Raw median is not used — it is unreliable when nebulosity or extended objects are present.

### Pass 2: Sigma-clipped accumulation

Uses the mean and M2 from Pass 1 to compute per-pixel standard deviation, then accumulates only pixels within `2.5σ` of the mean.

**Batched parallel processing pattern** (critical for memory safety):
```
for chunk in pass2_inputs.chunks(n_threads) {
    // Sequential: load one batch of pixel data
    let chunk_pixels = chunk.iter().map(load_pixels).collect();
    // Parallel: background estimate + resample each frame in chunk
    let aligned = chunk.par_iter().zip(chunk_pixels).map(process).collect();
    // Sequential: accumulate into sum_buf / clip_count
    for (inp, aligned) in chunk.iter().zip(aligned_buffers.iter()) { ... }
    // aligned_buffers dropped here — memory released
}
```

**Do not pre-extract all frames' pixel data before chunking.** This was tried and caused catastrophic memory pressure (~13.8GB simultaneous allocation). The correct pattern is: sequential load → parallel process → sequential accumulate → drop.

Batch size = `ctx.rayon_thread_count` (default 15 on an 8-core/16-thread machine; -1 = `rayon::current_num_threads()`).

### Output normalization

`normalize_output()` must use a **single global min/max across all channels**. Per-channel normalization destroys color balance by stretching each channel independently.

---

## 7. Performance

Benchmark dataset: 128-frame M82 session, 3008×3008 OSC, `rayon_thread_count = 15`.

| Optimization | Result |
|---|---|
| Triangle matching parallelized via `par_iter().fold().reduce()` | ~2,900ms → ~350ms per frame |
| Pass 2 batched parallel processing | Significant reduction |
| `TRI_MAX_STARS` reduced from 60 → 40 | 71% cut in triangle pairs, no quality loss |
| **Combined end-to-end** | **9m 57s → 3m 35s (2.8× speedup)** |

---

## 8. Known Issues and Deferred Work

### Known issues

1. **Alignment validation disabled.** `validate_alignment()` exists but is bypassed — all frames are currently accepted. The function was causing correct frames to fail. Root cause: coordinate space mismatch between star positions used for validation and the transform being validated. Not yet fully diagnosed. Re-enable only after understanding why it was failing.

2. **Autocrop not implemented.** After stacking frames with varying offsets, the valid coverage area is an irregular polygon. The largest inscribed rectangle avoiding zero-coverage border pixels is not computed — the full frame including zero-filled borders is returned.

3. **Within-group RANSAC sanity check threshold (15px) may need tuning.** Raised from original 5px. Frame 50 in the M82 dataset consistently gets a bad RANSAC solve (~52px residual) that is correctly rejected at 15px.

4. **Residual ~0.112° rotation between groups.** Still present in some multi-night stacks. Whether this is a real physical rotation or a RANSAC artifact has not been definitively resolved. To test: set `b = 0.0` in M_cross after solving and re-stack. If ghost disappears, it was an artifact; if it persists, the rotation is real.

5. **Timing instrumentation in Pass 1.** Temporary timing `info!()` calls were added to `stack_frames.rs` Pass 1 during performance work. These must be removed before any release build.

### Deferred work

- **Eliminate double debayer** — currently the highest-value remaining optimization. Each frame is debayered in both Pass 1 and Pass 2. Pass 1 result could be cached and reused.
- **Skip background estimation for calibrated frames** — background normalization is only needed for uncalibrated stacking
- **Empirically validate `TRI_MAX_STARS = 30`** on sparse-star sessions before committing to a lower floor
- **Autocrop** — compute largest inscribed rectangle of valid coverage
- **`PromoteStack` command** — transition stacked result into a single-image session for post-stack work
- **Post-stack pipeline order:** `StackFrames → PromoteStack → CommitStretch → WriteXISF`
- **Live stacking** — requires `notify` crate, background accumulation thread, and incremental display update shipping together
- **Median stacking** — more robust than sigma clipping; slower
- **Rejection map** — visual overlay showing which pixels were rejected
- **Drizzle** — sub-pixel integration for oversampled data

---

## 9. Async Architecture (Backend/Frontend Communication)

`StackFrames` runs on a background thread spawned by `run_script`. The Tauri `run_script` command returns `{ accepted: true }` immediately — it does not block the frontend.

### Progress reporting

Three globals in `lib.rs`:
```rust
pub static PROGRESS_CURRENT: AtomicU32
pub static PROGRESS_TOTAL:   AtomicU32
pub static PROGRESS_LABEL:   OnceCell<Mutex<String>>
```

Plugins call `crate::set_progress(label, current, total)` at each iteration. The frontend polls `get_progress()` every 500ms and displays `"StackFrames: Registering — 38/128 frames"` in the status bar.

Progress labels used by `StackFrames`:
- `"Analyzing frames"` — snapshot collection and star detection phase
- `"Registering"` — Pass 1 (FFT + triangle alignment + Welford accumulation)
- `"Integrating"` — Pass 2 (sigma-clipped accumulation)

Each plugin resets progress to `set_progress("", 0, 0)` before returning.

### Job result delivery

Results are delivered via `JOB_RESULT: OnceCell<Mutex<Option<JobResult>>>`. The background thread writes the completed `JobResult` to this slot; the frontend polling loop calls `get_job_result()` every 500ms and dispatches results to the owning component (`Console`, `QuickLaunch`, or `StackingWorkspace`) via `jobOwner` / `jobResult` Svelte stores.

---

## 10. Files Involved

| File | Role |
|---|---|
| `src-tauri/src/plugins/stack_frames.rs` | Main stacking pipeline |
| `src-tauri/src/analysis/star_align.rs` | `AffineRigid` type, triangle matching, `compose()` |
| `src-tauri/src/analysis/fft_align.rs` | FFT phase correlation |
| `src-tauri/src/analysis/stars.rs` | Star detection |
| `src-tauri/src/analysis/stack_metrics.rs` | `FrameContribution`, `StackSummary` |
| `src-tauri/src/analysis/background.rs` | Background estimation for normalization |
| `src-tauri/src/analysis/debayer.rs` | `debayer_bilinear()` used per-frame |
| `src-tauri/src/lib.rs` | `PROGRESS_*` globals, `set_progress()`, `JOB_RESULT`, `run_script` async dispatch |
| `src-svelte/lib/stores/progress.ts` | Frontend polling store |
| `src-svelte/lib/components/StackingWorkspace.svelte` | UI surface for stacking |
| `src-svelte/lib/components/StatusBar.svelte` | Progress display |

---

## 11. Lessons Learned

**Sign conventions are the most dangerous part of alignment code.** Document them explicitly and verify with a simple known-offset test before trusting any new alignment path. The `compute_translation` convention (positive dx = target shifted right) caused multiple bugs when applied inconsistently across the pipeline.

**Debayer before any spatial operation.** Reversing, rotating, or flipping raw Bayer data changes the color filter pattern and produces subtle but systematic errors. Always debayer to RGB or extract luma from debayered RGB before any spatial transform.

**Memory pressure is catastrophic above ~4GB simultaneous allocation on a 32GB machine.** The OS starts swapping and the operation takes 10x longer. Batch processing with explicit drop boundaries is mandatory for large frame sets.

**Parallel accumulation requires owned data.** `AppContext` is behind a `Mutex`. `&mut AppContext` cannot be borrowed inside Rayon closures. Extract all needed data into owned types before entering parallel sections.

**f64 for pixel accumulation.** f32 loses precision over 128+ frames. Always accumulate in f64 and convert to f32 at the end.

**Numerical stability in least-squares.** Star centroids on a 3008×3008 image are large numbers (0–3008). Least-squares matrix operations on these coordinates are poorly conditioned. Either normalize coordinates to image center first, or (as currently implemented) skip refinement and return the voted result directly.

**The `fft_align.rs` downsampling is not optional.** Full-resolution FFT on 3008×3008 takes minutes per frame. The downsampling to ~1024px is what makes the pipeline practical.

**Test multi-night datasets early.** Single-night sessions hide polar alignment drift between sessions. The `DATE-OBS` gap detection was added specifically because two-night datasets produced elongated stars even after correct meridian flip handling.

---

*Document version: 5 — June 2026*
