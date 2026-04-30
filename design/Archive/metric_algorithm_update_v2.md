# Photyx Metric Algorithm Update — Tech Note v2
## Comparison with PixInsight SubframeSelector and Recommended Changes

---

## Overview

This note documents findings from a direct comparison of Photyx analysis metrics against
PixInsight SubframeSelector (PI) metrics across a real dataset. It summarizes what tracks
well, what diverges, what should be changed, and what should be removed. All recommended
algorithm changes should be implemented together in a single update pass, coordinated with
the switch to moment-based FWHM/eccentricity.

The PI algorithm reference is: "New Image Weighting Algorithms in PixInsight" by Juan
Conejero, Edoardo Luca Radice, Roberto Sartori, and John Pane (Pleiades Astrophoto, 2022).

---

## Workflow Philosophy

Photyx's role in the imaging workflow is to identify and remove **extreme outliers** that
would actively harm the final stacked image. PixInsight's PSFSW weighting system handles
fine-grained quality differentiation — bad frames get low weights, good frames get high
weights. Photyx does not need to replicate this sophistication.

The bias should always be toward **keeping frames**. The cost of a bad frame in a weighted
stack is low; the cost of discarding a good frame is permanent signal loss. Only frames
that are genuinely harmful — severe cloud cover, focus failure, major tracking failure —
should be rejected.

---

## Classification: PASS / REJECT Only

The SUSPECT classification has been removed. It provided no actionable information —
SUSPECT frames were always going to be kept, making the classification equivalent to PASS.
SUSPECT borders in the blink overlay added visual noise without value.

The system now classifies each frame as either **PASS** or **REJECT** only. The reject
thresholds are the only thresholds that matter.

---

## PXSCORE Removed

PXSCORE has been removed. A PXSCORE of 83 could appear on both a PASS and a REJECT frame
because the score was a weighted average across all metrics while PXFLAG was triggered by
any single metric exceeding its threshold. This inconsistency made the score actively
misleading. PXFLAG alone is the actionable output.

---

## Current Metric Set (7 metrics)

Highlight Clipping has been removed. Typical astrophotography frames have clipping values
of 0.001% or less — far below any meaningful threshold. Hot pixels and cosmic rays (the
primary sources of isolated saturated pixels) are better handled by PI's stacking rejection
algorithms. The metric would never trigger in normal use.

The remaining seven metrics are:

1. Background Median
2. Background Std Dev
3. Background Gradient
4. SNR Estimate
5. FWHM
6. Eccentricity
7. Star Count

---

## Metric-by-Metric Findings

### 1. Background Median
**Status: KEEP, no algorithm change needed.**

Tracks extremely well against PI's Median metric — shapes match and absolute values are
almost identical. Also tracks closely with PI's N* robust noise estimator (MMT-based),
confirming it is accurately measuring sky background level. Our sigma-clipped background
estimation is producing correct results.

PI equivalent: **Median, N***

---

### 2. Background Std Dev
**Status: KEEP, no algorithm change needed.**

Has a distinctive shape with no clear PI equivalent. Stands alone as a valid metric for
detecting elevated noise conditions. Does not track with Background Median, confirming
it is measuring a different quality dimension.

PI equivalent: **None identified**

---

### 3. Background Gradient
**Status: KEEP, increase grid resolution.**

Detects partial cloud coverage during a single exposure — a class of problem no other
metric catches. When clouds cross the field during an exposure, Background Gradient spikes
while Background Median, Star Count, SNR, and FWHM all remain normal. This is a unique
and valuable detection capability.

The stepwise character of the metric is due to the coarse 4×4 grid. Increasing to 8×8
will produce a smoother curve without affecting detection capability.

Rough visual similarity to PI's Noise and NoiseRatio metrics — both respond to the same
underlying sky conditions.

PI equivalent: **Approximate similarity to Noise/NoiseRatio**

---

### 4. SNR Estimate
**Status: KEEP, rethink implementation.**

Our SNR Estimate tracks closely with PI's Noise, M*, N*, and StarResidual metrics. It
does NOT track with PI's SNR metric. This indicates our implementation is not measuring
true signal-to-noise ratio — it is a composite metric that responds to both background
level and PSF quality.

Root cause: Our flood-fill star pixel collection (down to 2σ above background) captures
too many near-background pixels. The median of this set correlates with background level
rather than true star signal.

However, the fact that our SNR spikes on cloudy frames (tracking with StarResidual)
means it is inadvertently capturing PSF degradation from clouds — which is actually
useful behavior. Our SNR Estimate is doing double duty as both a background noise
indicator and a cloud/PSF quality detector.

The current implementation should be renamed or redefined to accurately reflect what it
measures. A proper SNR implementation based on PI's ratio-of-powers formulation would
use star core pixels only (above 50% of peak value) rather than all flood-filled pixels.

PI equivalent: **Noise, M*, N*, StarResidual (composite — not true SNR)**

---

### 5. FWHM
**Status: KEEP, algorithm upgrade needed.**

Values are consistently lower than PI's FWHM, especially at higher values. Our
half-maximum axis-crossing approach underestimates FWHM because it finds crossing points
on 4 discrete axes rather than fitting the full star profile.

**Important note on FWHM vs Eccentricity relationship:** These metrics can appear
inversely related with our current implementation. A mildly trailed star can have high
eccentricity but modest FWHM because our axis-crossing approach is pulled down by the
narrow axis measurements. The narrow dimension of a trailed star (perpendicular to the
trail) can still be 2.5px even though the star is obviously elongated.

