# Photyx AnalyzeFrames — Metric Correlation Analysis

## Datasets

| Session   | Frames | Filter     | Altitude | Dominant driver                                |
| --------- | ------ | ---------- | -------- | ---------------------------------------------- |
| NGC6910-B | 80     | Broadband  | Mid      | Sky brightness declining (dew/fog?)            |
| NGC6910-C | 80     | Broadband  | Mid      | Sky transparency arc (brightening then fading) |
| M104      | 62     | Broadband  | Low      | Airmass (low altitude target)                  |
| NGC7380   | 160    | Narrowband | Mid      | Airmass (star count decline over time)         |
| M13       | 107    | Broadband  | High     | Stable — seeing and tracking only              |

**Note on NGC6910:** Three sessions were collected from this field. The original
session used in the prior two-session analysis could not be positively identified.
NGC6910-B and NGC6910-C are the two recovered sessions; they show different
dominant drivers and are treated as independent datasets.

**Note on data precision:** All sessions were initially analyzed with display-
truncated data (Bg Std Dev and Bg Gradient showing only 3 decimal places). This
document uses corrected full-precision values from all sessions.

---

## Pairwise Pearson Correlations — NGC6910-B (80 frames, broadband, mid-altitude)

**Session character:** Sky background declining monotonically across the session
(Bg Median 5.274e-2 → 4.828e-2), star count rising. Likely sky transparency
improving as session progressed, or target rising through airmass.

| Metric A     | Metric B     | r      | Strength              |
| ------------ | ------------ | ------ | --------------------- |
| FWHM         | Eccentricity | −0.047 | Negligible            |
| FWHM         | Star Count   | +0.240 | Weak positive         |
| FWHM         | SNR          | +0.279 | Weak positive         |
| FWHM         | Bg Median    | −0.449 | Moderate negative     |
| FWHM         | Bg Std Dev   | −0.441 | Moderate negative     |
| FWHM         | Bg Gradient  | −0.209 | Weak negative         |
| Eccentricity | Star Count   | −0.107 | Negligible            |
| Eccentricity | SNR          | +0.041 | Negligible            |
| Eccentricity | Bg Median    | +0.092 | Negligible            |
| Eccentricity | Bg Std Dev   | +0.106 | Negligible            |
| Eccentricity | Bg Gradient  | +0.246 | Weak positive         |
| Star Count   | SNR          | +0.461 | Moderate positive     |
| Star Count   | Bg Median    | −0.957 | Near-perfect negative |
| Star Count   | Bg Std Dev   | −0.958 | Near-perfect negative |
| Star Count   | Bg Gradient  | −0.599 | Moderate negative     |
| SNR          | Bg Median    | −0.584 | Moderate negative     |
| SNR          | Bg Std Dev   | −0.590 | Moderate negative     |
| SNR          | Bg Gradient  | −0.415 | Moderate negative     |
| Bg Median    | Bg Std Dev   | +0.999 | Near-perfect positive |
| Bg Median    | Bg Gradient  | +0.632 | Strong positive       |
| Bg Std Dev   | Bg Gradient  | +0.640 | Strong positive       |

---

## Pairwise Pearson Correlations — NGC6910-C (80 frames, broadband, mid-altitude)

**Session character:** SNR follows a strong arc — high early (7.9–8.0), dropping
through the middle (5.4–5.7), then recovering slightly. Bg Median mirrors this
inversely. Star count follows the same arc inversely. Likely a transparency
episode (thin cloud or dew) mid-session.

