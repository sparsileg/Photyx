// analysis/session_stats.rs — session-level statistics for AnalyzeFrames
// PASS / REJECT classification only — no SUSPECT, no PXSCORE.
// Philosophy: remove extreme outliers only; PI weighting handles fine-grained quality.

use crate::analysis::AnalysisResult;
use crate::analysis::PxFlag;
use serde::{Deserialize, Serialize};

// ── Threshold table ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricThresholds {
    pub reject: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisThresholds {
    // Sigma-based (higher deviation = worse, except signal_weight and star_count)
    pub background_median:   MetricThresholds,  // +σ
    pub signal_weight:       MetricThresholds,  // -σ (lower = worse)
    pub fwhm:                MetricThresholds,  // +σ
    pub star_count:          MetricThresholds,  // -σ (fewer stars = worse)

    // Absolute
    pub eccentricity:        MetricThresholds,  // absolute 0.0–1.0
}

impl Default for AnalysisThresholds {
    fn default() -> Self {
        Self {
            background_median:   MetricThresholds { reject: 2.5 },
            signal_weight:       MetricThresholds { reject: 2.5 },
            fwhm:                MetricThresholds { reject: 2.5 },
            star_count:          MetricThresholds { reject: 3.0 },
            eccentricity:        MetricThresholds { reject: 0.85 },
        }
    }
}

// ── Filter-adjusted weights ───────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct MetricWeights {
    pub background_median:   f32,
    pub background_stddev:   f32,
    pub background_gradient: f32,
    pub signal_weight:       f32,
    pub fwhm:                f32,
    pub eccentricity:        f32,
    pub star_count:          f32,
}

#[allow(dead_code)]
impl MetricWeights {
    pub fn broadband() -> Self {
        Self {
            fwhm:                5.0,
            signal_weight:       4.0,
            eccentricity:        3.0,
            background_stddev:   2.0,
            background_gradient: 2.0,
            star_count:          2.0,
            background_median:   1.0,
        }
    }

    pub fn narrowband() -> Self {
        Self {
            fwhm:                5.0,
            signal_weight:       4.0,
            eccentricity:        3.0,
            background_stddev:   2.0,
            background_gradient: 2.0,
            star_count:          0.0,
            background_median:   1.0,
        }
    }

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
    pub signal_weight:       MetricStats,
    pub fwhm:                MetricStats,
    pub eccentricity:        MetricStats,
    pub star_count:          MetricStats,
}

// ── Bimodality detection ──────────────────────────────────────────────────────

/// Bimodality coefficient (BC) from skewness and excess kurtosis.
/// BC > 0.555 indicates a bimodal distribution.
/// Reference: Pfister et al. (2013), based on SAS bimodality coefficient.
///
/// Returns None if there are fewer than 4 values (insufficient for kurtosis).
fn bimodality_coefficient(values: &[f32]) -> Option<f32> {
    let n = values.len();
    if n < 4 {
        return None;
    }
    let nf = n as f64;
    let mean = values.iter().map(|&v| v as f64).sum::<f64>() / nf;
    let variance = values.iter().map(|&v| (v as f64 - mean).powi(2)).sum::<f64>() / nf;
    if variance < f64::EPSILON {
        return None;
    }
    let sd = variance.sqrt();
    let skew = values.iter().map(|&v| ((v as f64 - mean) / sd).powi(3)).sum::<f64>() / nf;
    let kurt = values.iter().map(|&v| ((v as f64 - mean) / sd).powi(4)).sum::<f64>() / nf - 3.0;

    // Sample-size correction to kurtosis denominator
    let kurt_denom = kurt + 3.0 * (nf - 1.0).powi(2) / ((nf - 2.0) * (nf - 3.0));
    if kurt_denom.abs() < f64::EPSILON {
        return None;
    }
    let bc = (skew.powi(2) + 1.0) / kurt_denom;
    Some(bc as f32)
}

