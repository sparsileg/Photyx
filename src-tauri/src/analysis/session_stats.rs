// analysis/session_stats.rs — session-level statistics for AnalyzeFrames

// PASS / REJECT classification
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
    // Sigma-based (higher deviation = worse, except star_count)
    pub background_median:   MetricThresholds,  // +σ
    pub fwhm:                MetricThresholds,  // +σ
    pub star_count:          MetricThresholds,  // -σ (fewer stars = worse)

    // Absolute
    pub eccentricity:        MetricThresholds,  // absolute 0.0–1.0
}

impl Default for AnalysisThresholds {
    fn default() -> Self {
        use crate::settings::defaults::{
            DEFAULT_BG_MEDIAN_SIGMA, DEFAULT_FWHM_SIGMA,
            DEFAULT_STAR_COUNT_SIGMA, DEFAULT_ECCENTRICITY_ABS,
        };
        Self {
            background_median:   MetricThresholds { reject: DEFAULT_BG_MEDIAN_SIGMA as f32 },
            fwhm:                MetricThresholds { reject: DEFAULT_FWHM_SIGMA as f32 },
            star_count:          MetricThresholds { reject: DEFAULT_STAR_COUNT_SIGMA as f32 },
            eccentricity:        MetricThresholds { reject: DEFAULT_ECCENTRICITY_ABS as f32 },
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
/// histogram of `values`. Returns the value at the valley midpoint together
/// with the valley depth ratio: the smoothed frame count at the valley
/// divided by the smaller of the two smoothed peak heights.
///
/// A genuine two-state population (e.g. clear vs. cloudy frames) leaves the
/// valley empty or nearly so (ratio ≈ 0.0). A continuous drifting population
/// keeps the valley populated (ratio well above zero), because frames exist
/// at every intermediate quality level. Callers use the ratio to distinguish
/// a real cluster gap from the artifact of valley-splitting a skewed but
/// unimodal distribution.
///
/// The valley and peak *locations* use the original integer-smoothed
/// histogram, preserving existing behaviour exactly. The depth ratio is
/// computed from float-smoothed counts at those same indices, because
/// integer division quantizes small per-bin counts too coarsely for a
/// ratio test at typical session sizes (30–100 frames).
///
/// `n_bins`: histogram resolution (20 is a good default for 30–100 frames).
fn find_valley(values: &[f32], n_bins: usize) -> Option<(f32, f32)> {
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

    // 3-point smoothing (integer — original behaviour, governs locations)
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

    // Float-smoothed counts at the chosen indices for the depth ratio.
    // Edge bins are unsmoothed, matching the integer smoothing above.
    let smooth_f = |i: usize| -> f32 {
        if i == 0 || i == n_bins - 1 {
            bins[i] as f32
        } else {
            (bins[i - 1] + bins[i] + bins[i + 1]) as f32 / 3.0
        }
    };
    let smaller_peak = smooth_f(peak1).min(smooth_f(peak2));
    let depth_ratio = if smaller_peak > f32::EPSILON {
        smooth_f(valley_idx) / smaller_peak
    } else {
        // Degenerate: a "peak" with no smoothed height — no real gap exists
        1.0
    };

    Some((valley_val, depth_ratio))
}

/// Bimodality threshold — BC above this value indicates a bimodal distribution.
const BIMODALITY_COEFFICIENT_THRESHOLD: f32 = 0.555;

/// Histogram bin count used for valley detection.
const BIMODAL_HISTOGRAM_BINS: usize = 20;

/// Maximum valley depth ratio at which bimodal anchoring may engage.
/// A genuine two-state split (clear/cloudy) leaves the histogram valley
/// essentially empty (ratio ≈ 0.0); a populated valley means the two
/// "clusters" are connected by a continuum, so anchoring to the upper group
/// would punish normal frame-to-frame drift. Above this ratio we fall back
/// to whole-population stats. Empirically: continuous-drift session IC5146
/// 2024-12-03 measured 0.33; all genuine two-state scenarios measured 0.00,
/// so 0.2 has wide margin on both sides rather than being fitted.
const VALLEY_DEPTH_RATIO_MAX: f32 = 0.2;


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
                if let Some((valley, depth_ratio)) = find_valley(values, BIMODAL_HISTOGRAM_BINS) {
                    if depth_ratio > VALLEY_DEPTH_RATIO_MAX {
                        // BC fired, but the valley is populated: the two
                        // "clusters" are connected by a continuum (skewed
                        // unimodal drift), not a genuine two-state split.
                        // Anchoring here would punish normal drift — fall
                        // back to whole-population stats.
                        tracing::info!(
                            "BimodalStats: BC={:.3} > {:.3} but valley depth \
                             ratio {:.2} > {:.2} — populated valley, treating \
                             as unimodal (plain stats)",
                            bc,
                            BIMODALITY_COEFFICIENT_THRESHOLD,
                            depth_ratio,
                            VALLEY_DEPTH_RATIO_MAX,
                        );
                    } else {
                        let upper: Vec<f32> = if higher_is_better {
                            values.iter().cloned().filter(|&v| v > valley).collect()
                        } else {
                            values.iter().cloned().filter(|&v| v < valley).collect()
                        };
                        if upper.len() >= 2 {
                            tracing::info!(
                                "BimodalStats: BC={:.3} > {:.3}, valley={:.3} \
                                 (depth ratio {:.2}), anchoring to {} \
                                 upper-cluster values",
                                bc,
                                BIMODALITY_COEFFICIENT_THRESHOLD,
                                valley,
                                depth_ratio,
                                upper.len(),
                            );
                            return MetricStats::from_values(&upper);
                        }
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
/// carried through unchanged. Only the non-bimodal metrics (FWHM, background)
/// are recomputed on the cleaned subset after outlier removal.
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
/// T = Transparency   — StarCount (without BackgroundMedian)
///
/// Ordering: O always leads. When B and T are both present, B leads T.
pub fn categorize_rejection(triggered: &[String]) -> Option<String> {
    if triggered.is_empty() {
        return None;
    }

    let has_optical = triggered.iter().any(|t| t == "FWHM" || t == "Eccentricity");
    let has_bg      = triggered.iter().any(|t| t == "BackgroundMedian");
    let has_transp  = triggered.iter().any(|t| t == "StarCount");

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

    fn make_result(filename: &str, fwhm: f32, ecc: f32, stars: u32, bg: f32) -> AnalysisResult {
        AnalysisResult {
            filename:           filename.to_string(),
            background_median:  Some(bg),
            fwhm:               Some(fwhm),
            eccentricity:       Some(ecc),
            star_count:         Some(stars),
            flag:               None,
            triggered_by:       vec![],
            is_reference:       false,
            rejection_category: None,
        }
    }

    #[test]
    fn test_session_stats_basic() {
        let r1 = make_result("f1", 2.5, 0.4, 600, 0.05);
        let r2 = make_result("f2", 2.7, 0.5, 580, 0.05);
        let r3 = make_result("f3", 2.6, 0.45, 610, 0.05);
        let results = vec![&r1, &r2, &r3];
        let stats = compute_session_stats(&results);
        assert!((stats.fwhm.mean - 2.6).abs() < 0.01);
        assert!(stats.fwhm.stddev > 0.0);
    }

    #[test]
    fn test_classify_average_frame_passes() {
        let r1 = make_result("f1", 2.5, 0.4, 600, 0.05);
        let r2 = make_result("f2", 2.5, 0.4, 600, 0.05);
        let r3 = make_result("f3", 2.5, 0.4, 600, 0.05);
        let results = vec![&r1, &r2, &r3];
        let stats = compute_session_stats(&results);
        let thresholds = AnalysisThresholds::default();
        let (flag, triggered) = classify_frame(&r1, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Pass);
        assert!(triggered.is_empty());
    }

    #[test]
    fn test_classify_bad_eccentricity_rejects() {
        let r1 = make_result("f1", 2.5, 0.4,  600, 0.05);
        let r2 = make_result("f2", 2.5, 0.4,  600, 0.05);
        let r3 = make_result("f3", 2.5, 0.86, 600, 0.05);
        let results = vec![&r1, &r2, &r3];
        let stats = compute_session_stats(&results);
        let thresholds = AnalysisThresholds::default();
        let (flag, triggered) = classify_frame(&r3, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Reject);
        assert!(triggered.contains(&"Eccentricity".to_string()));
    }

    #[test]
    fn test_triggered_by_populated() {
        let g1 = make_result("g1", 2.4, 0.4, 600, 0.05);
        let g2 = make_result("g2", 2.5, 0.4, 600, 0.05);
        let g3 = make_result("g3", 2.6, 0.4, 600, 0.05);
        let g4 = make_result("g4", 2.5, 0.4, 600, 0.05);
        let g5 = make_result("g5", 2.4, 0.4, 600, 0.05);
        let bad = make_result("bad", 10.0, 0.4, 600, 0.05);
        let good_results = vec![&g1, &g2, &g3, &g4, &g5];
        let stats = compute_session_stats(&good_results);
        let thresholds = AnalysisThresholds::default();
        let (flag, triggered) = classify_frame(&bad, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Reject);
        assert!(triggered.contains(&"FWHM".to_string()));
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
        // Simulate 20 clear frames (~250 stars, with realistic jitter) and
        // 15 cloudy frames (~50 stars). Issue 131: the original fixture
        // gave every clear frame the exact same star_count (250), so the
        // bimodal-anchored upper cluster had zero variance — sigma_dev's
        // divide-by-zero guard then made the sigma check permanently
        // unable to fire, regardless of how far the cloudy value sat from
        // the cluster. This was purely a fixture defect: the bimodal
        // anchoring itself was already correct (proven by the mean
        // assertion below, which passed even before this fix).
        let mut results: Vec<AnalysisResult> = (0..20)
            .map(|i| make_result(&format!("clear_{i}"), 3.0, 0.5, 240 + (i * 3) % 21, 0.049))
            .collect();
        results.extend((0..15).map(|i| make_result(&format!("cloudy_{i}"), 3.2, 0.6, 50, 0.055)));

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

    #[test]
    fn test_populated_valley_falls_back_to_plain_stats() {
        // Regression for the IC5146 Cocoon Nebula 2024-12-03 duo-band
        // session (real data, 65 frames): continuous right-skewed
        // transparency drift, no cloud events (confirmed by blink).
        // BC measured 0.5602 — marginally above the 0.555 threshold via
        // BC's skewness blind spot — and the valley split placed only
        // 7 of 65 frames in the "good" cluster (anchored mean 1915.4,
        // stddev 90.4), rejecting 58/65 on StarCount at the Duo-band
        // profile's 1.75σ. The valley depth ratio (~0.33) must trip the
        // VALLEY_DEPTH_RATIO_MAX guard so stats fall back to the plain
        // whole-population values, under which zero frames reject.
        let vals: Vec<f32> = vec![
            1671.0, 1225.0, 1788.0, 1561.0, 1365.0, 1411.0, 1219.0, 1314.0, 1193.0, 1972.0,
            1955.0, 2022.0, 1675.0, 1558.0, 1441.0, 1572.0, 1439.0, 1388.0, 1647.0, 1824.0,
            2014.0, 1833.0, 1632.0, 1533.0, 1584.0, 1390.0, 1316.0, 1422.0, 1191.0, 1407.0,
            1194.0, 1413.0, 1440.0, 1354.0, 1124.0, 1249.0, 1160.0, 1189.0, 1374.0, 1535.0,
            1431.0, 1408.0, 1320.0, 1225.0, 1371.0, 1223.0, 1340.0, 1432.0, 1317.0, 1132.0,
            1124.0, 1355.0, 1216.0, 1180.0, 1199.0, 1342.0, 1168.0, 1004.0, 1243.0, 1290.0,
            1251.0, 1165.0, 1204.0, 1210.0, 1093.0,
        ];

        let bimodal = compute_metric_stats(&vals, true, true);
        let plain   = compute_metric_stats(&vals, false, true);

        // Guard must have fired: bimodal-enabled stats identical to plain
        assert!(
            (bimodal.mean - plain.mean).abs() < 0.01
                && (bimodal.stddev - plain.stddev).abs() < 0.01,
            "Populated-valley session must fall back to plain stats \
             (got mean={} stddev={}, plain mean={} stddev={})",
            bimodal.mean, bimodal.stddev, plain.mean, plain.stddev,
        );

        // Under plain stats, no frame reaches -1.75σ (Duo-band StarCount
        // reject threshold): worst frame (1004) sits at ≈ -1.67σ.
        let rejects = vals.iter()
            .filter(|&&v| (v - bimodal.mean) / bimodal.stddev <= -1.75)
            .count();
        assert_eq!(
            rejects, 0,
            "No frame in this session should reject on StarCount at 1.75σ",
        );
    }

    #[test]
    fn test_empty_valley_minority_good_cluster_still_anchors() {
        // Counterpart to test_populated_valley_falls_back_to_plain_stats:
        // the valley depth guard must NOT suppress anchoring when the
        // split is genuine. Scenario: clouds rolled in early — only 15
        // clear frames (~1850–1941 stars) against 50 cloudy frames
        // (~550–648 stars). The good cluster is a 23% minority (which is
        // exactly why a minimum-cluster-fraction guard was rejected),
        // but the histogram valley between the clusters is empty
        // (depth ratio 0.0), so anchoring must engage and the cloudy
        // block must reject on StarCount.
        let mut results: Vec<AnalysisResult> = (0..15)
            .map(|i| make_result(&format!("clear_{i}"), 3.0, 0.5, 1850 + (i * 13) % 101, 0.05))
            .collect();
        results.extend(
            (0..50).map(|i| make_result(&format!("cloudy_{i}"), 3.0, 0.5, 550 + (i * 17) % 101, 0.05)),
        );

        let refs: Vec<&AnalysisResult> = results.iter().collect();
        let stats = compute_session_stats(&refs);
        let thresholds = AnalysisThresholds::default();

        // Anchoring must have engaged: mean near the clear cluster
        // (~1897), nowhere near the plain whole-population mean (~899).
        assert!(
            stats.star_count.mean > 1800.0,
            "Star count mean={} should be anchored to the minority clear \
             cluster despite it being only 15 of 65 frames",
            stats.star_count.mean,
        );

        // Cloudy frames must reject with StarCount triggered
        let cloudy = &results[15];
        let (flag, triggered) = classify_frame(cloudy, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Reject);
        assert!(triggered.contains(&"StarCount".to_string()));

        // A mid-cluster clear frame must pass. (Deliberately not the
        // cluster-minimum frame: with a ~uniform jitter spread, the
        // extreme cluster member sits near the default 1.5σ line by
        // construction — the same property the balanced-fixture test
        // has — and testing threshold-edge behaviour is not this
        // test's job.)
        let clear = &results[3]; // star_count = 1889, near cluster mean
        let (flag, _) = classify_frame(clear, &stats, &thresholds);
        assert_eq!(flag, PxFlag::Pass);
    }
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
