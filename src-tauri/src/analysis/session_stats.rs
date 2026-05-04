// analysis/session_stats.rs — session-level statistics for AnalyzeFrames
// PASS / REJECT classification only — no SUSPECT, no PXSCORE.
// Philosophy: remove extreme outliers only; PI weighting handles fine-grained quality.

use crate::analysis::AnalysisResult;
use crate::analysis::PxFlag;
use serde::{Deserialize, Serialize};

// ── Threshold table ───────────────────────────────────────────────────────────
// Sigma-based: deviation from session mean in units of session std dev.
// Absolute: fixed value regardless of session statistics.
// Only reject thresholds — SUSPECT has been removed.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricThresholds {
    pub reject: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisThresholds {
    // Sigma-based (higher deviation = worse, except SNR and star count)
    pub background_median:   MetricThresholds,  // +σ
    pub snr_estimate:        MetricThresholds,  // retained for display; not used in classification
    pub fwhm:                MetricThresholds,  // +σ
    pub star_count:          MetricThresholds,  // -σ (fewer stars = worse)

    // Absolute
    pub eccentricity:        MetricThresholds,  // absolute 0.0–1.0
}

impl Default for AnalysisThresholds {
    fn default() -> Self {
        Self {
            background_median:   MetricThresholds { reject: 2.5 },
            snr_estimate:        MetricThresholds { reject: 2.5 },
            fwhm:                MetricThresholds { reject: 2.5 },
            // Raised from 1.5 to 3.0: mild transparency events are better handled
            // by SFS weighting than hard rejection.
            star_count:          MetricThresholds { reject: 3.0 },
            eccentricity:        MetricThresholds { reject: 0.85 },
        }
    }
}

// ── Filter-adjusted weights ───────────────────────────────────────────────────
// Weights are used only for informational purposes — classification is driven
// by individual metric thresholds, not weighted scores.

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MetricWeights {
    pub background_median:   f32,
    pub background_stddev:   f32,
    pub background_gradient: f32,
    pub snr_estimate:        f32,
    pub fwhm:                f32,
    pub eccentricity:        f32,
    pub star_count:          f32,
}

#[allow(dead_code)]
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
        }
    }

    /// Select weights based on FILTER keyword value.
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

/// Two-pass iterative sigma clipping.
///
/// Pass 2a: compute initial stats across all frames.
/// Pass 2b: identify frames where any metric deviates > OUTLIER_SIGMA_THRESHOLD σ
///          from the initial mean. These are extreme outliers (clouds, satellites, etc.)
/// Pass 2c: recompute stats excluding those outlier frames.
///
/// Returns (SessionStats, HashSet<filename>) where the set contains paths of
/// outlier-excluded frames. All frames — including outliers — are still classified
/// in Pass 2 of AnalyzeFrames; they will almost certainly be REJECT anyway.
pub fn compute_session_stats_iterative(
    results: &[&AnalysisResult],
) -> (SessionStats, std::collections::HashSet<String>) {
    use crate::settings::defaults::OUTLIER_SIGMA_THRESHOLD;
    let threshold = OUTLIER_SIGMA_THRESHOLD as f32;

    // Pass 2a — initial stats across all frames
    let initial_stats = compute_session_stats(results);

    // Pass 2b — identify outliers: any frame where any metric deviates > threshold σ
    // Eccentricity is absolute so we skip it here — sigma clipping doesn't apply.
    let mut outlier_paths = std::collections::HashSet::new();

    for r in results {
        let mut is_outlier = false;

        macro_rules! check_outlier {
            ($field:ident, $stats:expr) => {
                if let Some(v) = r.$field {
                    let s = &$stats;
                    if s.stddev > f32::EPSILON {
                        let dev = ((v - s.mean) / s.stddev).abs();
                        if dev > threshold {
                            is_outlier = true;
                        }
                    }
                }
            };
        }

        check_outlier!(background_median, initial_stats.background_median);
        check_outlier!(snr_estimate,      initial_stats.snr_estimate);
        check_outlier!(fwhm,              initial_stats.fwhm);

        if let Some(sc) = r.star_count {
            let s = &initial_stats.star_count;
            if s.stddev > f32::EPSILON {
                let dev = ((sc as f32 - s.mean) / s.stddev).abs();
                if dev > threshold {
                    is_outlier = true;
                }
            }
        }

        if is_outlier {
            outlier_paths.insert(r.filename.clone());
        }
    }

    // Pass 2c — recompute stats excluding outliers
    // If ALL frames are outliers (degenerate session), fall back to initial stats.
    let clean: Vec<&AnalysisResult> = results.iter()
        .copied()
        .filter(|r| !outlier_paths.contains(&r.filename))
        .collect();

    let final_stats = if clean.is_empty() {
        initial_stats
    } else {
        compute_session_stats(&clean)
    };

    (final_stats, outlier_paths)
}

// ── Sigma deviation ───────────────────────────────────────────────────────────

