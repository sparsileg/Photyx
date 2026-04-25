# Quick Astrophotography Stacking (Preview Pipeline)

**Version:** 1
**Date:** 23 April 2026
**Status:** Theory and Top-level Design


## Goal

Build a fast, lightweight stacking pipeline to answer a single question:

> “Did my imaging session go off the rails?”

This is **not** a production-quality stacker. It is a **diagnostic preview tool** to quickly assess:
- Framing
- Focus
- Tracking quality
- Cloud interference
- Major gradients or issues

---

## Minimum Viable Pipeline

A useful preview stacker can be implemented in four stages:

### 1) Load and Normalize Frames

- Read raw subframes (e.g., FITS, XISF)
- Convert pixel data to a common format (`f32` recommended)
- Normalize each frame:
  - Divide by median or mean pixel value

#### Notes
- This ensures consistent brightness across frames
- Skip calibration (bias/dark/flat) for speed

---

### 2) Star Alignment (Registration)

This is the most complex step, but can be simplified significantly.

#### Option A: Star-Based Alignment
- Detect brightest stars (threshold + local maxima)
- Compute centroids
- Match stars between frames
- Solve transform:
  - Translation (minimum)
  - Optional: rotation and scale

#### Option B: FFT-Based Alignment (Recommended)
- Use phase correlation (FFT)
- Computes translation only
- Very fast and robust

#### Recommendation
If your tracking is reasonably good:
> Use **translation-only alignment via FFT**

---

### 3) Stacking

Keep this simple.

#### Options

- **Average (fastest)**
- Median (more robust, slower)
- Sigma-clipped mean (better, more complex)

#### Recommendation

For preview purposes:
> Use a **simple average stack after normalization**

This is sufficient to reveal:
- Star visibility
- Tracking issues
- Focus problems
- Cloud interference

---

### 4) Stretch for Display

Critical step — without stretching, the image will look blank.

#### Options
- Histogram stretch (black point + midtones)
- Asinh stretch

#### Recommendation
> Apply a simple histogram or asinh stretch

---

## What You Can Skip

For a diagnostic preview, you do NOT need:

- Dark frames
- Flat frames
- Bias frames
- Cosmetic correction
- Drizzle
- Distortion correction
- Background extraction
- Photometric calibration

---

## Expected Development Effort

Assuming you already handle image I/O:

| Component                  | Estimated Time |
|--------------------------|----------------|
| Load + normalize         | 1–2 hours      |
| Star detection           | 2–4 hours      |
| Alignment                | 4–8 hours      |
| Stack + stretch          | 2–3 hours      |

**Total:** ~1 weekend for a working prototype

---

## Practical Design Advice

### Prioritize Speed Over Accuracy
- Downsample frames (2× or 4×) before alignment
- Limit number of detected stars
- Use simple transforms

### Make It Incremental
- Stack frames as they are captured
- Continuously update preview

### Fail Gracefully
If alignment fails:
- Fall back to unaligned averaging
- Still useful for detecting major issues

---

## Suggested Pipeline Architecture

for each subframe:
load → normalize
align to reference (FFT or stars)
resample into stack buffer
accumulate (sum + count)

final_image = sum / count
apply stretch
display

for each subframe:
load → normalize
align to reference (FFT or stars)
resample into stack buffer
accumulate (sum + count)

final_image = sum / count
apply stretch
display


---

## Key Insight

You are not optimizing for signal-to-noise ratio or scientific accuracy.

You are optimizing for:

> **Fast visibility of problems**

Even a noisy or slightly misaligned stack will clearly reveal:
- Clouds (signal disappears)
- Tracking errors (star elongation)
- Focus issues (blurred stars)
- Framing mistakes

---

## Recommended Starting Point

To maximize value with minimal effort:

- FFT-based translation alignment
- Average stacking
- Simple stretch

This delivers:
> **~80% of the usefulness for ~20% of the effort**

---

## Future Enhancements (Optional)

Once the basic system works, you can incrementally add:

- Rotation/scale alignment
- Sigma clipping
- Star rejection
- Background normalization
- Calibration frame support

---

## Summary

A quick stacking tool for astrophotography preview is:

- Feasible in a short time
- Computationally lightweight
- Extremely valuable during acquisition

Focus on:
- Simplicity
- Speed
- Robustness

Avoid premature complexity—this tool is for **diagnostics, not
perfection**.
