# Photyx AnalyzeFrames — Metric Correlation Analysis

## Datasets: NGC6910 (80 frames) + M104 (62 frames)

---

## Pairwise Pearson Correlations — NGC6910

| Metric A     | Metric B     | r      | Strength             |
| ------------ | ------------ | ------ | -------------------- |
| FWHM         | Eccentricity | +0.312 | Weak positive        |
| FWHM         | Star Count   | −0.701 | Strong negative      |
| FWHM         | SNR          | −0.155 | Negligible           |
| FWHM         | Bg Median    | +0.079 | Negligible           |
| FWHM         | Bg Std Dev   | −0.054 | Negligible           |
| FWHM         | Bg Gradient  | +0.031 | Negligible           |
| Eccentricity | Star Count   | −0.182 | Weak negative        |
| Eccentricity | SNR          | −0.003 | Negligible           |
| Eccentricity | Bg Median    | +0.121 | Negligible           |
| Eccentricity | Bg Std Dev   | +0.069 | Negligible           |
| Eccentricity | Bg Gradient  | +0.047 | Negligible           |
| Star Count   | SNR          | +0.756 | Strong positive      |
| Star Count   | Bg Median    | −0.935 | Very strong negative |
| Star Count   | Bg Std Dev   | −0.937 | Very strong negative |
| Star Count   | Bg Gradient  | −0.617 | Moderate negative    |
| SNR          | Bg Median    | −0.812 | Very strong negative |
| SNR          | Bg Std Dev   | −0.818 | Very strong negative |
| SNR          | Bg Gradient  | −0.484 | Moderate negative    |
| Bg Median    | Bg Std Dev   | +0.971 | Very strong positive |
| Bg Median    | Bg Gradient  | +0.706 | Strong positive      |
| Bg Std Dev   | Bg Gradient  | +0.681 | Strong positive      |

---

## Pairwise Pearson Correlations — M104

| Metric A     | Metric B     | r      | Strength              |
| ------------ | ------------ | ------ | --------------------- |
| FWHM         | Eccentricity | +0.280 | Weak positive         |
| FWHM         | Star Count   | −0.960 | Very strong negative  |
| FWHM         | SNR          | +0.620 | Moderate positive     |
| FWHM         | Bg Median    | +0.920 | Very strong positive  |
| FWHM         | Bg Std Dev   | +0.900 | Very strong positive  |
| FWHM         | Bg Gradient  | +0.720 | Strong positive       |
| Eccentricity | Star Count   | −0.260 | Weak negative         |
| Eccentricity | SNR          | +0.030 | Negligible            |
| Eccentricity | Bg Median    | +0.250 | Weak positive         |
| Eccentricity | Bg Std Dev   | +0.240 | Weak positive         |
| Eccentricity | Bg Gradient  | +0.250 | Weak positive         |
| Star Count   | SNR          | −0.550 | Moderate negative     |
| Star Count   | Bg Median    | −0.930 | Very strong negative  |
| Star Count   | Bg Std Dev   | −0.910 | Very strong negative  |
| Star Count   | Bg Gradient  | −0.730 | Strong negative       |
| SNR          | Bg Median    | +0.520 | Moderate positive     |
| SNR          | Bg Std Dev   | +0.520 | Moderate positive     |
| SNR          | Bg Gradient  | +0.380 | Weak positive         |
| Bg Median    | Bg Std Dev   | +0.990 | Near-perfect positive |
| Bg Median    | Bg Gradient  | +0.840 | Strong positive       |
| Bg Std Dev   | Bg Gradient  | +0.830 | Strong positive       |

---

## Cross-Session Comparison

| Metric Pair             | NGC6910 r | M104 r | Consistency                              |
| ----------------------- | --------- | ------ | ---------------------------------------- |
| Bg Median vs Bg Std Dev | +0.971    | +0.990 | Redundant in all sessions                |
| FWHM vs Star Count      | −0.701    | −0.960 | Strong, session-dependent magnitude      |
| FWHM vs Bg Median       | +0.079    | +0.920 | Highly session-dependent                 |
| Eccentricity vs all     | < 0.32    | < 0.28 | Always independent                       |
| SNR vs Bg Median        | −0.812    | +0.520 | Session-dependent, partially independent |

