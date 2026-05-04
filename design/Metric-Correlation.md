# Photyx AnalyzeFrames — Cross-Session Analysis Findings

**Date:** May 2026  
**Sessions analyzed:** 5 broadband sessions, 753 total frames, 67 rejections (8.9%)

---

## Sessions

| Session | Target           | Exp  | Bin | Frames | Rejects | Primary failures                                                 |
| ------- | ---------------- | ---- | --- | ------ | ------- | ---------------------------------------------------------------- |
| NGC7380 | Wizard Nebula    | 180s | 1×1 | 130    | 16      | Twilight ramp (start) + seeing events                            |
| M104    | Sombrero Galaxy  | 60s  | 1×1 | 62     | 5       | Focus excursion                                                  |
| M101    | Pinwheel Galaxy  | 180s | 2×2 | 80     | 4       | Seeing spike + dawn ramp (end)                                   |
| M100    | Blowdryer Galaxy | 30s  | 1×1 | 335    | 26      | Cloud block + seeing spike + horizon ramp                        |
| M82     | Cigar Galaxy     | 180s | 1×1 | 146    | 16      | Horizon seeing ramp (start) + turbulence spikes + post-gap event |

---

## Headline Finding: The Current Thresholds Are Correct

**Do not change the default thresholds.**

Across 67 rejections in 5 diverse sessions — different targets, seeing conditions, equipment, binning, exposure times, and sky conditions — there were **zero false positives**. Every rejected frame was genuinely inferior. The three borderline cases (NGC7380 frames 9–11, M82 frame 19, M101 frame 78) all represent real frame quality degradation. The sigma thresholds are doing exactly what they should.

Current defaults to retain:

| Metric            | Default           | Direction   |
| ----------------- | ----------------- | ----------- |
| FWHM              | +2.5σ             | High is bad |
| Eccentricity      | > 0.85 (absolute) | High is bad |
| Star Count        | −1.5σ             | Low is bad  |
| SNR               | −2.5σ             | Low is bad  |
| Background Median | +2.5σ             | High is bad |

---

## Metric Performance

### FWHM

- Fired correctly in **all 5 sessions**. Zero false positives.
- Most universal metric — the only one present in every session's rejections.
- The session-relative nature is a strength: M82's wide-seeing session had a large FWHM std that correctly absorbed frame 105 at 4.407px (+2.85σ), which would have been an unambiguous reject in the tight M101 session. The threshold is self-adapting to conditions.

### Eccentricity

- Fired correctly in NGC7380 (seeing events) and M82 (horizon seeing ramp). Zero false positives.
- Co-fires with FWHM for seeing and tracking events.
- **Key diagnostic:** eccentricity is *lower* than normal during defocus events (symmetric circular blob vs session mean), and *higher* during seeing/tracking events (elongated stars). This directional signature distinguishes the failure type and is useful for rejection category annotation.

### Star Count

- Fired correctly in **all 5 sessions**. Zero false positives.
- The second most important metric and the **only one that catches transparency events**.
- Caught the entire 22-frame M100 cloud block (visually confirmed during blink review) while background median showed nothing — proving cloud attenuation can be invisible to background metrics.
- Fires on horizon extinction before FWHM does, making it the early-warning detector for low-altitude targets (M82 opening ramp).
- Also fires on twilight/dawn ramps in combination with background median.

### SNR

- Purely a corroborating metric. Never drove a rejection alone across all 5 sessions.
- Co-fires when FWHM or transparency degradation is severe.
- Adds confidence to multi-metric rejections but carries no unique detection capability.
- Zero false positives.

### Background Median

- Fired correctly in exactly two sessions: NGC7380 (twilight startup) and M101 (dawn ending). Stayed correctly silent in M104, M100, and M82.
- Zero false positives, though NGC7380 frames 9–11 are borderline (~2.5–2.7σ elevated but with normal optics).
- **Key insight:** background std varies enormously across sessions (0.00041 to 0.00135) depending on sky stability. The same absolute sky brightness change can appear as very different sigma values in different sessions. This is correct behavior — a tight stable sky makes the metric more sensitive, which is appropriate.
- The M82 horizon ramp reached +3.85σ background but star count caught the frames first, so background was not the deciding metric.

---

## Three Rejection Categories

Five sessions consistently produced three physically distinct failure modes. These are cleanly separable and have different implications for SFS integration strategy.

### Category 1 — Optical Quality

**Trigger metrics:** FWHM elevated (primary); eccentricity elevated or depressed (secondary)  
**Physical causes:** Atmospheric turbulence, focus drift, tracking error  
**SFS recoverable:** **No.** PSF distortion (bloated or elongated stars) causes halos and artifacts that persist in the integrated stack at any integration weight. These frames should be hard-excluded.  
**Eccentricity direction:** High eccentricity = seeing or tracking (elongated); Low eccentricity = defocus (symmetric blob)  
**Observed in:** M104 (focus excursion, ecc below normal), M82 frames 113/114/122 (turbulence spikes), M101 frame 7 (isolated seeing spike), NGC7380 frames 61/66/69/70/126

### Category 2 — Transparency

**Trigger metrics:** Star count below threshold (primary); SNR low (secondary); background median unchanged  
**Physical causes:** Cloud, haze, aerosols — attenuates target signal without brightening the sky background  
**SFS recoverable:** **Partially.** SFS will correctly assign lower weights. However, severely attenuated frames contribute attenuated signal at reduced weight — their photons still count, but less. Whether to pass or hard-exclude depends on severity.  
**Observed in:** M100 frames 278–299 (visually confirmed cloud, -3σ star count), M82 frame 138 (post-gap severe event, -5.9σ stars), M100 frames 333–335 (horizon extinction without sky brightening)

