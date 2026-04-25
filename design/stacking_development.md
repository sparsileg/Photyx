# Photyx — Quick Astrophotography Stacking (Preview Pipeline)

**Version:** 2
**Date:** 25 April 2026 3:23pm
**Status:** Theory and Top-level Design

---

## 1. Goal

Build a fast, lightweight stacking pipeline to answer a single question:

> "Did my imaging session go off the rails?"

This is **not** a production-quality stacker. It is a **diagnostic preview tool** to quickly assess:

- Framing
- Focus
- Tracking quality
- Cloud interference
- Major gradients or issues

---

## 2. Scope

Two distinct stacking modes are defined. They share the same core pipeline but differ in how frames are delivered.

| Mode | Description | Status |
|---|---|---|
| **Batch diagnostic** | Stack all currently loaded frames on demand | In scope — implement first |
| **Live stack** | Incrementally stack frames as they arrive from the file system | Deferred — separate feature; requires `notify`-based file watching and a background accumulation thread |

These modes must not be conflated during design or implementation. The remainder of this document addresses **batch diagnostic stacking only**.

---

## 3. Triggering and UI Surface

### 3.1 pcode Command

The stacking pipeline is triggered by a `StackFrames` pcode command. This follows the standard built-in native plugin pattern.

```
StackFrames
```

No required arguments for the initial implementation. Optional arguments (e.g. alignment method, stack method) may be added as the feature matures.

### 3.2 UI Entry Points

- **Menu:** Analyze → Stack Frames
- **Quick Launch:** A "Stack" button may be pinned by the user in the standard way

### 3.3 Session Behavior

- The stacked result appears in the viewer as a standalone image
- It does **not** replace or disturb the currently loaded file list or session
- The stacked result has no source file path — it is a transient in-memory `ImageBuffer`
- Writing to disk requires an explicit command from the user: `WriteXISF`, `WriteFIT`, or `WriteTIFF`
- No automatic write occurs under any circumstance

---

## 4. Pipeline

### Stage 1 — Load and Normalize

- All frames currently loaded in the session are used as input
- Pixel data is converted to `f32` (already the internal working format — no conversion layer required)
- Each frame is normalized by dividing by its **sigma-clipped sky background level**, computed using the existing `background.rs` module
  - Raw median is **not** used — it is unreliable when significant nebulosity or extended objects are present in the field
  - `background.rs` already handles this correctly via sigma-clipped estimation

Calibration frames (bias, dark, flat) are skipped. This is intentional — speed is the priority.

### Stage 2 — Alignment (Registration)

#### Method: FFT Phase Correlation

- Computes translation-only alignment between each frame and the reference frame
- The reference frame is the first frame in the loaded list (index 0)
- FFT phase correlation is fast and robust under normal tracking conditions
- Sub-pixel translation is supported

#### Alignment Validation

FFT phase correlation can fail silently when a frame contains a satellite trail, cosmic ray hit, or significant cloud gradient — the cross-correlation peak becomes corrupted and yields a plausible but incorrect translation. For a diagnostic tool specifically designed to detect bad frames, silent misalignment is the worst possible failure mode.

**Validation step (mandatory):** After computing the FFT translation for a frame, confirm it by verifying that a sample of bright stars (already detected by the existing star detector in `analysis/stars.rs`) land within an acceptable pixel tolerance of their predicted positions. If validation fails:

1. Flag the frame as **alignment-failed**
2. Either skip it from the stack, or fall back to unaligned averaging for that frame
3. Report the failure to the pcode console

This reuses the existing star detection infrastructure and costs negligible additional time.

#### Translation Only

Rotation and scale correction are **not** implemented in the initial version. This is appropriate for well-tracked sessions. Rotation/scale alignment is a future enhancement (see §8).

### Stage 3 — Stacking

- **Method:** Simple average (sum / count) after normalization and alignment
- Frames that failed alignment validation are excluded from the sum
- This is sufficient to reveal tracking errors, focus problems, cloud interference, and framing issues

More sophisticated methods (median, sigma-clipped mean) are future enhancements (see §8).

### Stage 4 — Stretch for Display