| Metric A     | Metric B     | r      | Strength              |
| ------------ | ------------ | ------ | --------------------- |
| FWHM         | Eccentricity | −0.105 | Negligible            |
| FWHM         | Star Count   | −0.061 | Negligible            |
| FWHM         | SNR          | +0.257 | Weak positive         |
| FWHM         | Bg Median    | −0.235 | Weak negative         |
| FWHM         | Bg Std Dev   | −0.235 | Weak negative         |
| FWHM         | Bg Gradient  | −0.157 | Negligible            |
| Eccentricity | Star Count   | −0.385 | Weak negative         |
| Eccentricity | SNR          | −0.444 | Moderate negative     |
| Eccentricity | Bg Median    | +0.479 | Moderate positive     |
| Eccentricity | Bg Std Dev   | +0.489 | Moderate positive     |
| Eccentricity | Bg Gradient  | +0.326 | Weak positive         |
| Star Count   | SNR          | +0.885 | Very strong positive  |
| Star Count   | Bg Median    | −0.939 | Very strong negative  |
| Star Count   | Bg Std Dev   | −0.931 | Very strong negative  |
| Star Count   | Bg Gradient  | −0.399 | Weak negative         |
| SNR          | Bg Median    | −0.964 | Near-perfect negative |
| SNR          | Bg Std Dev   | −0.955 | Near-perfect negative |
| SNR          | Bg Gradient  | −0.367 | Weak negative         |
| Bg Median    | Bg Std Dev   | +0.998 | Near-perfect positive |
| Bg Median    | Bg Gradient  | +0.443 | Moderate positive     |
| Bg Std Dev   | Bg Gradient  | +0.454 | Moderate positive     |

---

## Pairwise Pearson Correlations — M104 (62 frames, broadband, low altitude)

**Session character:** Airmass-dominated. Target tracked through significant
altitude change. Contains extreme outlier frames (FWHM 8.024, 5.834) from
likely atmospheric event near culmination.

| Metric A     | Metric B     | r      | Strength              |
| ------------ | ------------ | ------ | --------------------- |
| FWHM         | Eccentricity | −0.293 | Weak negative         |
| FWHM         | Star Count   | −0.848 | Very strong negative  |
| FWHM         | SNR          | +0.800 | Strong positive       |
| FWHM         | Bg Median    | +0.653 | Strong positive       |
| FWHM         | Bg Std Dev   | +0.650 | Strong positive       |
| FWHM         | Bg Gradient  | +0.338 | Weak positive         |
| Eccentricity | Star Count   | +0.101 | Negligible            |
| Eccentricity | SNR          | −0.341 | Weak negative         |
| Eccentricity | Bg Median    | +0.000 | Negligible            |
| Eccentricity | Bg Std Dev   | −0.005 | Negligible            |
| Eccentricity | Bg Gradient  | +0.052 | Negligible            |
| Star Count   | SNR          | −0.542 | Moderate negative     |
| Star Count   | Bg Median    | −0.898 | Very strong negative  |
| Star Count   | Bg Std Dev   | −0.896 | Very strong negative  |
| Star Count   | Bg Gradient  | −0.370 | Weak negative         |
| SNR          | Bg Median    | +0.277 | Weak positive         |
| SNR          | Bg Std Dev   | +0.282 | Weak positive         |
| SNR          | Bg Gradient  | +0.347 | Weak positive         |
| Bg Median    | Bg Std Dev   | +0.999 | Near-perfect positive |
| Bg Median    | Bg Gradient  | +0.417 | Moderate positive     |
| Bg Std Dev   | Bg Gradient  | +0.417 | Moderate positive     |

---

## Pairwise Pearson Correlations — NGC7380 (160 frames, narrowband, mid-altitude)

**Session character:** Narrowband filter. Bg Median near-constant (4.297e-2 to
4.315e-2) — sky brightness essentially suppressed by filter. Star count declines
monotonically as target sets through airmass. Capture date: 2024-10-05.

