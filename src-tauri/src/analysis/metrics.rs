// analysis/metrics.rs — highlight clipping and SNR estimate
// Spec §15.4
//
// Both metrics are single-pass pixel statistics that do not depend on star shape.
// Internal functions accept a pre-detected star list so AnalyzeFrames can
// run star detection once and share the result across all metrics.

use crate::analysis::stars::StarCandidate;
use crate::analysis::background::estimate_background;
use crate::analysis::fwhm::star_fwhm;
use crate::analysis::SigmaClipConfig;

// ── Highlight clipping ────────────────────────────────────────────────────────

/// Fixed clipping threshold per spec §8.8: pixels at or above this value
/// are considered highlight-clipped.
#[allow(dead_code)]
pub const CLIP_THRESHOLD: f32 = 0.995;

/// Compute highlight clipping fraction for a luminance image.
/// Returns the fraction of pixels at or above CLIP_THRESHOLD (0.0–1.0).
/// Multiply by 100.0 for percentage.
#[allow(dead_code)]
pub fn highlight_clipping(luma: &[f32]) -> f32 {
    if luma.is_empty() {
        return 0.0;
    }
    let clipped = luma.iter().filter(|&&v| v >= CLIP_THRESHOLD).count();
    clipped as f32 / luma.len() as f32
}

// ── SNR estimate ──────────────────────────────────────────────────────────────
//
// SNR = median(peak_i / (fwhm_i * noise))
//
// Per-star SNR = peak / (fwhm * noise), where:
//   - peak  is the background-subtracted peak pixel value stored in StarCandidate
//   - fwhm  is computed via intensity-weighted second-order moments (same as
//           the FWHM metric), so a bloated PSF is penalized proportionally
//   - noise is the sigma-clipped background standard deviation
//
// Session SNR = median of per-star SNR values (robust against outliers).
// Stars for which fwhm cannot be computed are skipped.
//
// This is a relative SNR — meaningful for comparing frames within a session.
// Frames with poor seeing produce larger FWHM values and therefore lower SNR,
// correctly reflecting reduced signal quality regardless of integrated flux.

pub struct SnrResult {
    pub snr:        f32,
    pub noise:      f32,
    pub star_count: usize,
}

