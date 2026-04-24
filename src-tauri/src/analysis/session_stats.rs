// analysis/session_stats.rs — session-level statistics for AnalyzeFrames
// Computes mean and std dev for each metric across all frames in a session,
// then derives sigma deviations and PXSCORE/PXFLAG classifications.

use crate::analysis::AnalysisResult;
use crate::analysis::PxFlag;
use serde::{Deserialize, Serialize};

// ── Threshold table ───────────────────────────────────────────────────────────
// Default thresholds per spec §9.9.
// Sigma-based: deviation from session mean in units of session std dev.
// Absolute: fixed value regardless of session statistics.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricThresholds {
    pub suspect: f32,
    pub reject:  f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisThresholds {
    // Sigma-based (higher deviation = worse, except SNR and star count)
    pub background_median:   MetricThresholds,  // +σ
    pub background_stddev:   MetricThresholds,  // +σ
    pub background_gradient: MetricThresholds,  // +σ
    pub snr_estimate:        MetricThresholds,  // -σ (lower SNR = worse)
    pub fwhm:                MetricThresholds,  // +σ
    pub star_count:          MetricThresholds,  // -σ (fewer stars = worse)

    // Absolute
    pub highlight_clipping:  MetricThresholds,  // fraction (not %)
    pub eccentricity:        MetricThresholds,  // absolute 0.0–1.0
}

impl Default for AnalysisThresholds {
    fn default() -> Self {
        Self {
            background_median:   MetricThresholds { suspect: 1.5, reject: 2.5 },
            background_stddev:   MetricThresholds { suspect: 1.5, reject: 2.5 },
            background_gradient: MetricThresholds { suspect: 1.5, reject: 2.5 },
            snr_estimate:        MetricThresholds { suspect: 1.5, reject: 2.5 },
            fwhm:                MetricThresholds { suspect: 1.5, reject: 2.5 },
            star_count:          MetricThresholds { suspect: 1.0, reject: 1.5 },
            highlight_clipping:  MetricThresholds { suspect: 0.001, reject: 0.005 }, // 0.1% / 0.5%
            eccentricity:        MetricThresholds { suspect: 0.65, reject: 0.80 },
        }
    }
}

// ── Filter-adjusted weights ───────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MetricWeights {
    pub background_median:   f32,
    pub background_stddev:   f32,
    pub background_gradient: f32,
    pub snr_estimate:        f32,
    pub fwhm:                f32,
    pub eccentricity:        f32,
    pub star_count:          f32,
    pub highlight_clipping:  f32,
}

impl MetricWeights {
    /// Broadband weights (FILTER = ircut or absent)
    pub fn broadband() -> Self {
        Self {
            fwhm:                5.0,
            snr_estimate:        4.0,
            eccentricity:        3.0,
            background_stddev:   2.0,
            background_gradient: 2.0,
            star_count:          2.0,
            background_median:   1.0,
            highlight_clipping:  1.0,
        }
    }

    /// Narrowband/duo-band weights (FILTER = duo)
    /// Star count dropped to 0 — unreliable under duo-band filter.
    pub fn narrowband() -> Self {
        Self {
            fwhm:                5.0,
            snr_estimate:        4.0,
            eccentricity:        3.0,
            background_stddev:   2.0,
            background_gradient: 2.0,
            star_count:          0.0,
            background_median:   1.0,
            highlight_clipping:  1.0,
        }
    }

    pub fn total(&self) -> f32 {
        self.background_median
            + self.background_stddev
            + self.background_gradient
            + self.snr_estimate
            + self.fwhm
            + self.eccentricity
            + self.star_count
            + self.highlight_clipping
    }

    /// Select weights based on FILTER keyword value.
    /// Defaults to broadband if keyword is absent or unrecognized.
    pub fn for_filter(filter: Option<&str>) -> Self {
        match filter {
            Some(f) if f.to_lowercase().contains("duo") => Self::narrowband(),
            _ => Self::broadband(),
        }
    }
}

// ── Session statistics ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct MetricStats {
    pub mean:   f32,
    pub stddev: f32,
}

impl MetricStats {
    fn from_values(values: &[f32]) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        let n = values.len() as f32;
        let mean = values.iter().sum::<f32>() / n;
        let variance = values.iter().map(|&x| (x - mean) * (x - mean)).sum::<f32>() / n;
        Self { mean, stddev: variance.sqrt() }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    pub background_median:   MetricStats,
    pub background_stddev:   MetricStats,
    pub background_gradient: MetricStats,
    pub highlight_clipping:  MetricStats,
    pub snr_estimate:        MetricStats,
    pub fwhm:                MetricStats,
    pub eccentricity:        MetricStats,
    pub star_count:          MetricStats,
}

