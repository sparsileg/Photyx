// analysis/fwhm.rs — FWHM computation
// Spec §7.8, §15.4
//
// Moment-based FWHM using intensity-weighted second-order moments.
// This matches PI's approach of using the geometric mean of major/minor axes:
//
//   Mxx = Σ(w * dx²) / Σw
//   Myy = Σ(w * dy²) / Σw
//   FWHM = 2.355 * sqrt((Mxx + Myy) / 2)
//
// This is more accurate than the previous axis-crossing approach, especially
// for elongated stars where the narrow axis was pulling the measurement down.
// The geometric mean correctly captures the overall star size regardless of
// orientation, matching PI's SubframeSelector FWHM values more closely.
//
// Final result = median FWHM across all stars (robust against outliers).

use crate::analysis::stars::StarCandidate;

// ── Per-star FWHM ─────────────────────────────────────────────────────────────

/// Compute FWHM in pixels for a single star using intensity-weighted moments.
/// Returns None if the star profile is too small or degenerate.
pub fn star_fwhm(star: &StarCandidate) -> Option<f32> {
    let (x0, y0, x1, y1) = star.bbox;
    let bw = x1 - x0 + 1;
    let bh = y1 - y0 + 1;

    if bw < 2 || bh < 2 {
        return None;
    }

    if star.peak <= 0.0 {
        return None;
    }

    // Centroid in patch-local coordinates
    let lcx = star.cx - x0 as f32;
    let lcy = star.cy - y0 as f32;

    // Intensity-weighted second-order moments
    let mut mxx = 0.0f64;
    let mut myy = 0.0f64;
    let mut total_weight = 0.0f64;

    for y in 0..bh {
        for x in 0..bw {
            let w = star.patch[y * bw + x] as f64;
            if w <= 0.0 { continue; }
            let dx = x as f64 - lcx as f64;
            let dy = y as f64 - lcy as f64;
            mxx += w * dx * dx;
            myy += w * dy * dy;
            total_weight += w;
        }
    }

    if total_weight <= 0.0 {
        return None;
    }

    mxx /= total_weight;
    myy /= total_weight;

    // FWHM = 2.355 * sqrt((Mxx + Myy) / 2)
    // Geometric mean of the two axes — matches PI's approach
    let sigma_mean = ((mxx + myy) / 2.0).sqrt() as f32;
    let fwhm = 2.355 * sigma_mean;

    if fwhm <= 0.0 || !fwhm.is_finite() {
        return None;
    }

    Some(fwhm)
}

// ── Image-level FWHM ──────────────────────────────────────────────────────────

pub struct FwhmResult {
    pub fwhm_pixels:    f32,
    pub fwhm_arcsec:    Option<f32>,
    pub star_count:     usize,
    pub rejected_count: usize,
}

/// Compute median FWHM across all detected stars.
pub fn compute_fwhm(
    stars:       &[StarCandidate],
    plate_scale: Option<f32>,
) -> Option<FwhmResult> {
    if stars.is_empty() {
        return None;
    }

    let mut measurements: Vec<f32> = stars
        .iter()
        .filter_map(star_fwhm)
        .filter(|&f| f > 0.5 && f < 50.0)
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

    fn gaussian_star(sigma: f32) -> StarCandidate {
        let size = (sigma * 6.0).ceil() as usize | 1;
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
            cx, cy, peak,
            bbox: (0, 0, size - 1, size - 1),
            patch,
            pixel_count: size * size,
        }
    }

    fn elongated_gaussian_star(sigma_x: f32, sigma_y: f32) -> StarCandidate {
        let size_x = (sigma_x * 6.0).ceil() as usize | 1;
        let size_y = (sigma_y * 6.0).ceil() as usize | 1;
        let cx = (size_x / 2) as f32;
        let cy = (size_y / 2) as f32;
        let mut patch = vec![0.0f32; size_x * size_y];
        let mut peak = 0.0f32;

        for y in 0..size_y {
            for x in 0..size_x {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let val = (-(dx*dx/(2.0*sigma_x*sigma_x) + dy*dy/(2.0*sigma_y*sigma_y))).exp();
                patch[y * size_x + x] = val;
                if val > peak { peak = val; }
            }
        }

        StarCandidate {
            cx, cy, peak,
            bbox: (0, 0, size_x - 1, size_y - 1),
            patch,
            pixel_count: size_x * size_y,
        }
    }

    #[test]
    fn test_fwhm_gaussian_sigma2() {
        let star = gaussian_star(2.0);
        let fwhm = star_fwhm(&star).expect("should measure FWHM");
        let expected = 2.0 * 2.355_f32;
        assert!(
            (fwhm - expected).abs() < 0.3,
            "FWHM {} not near expected {}", fwhm, expected
        );
    }

    #[test]
    fn test_fwhm_gaussian_sigma3() {
        let star = gaussian_star(3.0);
        let fwhm = star_fwhm(&star).expect("should measure FWHM");
        let expected = 3.0 * 2.355_f32;
        assert!(
            (fwhm - expected).abs() < 0.3,
            "FWHM {} not near expected {}", fwhm, expected
        );
    }

    #[test]
    fn test_fwhm_elongated_star_geometric_mean() {
        // sigma_x=3, sigma_y=1 → expected FWHM = 2.355 * sqrt((9+1)/2) = 2.355 * sqrt(5) ≈ 5.27
        let star = elongated_gaussian_star(3.0, 1.0);
        let fwhm = star_fwhm(&star).expect("should measure FWHM");
        let expected = 2.355 * (5.0f32).sqrt();
        assert!(
            (fwhm - expected).abs() < 0.5,
            "FWHM {} not near expected geometric mean {}", fwhm, expected
        );
    }

    #[test]
    fn test_compute_fwhm_with_plate_scale() {
        let stars = vec![gaussian_star(2.0), gaussian_star(2.0), gaussian_star(2.0)];
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
