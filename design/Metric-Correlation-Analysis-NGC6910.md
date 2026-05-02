# Photyx AnalyzeFrames — Metric Correlation Analysis

## Dataset: NGC6910, 80 frames

---

## Pairwise Pearson Correlations

| Metric A     | Metric B     | r      | Strength             |
| ------------ | ------------ | ------ | -------------------- |
| FWHM         | Eccentricity | +0.312 | Weak positive        |
| FWHM         | Star Count   | -0.701 | Strong negative      |
| FWHM         | SNR          | -0.155 | Negligible           |
| FWHM         | Bg Median    | +0.079 | Negligible           |
| FWHM         | Bg Std Dev   | -0.054 | Negligible           |
| FWHM         | Bg Gradient  | +0.031 | Negligible           |
| Eccentricity | Star Count   | -0.182 | Weak negative        |
| Eccentricity | SNR          | -0.003 | Negligible           |
| Eccentricity | Bg Median    | +0.121 | Negligible           |
| Eccentricity | Bg Std Dev   | +0.069 | Negligible           |
| Eccentricity | Bg Gradient  | +0.047 | Negligible           |
| Star Count   | SNR          | +0.756 | Strong positive      |
| Star Count   | Bg Median    | -0.935 | Very strong negative |
| Star Count   | Bg Std Dev   | -0.937 | Very strong negative |
| Star Count   | Bg Gradient  | -0.617 | Moderate negative    |
| SNR          | Bg Median    | -0.812 | Very strong negative |
| SNR          | Bg Std Dev   | -0.818 | Very strong negative |
| SNR          | Bg Gradient  | -0.484 | Moderate negative    |
| Bg Median    | Bg Std Dev   | +0.971 | Very strong positive |
| Bg Median    | Bg Gradient  | +0.706 | Strong positive      |
| Bg Std Dev   | Bg Gradient  | +0.681 | Strong positive      |

---

## Key Findings

### 1. Sky brightness dominates the session

Star Count and Bg Median/Std Dev are very strongly anti-correlated (r ≈ −0.94),
and SNR and Bg Median/Std Dev are very strongly anti-correlated (r ≈ −0.81).
The dominant trend across this 80-frame session is sky brightening over time,
which simultaneously suppresses star detection and degrades SNR.

### 2. The three background metrics are largely redundant

Bg Median and Bg Std Dev are nearly identical (r = 0.971). Bg Gradient is
moderately correlated with both (r = 0.68–0.71). All three are measuring the
same underlying phenomenon — sky brightness — from slightly different angles.
Retaining all three adds computation and threshold complexity without
meaningfully increasing the information content of the rejection decision.

### 3. FWHM is independent of sky conditions

FWHM shows negligible correlation with all three background metrics (r < 0.08),
confirming it is driven by atmospheric seeing rather than sky brightness.
It varies independently and cannot be inferred from any other metric.

### 4. Eccentricity is orthogonal to everything

Eccentricity shows no meaningful correlation with any other metric (all r < 0.32).
It is measuring a completely different physical phenomenon — tracking errors and
optical aberrations — that no other metric would catch.

### 5. SNR is partially redundant with Star Count and Bg Median

SNR correlates strongly with both Star Count (r = +0.756) and Bg Median
(r = −0.812). Much of its signal is already captured by those two metrics.
However it does retain some independent value as a direct measure of signal
quality rather than an indirect proxy.

---

## Recommendation

### Keep without question

| Metric       | Rationale                                                                     |
| ------------ | ----------------------------------------------------------------------------- |
| FWHM         | Primary seeing metric; independent of all sky conditions; irreplaceable       |
| Eccentricity | Fully orthogonal; catches tracking and optical problems nothing else detects  |
| Bg Median    | Best single representative of sky brightness; physically interpretable        |
| Star Count   | Strong independent signal; sensitive to both sky conditions and frame quality |

### Remove

| Metric     | Rationale                                                               |
| ---------- | ----------------------------------------------------------------------- |
| Bg Std Dev | r = 0.971 with Bg Median — effectively a duplicate; adds no information |

### Consider making optional (advanced / power user)

| Metric      | Rationale                                                                                                                                 |
| ----------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Bg Gradient | Measures spatial uniformity rather than absolute brightness; useful for light pollution diagnosis but moderately redundant with Bg Median |
| SNR         | Partially redundant with Bg Median and Star Count combined; retains independent value as a direct signal quality measure                  |

---

## Caveats

This analysis is based on a single 80-frame session of NGC6910 under specific
sky conditions. Correlation structure may differ significantly for:

- Targets at different altitudes (airmass variation affects background and FWHM differently)
- Sessions with variable seeing rather than variable sky brightness
- Narrowband or duo-band filter sessions (star count suppression changes the dynamics)
- Multi-night integrations where equipment or conditions vary

**Recommendation:** Do not permanently remove metrics from the computation engine
based on one dataset. The preferable approach is to retain all seven metrics in
the analysis but allow users to disable specific metrics in their threshold
profile — giving power users the flexibility to tune rejection criteria to their
equipment and sky conditions while preserving the full information for sessions
where all metrics are informative.
