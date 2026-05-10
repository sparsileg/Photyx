## Photyx Algorithm Improvement Plan

### Frame Analysis Metrics Overhaul

**Version:** 1.0  
**Date:** 2026-05-10  
**Status:** Approved for implementation

---

### 1. Background

Comparative analysis between Photyx AnalyzeFrames output and PixInsight SubframeSelector (SFS) output across multiple imaging sessions revealed strong correlations for most metrics but identified algorithmic gaps that, when closed, will improve metric accuracy, increase correlation with PI's industry-standard measurements, and add Signal Weight as a fifth rejection metric.

The analysis also identified satellite trail detection as a future capability but determined it requires spatial pixel analysis rather than aggregate statistics and is deferred to a future phase.

---

### 2. Summary of Changes

| Metric                      | Current Algorithm                      | New Algorithm                                       | Role Change                  |
| --------------------------- | -------------------------------------- | --------------------------------------------------- | ---------------------------- |
| FWHM                        | Intensity-weighted 2nd-order moments   | Moffat PSF fitting                                  | None — accuracy improvement  |
| Eccentricity                | Intensity-weighted 2nd-order moments   | Elliptical Moffat PSF fitting                       | None — accuracy improvement  |
| Star Count                  | Lenient connected-pixel detection      | Stricter Moffat PSF acceptance                      | None — accuracy improvement  |
| Signal Weight               | Raw pixel SNR (not a rejection metric) | Moffat-derived PSF Signal Weight (rejection metric) | Promoted to rejection metric |
| Background Median           | Unchanged                              | Unchanged                                           | None                         |
| PSF Residual                | New                                    | Moffat goodness-of-fit                              | Display only                 |
| Eccentricity Mean Deviation | New                                    | Per-frame ecc variance from Moffat fit              | Display only                 |
| Moffat Beta (β)             | New                                    | Moffat shape parameter                              | Display only                 |

---

### 3. Core Infrastructure: Moffat PSF Fitting

All metric improvements flow from a single new infrastructure component: **elliptical 2D Moffat profile fitting** per detected star.

#### 3.1 Moffat Profile

The 2D elliptical Moffat function is:

```
I(x,y) = A · [1 + ((x-x0)²/a² + (y-y0)²/b²)]^(-β) + B
```

Where:

- `A` — peak amplitude above background
- `x0, y0` — centroid position
- `a, b` — semi-major and semi-minor axis scale parameters
- `β` — Moffat shape parameter (controls wing falloff)
- `B` — local background level

#### 3.2 Fitting Procedure

1. Detect candidate stars using existing connected-pixel detection as the seed step
2. For each candidate, extract a pixel stamp (typically 15×15 or scaled to ~3× estimated FWHM)
3. Estimate initial parameters: centroid from moments, background from stamp border, amplitude from peak pixel
4. Fit the elliptical Moffat model using nonlinear least squares (Levenberg-Marquardt)
5. Apply acceptance criteria to filter non-stellar detections (see §6)
6. Derive all metrics from accepted fits

#### 3.3 Implementation Notes

- Fitting should be parallelized per-star via Rayon
- Stars too close to frame edges or other stars should be excluded from fitting
- Stars with saturated pixels should be excluded
- Fitting failures (non-convergence) increment a rejection counter but do not halt analysis
- The existing connected-pixel detection is retained as the candidate generation step; Moffat fitting is the acceptance gate

---

### 4. Updated Metrics

#### 4.1 FWHM

**Current:** Median FWHM computed from intensity-weighted second-order moments across all detected stars.

**New:** Median FWHM derived from Moffat PSF fits. For each accepted star:

```
FWHM = 2 · sqrt(2^(1/β) - 1) · sqrt(a · b)
```

Where `a` and `b` are the fitted semi-axis parameters and `β` is the fitted shape parameter. The geometric mean of the axes gives a single FWHM value per star. The frame FWHM is the median across all accepted stars.

**Expected outcome:** Closer agreement with PI SFS FWHM values. Current correlation r=0.91; target r>0.95.

**Rejection role:** Unchanged — sigma-clipping rejection metric, reject above +2.5σ.

---

#### 4.2 Eccentricity

**Current:** Eccentricity derived from intensity-weighted second-order moments. Systematically underestimates eccentricity relative to PI by a mean of ~0.039, with a linear relationship PI_ecc ≈ 1.085 × Photyx_ecc − 0.015. Current correlation r=0.978.

**New:** Eccentricity derived from the fitted ellipse semi-axes:

```
e = sqrt(1 - (b/a)²)
```

Where `a` is the semi-major axis and `b` is the semi-minor axis from the Moffat fit, with `a ≥ b` enforced. The frame eccentricity is the median across all accepted stars.

**Expected outcome:** Absolute values converge with PI SFS eccentricity. The Moffat fit is sensitive to PSF wings where elongation is most apparent, which is the source of the current systematic underestimate.

**Rejection role:** Unchanged — absolute threshold, reject above 0.85.

---

#### 4.3 Star Count

**Current:** All connected-pixel regions above threshold that meet minimum size criteria are counted. Counts approximately 1.8× PI's star count on average. Highly susceptible to satellite trail inflation (frame 54: 330 Photyx vs 88 PI).

**New:** Only stars that successfully pass Moffat PSF fitting with acceptable residuals are counted. Acceptance criteria:

- Fitted peak amplitude `A` above minimum SNR threshold
- Semi-axis ratio `b/a` above minimum (reject extremely elongated detections)
- PSF residual below maximum threshold
- Convergence achieved within iteration limit
- No saturated pixels within fitting stamp