/// Locate the deepest valley between the two largest peaks in a smoothed
/// histogram of `values`. Returns the value at the valley midpoint.
///
/// `n_bins`: histogram resolution (20 is a good default for 30–100 frames).
fn find_valley(values: &[f32], n_bins: usize) -> Option<f32> {
    if values.len() < 4 || n_bins < 4 {
        return None;
    }
    let lo = values.iter().cloned().fold(f32::INFINITY, f32::min);
    let hi = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    if (hi - lo) < f32::EPSILON {
        return None;
    }
    let bin_width = (hi - lo) / n_bins as f32;

    let mut bins = vec![0usize; n_bins];
    for &v in values {
        let idx = ((v - lo) / bin_width) as usize;
        bins[idx.min(n_bins - 1)] += 1;
    }

    // 3-point smoothing
    let mut smoothed = bins.clone();
    for i in 1..n_bins - 1 {
        smoothed[i] = (bins[i - 1] + bins[i] + bins[i + 1]) / 3;
    }

    // Find the dominant peak in each half
    let half = n_bins / 2;
    let peak1 = (0..half).max_by_key(|&i| smoothed[i])?;
    let peak2 = (half..n_bins).max_by_key(|&i| smoothed[i])?;
    if peak1 >= peak2 {
        return None;
    }

    // Deepest valley between the two peaks
    let valley_idx = (peak1..=peak2).min_by_key(|&i| smoothed[i])?;
    let valley_val = lo + (valley_idx as f32 + 0.5) * bin_width;
    Some(valley_val)
}

/// Bimodality threshold — BC above this value indicates a bimodal distribution.
const BIMODALITY_COEFFICIENT_THRESHOLD: f32 = 0.555;

/// Histogram bin count used for valley detection.
const BIMODAL_HISTOGRAM_BINS: usize = 20;

/// Compute mean and stddev for a metric, with optional bimodality-aware anchoring.
///
/// When `use_bimodal` is true and the distribution is detected as bimodal:
///   - The valley between the two peaks is located.
///   - Mean and stddev are computed from the upper cluster only
///     (values above the valley for `higher_is_better = true`, below for false).
///   - This anchors classification thresholds to the good-quality population,
///     preventing a large block of degraded frames from pulling the mean down
///     and collapsing the reject threshold.
///
/// When `use_bimodal` is false, or bimodality is not detected, the full
/// population mean and stddev are returned unchanged — identical to the
/// previous behaviour.
///
/// To enable bimodal detection for additional metrics in the future, pass
/// `use_bimodal: true` for that metric's values in `compute_session_stats`.
pub fn compute_metric_stats(
    values:           &[f32],
    use_bimodal:      bool,
    higher_is_better: bool,
) -> MetricStats {
    if values.is_empty() {
        return MetricStats::default();
    }

    if use_bimodal {
        if let Some(bc) = bimodality_coefficient(values) {
            if bc > BIMODALITY_COEFFICIENT_THRESHOLD {
                if let Some(valley) = find_valley(values, BIMODAL_HISTOGRAM_BINS) {
                    let upper: Vec<f32> = if higher_is_better {
                        values.iter().cloned().filter(|&v| v > valley).collect()
                    } else {
                        values.iter().cloned().filter(|&v| v < valley).collect()
                    };
                    if upper.len() >= 2 {
                        tracing::info!(
                            "BimodalStats: BC={:.3} > {:.3}, valley={:.3}, \
                             anchoring to {} upper-cluster values",
                            bc,
                            BIMODALITY_COEFFICIENT_THRESHOLD,
                            valley,
                            upper.len(),
                        );
                        return MetricStats::from_values(&upper);
                    }
                }
            }
        }
    }

    MetricStats::from_values(values)
}

/// Compute session stats from a population without bimodal detection.
/// Used internally for the iterative clipping pass on the cleaned subset.
fn compute_session_stats_plain(results: &[&AnalysisResult]) -> SessionStats {
    macro_rules! collect {
        ($field:ident) => {
            results.iter()
                .filter_map(|r| r.$field)
                .collect::<Vec<f32>>()
        };
    }

    let star_vals: Vec<f32> = results.iter()
        .filter_map(|r| r.star_count.map(|v| v as f32))
        .collect();

    SessionStats {
        background_median: MetricStats::from_values(&collect!(background_median)),
        signal_weight:     MetricStats::from_values(&collect!(signal_weight)),
        fwhm:              MetricStats::from_values(&collect!(fwhm)),
        eccentricity:      MetricStats::from_values(&collect!(eccentricity)),
        star_count:        MetricStats::from_values(&star_vals),
    }
}

