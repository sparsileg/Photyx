// analysis/metrics.rs — highlight clipping and Signal Weight
// Spec §11.2
//
// Signal Weight is derived from per-star Moffat PSF fits.
// It replaces the prior pixel-based SNR estimate.

use crate::analysis::moffat::fit_star;
use crate::analysis::stars::StarCandidate;

// ── Highlight clipping ────────────────────────────────────────────────────────

/// Fixed clipping threshold per spec §8.8: pixels at or above this value
/// are considered highlight-clipped.
#[allow(dead_code)]
pub const CLIP_THRESHOLD: f32 = 0.995;

/// Compute highlight clipping fraction for a luminance image.
/// Returns the fraction of pixels at or above CLIP_THRESHOLD (0.0–1.0).
#[allow(dead_code)]
pub fn highlight_clipping(luma: &[f32]) -> f32 {
    if luma.is_empty() {
        return 0.0;
    }
    let clipped = luma.iter().filter(|&&v| v >= CLIP_THRESHOLD).count();
    clipped as f32 / luma.len() as f32
}

// ── Signal Weight ─────────────────────────────────────────────────────────────
//
// Signal Weight = median(A² / (A + B·π·a·b)) across all accepted Moffat fits.
//
// Per-star: fit the elliptical Moffat model; if accepted, compute
//   W = A² / (A + B·π·a·b)
// where A = fitted peak amplitude, B = local background, a/b = semi-axes.
//
// Frame Signal Weight = median of per-star values (robust against outliers).
// Stars that fail Moffat acceptance criteria are excluded.
//
// This metric penalizes broad PSFs relative to narrow ones at the same peak
// flux, and is sensitive to transparency events and atmospheric extinction.

pub struct SignalWeightResult {
    pub signal_weight: f32,
}

/// Compute Signal Weight from a pre-detected star list.
/// Returns None if no stars pass Moffat fitting acceptance criteria.
pub fn compute_signal_weight(
    stars: &[StarCandidate],
) -> Option<SignalWeightResult> {
    if stars.is_empty() {
        return None;
    }

    let mut per_star: Vec<f32> = stars
        .iter()
        .filter_map(|star| fit_star(star).map(|f| f.signal_weight))
        .collect();

    if per_star.is_empty() {
        return None;
    }

    per_star.sort_unstable_by(|a, b| {
        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
    });
    let n = per_star.len();
    let median = if n % 2 == 1 {
        per_star[n / 2]
    } else {
        (per_star[n / 2 - 1] + per_star[n / 2]) * 0.5
    };

    Some(SignalWeightResult {
        signal_weight: median,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipping_none() {
        let luma = vec![0.5f32; 1000];
        assert_eq!(highlight_clipping(&luma), 0.0);
    }

    #[test]
    fn test_clipping_all() {
        let luma = vec![1.0f32; 1000];
        assert_eq!(highlight_clipping(&luma), 1.0);
    }

    #[test]
    fn test_clipping_half() {
        let mut luma = vec![0.5f32; 500];
        luma.extend(vec![1.0f32; 500]);
        let clip = highlight_clipping(&luma);
        assert!((clip - 0.5).abs() < 0.001, "clipping {} should be 0.5", clip);
    }

    #[test]
    fn test_clipping_threshold() {
        let luma = vec![CLIP_THRESHOLD; 100];
        assert_eq!(highlight_clipping(&luma), 1.0);
        let luma2 = vec![CLIP_THRESHOLD - 0.001; 100];
        assert_eq!(highlight_clipping(&luma2), 0.0);
    }

    #[test]
    fn test_clipping_empty() {
        assert_eq!(highlight_clipping(&[]), 0.0);
    }

    #[test]
    fn test_signal_weight_no_stars() {
        assert!(compute_signal_weight(&[]).is_none());
    }
}