- Apply Auto-STF using the existing `AutoStretch` plugin
- The existing implementation is PixInsight-compatible and handles both mono and RGB
- No new stretch code is required

---

## 5. Pipeline Architecture

```
for each loaded frame:
    convert to f32 (no-op — already internal format)
    normalize by sigma-clipped background (background.rs)
    compute FFT translation vs. reference frame
    validate translation against star positions (stars.rs)
    if validation passes:
        resample into stack buffer at computed offset
        accumulate (sum + count)
    else:
        flag frame as alignment-failed
        report to console

final_image = sum / count
apply AutoStretch
place result in viewer as transient ImageBuffer
```

---

## 6. Integration with Photyx

### 6.1 Existing Infrastructure Reused

| Component | Reuse |
|---|---|
| `analysis/stars.rs` | Star detection for alignment validation |
| `analysis/background.rs` | Sigma-clipped background for normalization |
| `AutoStretch` plugin | Stretch for display |
| `AppContext.image_buffers` | Source frames |
| `get_current_frame` display path | Render stacked result |
| pcode console | Progress and error reporting |

### 6.2 New Infrastructure Required

| Component | Notes |
|---|---|
| `StackFrames` plugin | Built-in native plugin; wraps the stacking pipeline |
| FFT phase correlation | New Rust implementation; candidate crate: `rustfft` |
| Transient `ImageBuffer` slot | Mechanism to hold a stacked result in `AppContext` without a source file path |
| Alignment validation logic | Thin wrapper coordinating FFT result + star position check |

### 6.3 Plugin Classification

`StackFrames` is a **built-in native plugin** per the Photyx plugin designation model. It operates on `AppContext` data and produces a result that enters the display pipeline — it must be native, not WASM.

---

## 7. Per-Frame Contribution Metrics

### 7.1 Intent

The stacking run should produce a per-frame quality summary showing how each frame contributed to (or detracted from) the final stack. This mirrors the `AnalyzeFrames` / Analysis Graph pattern.

### 7.2 To Do — Design Required

The output struct and display surface for per-frame contribution metrics are **not yet designed**. The following questions must be resolved before implementation:

- What metrics are reported per frame? Candidates: normalized background level, alignment offset (dx, dy), validation pass/fail, alignment confidence score, contribution weight
- Are these stored in `AppContext` alongside `AnalysisResult`, or in a separate struct?
- Is a new viewer-region component required (similar to the Analysis Graph), or does the Analysis Graph component extend to support stacking metrics?
- Can contribution metrics be exported to the console or written to a log file?

This is deferred to a design pass before implementation of this feature.

---

## 8. Future Enhancements

| Enhancement | Notes |
|---|---|
| Rotation and scale alignment | Extend FFT/star-match to solve similarity transform |
| Median stacking | More robust; slower |
| Sigma-clipped mean | Better rejection of outliers; more complex |
| Star rejection | Per-pixel outlier rejection based on star positions |
| Background normalization across frames | Useful for sessions with variable sky background |
| Calibration frame support | Bias, dark, flat subtraction |
| Live stack mode | Incremental accumulation as frames arrive; requires `notify` crate file watching and background thread; explicitly separate feature |
| Per-frame contribution display | Analysis Graph-style visualization of per-frame metrics (see §7) |

---

## 9. What Is Explicitly Out of Scope (Batch Diagnostic Version)

- Dark, flat, bias calibration
- Cosmetic correction
- Drizzle
- Distortion correction
- Background extraction
- Photometric calibration
- Rotation or scale alignment (initial version)
- Automatic write to disk
- Live / incremental stacking

---

## 10. Expected Development Effort

Estimates assume the Photyx plugin and display infrastructure is already in place (it is).

| Component | Estimated Effort |
|---|---|
| Load + normalize (via `background.rs`) | 2–4 hours |
| FFT phase correlation | 4–6 hours |
| Alignment validation (via `stars.rs`) | 2–3 hours |
| Average stacking | 2–3 hours |
| `StackFrames` plugin integration | 4–6 hours (wiring into `AppContext`, display pipeline, transient buffer) |
| pcode console reporting | 1–2 hours |
| **Total** | **~15–24 hours** |

---

*Previous version: 1*
*Next review: Prior to implementation*