| Metric A     | Metric B     | r      | Strength             |
| ------------ | ------------ | ------ | -------------------- |
| FWHM         | Eccentricity | +0.586 | Moderate positive    |
| FWHM         | Star Count   | −0.641 | Strong negative      |
| FWHM         | SNR          | +0.511 | Moderate positive    |
| FWHM         | Bg Median    | −0.070 | Negligible           |
| FWHM         | Bg Std Dev   | −0.123 | Negligible           |
| FWHM         | Bg Gradient  | −0.051 | Negligible           |
| Eccentricity | Star Count   | −0.407 | Moderate negative    |
| Eccentricity | SNR          | +0.367 | Weak positive        |
| Eccentricity | Bg Median    | −0.054 | Negligible           |
| Eccentricity | Bg Std Dev   | −0.083 | Negligible           |
| Eccentricity | Bg Gradient  | −0.103 | Negligible           |
| Star Count   | SNR          | −0.575 | Moderate negative    |
| Star Count   | Bg Median    | −0.313 | Weak negative        |
| Star Count   | Bg Std Dev   | −0.265 | Weak negative        |
| Star Count   | Bg Gradient  | +0.508 | Moderate positive    |
| SNR          | Bg Median    | +0.108 | Negligible           |
| SNR          | Bg Std Dev   | −0.009 | Negligible           |
| SNR          | Bg Gradient  | −0.081 | Negligible           |
| Bg Median    | Bg Std Dev   | +0.920 | Very strong positive |
| Bg Median    | Bg Gradient  | +0.211 | Weak positive        |
| Bg Std Dev   | Bg Gradient  | +0.172 | Negligible           |

---

## Pairwise Pearson Correlations — M13 (107 frames, broadband, high-altitude)

**Session character:** Globular cluster near zenith. Most stable session in the
dataset — airmass essentially constant, sky brightness barely varies (4.255e-2
to 5.457e-2 across session start/end). Only meaningful variables are seeing and
tracking quality. Full-precision data confirmed after display fix.

| Metric A     | Metric B     | r      | Strength              |
| ------------ | ------------ | ------ | --------------------- |
| FWHM         | Eccentricity | −0.409 | Moderate negative     |
| FWHM         | Star Count   | −0.627 | Strong negative       |
| FWHM         | SNR          | +0.374 | Weak positive         |
| FWHM         | Bg Median    | +0.005 | Negligible            |
| FWHM         | Bg Std Dev   | +0.014 | Negligible            |
| FWHM         | Bg Gradient  | −0.000 | Negligible            |
| Eccentricity | Star Count   | +0.447 | Moderate positive     |
| Eccentricity | SNR          | −0.232 | Weak negative         |
| Eccentricity | Bg Median    | −0.229 | Weak negative         |
| Eccentricity | Bg Std Dev   | −0.243 | Weak negative         |
| Eccentricity | Bg Gradient  | −0.220 | Weak negative         |
| Star Count   | SNR          | +0.046 | Negligible            |
| Star Count   | Bg Median    | −0.745 | Strong negative       |
| Star Count   | Bg Std Dev   | −0.744 | Strong negative       |
| Star Count   | Bg Gradient  | −0.435 | Moderate negative     |
| SNR          | Bg Median    | −0.471 | Moderate negative     |
| SNR          | Bg Std Dev   | −0.469 | Moderate negative     |
| SNR          | Bg Gradient  | −0.093 | Negligible            |
| Bg Median    | Bg Std Dev   | +0.997 | Near-perfect positive |
| Bg Median    | Bg Gradient  | +0.569 | Moderate positive     |
| Bg Std Dev   | Bg Gradient  | +0.561 | Moderate positive     |

---

## Cross-Session Summary

| Metric Pair              | NGC6910-B | NGC6910-C | M104   | NGC7380 | M13    | Consistency                                    |
| ------------------------ | --------- | --------- | ------ | ------- | ------ | ---------------------------------------------- |
| Bg Median vs Bg Std Dev  | +0.999    | +0.998    | +0.999 | +0.920  | +0.997 | Near-perfect in ALL five sessions              |
| FWHM vs Star Count       | +0.240    | −0.061    | −0.848 | −0.641  | −0.627 | Session-dependent; negative in 3 of 5          |
| FWHM vs Eccentricity     | −0.047    | −0.105    | −0.293 | +0.586  | −0.409 | Mostly negligible-to-weak; sign varies         |
| FWHM vs SNR              | +0.279    | +0.257    | +0.800 | +0.511  | +0.374 | Consistently positive; PSF artifact confirmed  |
| FWHM vs Bg Median        | −0.449    | −0.235    | +0.653 | −0.070  | +0.005 | Highly session-dependent; sign reverses        |
| Star Count vs Bg Median  | −0.957    | −0.939    | −0.898 | −0.313  | −0.745 | Consistently strong negative in all sessions   |
| Star Count vs Bg Std Dev | −0.958    | −0.931    | −0.896 | −0.265  | −0.744 | Mirrors Bg Median — confirms redundancy        |
| Star Count vs SNR        | +0.461    | +0.885    | −0.542 | −0.575  | +0.046 | Highly session-dependent; unreliable as a pair |
| SNR vs Bg Median         | −0.584    | −0.964    | +0.277 | +0.108  | −0.471 | Session-dependent; sign reverses with airmass  |
| Eccentricity vs all bg   | < 0.25    | < 0.49    | < 0.05 | < 0.11  | < 0.24 | Always weak or negligible                      |

