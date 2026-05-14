# Photyx — Quick Astrophotography Stacking (Preview Pipeline)

**Version:** 3
**Date:** 27 April 2026
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

Two distinct stacking modes are defined. They share the same core pipeline but differ in how frames are delivered. They are explicitly separated — do not conflate them during design or implementation.

| Mode                 | Description                                                                                                      | Release        |
| -------------------- | ---------------------------------------------------------------------------------------------------------------- | -------------- |
| **Batch diagnostic** | Stack all currently loaded frames on demand                                                                      | First release  |
| **Live stack**       | Incrementally stack frames as they arrive from the file system, with real-time display update on every new frame | Second release |

Live stacking requires three components that must ship together: file system watching (`notify` crate), a background accumulation thread, and incremental display update on every new frame. Implementing any one without the others produces no useful result. Live stacking is deferred to the second release in its entirety.

The remainder of this document covers both modes, with each addressed in its own section.

---

## 3. Batch Diagnostic Stacking (First Release)

### 3.1 Triggering and UI Surface

#### pcode Command

```
StackFrames
```

No required arguments for the initial implementation. Optional arguments (e.g. alignment method, stack method) may be added as the feature matures.

#### UI Entry Points

- **Menu:** Analyze → Stack Frames
- **Quick Launch:** A "Stack" button may be pinned by the user in the standard way

### 3.2 Session Behavior

- The stacked result appears in the viewer as a standalone image
- It does **not** replace or disturb the currently loaded file list or session
- The stacked result is automatically written to the active directory as an XISF file using the naming convention: `Stack_[directory_name]_[N]frames_[timestamp].xisf`
- No additional user action is required to save — the write is automatic on stack completion
- The user may also explicitly write in other formats using `WriteFIT` or `WriteTIFF` if desired
- A `ClearStack` pcode command discards the transient stacked result and returns the viewer to the normal session image

### 3.3 Stack Result Identification

The viewer displays a clear label overlay on the stacked result identifying it as a stack — not a loaded frame. The label includes the frame count and timestamp, e.g.:

```
STACKED RESULT — 62 frames — 2026-04-27 06:42
```

This overlay is distinct from the normal filename overlay and uses a visually differentiated style (e.g. different color or border) to prevent confusion with regular frame display.

### 3.4 Pipeline

#### Stage 1 — Load and Normalize

- All frames currently loaded in the session are used as input
- Pixel data is converted to `f32` (already the internal working format — no conversion layer required)
- Each frame is normalized by dividing by its **sigma-clipped sky background level**, computed using the existing `background.rs` module
  - Raw median is **not** used — it is unreliable when significant nebulosity or extended objects are present in the field
  - `background.rs` already handles this correctly via sigma-clipped estimation

Calibration frames (bias, dark, flat) are skipped. This is intentional — speed is the priority.

#### Stage 2 — Alignment (Registration)

**Method: FFT Phase Correlation**

- Computes translation-only alignment between each frame and the reference frame
- The reference frame is the first frame in the loaded list (index 0)
- FFT phase correlation is fast and robust under normal tracking conditions
- Sub-pixel translation is supported

**Alignment Validation (mandatory)**

FFT phase correlation can fail silently when a frame contains a satellite trail, cosmic ray hit, or significant cloud gradient — the cross-correlation peak becomes corrupted and yields a plausible but incorrect translation. For a diagnostic tool specifically designed to detect bad frames, silent misalignment is the worst possible failure mode.

After computing the FFT translation for a frame, confirm it by verifying that a sample of bright stars (detected by the existing star detector in `analysis/stars.rs`) land within an acceptable pixel tolerance of their predicted positions. If validation fails:

1. Flag the frame as **alignment-failed**
2. Either skip it from the stack, or fall back to unaligned averaging for that frame
3. Report the failure to the pcode console

This reuses the existing star detection infrastructure and costs negligible additional time.

**Translation Only**

Rotation and scale correction are **not** implemented in the initial version. This is appropriate for well-tracked sessions. Rotation/scale alignment is a future enhancement (see §8).

#### Stage 3 — Stacking

- **Method:** Simple average (sum / count) after normalization and alignment
- Frames that failed alignment validation are excluded from the sum
- This is sufficient to reveal tracking errors, focus problems, cloud interference, and framing issues

#### Stage 4 — Stretch for Display

- Apply Auto-STF using the existing `AutoStretch` plugin
- The existing implementation is PixInsight-compatible and handles both mono and RGB
- No new stretch code is required

### 3.5 Progress Reporting

During stacking, per-frame progress is reported to the pcode console and the status bar:

```
Stacking frame 12 / 62 (19%)…
Alignment failed: frame 7 — skipped
Stacking complete — 61 / 62 frames stacked
```

Percentage complete is computed as `(frames processed / total frames) * 100`, updated after each frame. The status bar uses `notifications.running()` during the operation and `notifications.success()` on completion.