/// Compute session-level mean and std dev for each metric across all results.
pub fn compute_session_stats(results: &[&AnalysisResult]) -> SessionStats {
    macro_rules! collect {
        ($field:ident) => {
            results.iter()
                .filter_map(|r| r.$field)
                .collect::<Vec<f32>>()
        };
    }

    SessionStats {
        background_median:   MetricStats::from_values(&collect!(background_median)),
        background_stddev:   MetricStats::from_values(&collect!(background_stddev)),
        background_gradient: MetricStats::from_values(&collect!(background_gradient)),
        highlight_clipping:  MetricStats::from_values(&collect!(highlight_clipping)),
        snr_estimate:        MetricStats::from_values(&collect!(snr_estimate)),
        fwhm:                MetricStats::from_values(&collect!(fwhm)),
        eccentricity:        MetricStats::from_values(&collect!(eccentricity)),
        star_count:          MetricStats::from_values(
            &results.iter()
                .filter_map(|r| r.star_count.map(|v| v as f32))
                .collect::<Vec<f32>>()
        ),
    }
}

// ── Sigma deviation ───────────────────────────────────────────────────────────

/// Compute signed sigma deviation: (value - mean) / stddev.
/// Returns 0.0 if stddev is zero (all frames identical).
fn sigma_dev(value: f32, stats: &MetricStats) -> f32 {
    if stats.stddev < f32::EPSILON {
        return 0.0;
    }
    (value - stats.mean) / stats.stddev
}

// ── Classification ────────────────────────────────────────────────────────────