---

## Session Notes

### NGC6910-B

Sky background declines monotonically across the session — star count and SNR
rise as sky darkens. Strong Star Count vs Bg Median correlation (−0.957). FWHM
is essentially independent of everything except background metrics (moderate
negative correlation), suggesting seeing was stable while sky conditions changed.
Eccentricity is fully orthogonal to all other metrics.

### NGC6910-C

The most extreme SNR variation in the dataset (5.435 to 8.087 — nearly 3× range).
SNR vs Bg Median = −0.964 (near-perfect): as sky brightened, SNR dropped and
star count fell. This session likely had a transparency event mid-run. Notable:
Eccentricity correlates moderately with Bg Median (+0.479) — frames taken during
the transparency event show slightly higher eccentricity, possibly due to
atmospheric dispersion or guide star quality degradation during the event.

### M104

Airmass-dominated with extreme outlier frames. FWHM vs SNR = +0.800 — the
strongest PSF artifact in the dataset. The extreme outlier frames (FWHM > 5)
with anomalously high SNR (7.5) confirm that the SNR estimator rewards bloated
star flux rather than signal quality.

### NGC7380

Narrowband session. Bg Median near-constant but not truly zero-variance (range
4.297e-2 to 4.315e-2). Bg Median vs Bg Std Dev = +0.920 — lower than other
sessions but still very strong. Star Count vs Bg Gradient sign reverses here
(+0.508 vs negative in all broadband sessions) — Bg Gradient is measuring
spatial field variation driven by airmass rather than sky brightness gradient.

### M13

Most stable session. The FWHM vs Eccentricity sign reversal (−0.409 vs positive
in other sessions) confirms these metrics are measuring different physical
phenomena. In stable conditions, turbulence symmetrizes the PSF (reducing
eccentricity) in worse-seeing frames, while better-seeing frames show residual
tracking elongation. Eccentricity is essential precisely because it is
orthogonal to seeing in stable sessions.

---

## Key Findings (Five Sessions — Final)

### 1. Bg Std Dev is redundant in every session — removal confirmed

Bg Median vs Bg Std Dev: +0.920 to +0.999 across all five sessions. The lowest
value (NGC7380, +0.920) is still very strong. In no session does Bg Std Dev
provide information not already captured by Bg Median. **Remove from the
analysis engine. This finding is definitive.**

### 2. FWHM vs Star Count is session-dependent — not universally reliable

Previously assumed to be consistently strongly negative. With five sessions:

- Strong negative (−0.627 to −0.848): M104, NGC7380, M13
- Weak or negligible (−0.061 to +0.240): NGC6910-B, NGC6910-C

In the NGC6910 sessions, sky transparency was the dominant driver and FWHM
varied independently of star count. **Both metrics are needed — they are not
redundant but they are not consistently correlated either.**

### 3. Eccentricity is consistently independent of background metrics

All r < 0.49 against any background metric across all five sessions, and the
higher values (NGC6910-C: +0.479 with Bg Median) are driven by a specific
atmospheric event, not a structural relationship. Eccentricity measures tracking
and optical quality — phenomena no other metric reliably captures.

