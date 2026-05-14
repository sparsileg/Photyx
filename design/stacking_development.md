# Photyx — Quick Astrophotography Stacking (Preview Pipeline)

**Version:** 4 **Date:** 13 May 2026 **Status:** Design Complete — Ready for Implementation

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
- The stacked result is held in a transient `ImageBuffer` slot in `AppContext` with no source file path
- The user explicitly writes the result using `WriteXISF`, `WriteFIT`, or `WriteTIFF` with a destination path — there is no automatic write
- When writing, Photyx auto-generates a suggested filename (see §3.10) that the user may accept or change
- A `ClearStack` pcode command discards the transient stacked result and returns the viewer to the normal session image

### 3.3 Stack Result Identification

The viewer displays a clear label overlay on the stacked result identifying it as a stack — not a loaded frame. The label includes the frame count and timestamp, e.g.:

```
STACKED RESULT — 62 frames — 2026-04-27 06:42
```

This overlay is distinct from the normal filename overlay and uses a visually differentiated style (e.g. different color or border) to prevent confusion with regular frame display.

### 3.4 Filter Validation

Before stacking begins, Photyx reads the FILTER keyword from all loaded frames and validates that only one filter type is present in the session.

- The reference frame's FILTER keyword is treated as the canonical filter for the stack

- Any frame whose FILTER keyword does not match the reference frame's FILTER is excluded from the stack

- Each exclusion is reported to the pcode console and written to the stack log file:
  
  ```
  Filter mismatch: frame 14 (OIII) excluded — stack filter is Ha
  ```

- Stacking proceeds with the matching frames only — this is not a hard stop

- If FILTER keywords are absent from all frames, no filter validation is performed and stacking proceeds normally

### 3.5 Pipeline

#### Stage 1 — Reference Frame Selection

- If `AnalyzeFrames` results are present in `AppContext`, use the cached FWHM and eccentricity metrics
- If no analysis results exist, recompute FWHM and eccentricity for all frames before proceeding
- Reference frame = the PASS frame (or any frame if no analysis has been run) with the lowest FWHM; ties broken by eccentricity (lowest wins)
- Filter validation (§3.4) is performed at this stage using the reference frame's FILTER keyword

#### Stage 2 — Load and Normalize

- All frames passing filter validation are used as input
- Pixel data is converted to `f32` (already the internal working format — no conversion layer required)
- Each frame is normalized by dividing by its **sigma-clipped sky background level**, computed using the existing `background.rs` module
  - Raw median is **not** used — it is unreliable when significant nebulosity or extended objects are present in the field
  - `background.rs` already handles this correctly via sigma-clipped estimation

Calibration frames (bias, dark, flat) are skipped in the initial release. See §5.1 for future calibration support.

#### Stage 3 — Alignment (Registration)

**Method: FFT Phase Correlation**

- Computes translation-only alignment between each frame and the reference frame
- FFT phase correlation is fast and robust under normal tracking conditions
- Sub-pixel translation is supported

**Alignment Validation (mandatory)**

FFT phase correlation can fail silently when a frame contains a satellite trail, cosmic ray hit, or significant cloud gradient — the cross-correlation peak becomes corrupted and yields a plausible but incorrect translation. For a diagnostic tool specifically designed to detect bad frames, silent misalignment is the worst possible failure mode.

After computing the FFT translation for a frame, confirm it by verifying that a sample of bright stars (detected by the existing star detector in `analysis/stars.rs`) land within an acceptable pixel tolerance of their predicted positions. If validation fails:

1. Flag the frame as **alignment-failed**
2. Skip it from the stack
3. Report the failure to the pcode console and stack log file

This reuses the existing star detection infrastructure and costs negligible additional time.

**Translation Only**

Rotation and scale correction are **not** implemented in the initial version. This is appropriate for well-tracked sessions. Rotation/scale alignment is a future enhancement (see §8).

#### Stage 4 — Stacking

- **Method:** Sigma clipping (default). The stacking method is exposed as a select control in the UI — additional methods (e.g. simple average, median) may be added in the future
- Frames that failed filter validation or alignment validation are excluded from the sum
- The sigma-clipped mean is computed per pixel across all contributing frames