/// Classify a single frame given its metrics, session stats, and thresholds.
/// Returns (PxFlag, PXSCORE 0–100).
pub fn classify_frame(
    result:     &AnalysisResult,
    stats:      &SessionStats,
    thresholds: &AnalysisThresholds,
    weights:    &MetricWeights,
) -> (PxFlag, u32) {
    let total_weight = weights.total();

    // ── Per-metric penalty and flag evaluation ────────────────────────────────
    // penalty: 0.0 = perfect, 1.0 = at reject threshold, >1.0 = beyond reject
    // For sigma metrics: penalty = |deviation| / reject_threshold
    // For absolute metrics: penalty = value / reject_threshold

    struct MetricEval {
        is_reject:  bool,
        is_suspect: bool,
        penalty:    f32,
        weight:     f32,
    }

    let mut evals: Vec<MetricEval> = Vec::with_capacity(8);

    // Helper: higher-is-worse sigma metric (background median, stddev, gradient, FWHM)
    let sigma_high = |value: Option<f32>, stats: &MetricStats, thresh: &MetricThresholds, weight: f32| -> MetricEval {
        match value {
            None => MetricEval { is_reject: false, is_suspect: false, penalty: 0.0, weight },
            Some(v) => {
                let dev = sigma_dev(v, stats);
                let penalty = (dev / thresh.reject).max(0.0).min(2.0);
                MetricEval {
                    is_reject:  dev >= thresh.reject,
                    is_suspect: dev >= thresh.suspect,
                    penalty,
                    weight,
                }
            }
        }
    };

    // Helper: lower-is-worse sigma metric (SNR, star count)
    let sigma_low = |value: Option<f32>, stats: &MetricStats, thresh: &MetricThresholds, weight: f32| -> MetricEval {
        match value {
            None => MetricEval { is_reject: false, is_suspect: false, penalty: 0.0, weight },
            Some(v) => {
                let dev = sigma_dev(v, stats); // negative = worse
                let penalty = ((-dev) / thresh.reject).max(0.0).min(2.0);
                MetricEval {
                    is_reject:  dev <= -thresh.reject,
                    is_suspect: dev <= -thresh.suspect,
                    penalty,
                    weight,
                }
            }
        }
    };

    // Helper: absolute metric (highlight clipping, eccentricity)
    let absolute = |value: Option<f32>, thresh: &MetricThresholds, weight: f32| -> MetricEval {
        match value {
            None => MetricEval { is_reject: false, is_suspect: false, penalty: 0.0, weight },
            Some(v) => {
                let penalty = (v / thresh.reject).max(0.0).min(2.0);
                MetricEval {
                    is_reject:  v >= thresh.reject,
                    is_suspect: v >= thresh.suspect,
                    penalty,
                    weight,
                }
            }
        }
    };

    evals.push(sigma_high(result.background_median,   &stats.background_median,   &thresholds.background_median,   weights.background_median));
    evals.push(sigma_high(result.background_stddev,   &stats.background_stddev,   &thresholds.background_stddev,   weights.background_stddev));
    evals.push(sigma_high(result.background_gradient, &stats.background_gradient, &thresholds.background_gradient, weights.background_gradient));
    evals.push(sigma_high(result.fwhm,                &stats.fwhm,                &thresholds.fwhm,                weights.fwhm));
    evals.push(sigma_low( result.snr_estimate,        &stats.snr_estimate,        &thresholds.snr_estimate,        weights.snr_estimate));
    evals.push(sigma_low( result.star_count.map(|v| v as f32), &stats.star_count, &thresholds.star_count,          weights.star_count));
    evals.push(absolute(  result.highlight_clipping,  &thresholds.highlight_clipping,                              weights.highlight_clipping));
    evals.push(absolute(  result.eccentricity,        &thresholds.eccentricity,                                    weights.eccentricity));

    // ── PXFLAG: REJECT if any metric exceeds reject threshold ─────────────────
    // SUSPECT if any metric exceeds suspect threshold but none exceed reject
    let flag = if evals.iter().any(|e| e.is_reject) {
        PxFlag::Reject
    } else if evals.iter().any(|e| e.is_suspect) {
        PxFlag::Suspect
    } else {
        PxFlag::Pass
    };

    // ── PXSCORE: weighted penalty → 0–100 (higher = better) ──────────────────
    let weighted_penalty: f32 = if total_weight > 0.0 {
        evals.iter().map(|e| e.penalty * e.weight).sum::<f32>() / total_weight
    } else {
        0.0
    };

    // penalty of 0.0 → score 100, penalty of 1.0 → score 50, penalty of 2.0 → score 0
    let score = ((1.0 - weighted_penalty * 0.5) * 100.0).clamp(0.0, 100.0).round() as u32;

    (flag, score)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::AnalysisResult;

    fn make_result(filename: &str, fwhm: f32, ecc: f32, snr: f32, stars: u32, bg: f32) -> AnalysisResult {
        AnalysisResult {
            filename:             filename.to_string(),
            background_median:    Some(bg),
            background_stddev:    Some(bg * 0.1),
            background_gradient:  Some(bg * 0.05),
            highlight_clipping:   Some(0.0),
            snr_estimate:         Some(snr),
            fwhm:                 Some(fwhm),
            eccentricity:         Some(ecc),
            star_count:           Some(stars),
            flag:                 None,
        }
    }

    #[test]
    fn test_session_stats_basic() {
        let r1 = make_result("f1", 2.5, 0.4, 6.0, 600, 0.05);
        let r2 = make_result("f2", 2.7, 0.5, 5.8, 580, 0.05);
        let r3 = make_result("f3", 2.6, 0.45, 6.1, 610, 0.05);
        let results = vec![&r1, &r2, &r3];
        let stats = compute_session_stats(&results);
        assert!((stats.fwhm.mean - 2.6).abs() < 0.01);
        assert!(stats.fwhm.stddev > 0.0);
    }

    #[test]
    fn test_classify_average_frame_passes() {
        let r1 = make_result("f1", 2.5, 0.4, 6.0, 600, 0.05);
        let r2 = make_result("f2", 2.5, 0.4, 6.0, 600, 0.05);
        let r3 = make_result("f3", 2.5, 0.4, 6.0, 600, 0.05);
        let results = vec![&r1, &r2, &r3];
        let stats = compute_session_stats(&results);
        let thresholds = AnalysisThresholds::default();
        let weights = MetricWeights::broadband();
        let (flag, score) = classify_frame(&r1, &stats, &thresholds, &weights);
        assert_eq!(flag, PxFlag::Pass);
        assert!(score >= 90, "score {} should be high for average frame", score);
    }

    #[test]
    fn test_classify_bad_eccentricity_rejects() {
        let r1 = make_result("f1", 2.5, 0.4,  6.0, 600, 0.05);
        let r2 = make_result("f2", 2.5, 0.4,  6.0, 600, 0.05);
        let r3 = make_result("f3", 2.5, 0.85, 6.0, 600, 0.05); // eccentricity above reject (0.80)
        let results = vec![&r1, &r2, &r3];
        let stats = compute_session_stats(&results);
        let thresholds = AnalysisThresholds::default();
        let weights = MetricWeights::broadband();
        let (flag, _) = classify_frame(&r3, &stats, &thresholds, &weights);
        assert_eq!(flag, PxFlag::Reject);
    }

    #[test]
    fn test_weights_narrowband_zeroes_star_count() {
        let w = MetricWeights::narrowband();
        assert_eq!(w.star_count, 0.0);
        assert_eq!(w.fwhm, 5.0);
    }

    #[test]
    fn test_filter_selection() {
        let w_duo    = MetricWeights::for_filter(Some("duo"));
        let w_ircut  = MetricWeights::for_filter(Some("ircut"));
        let w_none   = MetricWeights::for_filter(None);
        let w_duo_upper = MetricWeights::for_filter(Some("DUO"));
        assert_eq!(w_duo.star_count,   0.0);
        assert_eq!(w_ircut.star_count, 2.0);
        assert_eq!(w_none.star_count,  2.0);
        assert_eq!(w_duo_upper.star_count, 0.0);
    }

    #[test]
    fn test_score_perfect_frame() {
        // All frames identical → all deviations zero → score 100
        let r = make_result("f1", 2.5, 0.4, 6.0, 600, 0.05);
        let results = vec![&r, &r, &r];
        let stats = compute_session_stats(&results);
        let (_, score) = classify_frame(&r, &stats, &AnalysisThresholds::default(), &MetricWeights::broadband());
        assert!(score >= 90, "score {} should be high for perfect frame", score);
    }
}


// ----------------------------------------------------------------------
