// analysis/fwhm.rs — FWHM computation
// Spec §7.8, §15.4
//
// For each StarCandidate:
//   1. Sample the intensity profile through the centroid on 4 axes
//      (horizontal, vertical, diagonal /, diagonal \)
//   2. Find the half-maximum points on each side of the centroid
//      using sub-pixel linear interpolation
//   3. FWHM for the star = mean across all valid axis measurements
//
// Final result = median FWHM across all stars (robust against outliers).

use crate::analysis::stars::StarCandidate;

// ── Per-star FWHM ─────────────────────────────────────────────────────────────

/// Compute FWHM in pixels for a single star using its pixel patch.
/// Returns None if the star profile is too flat or malformed to measure.
pub fn star_fwhm(star: &StarCandidate) -> Option<f32> {
    let (x0, y0, x1, y1) = star.bbox;
    let bw = x1 - x0 + 1;
    let bh = y1 - y0 + 1;

    // Centroid in patch-local coordinates
    let lcx = (star.cx - x0 as f32).round() as isize;
    let lcy = (star.cy - y0 as f32).round() as isize;

    if lcx < 0 || lcy < 0 || lcx >= bw as isize || lcy >= bh as isize {
        return None;
    }

    let peak = star.peak;
    if peak <= 0.0 {
        return None;
    }
    let half_max = peak * 0.5;

    // Sample on 4 axes through the centroid
    let axes: &[(isize, isize)] = &[
        (1,  0),   // horizontal
        (0,  1),   // vertical
        (1,  1),   // diagonal \
        (1, -1),   // diagonal /
    ];

    let mut measurements: Vec<f32> = Vec::new();

    for &(dx, dy) in axes {
        // Collect samples along this axis in both directions from centroid
        let pos_samples = axis_samples(&star.patch, bw, bh, lcx, lcy,  dx,  dy);
        let neg_samples = axis_samples(&star.patch, bw, bh, lcx, lcy, -dx, -dy);

        if pos_samples.len() < 2 || neg_samples.len() < 2 {
            continue;
        }

        // Find half-max crossing in each direction
        let pos_hm = half_max_distance(&pos_samples, half_max);
        let neg_hm = half_max_distance(&neg_samples, half_max);

        if let (Some(p), Some(n)) = (pos_hm, neg_hm) {
            // For diagonal axes, scale by sqrt(2) to get pixel distance
            let scale = if dx != 0 && dy != 0 { std::f32::consts::SQRT_2 } else { 1.0 };
            measurements.push((p + n) * scale);
        }
    }

    if measurements.is_empty() {
        return None;
    }

    Some(measurements.iter().sum::<f32>() / measurements.len() as f32)
}

/// Extract intensity samples along a direction from (cx, cy) inclusive.
/// Steps in direction (dx, dy) until the patch boundary is reached.
fn axis_samples(
    patch: &[f32],
    bw:    usize,
    bh:    usize,
    cx:    isize,
    cy:    isize,
    dx:    isize,
    dy:    isize,
) -> Vec<f32> {
    let mut samples = Vec::new();
    let mut x = cx;
    let mut y = cy;

    loop {
        if x < 0 || y < 0 || x >= bw as isize || y >= bh as isize {
            break;
        }
        samples.push(patch[y as usize * bw + x as usize]);
        x += dx;
        y += dy;
    }
    samples
}

/// Find the fractional distance from index 0 at which the profile
/// crosses half_max, using linear interpolation between samples.
/// Returns None if the profile never drops below half_max.
fn half_max_distance(samples: &[f32], half_max: f32) -> Option<f32> {
    // samples[0] is the peak (centroid); we walk outward
    for i in 1..samples.len() {
        let prev = samples[i - 1];
        let curr = samples[i];
        if curr <= half_max {
            // Linear interpolation: fraction of the step at which crossing occurs
            if (prev - curr).abs() < f32::EPSILON {
                return Some(i as f32);
            }
            let frac = (prev - half_max) / (prev - curr);
            return Some((i - 1) as f32 + frac);
        }
    }
    None // profile never dropped to half-max within the patch
}

// ── Image-level FWHM ──────────────────────────────────────────────────────────