#### Stage 5 — Debayer (Bayer input only)

- If the input frames have `ColorSpace::Bayer`, the stacked mono result is debayered
  using the existing `DebayerImage` infrastructure (Bilinear method by default)
- The debayered output is RGB; the `ImageBuffer` color space is updated accordingly
- Mono input frames produce a mono stack result — no debayering is performed

#### Stage 6 — Stretch for Display

- Apply Auto-STF using the existing `AutoStretch` plugin
- The existing implementation is PixInsight-compatible and handles both mono and RGB
- No new stretch code is required

### 3.6 Progress Reporting

During stacking, per-frame progress is reported to the pcode console, the stack log file, and the status bar:

```
Stack filter: Ha (reference frame)
Filter mismatch: frame 14 (OIII) excluded
Stacking frame 12 / 61 (20%)…
Alignment failed: frame 7 — skipped
Stacking complete — 59 / 61 frames stacked
```

Percentage complete is computed as `(frames processed / total frames) * 100`, updated after each frame. The status bar uses `notifications.running()` during the operation and `notifications.success()` on completion.

### 3.7 Stack Quality Score

On completion, a stack quality score is computed and reported to the console and stack log file:

- **SNR improvement estimate** — theoretical SNR gain vs a single frame (`sqrt(N)` for N frames, where N is the number of frames actually stacked, adjusted for rejected frames)
- **Alignment success rate** — percentage of frames successfully aligned
- **Background uniformity** — variance of per-frame background levels after normalization

Reported as a structured summary block in the console output:

```
Stack Quality Summary:
  Frames stacked:        59 / 61
  SNR improvement:       ~7.7x (vs single frame)
  Alignment success:     96.7%
  Background uniformity: good
```

The UI placement for a graphical quality display is deferred to a future design pass.

### 3.8 Per-Frame Contribution Metrics

The stacking run produces a per-frame contribution summary stored in a dedicated struct in `AppContext` (separate from `AnalysisResult`). This is intended for debugging and algorithm development.

**Metrics reported per frame:**

| Metric              | Description                                                 |
| ------------------- | ----------------------------------------------------------- |
| Frame index         | Zero-based index in the session file list                   |
| Filename            | Source file path                                            |
| Filter              | FILTER keyword value (if present)                           |
| Included            | Whether the frame was included in the final stack           |
| Exclusion reason    | `filter_mismatch`, `alignment_failed`, or blank if included |
| Background level    | Sigma-clipped background estimate before normalization      |
| FFT translation     | Computed X/Y pixel offset from reference frame              |
| Alignment validated | Whether star position check passed                          |
| FWHM                | Per-frame FWHM (from cached analysis or recomputed)         |
| Eccentricity        | Per-frame eccentricity (from cached analysis or recomputed) |

**Storage:** Separate struct in `AppContext`; cleared when `ClearStack` is called or a new stack is run.

**Viewer component:** A new viewer-region component (`StackingResults`) displays the per-frame contribution table, following the same viewer-region pattern as `AnalysisResults`. Accessible via the View menu or a toolbar button after stacking completes.

**Export:** The stack log file (see §3.9) contains the full per-frame contribution table in addition to progress and quality summary output.

### 3.9 Stack Log File

All stacking output — progress, filter exclusions, alignment failures, quality summary, and per-frame contribution table — is written to a dedicated stack log file.

- Named: `photyx_stack_<timestamp>.log`
- Written to the same logs directory used by the application logger
- Accessible via the existing Log Viewer

### 3.10 Suggested Output Filename

When the user invokes `WriteXISF` (or `WriteFIT` / `WriteTIFF`) on a stacked result, Photyx generates a suggested filename using the following convention:

```
Photyx_stack_<target>_<filter>_<integration_seconds>s_<timestamp>.xisf
```