**Expected outcome:** Star count closer to PI SFS values (current ratio PI/Photyx ~0.55; target ~0.85+). Satellite trail false detections naturally filtered by PSF acceptance criteria since trail segments do not fit a stellar Moffat profile.

**Rejection role:** Unchanged — sigma-clipping rejection metric, reject below −1.5σ.

---

#### 4.4 Signal Weight (formerly SNR)

**Current:** Raw pixel-based SNR computed from background statistics. Not currently used as a rejection metric due to insufficient additional rejection value over the other four metrics. Correlates poorly with PI PSF Signal Weight (r=0.35 vs PI SNR; r=0.81 vs PI PSF Signal Weight).

**New:** PSF-based Signal Weight computed from Moffat fit parameters per star, aggregated as the median across accepted stars:

```
Signal Weight ∝ A² / (A + B · π · a · b)
```

Where:

- `A` — fitted peak amplitude (above background)
- `B` — fitted local background level
- `π · a · b` — effective PSF area from fitted semi-axes

This formulation captures shot-noise-limited SNR accounting for PSF size, penalizing broad PSFs relative to narrow ones at the same peak flux. It is sensitive to transparency events and atmospheric extinction in ways that Star Count is not.

**Metric rename:** SNR → Signal Weight throughout the UI, pcode command dictionary, keyword output, and documentation.

**Expected outcome:** Correlation with PI PSF Signal Weight increases from r=0.81 toward r>0.90 as algorithm more closely follows PI's approach.

**Rejection role:** **Promoted to rejection metric.** Sigma-clipping, reject below −2.5σ. Rationale: Signal Weight catches transparency and thin cloud events that Star Count misses, and is the only metric that correctly signals problems on satellite trail frames where Star Count is inflated.

---

#### 4.5 Background Median

No change. Current algorithm is essentially identical to PI (correlation r=0.9996, scale factor 0.9976). No improvement is possible or necessary.

---

##### 5. Internal Metrics (Not User-Facing)

PSF Residual is computed internally as the acceptance gate for star detection — a Moffat fit must fall below a residual threshold to be counted as a valid star — but is never surfaced in the UI or results output. 

---

### 6. Rejection Metric Summary (Post-Implementation)

| Metric            | Type                     | Reject Threshold |
| ----------------- | ------------------------ | ---------------- |
| FWHM              | Sigma (session-relative) | > +2.5σ          |
| Eccentricity      | Absolute                 | > 0.85           |
| Background Median | Sigma (session-relative) | > +2.5σ          |
| Star Count        | Sigma (session-relative) | < −1.5σ          |
| Signal Weight     | Sigma (session-relative) | < −2.5σ          |

---

### 7. Pcode and Keyword Changes

#### 7.1 Metric Rename

`SNR` is renamed to `Signal Weight` in all contexts:

- Analysis results table column header
- Analysis graph tooltip and axis label
- `AnalyzeFrames` plugin output
- `GetSessionProperty` interrogation properties
- `photyx_reference.md` metrics table
- All user-facing documentation

#### 7.2 PXFLAG Behavior

No change to PXFLAG keyword values (PASS/REJECT) or writing behavior. The addition of Signal Weight as a rejection metric means more frames may be classified as REJECT; the `triggered_by` field will correctly attribute Signal Weight rejections.

#### 7.3 Category Code

Signal Weight rejections will use category code **T** (Transparency) since the metric primarily captures atmospheric transparency and extinction events. This is consistent with the existing category taxonomy (O=Optical, B=Brightness, T=Transparency).

---

### 8. Implementation Order

The Moffat fitting infrastructure is the prerequisite for all metric improvements. Recommended implementation sequence:

1. **Moffat fitting core** — 2D elliptical Moffat fitting engine with Levenberg-Marquardt solver, per-star stamp extraction, acceptance criteria
2. **FWHM and Eccentricity** — replace moment-based computation with Moffat-derived values; validate against PI SFS output
3. **Star Count** — replace lenient detection with Moffat acceptance gate; validate count ratios
4. **Signal Weight** — implement PSF-based formula; wire up as rejection metric; rename SNR throughout
5. **Threshold validation** — run AnalyzeFrames on multiple sessions with new algorithms; verify rejection thresholds remain well-calibrated

---

### 9. Deferred Items

**Satellite Trail Detection** — Reliable detection requires spatial pixel analysis (e.g. Hough transform on gradient image) rather than aggregate statistics. The improved Star Count and Signal Weight algorithms will reduce false-negative trail frames but will not eliminate them. A dedicated `DetectTrails` analysis plugin is noted for a future phase.

---

### 10. Files Affected

| File                                             | Change                                |
| ------------------------------------------------ | ------------------------------------- |
| `src-tauri/src/plugins/analyze_frames.rs`        | Core algorithm replacement            |
| `src-tauri/src/plugins/compute_fwhm.rs`          | Replace moments with Moffat           |
| `src-tauri/src/plugins/compute_eccentricity.rs`  | Replace moments with Moffat           |
| `src-tauri/src/analysis/moffat.rs`               | New — Moffat fitting engine           |
| `src-svelte/lib/components/AnalysisGraph.svelte` | New columns, SNR→Signal Weight rename |
| `src-svelte/lib/components/AnalysisPanel.svelte` | New columns, SNR→Signal Weight rename |
| `photyx_spec.md`                                 | Update metrics section                |
| `photyx_reference.md`                            | Update metrics table, rename SNR      |
| `development_notes.md`                           | Document new algorithms               |