### 4. The SNR estimator has a confirmed PSF artifact across multiple sessions

FWHM vs SNR is positive in all five sessions (+0.257 to +0.800). This is not
a coincidence — worse-seeing frames produce bloated stars with more integrated
flux, which the current estimator reads as higher SNR. In M104, the extreme
outlier frames (FWHM > 5) have SNR of 7.5 vs a session mean of ~5.9. **The SNR
estimator needs revision before this metric can be trusted for classification.**

### 5. Star Count vs Bg Median is the most consistent non-trivial correlation

Negative in all five sessions (−0.313 to −0.957). Even in the narrowband session
with near-constant Bg Median, the correlation is −0.313. Higher sky background
consistently corresponds to fewer detected stars. This is physically meaningful
and consistent across all session types.

### 6. Bg Gradient is session-dependent and unreliable cross-session

Sign reverses between broadband and narrowband sessions. Moderate correlations
with Bg Median in some sessions, negligible in others. **Should be user-
disableable per threshold profile.**

### 7. The session driver determines the correlation structure

| Driver            | Sessions             | Dominant effect                                       |
| ----------------- | -------------------- | ----------------------------------------------------- |
| Sky transparency  | NGC6910-B, NGC6910-C | SNR/StarCount/BgMedian cluster; FWHM independent      |
| Airmass           | M104, NGC7380        | FWHM/StarCount/BgMedian all co-vary                   |
| Stable high-alt   | M13                  | All metrics near-independent; only BgMedian/StarCount |
| Narrowband filter | NGC7380              | BgMedian near-constant; Bg Std Dev reduced variance   |

---

##### Recommendation (Five Sessions — Final)

#### Remove

| Metric      | Rationale                                                                                                                                                                                                                                                                                                |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Bg Std Dev  | r = 0.920–0.999 with Bg Median across all five sessions. Redundant in every case examined. Remove from engine.                                                                                                                                                                                           |
| Bg Gradient | Session-dependent; sign reverses between broadband and narrowband; uniquely caught failure modes (spatial gradients) are almost always constant across a session and therefore carry near-zero sigma deviation for every frame, making them unable to trigger rejection in practice. Remove from engine. |

#### Keep without question

| Metric       | Rationale                                                                                                               |
| ------------ | ----------------------------------------------------------------------------------------------------------------------- |
| FWHM         | Primary seeing metric. Essential in all session types regardless of correlation structure.                              |
| Eccentricity | Independent of background metrics in all sessions. Catches tracking/optical errors nothing else detects.                |
| Star Count   | Consistent strong negative correlation with Bg Median across all sessions. Catches dropout independently of seeing.     |
| Bg Median    | Best single representative of sky background. Consistent across all session types. Essential counterpart to Star Count. |

#### Make optional (user-disableable per threshold profile)

| Metric | Rationale                                                                                                                   |
| ------ | --------------------------------------------------------------------------------------------------------------------------- |
| SNR    | Confirmed PSF artifact across all five sessions. Needs estimator revision. Treat with caution until revised; make optional. |

#### Action items

1. **Remove Bg Std Dev and Bg Gradient from the analysis engine.** Both confirmed removable across all five sessions. No further data needed to support this decision.
2. **Revise the SNR estimator.** The current estimator rewards integrated star flux rather than signal quality. This artifact appears in all five sessions. The revised estimator should account for PSF size when computing SNR — a frame with 2× the FWHM should not score higher SNR for the same target.
3. **Add per-metric enable/disable to threshold profiles.** Allows users to disable SNR until the estimator is revised.

---

### Caveats

Analysis is based on five sessions (409 total frames) covering broadband sky-brightness, broadband transparency variation, broadband airmass, narrowband airmass, and broadband high-altitude stable conditions. All sessions are from the same equipment setup. Correlation structure will vary with different optical systems, mount quality, and sky conditions. The core four metrics (FWHM, Eccentricity, Star Count, Bg Median) are computed for all sessions regardless of filter type or session character — they are the minimum set that provides reliable discriminating power across all conditions examined. 