fn sigma_dev(value: f32, stats: &MetricStats) -> f32 {
    if stats.stddev < f32::EPSILON {
        return 0.0;
    }
    (value - stats.mean) / stats.stddev
}

// ── Classification ────────────────────────────────────────────────────────────

/// Classify a single frame — PASS or REJECT only.
/// Returns (PxFlag, Vec<String>) where the Vec contains names of metrics that
/// triggered REJECT (empty for PASS).
///
/// NOTE: SNR is intentionally excluded from rejection classification.
/// It is retained in AnalysisResult for diagnostic display only.
/// Cross-session analysis showed SNR never drove a unique rejection
/// not already caught by FWHM or StarCount.
pub fn classify_frame(
    result:     &AnalysisResult,
    stats:      &SessionStats,
    thresholds: &AnalysisThresholds,
) -> (PxFlag, Vec<String>) {
    let mut triggered: Vec<String> = Vec::new();

    // Higher-is-worse sigma metrics
    macro_rules! check_high {
        ($field:expr, $stats:expr, $thresh:expr, $name:expr) => {
            if let Some(v) = $field {
                if sigma_dev(v, $stats) >= $thresh.reject {
                    triggered.push($name.to_string());
                }
            }
        };
    }

    // Lower-is-worse sigma metrics
    macro_rules! check_low {
        ($field:expr, $stats:expr, $thresh:expr, $name:expr) => {
            if let Some(v) = $field {
                if sigma_dev(v, $stats) <= -$thresh.reject {
                    triggered.push($name.to_string());
                }
            }
        };
    }

    check_high!(result.background_median, &stats.background_median, &thresholds.background_median, "BackgroundMedian");
    check_high!(result.fwhm,              &stats.fwhm,              &thresholds.fwhm,              "FWHM");
    // SNR deliberately excluded — see function doc comment above
    check_low!( result.star_count.map(|v| v as f32), &stats.star_count, &thresholds.star_count,    "StarCount");

    // Absolute metric
    if let Some(ecc) = result.eccentricity {
        if ecc >= thresholds.eccentricity.reject {
            triggered.push("Eccentricity".to_string());
        }
    }

    let flag = if triggered.is_empty() {
        PxFlag::Pass
    } else {
        PxFlag::Reject
    };

    (flag, triggered)
}

// ── Rejection category ────────────────────────────────────────────────────────

