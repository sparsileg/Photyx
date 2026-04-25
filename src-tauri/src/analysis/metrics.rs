// analysis/metrics.rs — highlight clipping and SNR estimate
// Spec §15.4
//
// Both metrics are single-pass pixel statistics that do not depend on star shape.
// Internal functions accept a pre-detected star list so AnalyzeFrames can
// run star detection once and share the result across all metrics.

use crate::analysis::stars::StarCandidate;
use crate::analysis::background::estimate_background;
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
// SNR = signal / noise
//
// Signal = median of all star pixels (background-subtracted)
//          where star pixels are all pixels belonging to detected star
//          connected components (the same pixels used for FWHM/eccentricity).
//
// Noise  = background standard deviation (sigma-clipped).
//
// This is a relative SNR — meaningful for comparing frames within a session.

pub struct SnrResult {
    pub snr:            f32,
    pub signal_median:  f32,
    pub noise:          f32,
    pub star_pixels:    usize,
}

/// Compute SNR estimate from a pre-detected star list and the full luminance image.
/// Returns None if no stars were detected or all star regions are empty.
pub fn snr_estimate(
    luma:        &[f32],
    width:       usize,
    height:      usize,
    stars:       &[StarCandidate],
    sigma_clip:  &SigmaClipConfig,
) -> Option<SnrResult> {
    if stars.is_empty() || luma.is_empty() {
        return None;
    }

    // Background estimate for noise and subtraction
    let bg_est = estimate_background(luma, sigma_clip);
    let bg     = bg_est.median;
    let noise  = bg_est.stddev;

    if noise <= 0.0 {
        return None;
    }

    // Collect all star pixels (background-subtracted) from star bounding boxes
    let mut star_pixels: Vec<f32> = Vec::new();

    for star in stars {
        let (x0, y0, x1, y1) = star.bbox;
        let bw = x1 - x0 + 1;
        let bh = y1 - y0 + 1;

        for py in 0..bh {
            for px in 0..bw {
                // Only include pixels that are part of the star (patch value > 0)
                let patch_val = star.patch[py * bw + px];
                if patch_val > 0.0 {
                    let ix = x0 + px;
                    let iy = y0 + py;
                    if ix < width && iy < height {
                        let raw = luma[iy * width + ix];
                        let bgsub = (raw - bg).max(0.0);
                        star_pixels.push(bgsub);
                    }
                }
            }
        }
    }

    if star_pixels.is_empty() {
        return None;
    }

    // Median of background-subtracted star pixels
    star_pixels.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = star_pixels.len();
    let signal_median = if n % 2 == 1 {
        star_pixels[n / 2]
    } else {
        (star_pixels[n / 2 - 1] + star_pixels[n / 2]) * 0.5
    };

    if signal_median <= 0.0 {
        return None;
    }

    let snr = signal_median / noise;

    Some(SnrResult {
        snr,
        signal_median,
        noise,
        star_pixels: n,
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

    fn make_star_with_patch(
        cx: f32, cy: f32,
        x0: usize, y0: usize,
        patch: Vec<f32>,
        bw: usize, bh: usize,
    ) -> StarCandidate {
        StarCandidate {
            cx,
            cy,
            peak: patch.iter().cloned().fold(0.0f32, f32::max),
            bbox: (x0, y0, x0 + bw - 1, y0 + bh - 1),
            patch,
            pixel_count: bw * bh,
        }
    }

    #[test]
    fn test_snr_basic() {
        // 100x100 background at 0.05, one bright star region
        let width  = 100usize;
        let height = 100usize;
        let mut luma = vec![0.05f32; width * height];

        // Place a 5x5 star at (45,45) with value 0.5
        let bw = 5usize;
        let bh = 5usize;
        let x0 = 45usize;
        let y0 = 45usize;
        for py in 0..bh {
            for px in 0..bw {
                luma[(y0 + py) * width + (x0 + px)] = 0.5;
            }
        }

        let patch = vec![0.5f32 - 0.05; bw * bh]; // background-subtracted patch
        let star = make_star_with_patch(
            x0 as f32 + 2.0, y0 as f32 + 2.0,
            x0, y0, patch, bw, bh,
        );

        let config = SigmaClipConfig::default();
        let result = snr_estimate(&luma, width, height, &[star], &config);
        assert!(result.is_some(), "SNR should be computable");
        let r = result.unwrap();
        assert!(r.snr > 1.0, "SNR {} should be > 1", r.snr);
        assert!(r.noise > 0.0, "noise should be > 0");
    }

    #[test]
    fn test_snr_no_stars() {
        let luma = vec![0.05f32; 100 * 100];
        let config = SigmaClipConfig::default();
        assert!(snr_estimate(&luma, 100, 100, &[], &config).is_none());
    }
}


// ----------------------------------------------------------------------