### Category 3 — Sky Brightness / Horizon Effects

**Trigger metrics:** Background median elevated (primary); star count low (secondary, due to elevated sky suppressing faint star detection)  
**Physical causes:** Astronomical twilight, dawn, target near horizon (atmospheric depth), light pollution spikes  
**SFS recoverable:** **Yes (mild) to Partially (severe).** The PSF is undamaged — stars are correctly shaped, sky is just brighter. SFS will downweight these frames. Their photons are real target photons and they contribute usable signal at reduced weight. Only the most extreme cases (>4–5σ background) represent a noise floor elevation that significantly degrades the stack.  
**Temporal pattern:** In all 5 sessions, Category 3 rejections occurred **exclusively at session start or session end** — never mid-session. This is a reliable diagnostic for the twilight/horizon failure mode.  
**Observed in:** NGC7380 frames 1–11 (startup twilight), M101 frames 78–80 (dawn ending), M82 frames 1–10 (horizon seeing + sky), M100 frame 1 (single startup frame)

### Multi-Category Frames

Some frames simultaneously satisfy multiple categories. M82's opening frames (1–10) show both elevated eccentricity/FWHM (optical — poor horizon seeing) and elevated background + suppressed stars (sky brightness — target at low altitude). Both categories should be reported, not just the worst.

---

## Implications for SFS Integration Strategy

| Category                             | Hard Exclude? | Pass to SFS? | Notes                                        |
| ------------------------------------ | ------------- | ------------ | -------------------------------------------- |
| Optical Quality                      | Yes           | No           | Star halos survive at any weight             |
| Transparency (mild, -2 to -3σ stars) | No            | Yes          | SFS downweights appropriately                |
| Transparency (severe, >-3σ stars)    | Consider      | With caution | Signal attenuated >20%, limited contribution |
| Sky Brightness (mild, 2.5–3.5σ bg)   | No            | Yes          | Undamaged PSF, SFS handles it                |
| Sky Brightness (severe, >4σ bg)      | Consider      | With caution | Noise floor impact on stack                  |
| Multi-category: optical + anything   | Yes           | No           | Optical damage is the deciding factor        |

---

## Implementation Recommendations (Priority Order)

### 1. Rejection Category Annotation *(High value, fits existing architecture)*

Add a `rejection_category` field to `AnalysisResult` alongside the existing `triggered_by` field. Assign category from the metric combination that triggered rejection:

| Triggered metrics         | Category                   |
| ------------------------- | -------------------------- |
| FWHM alone, or FWHM + Ecc | `optical`                  |
| Stars alone (bg normal)   | `transparency`             |
| Bg alone, or Bg + Stars   | `sky_brightness`           |
| FWHM + Stars (bg normal)  | `optical` + `transparency` |
| Bg + Stars + FWHM         | all three categories       |

Surface this in the UI:

- **Analysis Results table:** color-coded badge (e.g. 🔴 optical, 🟡 transparency, 🔵 sky brightness)
- **Analysis Graph:** reject dots in three colors instead of current single reject color
- **Blink overlay:** border color on rejected frames matches category

This gives the user the information needed to make the SFS vs hard-exclude decision without changing any thresholds.

### 2. Broadband / Narrowband Parameter *(Sound idea, needs data)*

Implement the `broadband` / `narrowband` argument to `AnalyzeFrames` with separate default threshold sets. Narrowband sessions have fewer detectable stars, naturally lower sky backgrounds, and smaller star fields — all of which affect what "normal" looks like for star count and background median. Specific narrowband defaults should be proposed only after analyzing narrowband session data.

### 3. Temporal Monotonicity Detection *(Lower priority, more complex)*

Detect sessions where background rises or falls monotonically across the first or last N frames, and annotate those sky brightness rejections as the `twilight_ramp` subtype. This would allow the UI to display "sky brightness — twilight ramp" vs "sky brightness — light pollution spike," which are physically different events with different SFS implications. Useful refinement but not essential for the initial category annotation implementation.

---

## Key Observations Not in the Original Spec

1. **Eccentricity direction matters.** The current spec treats eccentricity as a single-direction metric (high is bad). But low eccentricity on a rejected frame is diagnostic for defocus, while high eccentricity indicates seeing or tracking. This directional information should be preserved in the category annotation.

2. **Star count is the transparency detector.** No other metric catches cloud without background elevation. This confirms star count is not a secondary metric — it's primary for an entire class of failure modes.

3. **Background median is a differential metric, not absolute.** A session imaged entirely in heavy light pollution will have a high but stable background and produce zero background-driven rejections. This is correct behavior. The metric catches *changes* from session baseline, not absolute sky brightness.

4. **The 75-minute gap in M82 (frames 137→152) produced the most severe single-frame event in the dataset** — frame 138 at FWHM +4.90σ and stars −5.90σ simultaneously. Post-session-gap frames should always be reviewed carefully in blink; the optics and atmosphere reset during the gap.

5. **Binning does not affect classification accuracy.** M101's 2×2 binned session produced the same category structure as unbinned sessions. Star counts and absolute FWHM values scale with binning but the sigma-relative thresholds adapt correctly.