- `<target>` — OBJECT keyword from the reference frame; `unknown` if absent
- `<filter>` — FILTER keyword from the reference frame; `nofilter` if absent
- `<integration_seconds>` — sum of EXPTIME keyword values across all stacked frames, rounded to nearest integer
- `<timestamp>` — UTC timestamp at stack completion, formatted `YYYYMMDD_HHMMSS`

Example: `Photyx_stack_M31_Ha_18300s_20260427_064215.xisf`

The suggested name is presented to the user and may be changed before writing.

### 3.11 Pipeline Architecture

```
notifications.running("Stacking frames…")

# Stage 1 — Reference frame selection
if analysis results exist in AppContext:
    use cached FWHM and eccentricity
else:
    recompute FWHM and eccentricity for all frames

reference_frame = frame with lowest FWHM among PASS frames (eccentricity as tiebreaker)
validate FILTER keywords against reference frame FILTER; log and exclude mismatches

# Stages 2–4 — Normalize, align, stack
for each frame passing filter validation (i of N):
    convert to f32 (no-op — already internal format)
    normalize by sigma-clipped background (background.rs)
    compute FFT translation vs. reference frame
    validate translation against star positions (stars.rs)
    if validation passes:
        resample into stack buffer at computed offset
        accumulate into sigma-clip working set
        record per-frame contribution metrics
        report progress: "Stacking frame i / N (pct%)"
    else:
        flag frame as alignment-failed
        record per-frame contribution metrics (exclusion_reason = alignment_failed)
        report to console and log: "Alignment failed: frame i — skipped"

final_image = sigma_clipped_mean(accumulation buffer)

# Stage 5 — Stretch and display
apply AutoStretch
place result in transient ImageBuffer slot in AppContext (no source path)
display stack result label overlay in viewer
display StackingResults viewer component

# Completion
compute stack quality score
report quality summary to console and log
write per-frame contribution table to log
notifications.success("Stack complete — N frames stacked")
```

### 3.12 Integration with Photyx

#### Existing Infrastructure Reused

| Component                                             | Reuse                                            |
| ----------------------------------------------------- | ------------------------------------------------ |
| `analysis/stars.rs`                                   | Star detection for alignment validation          |
| `analysis/fwhm.rs`, `analysis/eccentricity.rs`        | Reference frame selection when no cached results |
| `analysis/background.rs`                              | Sigma-clipped background for normalization       |
| `AutoStretch` plugin                                  | Stretch for display                              |
| `AppContext.image_buffers`                            | Source frames                                    |
| pcode console + `consolePipe`                         | Progress and error reporting                     |
| `notifications.running()` / `notifications.success()` | Status bar pulse and completion                  |
| Log Viewer                                            | Stack log file access                            |

#### New Infrastructure Required

| Component                          | Notes                                                                                |
| ---------------------------------- | ------------------------------------------------------------------------------------ |
| `StackFrames` plugin               | Built-in native plugin; wraps the stacking pipeline                                  |
| `ClearStack` plugin                | Built-in native plugin; discards transient stack buffer and contribution metrics     |
| FFT phase correlation              | New Rust implementation; candidate crate: `rustfft`                                  |
| Transient `ImageBuffer` slot       | Holds stacked result in `AppContext` without a source file path                      |
| Alignment validation logic         | Thin wrapper coordinating FFT result + star position check                           |
| Sigma clipping accumulator         | Per-pixel sigma-clipped mean across frame set                                        |
| Stack quality computation          | Composite metric from SNR estimate, alignment rate, background uniformity            |
| Per-frame contribution struct      | Separate from `AnalysisResult`; stored in `AppContext`                               |
| `StackingResults` viewer component | New viewer-region component; displays per-frame contribution table                   |
| Stack log file writer              | Writes progress, exclusions, quality summary, and contribution table                 |
| Suggested filename generator       | Reads OBJECT, FILTER, EXPTIME from reference frame; constructs suggested output name |
| Stack result viewer label          | Overlay distinct from normal filename overlay                                        |
| Stacking method select control     | UI control to choose stack method; sigma clipping is default                         |

#### Plugin Classification

`StackFrames` and `ClearStack` are **built-in native plugins** per the Photyx plugin designation model.

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