---

## Key Findings

### 1. Bg Median and Bg Std Dev are redundant in every session examined

r = 0.971 in NGC6910, r = 0.990 in M104. Across two very different sessions —
one sky-brightness dominated, one airmass dominated — these two metrics move
in near-perfect lockstep. Retaining both adds no information.

### 2. The dominant driver is session-dependent

In NGC6910, sky brightness brightening over time drives the correlations. In
M104, airmass change as the target tracks through low altitude is the dominant
driver. This produces very different correlation structures between sessions,
particularly for FWHM vs background metrics (r = 0.08 vs r = 0.92).

### 3. FWHM and Star Count are highly correlated in airmass-dominated sessions

In M104, r = −0.96 — nearly perfectly anti-correlated. Both are degraded by the
same airmass-induced seeing. In NGC6910, where seeing varies independently of
sky conditions, r = −0.70 — still strong but with more independent signal.

### 4. Eccentricity is orthogonal to everything in both sessions

All r < 0.32 in NGC6910, all r < 0.28 in M104. It measures a completely
different physical phenomenon — tracking errors and optical aberrations — that
no other metric would catch. It is essential regardless of session type.

### 5. SNR is partially redundant but session-dependent

In NGC6910, SNR is strongly anti-correlated with Bg Median (r = −0.812) — both
driven by sky brightness. In M104, SNR is only moderately correlated with Bg
Median (r = +0.520), retaining more independent signal. Its value depends on
session type.

### 6. Bg Gradient has moderate independent value

r = 0.68–0.84 with Bg Median across both sessions — correlated but not
redundant. It specifically measures spatial non-uniformity across the field,
which Bg Median does not capture. Useful for detecting light pollution gradients
and uneven sky illumination.

---

## Recommendation

### Keep without question

| Metric       | Rationale                                                                                    |
| ------------ | -------------------------------------------------------------------------------------------- |
| FWHM         | Primary seeing metric; independent of sky conditions in zenith sessions; irreplaceable       |
| Eccentricity | Fully orthogonal in all sessions; catches tracking and optical problems nothing else detects |
| Bg Median    | Best single representative of sky brightness; physically interpretable                       |

### Remove

| Metric     | Rationale                                                                                                                |
| ---------- | ------------------------------------------------------------------------------------------------------------------------ |
| Bg Std Dev | r = 0.971–0.990 with Bg Median across all sessions — effectively a duplicate in every case examined; adds no information |

### Consider making optional (advanced / power user)

| Metric      | Rationale                                                                                                                                                                                                                                         |
| ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Star Count  | r = −0.70 to −0.96 with FWHM depending on session; often redundant with FWHM but provides independent signal when seeing is stable but clouds or focus reduce detectable stars without affecting FWHM; should be disabled for narrowband sessions |
| Bg Gradient | Measures spatial non-uniformity rather than absolute brightness; moderately independent (r = 0.68–0.84); useful for light pollution gradient detection                                                                                            |
| SNR         | Partially redundant with Bg Median and Star Count in some sessions; retains independent signal in others; direct measure of signal quality rather than a proxy                                                                                    |

---

## Caveats

Analysis is based on two sessions under specific sky conditions. Correlation
structure varies significantly with:

- Target altitude — airmass-dominated sessions produce very different structures
  than zenith sessions
- Session duration and sky stability — short stable sessions vs long sessions
  with varying conditions
- Filter type — narrowband and duo-band filters suppress star count, changing
  the Star Count vs FWHM relationship significantly
- Equipment — different optical systems will produce different eccentricity and
  FWHM characteristics

**Recommendation:** Do not permanently remove metrics from the computation engine
based on these results. The preferable approach is to retain all seven metrics in
the analysis but allow users to disable specific metrics in their threshold
profile — giving power users the flexibility to tune rejection criteria to their
equipment and sky conditions while preserving the full information for sessions
where all metrics are informative.