PI's FWHM uses the geometric mean of major and minor PSF axes:
  FWHM = sqrt(FWHM_major × FWHM_minor)
This more accurately captures true star size regardless of orientation.

The moment-based FWHM update will use the same geometric mean approach, making FWHM
and eccentricity more consistent with each other.

PI equivalent: **FWHM** (same concept, different algorithm)

---

### 6. Eccentricity
**Status: KEEP, algorithm upgrade needed — implement simultaneously with FWHM.**

Close agreement with PI at low eccentricities (delta ~0.02), diverges at higher
eccentricities (delta ~0.07). Our median eccentricity is ~0.639 vs PI's ~0.674.

Root cause: Flood-fill bounding box clips the wings of elongated stars, missing some
of the elongation signal. Faint pixels near the detection threshold dilute the
eccentricity signal toward circular.

The reject threshold should be conservative (0.85) since moderate eccentricity with
good FWHM is acceptable — a slightly elongated sharp star is better for the final
stack than a round blurry star. Only severely trailed frames should be rejected on
eccentricity alone.

PI equivalent: **Eccentricity** (same concept, PI uses PSF fitting)

---

### 7. Star Count
**Status: KEEP, consider threshold adjustment.**

Similar shape to PI's Stars metric. Our median count (~190) is higher than PI's (~144).
We detect more stars due to lower default detection threshold (5σ) and no minimum pixel
area filter. The relative frame rankings track well, which is what matters for rejection.

PI equivalent: **Stars (PSFCount)**

---

## Analysis Graph — Pending UI Changes

The following changes to the Analysis Graph are planned for the next code update pass:

1. **Metric 2 default** — change initial value from 'eccentricity' to 'none'

2. **Remove PXSCORE from dropdowns** — consistent with removal from AnalyzeFrames

3. **Rejection threshold line** — draw a horizontal line on the chart showing the
   rejection threshold for Metric 1, making it immediately visible which frames crossed
   the threshold for that metric

4. **Metric 2 dotted line** — draw Metric 2 with a dotted line to visually communicate
   it is a reference/secondary metric, not the primary metric being analyzed

5. **Tooltip positioning** — tooltip currently always opens to the right of the dot,
   cutting off on right-side frames. Fix: left third of chart opens right, middle third
   centers on dot, right third opens left

6. **Multi-line tooltip** — two lines:
   - Line 1: metric value | FLAG (and which metrics triggered REJECT if applicable)
   - Line 2: filename
   Requires backend change to classify_frame to return triggered_by: Vec<String>

7. **Click navigation** — when clicking a dot, navigate to that frame at 100% zoom
   in Pixels mode using displayFrame() from commands.ts, then close the graph

8. **Refresh fix** — call resizeCanvas() after loadData() completes to prevent the
   blurry/scaled rendering on refresh

9. **Theme awareness** — replace hardcoded colors with CSS theme variables read via
   getComputedStyle() at draw time so the graph matches the active theme

---

## Blink Overlay — Pending Changes

1. **Remove X marking for REJECT** — replace with red border (~5px), consistent with
   SUSPECT yellow border style. Red and yellow are distinguishable for color-blind users.

2. **Remove SUSPECT overlay entirely** — SUSPECT classification has been removed,
   so the yellow border is no longer needed

---

## Viewer — Pending Changes

1. **Filename overlay** — display the current frame's filename at the bottom of the
   viewer area at all times (not just during blink), same pattern as the existing
   blink filename overlay, smaller font to ensure full path fits

---

## Histogram Fix

The histogram canvas width calculation uses `canvas.offsetWidth` which returns 0 when
the Analysis Graph is showing (taking over the viewer area). Fix: use
`canvas.parentElement?.clientWidth || canvas.offsetWidth || 400`

---

## Pending Code Changes — Summary

### Algorithm Changes
| Item | Change |
|---|---|
| FWHM | Switch to moment-based: FWHM = 2.355 × sqrt((Mxx + Myy) / 2) using geometric mean of axes |
| Eccentricity | Improve moment calculation with deeper flood threshold, implement simultaneously with FWHM |
| Background Gradient | Increase grid from 4×4 to 8×8 |
| Star Count | Add minimum pixel count filter (≥5 pixels), consider raising default threshold to 6σ |

### Classification Changes
| Item | Change |
|---|---|
| SUSPECT | Remove entirely — PASS/REJECT only |
| PXSCORE | Remove entirely |
| Highlight Clipping | Remove as metric |
| Eccentricity reject threshold | Change from 0.80 to 0.85 |
| Thresholds | Make user-configurable as AnalyzeFrames arguments (e.g. AnalyzeFrames fwhm_reject=3.5) as immediate solution; full UI settings in Phase 9 |

### UI Changes
| Item | Change |
|---|---|
| Analysis Graph | See Analysis Graph pending changes above |
| Blink overlay | Red border for REJECT, remove yellow border (SUSPECT removed) |
| Viewer filename | Always-visible filename overlay at bottom of viewer |
| Histogram | Fix canvas width calculation |

---

## Implementation Order

1. Classification simplification (PASS/REJECT, remove PXSCORE, remove Highlight Clipping)
2. Eccentricity reject threshold → 0.85
3. Histogram fix (one line change)
4. Analysis Graph UI changes
5. Blink overlay changes
6. Viewer filename overlay
7. Moment-based FWHM + Eccentricity (together)
8. Background Gradient grid → 8×8
9. Star Count minimum pixel filter
10. User-configurable thresholds (AnalyzeFrames arguments)
11. Theme-aware Analysis Graph colors