pub struct FwhmResult {
    /// Median FWHM across all measured stars, in pixels
    pub fwhm_pixels: f32,
    /// Median FWHM in arcseconds (None if plate scale not available)
    pub fwhm_arcsec: Option<f32>,
    /// Number of stars that contributed to the measurement
    pub star_count: usize,
    /// Number of stars that could not be measured (profile too flat/small)
    pub rejected_count: usize,
}

/// Compute median FWHM across all detected stars.
pub fn compute_fwhm(
    stars:       &[StarCandidate],
    plate_scale: Option<f32>,   // arcsec/pixel; None if unavailable
) -> Option<FwhmResult> {
    if stars.is_empty() {
        return None;
    }

    let mut measurements: Vec<f32> = stars
        .iter()
        .filter_map(star_fwhm)
        .filter(|&f| f > 0.5 && f < 50.0) // sanity bounds: 0.5–50px
        .collect();

    if measurements.is_empty() {
        return None;
    }

    measurements.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = measurements.len();
    let median_pixels = if n % 2 == 1 {
        measurements[n / 2]
    } else {
        (measurements[n / 2 - 1] + measurements[n / 2]) * 0.5
    };

    let fwhm_arcsec = plate_scale.map(|ps| median_pixels * ps);
    let rejected_count = stars.len() - n;

    Some(FwhmResult {
        fwhm_pixels: median_pixels,
        fwhm_arcsec,
        star_count: n,
        rejected_count,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::stars::StarCandidate;

    /// Create a synthetic Gaussian-ish star patch with known FWHM.
    /// sigma in pixels; FWHM = 2.355 * sigma
    fn gaussian_star(sigma: f32) -> StarCandidate {
        let size = (sigma * 6.0).ceil() as usize | 1; // odd size
        let cx = (size / 2) as f32;
        let cy = (size / 2) as f32;
        let mut patch = vec![0.0f32; size * size];
        let mut peak = 0.0f32;

        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let r2 = dx * dx + dy * dy;
                let val = (-r2 / (2.0 * sigma * sigma)).exp();
                patch[y * size + x] = val;
                if val > peak { peak = val; }
            }
        }

        StarCandidate {
            cx,
            cy,
            peak,
            bbox: (0, 0, size - 1, size - 1),
            patch,
            pixel_count: size * size,
        }
    }

    #[test]
    fn test_fwhm_gaussian_sigma2() {
        // sigma=2 → expected FWHM ≈ 4.71px
        let star = gaussian_star(2.0);
        let fwhm = star_fwhm(&star).expect("should measure FWHM");
        let expected = 2.0 * 2.355_f32;
        assert!(
            (fwhm - expected).abs() < 0.5,
            "FWHM {} not near expected {}", fwhm, expected
        );
    }

    #[test]
    fn test_fwhm_gaussian_sigma3() {
        let star = gaussian_star(3.0);
        let fwhm = star_fwhm(&star).expect("should measure FWHM");
        let expected = 3.0 * 2.355_f32;
        assert!(
            (fwhm - expected).abs() < 0.7,
            "FWHM {} not near expected {}", fwhm, expected
        );
    }

    #[test]
    fn test_compute_fwhm_with_plate_scale() {
        let stars: Vec<StarCandidate> = vec![
            gaussian_star(2.0),
            gaussian_star(2.0),
            gaussian_star(2.0),
        ];
        let result = compute_fwhm(&stars, Some(0.964)).expect("should return result");
        assert!(result.fwhm_pixels > 3.0 && result.fwhm_pixels < 7.0);
        assert!(result.fwhm_arcsec.is_some());
        let arcsec = result.fwhm_arcsec.unwrap();
        assert!(arcsec > 2.0 && arcsec < 8.0, "arcsec {} out of range", arcsec);
        assert_eq!(result.star_count, 3);
    }

    #[test]
    fn test_compute_fwhm_no_plate_scale() {
        let stars = vec![gaussian_star(2.0)];
        let result = compute_fwhm(&stars, None).expect("should return result");
        assert!(result.fwhm_arcsec.is_none());
    }

    #[test]
    fn test_compute_fwhm_empty() {
        assert!(compute_fwhm(&[], None).is_none());
    }
}


// ----------------------------------------------------------------------