/// Compute SNR estimate from a pre-detected star list and the full luminance image.
/// Returns None if no stars were detected or no valid per-star SNR can be computed.
pub fn snr_estimate(
    luma:       &[f32],
    width:      usize,
    height:     usize,
    stars:      &[StarCandidate],
    sigma_clip: &SigmaClipConfig,
) -> Option<SnrResult> {
    // width and height are not used directly but retained in the signature
    // for consistency with other metric functions and future use.
    let _ = (width, height);

    if stars.is_empty() || luma.is_empty() {
        return None;
    }

    // Background noise estimate
    let bg_est = estimate_background(luma, sigma_clip);
    let noise  = bg_est.stddev;

    if noise <= 0.0 {
        return None;
    }

    // Per-star SNR = peak / (fwhm * noise)
    // Stars where fwhm is degenerate or peak is zero are skipped.
    let mut per_star_snr: Vec<f32> = stars
        .iter()
        .filter_map(|star| {
            let fwhm = star_fwhm(star)?;
            if fwhm <= 0.0 || star.peak <= 0.0 {
                return None;
            }
            Some(star.peak / (fwhm * noise))
        })
        .collect();

    if per_star_snr.is_empty() {
        return None;
    }

    // Median across all stars — robust against outliers
    per_star_snr.sort_unstable_by(|a, b| {
        a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
    });
    let n = per_star_snr.len();
    let snr = if n % 2 == 1 {
        per_star_snr[n / 2]
    } else {
        (per_star_snr[n / 2 - 1] + per_star_snr[n / 2]) * 0.5
    };

    Some(SnrResult {
        snr,
        noise,
        star_count: n,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::stars::StarCandidate;

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
        // Exactly at threshold should be clipped
        let luma = vec![CLIP_THRESHOLD; 100];
        assert_eq!(highlight_clipping(&luma), 1.0);
        // Just below threshold should not be clipped
        let luma2 = vec![CLIP_THRESHOLD - 0.001; 100];
        assert_eq!(highlight_clipping(&luma2), 0.0);
    }

    #[test]
    fn test_clipping_empty() {
        assert_eq!(highlight_clipping(&[]), 0.0);
    }

    /// Build a Gaussian star patch for SNR testing.
    fn gaussian_star(cx: usize, cy: usize, sigma: f32, peak: f32) -> StarCandidate {
        let size = (sigma * 6.0).ceil() as usize | 1;
        let half = size / 2;
        let mut patch = vec![0.0f32; size * size];
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - half as f32;
                let dy = y as f32 - half as f32;
                let r2 = dx * dx + dy * dy;
                patch[y * size + x] = peak * (-r2 / (2.0 * sigma * sigma)).exp();
            }
        }
        StarCandidate {
            cx: cx as f32,
            cy: cy as f32,
            peak,
            bbox: (cx.saturating_sub(half), cy.saturating_sub(half),
                   cx + half, cy + half),
            patch,
            pixel_count: size * size,
        }
    }

    #[test]
    fn test_snr_basic() {
        // Flat background at 0.05 with one Gaussian star
        let width  = 200usize;
        let height = 200usize;
        let mut luma = vec![0.05f32; width * height];

        // Place star pixels into the image
        let sigma = 2.0f32;
        let size = (sigma * 6.0).ceil() as usize | 1;
        let half = size / 2;
        let cx = 100usize;
        let cy = 100usize;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - half as f32;
                let dy = y as f32 - half as f32;
                let r2 = dx * dx + dy * dy;
                let val = 0.05 + 0.5 * (-r2 / (2.0 * sigma * sigma)).exp();
                luma[(cy - half + y) * width + (cx - half + x)] = val;
            }
        }

        let star = gaussian_star(cx, cy, sigma, 0.5);
        let config = SigmaClipConfig::default();
        let result = snr_estimate(&luma, width, height, &[star], &config);
        assert!(result.is_some(), "SNR should be computable");
        let r = result.unwrap();
        assert!(r.snr > 0.0, "SNR {} should be positive", r.snr);
        assert!(r.noise > 0.0, "noise should be positive");
        assert_eq!(r.star_count, 1);
    }

    #[test]
    fn test_snr_poor_seeing_scores_lower() {
        // Two identical images but one has 2× the FWHM (worse seeing).
        // The poor-seeing frame should score lower SNR.
        let width  = 300usize;
        let height = 300usize;
        let bg = 0.05f32;
        let peak = 0.6f32;

        let make_image = |sigma: f32| -> Vec<f32> {
            let mut luma = vec![bg; width * height];
            let size = (sigma * 6.0).ceil() as usize | 1;
            let half = size / 2;
            let cx = 150usize;
            let cy = 150usize;
            for y in 0..size {
                for x in 0..size {
                    let dx = x as f32 - half as f32;
                    let dy = y as f32 - half as f32;
                    let r2 = dx * dx + dy * dy;
                    let val = bg + peak * (-r2 / (2.0 * sigma * sigma)).exp();
                    luma[(cy - half + y) * width + (cx - half + x)] = val;
                }
            }
            luma
        };

        let config = SigmaClipConfig::default();

        let good_luma = make_image(2.0);
        let good_star = gaussian_star(150, 150, 2.0, peak);
        let good_snr  = snr_estimate(&good_luma, width, height, &[good_star], &config)
            .expect("good SNR should compute").snr;

        let poor_luma = make_image(4.0);
        let poor_star = gaussian_star(150, 150, 4.0, peak);
        let poor_snr  = snr_estimate(&poor_luma, width, height, &[poor_star], &config)
            .expect("poor SNR should compute").snr;

        assert!(
            good_snr > poor_snr,
            "good seeing SNR ({:.3}) should exceed poor seeing SNR ({:.3})",
            good_snr, poor_snr
        );
    }

    #[test]
    fn test_snr_no_stars() {
        let luma = vec![0.05f32; 100 * 100];
        let config = SigmaClipConfig::default();
        assert!(snr_estimate(&luma, 100, 100, &[], &config).is_none());
    }

    #[test]
    fn test_snr_empty_image() {
        let config = SigmaClipConfig::default();
        assert!(snr_estimate(&[], 0, 0, &[], &config).is_none());
    }
}


// ----------------------------------------------------------------------