### 3.6 Stack Quality Score

On completion, a stack quality score is computed and reported to the console:

- **SNR improvement estimate** — theoretical SNR gain vs a single frame (`sqrt(N)` for N frames of equal quality, adjusted for rejected frames)
- **Alignment success rate** — percentage of frames successfully aligned
- **Background uniformity** — variance of per-frame background levels after normalization

Reported as a structured summary block in the console output:

```
Stack Quality Summary:
  Frames stacked:        61 / 62
  SNR improvement:       ~7.8x (vs single frame)
  Alignment success:     98.4%
  Background uniformity: good
```

The UI placement for a graphical quality display is deferred to a future design pass.

### 3.7 Pipeline Architecture

```
notifications.running("Stacking frames…")

for each loaded frame (i of N):
    convert to f32 (no-op — already internal format)
    normalize by sigma-clipped background (background.rs)
    compute FFT translation vs. reference frame
    validate translation against star positions (stars.rs)
    if validation passes:
        resample into stack buffer at computed offset
        accumulate (sum + count)
        report progress: "Stacking frame i / N (pct%)"
    else:
        flag frame as alignment-failed
        report to console: "Alignment failed: frame i — skipped"

final_image = sum / count
apply AutoStretch
compute stack quality score
write result to active directory as Stack_[dir]_[N]frames_[timestamp].xisf
place result in viewer as transient ImageBuffer
display stack result label overlay
notifications.success("Stack complete — N frames")
report quality summary to console
```

### 3.8 Integration with Photyx

#### Existing Infrastructure Reused

| Component                        | Reuse                                      |
| -------------------------------- | ------------------------------------------ |
| `analysis/stars.rs`              | Star detection for alignment validation    |
| `analysis/background.rs`         | Sigma-clipped background for normalization |
| `AutoStretch` plugin             | Stretch for display                        |
| `AppContext.image_buffers`       | Source frames                              |
| `get_current_frame` display path | Render stacked result                      |
| pcode console + `consolePipe`    | Progress and error reporting               |
| `notifications.running()`        | Status bar pulse during operation          |

#### New Infrastructure Required

| Component                    | Notes                                                                         |
| ---------------------------- | ----------------------------------------------------------------------------- |
| `StackFrames` plugin         | Built-in native plugin; wraps the stacking pipeline                           |
| `ClearStack` plugin          | Built-in native plugin; discards transient stack buffer                       |
| FFT phase correlation        | New Rust implementation; candidate crate: `rustfft`                           |
| Transient `ImageBuffer` slot | Mechanism to hold a stacked result in `AppContext` without a source file path |
| Alignment validation logic   | Thin wrapper coordinating FFT result + star position check                    |
| Stack quality computation    | Composite metric from SNR estimate, alignment rate, background uniformity     |
| Stack result XISF writer     | Auto-write to active directory on completion                                  |
| Stack result viewer label    | Overlay distinct from normal filename overlay                                 |

#### Plugin Classification

`StackFrames` and `ClearStack` are **built-in native plugins** per the Photyx plugin designation model. They operate on `AppContext` data and produce results that enter the display pipeline — they must be native, not WASM.

### 3.9 Per-Frame Contribution Metrics

The stacking run should produce a per-frame quality summary showing how each frame contributed to (or detracted from) the final stack. This mirrors the `AnalyzeFrames` / Analysis Graph pattern.

**To Do — Design Required before implementation:**

- What metrics are reported per frame? Candidates: normalized background level, alignment offset (dx, dy), validation pass/fail, alignment confidence score, contribution weight
- Are these stored in `AppContext` alongside `AnalysisResult`, or in a separate struct?
- Is a new viewer-region component required, or does the Analysis Graph component extend to support stacking metrics?
- Can contribution metrics be exported to the console or written to a log file?

---

## 4. Live Stacking (Second Release)

### 4.1 Overview

Live stacking watches a directory for new image files arriving from an active capture session and incrementally accumulates them into a running stack. The stacked result is updated and displayed in the viewer on every new frame arrival.

Live stacking is a complete feature that ships as a unit. Its three required components — file watching, background accumulation, and incremental display update — must all be implemented together. Shipping any subset produces no useful result.

### 4.2 Triggering

The user points Photyx at a directory that a capture application (e.g. N.I.N.A., SGP, ASIAIR) is actively writing new frames to. Photyx watches the directory for new files matching the active format filter.

#### pcode Command

```
LiveStack path="D:/Capture/M31"
```

Or using the active directory:

```
LiveStack
```

#### UI Entry Points

- **Menu:** Analyze → Live Stack
- **Quick Launch:** A "Live Stack" button may be pinned by the user

#### Stopping

```
StopLiveStack
```

Or closing the stack result viewer via the Close / `ClearStack` command.

### 4.3 Session Behavior