/// Derive a rejection category string from the triggered metric names.
///
/// Three categories, ordered by severity / recoverability (least recoverable first):
///   O = Optical quality  — FWHM or Eccentricity triggered
///   B = Sky brightness   — BackgroundMedian triggered (dominates StarCount)
///   T = Transparency     — StarCount triggered without BackgroundMedian
///
/// Ordering rules:
///   O always leads when present (optical damage is the decisive factor).
///   When both B and T are present, B leads (sky brightness causes star suppression,
///   so B is the root cause).
///   Multi-category strings are concatenated: "O", "T", "B", "OT", "OB", "BT", "OBT"
///
/// Returns None for PASS frames (empty triggered list).
pub fn categorize_rejection(triggered: &[String]) -> Option<String> {
    if triggered.is_empty() {
        return None;
    }

    let has_optical = triggered.iter().any(|t| t == "FWHM" || t == "Eccentricity");
    let has_bg      = triggered.iter().any(|t| t == "BackgroundMedian");
    let has_stars   = triggered.iter().any(|t| t == "StarCount");

    let mut cat = String::new();

    if has_optical { cat.push('O'); }

    if has_bg && has_stars {
        // Sky brightness is the root cause of star suppression → B leads T
        cat.push('B');
        cat.push('T');
    } else if has_bg {
        cat.push('B');
    } else if has_stars {
        cat.push('T');
    }

    if cat.is_empty() {
        // Fallback: triggered by something unexpected — mark as Optical
        Some("O".to_string())
    } else {
        Some(cat)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::AnalysisResult;

    fn make_result(filename: &str, fwhm: f32, ecc: f32, snr: f32, stars: u32, bg: f32) -> AnalysisResult {
        AnalysisResult {
            filename:           filename.to_string(),
            background_median:  Some(bg),
            snr_estimate:       Some(snr),
            fwhm:               Some(fwhm),
            eccentricity:       Some(ecc),
            star_count:         Some(stars),
            flag:               None,
            triggered_by:       vec![],
            rejection_category: None,
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
        let (flag, triggered) = classify_frame(&r1, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Pass);
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_classify_bad_eccentricity_rejects() {
        let r1 = make_result("f1", 2.5, 0.4,  6.0, 600, 0.05);
        let r2 = make_result("f2", 2.5, 0.4,  6.0, 600, 0.05);
        let r3 = make_result("f3", 2.5, 0.86, 6.0, 600, 0.05); // above 0.85
        let results = vec![&r1, &r2, &r3];
        let stats = compute_session_stats(&results);
        let thresholds = AnalysisThresholds::default();
        let (flag, triggered) = classify_frame(&r3, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Reject);
        assert!(triggered.contains(&"Eccentricity".to_string()));
    }

    #[test]
    fn test_snr_does_not_trigger_rejection() {
        // SNR far below session mean should NOT cause rejection
        let g1 = make_result("g1", 2.4, 0.4, 6.0, 600, 0.05);
        let g2 = make_result("g2", 2.5, 0.4, 6.0, 600, 0.05);
        let g3 = make_result("g3", 2.6, 0.4, 6.0, 600, 0.05);
        let g4 = make_result("g4", 2.5, 0.4, 6.0, 600, 0.05);
        let g5 = make_result("g5", 2.4, 0.4, 6.0, 600, 0.05);
        // Very low SNR — should still PASS since SNR is not a rejection metric
        let low_snr = make_result("low", 2.5, 0.4, 0.1, 600, 0.05);
        let results = vec![&g1, &g2, &g3, &g4, &g5];
        let stats = compute_session_stats(&results);
        let thresholds = AnalysisThresholds::default();
        let (flag, triggered) = classify_frame(&low_snr, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Pass);
        assert!(!triggered.contains(&"SNR".to_string()));
    }

    #[test]
    fn test_triggered_by_populated() {
        let g1 = make_result("g1", 2.4, 0.4, 6.0, 600, 0.05);
        let g2 = make_result("g2", 2.5, 0.4, 6.0, 600, 0.05);
        let g3 = make_result("g3", 2.6, 0.4, 6.0, 600, 0.05);
        let g4 = make_result("g4", 2.5, 0.4, 6.0, 600, 0.05);
        let g5 = make_result("g5", 2.4, 0.4, 6.0, 600, 0.05);
        // Bad frame with FWHM far above the session mean
        let bad = make_result("bad", 10.0, 0.4, 6.0, 600, 0.05);
        let good_results = vec![&g1, &g2, &g3, &g4, &g5];
        let stats = compute_session_stats(&good_results);
        let thresholds = AnalysisThresholds::default();
        let (flag, triggered) = classify_frame(&bad, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Reject);
        assert!(triggered.contains(&"FWHM".to_string()));
    }

    #[test]
    fn test_weights_narrowband_zeroes_star_count() {
        let w = MetricWeights::narrowband();
        assert_eq!(w.star_count, 0.0);
        assert_eq!(w.fwhm, 5.0);
    }

    #[test]
    fn test_filter_selection() {
        let w_duo       = MetricWeights::for_filter(Some("duo"));
        let w_ircut     = MetricWeights::for_filter(Some("ircut"));
        let w_none      = MetricWeights::for_filter(None);
        let w_duo_upper = MetricWeights::for_filter(Some("DUO"));
        assert_eq!(w_duo.star_count,       0.0);
        assert_eq!(w_ircut.star_count,     2.0);
        assert_eq!(w_none.star_count,      2.0);
        assert_eq!(w_duo_upper.star_count, 0.0);
    }

    // ── Category tests ────────────────────────────────────────────────────────

    #[test]
    fn test_category_optical_only() {
        assert_eq!(categorize_rejection(&["FWHM".to_string()]), Some("O".to_string()));
        assert_eq!(categorize_rejection(&["Eccentricity".to_string()]), Some("O".to_string()));
        assert_eq!(categorize_rejection(&["FWHM".to_string(), "Eccentricity".to_string()]), Some("O".to_string()));
    }

    #[test]
    fn test_category_transparency_only() {
        assert_eq!(categorize_rejection(&["StarCount".to_string()]), Some("T".to_string()));
    }

    #[test]
    fn test_category_sky_brightness_only() {
        assert_eq!(categorize_rejection(&["BackgroundMedian".to_string()]), Some("B".to_string()));
    }

    #[test]
    fn test_category_sky_brightness_with_stars() {
        // B dominates T when both present
        assert_eq!(
            categorize_rejection(&["BackgroundMedian".to_string(), "StarCount".to_string()]),
            Some("BT".to_string())
        );
    }

    #[test]
    fn test_category_optical_with_transparency() {
        assert_eq!(
            categorize_rejection(&["FWHM".to_string(), "StarCount".to_string()]),
            Some("OT".to_string())
        );
    }

    #[test]
    fn test_category_optical_with_sky_brightness() {
        assert_eq!(
            categorize_rejection(&["FWHM".to_string(), "BackgroundMedian".to_string()]),
            Some("OB".to_string())
        );
    }

    #[test]
    fn test_category_all_three() {
        assert_eq!(
            categorize_rejection(&[
                "FWHM".to_string(),
                "BackgroundMedian".to_string(),
                "StarCount".to_string(),
            ]),
            Some("OBT".to_string())
        );
    }

    #[test]
    fn test_category_none_for_pass() {
        assert_eq!(categorize_rejection(&[]), None);
    }
}