pub fn compute_session_stats(results: &[&AnalysisResult]) -> SessionStats {
    macro_rules! collect {
        ($field:ident) => {
            results.iter()
                .filter_map(|r| r.$field)
                .collect::<Vec<f32>>()
        };
    }

    let star_vals: Vec<f32> = results.iter()
        .filter_map(|r| r.star_count.map(|v| v as f32))
        .collect();

    SessionStats {
        background_median: MetricStats::from_values(&collect!(background_median)),
        signal_weight:     MetricStats::from_values(&collect!(signal_weight)),
        fwhm:              MetricStats::from_values(&collect!(fwhm)),
        eccentricity:      MetricStats::from_values(&collect!(eccentricity)),
        // Star count uses bimodal-aware anchoring: a large block of cloudy frames
        // produces a bimodal distribution that pulls the session mean down and
        // collapses the reject threshold, causing missed rejections.
        star_count:        compute_metric_stats(&star_vals, true, true),
    }
}

/// Two-pass iterative sigma clipping.
///
/// Bimodal star count stats are computed ONCE from the full population and
/// carried through unchanged. Only the non-bimodal metrics (FWHM, background,
/// signal weight) are recomputed on the cleaned subset after outlier removal.
/// This prevents the bimodal anchor from shifting between passes, which would
/// cause non-deterministic classification results.
pub fn compute_session_stats_iterative(
    results: &[&AnalysisResult],
) -> (SessionStats, std::collections::HashSet<String>) {
    use crate::settings::defaults::OUTLIER_SIGMA_THRESHOLD;
    let threshold = OUTLIER_SIGMA_THRESHOLD as f32;

    // Compute initial stats including bimodal star count anchor from full population
    let initial_stats = compute_session_stats(results);

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
        check_outlier!(signal_weight,     initial_stats.signal_weight);
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

    let clean: Vec<&AnalysisResult> = results.iter()
        .copied()
        .filter(|r| !outlier_paths.contains(&r.filename))
        .collect();

    // Recompute non-bimodal metrics on the cleaned subset, but preserve the
    // bimodal star count anchor from the full population — it must not change.
    let final_stats = if clean.is_empty() {
        initial_stats
    } else {
        let plain = compute_session_stats_plain(&clean);
        SessionStats {
            background_median: plain.background_median,
            signal_weight:     plain.signal_weight,
            fwhm:              plain.fwhm,
            eccentricity:      plain.eccentricity,
            star_count:        initial_stats.star_count, // bimodal anchor preserved
        }
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
pub fn classify_frame(
    result:     &AnalysisResult,
    stats:      &SessionStats,
    thresholds: &AnalysisThresholds,
) -> (PxFlag, Vec<String>) {
    let mut triggered: Vec<String> = Vec::new();

    macro_rules! check_high {
        ($field:expr, $stats:expr, $thresh:expr, $name:expr) => {
            if let Some(v) = $field {
                if sigma_dev(v, $stats) >= $thresh.reject {
                    triggered.push($name.to_string());
                }
            }
        };
    }

    macro_rules! check_low {
        ($field:expr, $stats:expr, $thresh:expr, $name:expr) => {
            if let Some(v) = $field {
                if sigma_dev(v, $stats) <= -$thresh.reject {
                    triggered.push($name.to_string());
                }
            }
        };
    }

    check_high!(result.background_median,             &stats.background_median, &thresholds.background_median, "BackgroundMedian");
    check_high!(result.fwhm,                          &stats.fwhm,              &thresholds.fwhm,              "FWHM");
    check_low!( result.signal_weight,                 &stats.signal_weight,     &thresholds.signal_weight,     "SignalWeight");
    check_low!( result.star_count.map(|v| v as f32),  &stats.star_count,        &thresholds.star_count,        "StarCount");

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
/// O = Optical        — FWHM and/or Eccentricity
/// B = Sky Brightness — BackgroundMedian
/// T = Transparency   — StarCount and/or SignalWeight (without BackgroundMedian)
///
/// Ordering: O always leads. When B and T are both present, B leads T.
pub fn categorize_rejection(triggered: &[String]) -> Option<String> {
    if triggered.is_empty() {
        return None;
    }

    let has_optical = triggered.iter().any(|t| t == "FWHM" || t == "Eccentricity");
    let has_bg      = triggered.iter().any(|t| t == "BackgroundMedian");
    let has_transp  = triggered.iter().any(|t| t == "StarCount" || t == "SignalWeight");

    let mut cat = String::new();

    if has_optical { cat.push('O'); }

    if has_bg && has_transp {
        cat.push('B');
        cat.push('T');
    } else if has_bg {
        cat.push('B');
    } else if has_transp {
        cat.push('T');
    }

    if cat.is_empty() {
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

    fn make_result(filename: &str, fwhm: f32, ecc: f32, sw: f32, stars: u32, bg: f32) -> AnalysisResult {
        AnalysisResult {
            filename:           filename.to_string(),
            background_median:  Some(bg),
            signal_weight:      Some(sw),
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
        let r3 = make_result("f3", 2.5, 0.86, 6.0, 600, 0.05);
        let results = vec![&r1, &r2, &r3];
        let stats = compute_session_stats(&results);
        let thresholds = AnalysisThresholds::default();
        let (flag, triggered) = classify_frame(&r3, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Reject);
        assert!(triggered.contains(&"Eccentricity".to_string()));
    }

    #[test]
    fn test_signal_weight_triggers_rejection() {
        let g1 = make_result("g1", 2.4, 0.4, 6.0, 600, 0.05);
        let g2 = make_result("g2", 2.5, 0.4, 6.0, 600, 0.05);
        let g3 = make_result("g3", 2.6, 0.4, 6.0, 600, 0.05);
        let g4 = make_result("g4", 2.5, 0.4, 6.0, 600, 0.05);
        let g5 = make_result("g5", 2.4, 0.4, 6.0, 600, 0.05);
        let low_sw = make_result("low", 2.5, 0.4, 0.001, 600, 0.05);
        let results = vec![&g1, &g2, &g3, &g4, &g5];
        let stats = compute_session_stats(&results);
        let thresholds = AnalysisThresholds::default();
        let (flag, triggered) = classify_frame(&low_sw, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Reject);
        assert!(triggered.contains(&"SignalWeight".to_string()));
    }

    #[test]
    fn test_triggered_by_populated() {
        let g1 = make_result("g1", 2.4, 0.4, 6.0, 600, 0.05);
        let g2 = make_result("g2", 2.5, 0.4, 6.0, 600, 0.05);
        let g3 = make_result("g3", 2.6, 0.4, 6.0, 600, 0.05);
        let g4 = make_result("g4", 2.5, 0.4, 6.0, 600, 0.05);
        let g5 = make_result("g5", 2.4, 0.4, 6.0, 600, 0.05);
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

    #[test]
    fn test_category_optical_only() {
        assert_eq!(categorize_rejection(&["FWHM".to_string()]), Some("O".to_string()));
        assert_eq!(categorize_rejection(&["Eccentricity".to_string()]), Some("O".to_string()));
        assert_eq!(categorize_rejection(&["FWHM".to_string(), "Eccentricity".to_string()]), Some("O".to_string()));
    }

    #[test]
    fn test_category_transparency_only() {
        assert_eq!(categorize_rejection(&["StarCount".to_string()]), Some("T".to_string()));
        assert_eq!(categorize_rejection(&["SignalWeight".to_string()]), Some("T".to_string()));
        assert_eq!(categorize_rejection(&["StarCount".to_string(), "SignalWeight".to_string()]), Some("T".to_string()));
    }

    #[test]
    fn test_category_sky_brightness_only() {
        assert_eq!(categorize_rejection(&["BackgroundMedian".to_string()]), Some("B".to_string()));
    }

    #[test]
    fn test_category_sky_brightness_with_transparency() {
        assert_eq!(
            categorize_rejection(&["BackgroundMedian".to_string(), "StarCount".to_string()]),
            Some("BT".to_string())
        );
        assert_eq!(
            categorize_rejection(&["BackgroundMedian".to_string(), "SignalWeight".to_string()]),
            Some("BT".to_string())
        );
    }

    #[test]
    fn test_category_optical_with_transparency() {
        assert_eq!(
            categorize_rejection(&["FWHM".to_string(), "StarCount".to_string()]),
            Some("OT".to_string())
        );
        assert_eq!(
            categorize_rejection(&["FWHM".to_string(), "SignalWeight".to_string()]),
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

    #[test]
    fn test_bimodality_coefficient_unimodal() {
        // Tightly clustered unimodal data — BC should be well below 0.555
        let vals: Vec<f32> = (0..50).map(|i| 200.0 + (i as f32 - 25.0) * 2.0).collect();
        let bc = bimodality_coefficient(&vals).unwrap();
        assert!(bc < BIMODALITY_COEFFICIENT_THRESHOLD, "BC={bc} should be < 0.555 for unimodal data");
    }

    #[test]
    fn test_bimodality_coefficient_bimodal() {
        // Two clearly separated clusters — BC should exceed 0.555
        let mut vals: Vec<f32> = (0..20).map(|_| 250.0).collect();
        vals.extend((0..20).map(|_| 50.0_f32));
        let bc = bimodality_coefficient(&vals).unwrap();
        assert!(bc > BIMODALITY_COEFFICIENT_THRESHOLD, "BC={bc} should be > 0.555 for bimodal data");
    }

    #[test]
    fn test_compute_metric_stats_bimodal_anchors_to_upper() {
        // Upper cluster ~250, lower cluster ~50
        let mut vals: Vec<f32> = (0..20).map(|i| 240.0 + i as f32).collect();
        vals.extend((0..20).map(|i| 40.0 + i as f32));
        let stats = compute_metric_stats(&vals, true, true);
        // Mean should be anchored near the upper cluster, not the mixed mean (~145)
        assert!(
            stats.mean > 200.0,
            "Bimodal anchor mean={} should be near upper cluster (~250)", stats.mean
        );
    }

    #[test]
    fn test_compute_metric_stats_unimodal_unchanged() {
        // Unimodal — bimodal flag should have no effect
        let vals: Vec<f32> = (0..30).map(|i| 200.0 + i as f32).collect();
        let stats_bimodal  = compute_metric_stats(&vals, true,  true);
        let stats_plain    = compute_metric_stats(&vals, false, true);
        assert!(
            (stats_bimodal.mean - stats_plain.mean).abs() < 0.01,
            "Unimodal data should produce identical stats regardless of use_bimodal flag"
        );
    }

    #[test]
    fn test_bimodal_star_count_rejects_cloudy_frames() {
        // Simulate 20 clear frames (~250 stars) and 15 cloudy frames (~50 stars)
        let mut results: Vec<AnalysisResult> = (0..20)
            .map(|i| make_result(&format!("clear_{i}"), 3.0, 0.5, 0.02, 250, 0.049))
            .collect();
        results.extend((0..15).map(|i| make_result(&format!("cloudy_{i}"), 3.2, 0.6, 0.015, 50, 0.055)));

        let refs: Vec<&AnalysisResult> = results.iter().collect();
        let stats = compute_session_stats(&refs);
        let thresholds = AnalysisThresholds::default();

        // Star count stats should be anchored to the clear-sky cluster
        assert!(stats.star_count.mean > 200.0,
            "Star count mean={} should be anchored near clear-sky cluster", stats.star_count.mean);

        // Cloudy frames should be rejected
        let cloudy = &results[20];
        let (flag, triggered) = classify_frame(cloudy, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Reject);
        assert!(triggered.contains(&"StarCount".to_string()));

        // Clear frames should pass
        let clear = &results[0];
        let (flag, _) = classify_frame(clear, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Pass);
    }
}