- Live stacking runs in the background while the user continues to use Photyx normally
- Each new frame is normalized, aligned, and accumulated automatically
- The viewer updates on every new frame arrival — the stacked result improves visibly in real time
- The stacked result is written to disk periodically (every N frames, user-configurable) using the same naming convention as batch stacking
- The user can stop live stacking at any time without losing the accumulated result

### 4.4 Required Infrastructure

| Component                      | Notes                                                                                          |
| ------------------------------ | ---------------------------------------------------------------------------------------------- |
| `notify` crate                 | File system watching — already in the planned crate list (spec §4.3)                           |
| Background accumulation thread | Runs independently of the UI thread; communicates via channels                                 |
| Incremental display update     | Triggers a viewer refresh on every new frame; uses existing `ui.requestFrameRefresh()` pattern |
| `LiveStack` plugin             | Built-in native plugin; starts the file watcher and accumulation thread                        |
| `StopLiveStack` plugin         | Built-in native plugin; stops the watcher and accumulation thread gracefully                   |

### 4.5 Pipeline

The live stack pipeline is identical to the batch pipeline per frame, executed incrementally:

```
on new file detected in watch directory:
    read frame (appropriate reader plugin)
    convert to f32
    normalize by sigma-clipped background (background.rs)
    compute FFT translation vs. reference frame
    validate translation
    if valid:
        accumulate into stack buffer
        update display (ui.requestFrameRefresh())
        report to console: "Live stack: frame N accumulated"
    else:
        report alignment failure to console
        skip frame
```

### 4.6 Debugging and Development Value

Live stacking is particularly valuable during development and integration of advanced features. It provides:

- A continuous, real-world test of the normalization, alignment, and accumulation pipeline under production conditions
- Immediate visual feedback when pipeline parameters are tuned (e.g. alignment tolerance, background estimation)
- A realistic stress test of the background thread / display update cycle
- A natural integration test for the `notify` crate file watching infrastructure

---

## 5. What Is Explicitly Out of Scope — First Release (Batch Only)

- Dark, flat, bias calibration
- Cosmetic correction
- Drizzle
- Distortion correction
- Background extraction
- Photometric calibration
- Rotation or scale alignment
- Live / incremental stacking (second release)
- Rejection map (future)
- Incremental live display update (second release)

---

## 6. What Is Explicitly Out of Scope — Second Release (Live Stack)

- Rotation or scale alignment
- Dark, flat, bias calibration
- Drizzle
- Distortion correction
- Background extraction
- Photometric calibration
- Rejection map (future)

---

## 7. Expected Development Effort

### 7.1 Batch Stacking (First Release)

Estimates assume the Photyx plugin and display infrastructure is already in place (it is).

| Component                                     | Estimated Effort |
| --------------------------------------------- | ---------------- |
| Load + normalize (via `background.rs`)        | 2–4 hours        |
| FFT phase correlation (`rustfft`)             | 4–6 hours        |
| Alignment validation (via `stars.rs`)         | 2–3 hours        |
| Average stacking                              | 2–3 hours        |
| `StackFrames` plugin integration              | 4–6 hours        |
| `ClearStack` plugin                           | 1 hour           |
| Progress reporting                            | 1–2 hours        |
| Stack quality score                           | 2–3 hours        |
| Stack result naming, auto-write, viewer label | 2–3 hours        |
| **Total**                                     | **~20–31 hours** |

### 7.2 Live Stacking (Second Release)

| Component                                             | Estimated Effort |
| ----------------------------------------------------- | ---------------- |
| `notify` crate integration + file watcher             | 3–5 hours        |
| Background accumulation thread + channel architecture | 4–6 hours        |
| Incremental display update on each new frame          | 2–3 hours        |
| `LiveStack` and `StopLiveStack` plugins               | 2–3 hours        |
| Periodic auto-write                                   | 1–2 hours        |
| **Total**                                             | **~12–19 hours** |

---

## 8. Future Enhancements (Beyond Second Release)

| Enhancement                            | Notes                                                                                                   |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Rotation and scale alignment           | Extend FFT/star-match to solve similarity transform                                                     |
| Median stacking                        | More robust; slower                                                                                     |
| Sigma-clipped mean                     | Better rejection of outliers; more complex                                                              |
| Star rejection                         | Per-pixel outlier rejection based on star positions                                                     |
| Background normalization across frames | Useful for sessions with variable sky background                                                        |
| Calibration frame support              | Bias, dark, flat subtraction                                                                            |
| Rejection map                          | Visual overlay showing which pixels were rejected across frames; saved as an image viewable by the user |
| Per-frame contribution display         | Analysis Graph-style visualization of per-frame stacking metrics (see §3.9)                             |
| Graphical stack quality display        | UI component for stack quality score; placement TBD                                                     |

---

*Previous version: 2*
*Next review: Prior to implementation of batch stacking*