Or using the common parent of the current session:

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

- Live stacking runs in the background while the user continues to use Photyx normally (if possible)
- The viewer updates on every new frame arrival — the stacked result improves visibly in real time
- The stacked result is written to disk periodically (every N frames, user-configurable) using the same naming convention as batch stacking (§3.10)
- The user can stop live stacking at any time without losing the accumulated result
- The index of the last frame successfully included in the stack is stored as a keyword in the output file, enabling resume if stacking is interrupted

### 4.4 Required Infrastructure

| Component                      | Notes                                                                                          |
| ------------------------------ | ---------------------------------------------------------------------------------------------- |
| `notify` crate                 | File system watching                                                                           |
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
    validate FILTER keyword against reference frame FILTER; skip and log if mismatch
    normalize by sigma-clipped background (background.rs)
    compute FFT translation vs. reference frame
    validate translation
    if valid:
        accumulate into stack buffer
        update display (ui.requestFrameRefresh())
        report to console: "Live stack: frame N accumulated"
    else:
        report alignment failure to console and log
        skip frame
```

### 4.6 Debugging and Development Value

Live stacking provides a continuous real-world test of the normalization, alignment, and accumulation pipeline under production conditions, with immediate visual feedback when pipeline parameters are tuned.

---

## 5. What Is Explicitly Out of Scope — First Release (Batch Only)

- Cosmetic correction
- Distortion correction
- Background extraction
- Photometric calibration
- Rotation or scale alignment
- Rejection map (future)
- Calibration frame support (bias, dark, flat) — see §5.1

### 5.1 Calibration Frame Support (Future)

Dark, flat, and bias frame subtraction are deferred to a future release. The pipeline architecture does not preclude adding a calibration stage between normalization and alignment.

---

## 6. What Is Explicitly Out of Scope — Second Release (Live Stack)

- Cosmetic correction
- Rotation or scale alignment
- Distortion correction
- Background extraction
- Photometric calibration
- Rejection map (future)

---

## 7. Implementation Plan

### 7.1 Phase A — Foundation (Batch Stacking Core)

All items required before any stacking result can be produced.

| Task                                          | Notes                                                                           |
| --------------------------------------------- | ------------------------------------------------------------------------------- |
| Transient `ImageBuffer` slot in `AppContext`  | Holds stack result without source path; cleared by `ClearStack`                 |
| Per-frame contribution struct in `AppContext` | Separate from `AnalysisResult`                                                  |
| Filter validation logic                       | Read FILTER from all frames; exclude mismatches; log exclusions                 |
| Reference frame selection                     | Use cached analysis results if present; otherwise recompute FWHM + eccentricity |
| Load + normalize via `background.rs`          | Convert to f32; normalize by sigma-clipped background                           |
| FFT phase correlation (`rustfft`)             | Translation-only; sub-pixel; candidate crate: `rustfft`                         |
| Alignment validation via `stars.rs`           | Star position check after FFT; flag and skip failures                           |
| Sigma clipping accumulator                    | Per-pixel sigma-clipped mean across frame set                                   |
| `StackFrames` plugin                          | Wraps the full pipeline; registers as pcode command                             |
| `ClearStack` plugin                           | Discards transient stack buffer and contribution metrics                        |

### 7.2 Phase B — Display and Reporting

All items required to surface the result to the user.

| Task                                         | Notes                                                                               |
| -------------------------------------------- | ----------------------------------------------------------------------------------- |
| Debayer stack result for Bayer input         | Apply bilinear debayer to stacked mono Bayer result; output becomes RGB             |
| AutoStretch on stack result                  |                                                                                     |
| Stack result viewer label overlay            | Distinct style from normal filename overlay; shows frame count and timestamp        |
| Stacking method select control               | UI control; sigma clipping default                                                  |
| Progress reporting to console and status bar | Per-frame progress; `notifications.running()` / `notifications.success()`           |
| Stack quality score computation              | SNR estimate, alignment rate, background uniformity                                 |
| Stack quality summary to console             | Structured block at completion                                                      |
| Stack log file writer                        | Progress, exclusions, quality summary, per-frame contribution table                 |
| `StackingResults` viewer-region component    | Per-frame contribution table; new viewer-region following `AnalysisResults` pattern |
| CSS file for `StackingResults`               | `static/css/stackingresults.css`                                                    |

### 7.3 Phase C — Output

| Task                                                                       | Notes                                                                                   |
| -------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Suggested filename generator                                               | Reads OBJECT, FILTER, EXPTIME from reference frame; constructs suggested name per §3.10 |
| `WriteXISF` / `WriteFIT` / `WriteTIFF` operating on transient stack buffer | Existing write plugins need to detect and handle the transient slot                     |
| Menu wiring: Analyze → Stack Frames                                        | Dispatches `StackFrames` via `consolePipe`                                              |
| Menu wiring: Analyze → Clear Stack                                         | Dispatches `ClearStack` via `consolePipe`                                               |

### 7.4 Phase D — Live Stacking (Second Release)

Delivered as a complete unit. Do not begin until Phase A–C are complete and stable.

| Task                                                  | Notes                                                       |
| ----------------------------------------------------- | ----------------------------------------------------------- |
| `notify` crate integration + file watcher             | File system watching for new frames                         |
| Background accumulation thread + channel architecture | Independent of UI thread                                    |
| Incremental display update on each new frame          | `ui.requestFrameRefresh()` on each accumulated frame        |
| `LiveStack` and `StopLiveStack` plugins               | Start/stop watcher and accumulation thread                  |
| Periodic auto-write                                   | Every N frames; user-configurable                           |
| Resume keyword                                        | Last-included frame index written as keyword to output file |
| Menu wiring: Analyze → Live Stack / Stop Live Stack   |                                                             |

---

## 8. Estimated Development Effort

### 8.1 Batch Stacking (Phases A–C)

| Phase | Component                                   | Estimated Effort |
| ----- | ------------------------------------------- | ---------------- |
| A     | Transient ImageBuffer + contribution struct | 1–2 hours        |
| A     | Filter validation                           | 1 hour           |
| A     | Reference frame selection                   | 1–2 hours        |
| A     | Load + normalize                            | 2–4 hours        |
| A     | FFT phase correlation                       | 4–6 hours        |
| A     | Alignment validation                        | 2–3 hours        |
| A     | Sigma clipping accumulator                  | 3–4 hours        |
| A     | StackFrames + ClearStack plugins            | 3–4 hours        |
| B     | Display, progress, quality score, log       | 4–6 hours        |
| B     | StackingResults viewer component            | 3–5 hours        |
| C     | Suggested filename + write integration      | 2–3 hours        |
| C     | Menu wiring                                 | 1 hour           |
|       | **Total**                                   | **~27–41 hours** |

### 8.2 Live Stacking (Phase D)

| Component                                             | Estimated Effort |
| ----------------------------------------------------- | ---------------- |
| `notify` crate integration + file watcher             | 3–5 hours        |
| Background accumulation thread + channel architecture | 4–6 hours        |
| Incremental display update                            | 2–3 hours        |
| `LiveStack` and `StopLiveStack` plugins               | 2–3 hours        |
| Periodic auto-write + resume keyword                  | 2–3 hours        |
| Menu wiring                                           | 1 hour           |
| **Total**                                             | **~14–21 hours** |

---

## 9. Future Enhancements (Beyond Second Release)

| Enhancement                            | Notes                                                                      |
| -------------------------------------- | -------------------------------------------------------------------------- |
| Rotation and scale alignment           | Extend FFT/star-match to solve similarity transform                        |
| Median stacking                        | More robust; slower                                                        |
| Calibration frame support              | Bias, dark, flat subtraction                                               |
| Rejection map                          | Visual overlay showing which pixels were rejected; saved as viewable image |
| Graphical stack quality display        | UI component for stack quality score; placement TBD                        |
| Drizzle                                | Sub-pixel integration for oversampled data                                 |
| Background normalization across frames | Useful for sessions with variable sky background                           |

---

*Previous version: 3* *Next review: Prior to implementation of Phase A*
