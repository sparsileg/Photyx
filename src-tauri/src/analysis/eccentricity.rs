// analysis/eccentricity.rs — eccentricity via intensity-weighted second-order moments
// Spec §7.8, §15.4
//
// For each StarCandidate:
//   1. Compute intensity-weighted second-order moments Mxx, Myy, Mxy
//   2. Derive semi-major (a) and semi-minor (b) axes of the equivalent ellipse
//   3. Eccentricity e = sqrt(1 - (b/a)^2)
//
// Final result = median eccentricity across all stars.
// e = 0.0 → perfectly circular; e → 1.0 → highly elongated / trailed.

use crate::analysis::stars::StarCandidate;

// ── Per-star eccentricity ─────────────────────────────────────────────────────

/// Compute eccentricity for a single star using intensity-weighted moments.
/// Returns None if the star is too small or the moments are degenerate.
pub fn star_eccentricity(star: &StarCandidate) -> Option<f32> {
    let (x0, y0, x1, y1) = star.bbox;
    let bw = x1 - x0 + 1;
    let bh = y1 - y0 + 1;

    if bw < 2 || bh < 2 {
        return None;
    }

    // Centroid in patch-local coordinates
    let lcx = star.cx - x0 as f32;
    let lcy = star.cy - y0 as f32;

    // Intensity-weighted second-order moments
    let mut mxx = 0.0f64;
    let mut myy = 0.0f64;
    let mut mxy = 0.0f64;
    let mut total_weight = 0.0f64;

    for y in 0..bh {
        for x in 0..bw {
            let w = star.patch[y * bw + x] as f64;
            if w <= 0.0 {
                continue;
            }
            let dx = x as f64 - lcx as f64;
            let dy = y as f64 - lcy as f64;
            mxx += w * dx * dx;
            myy += w * dy * dy;
            mxy += w * dx * dy;
            total_weight += w;
        }
    }

    if total_weight <= 0.0 {
        return None;
    }

    mxx /= total_weight;
    myy /= total_weight;
    mxy /= total_weight;

    // Eigenvalues of the moment matrix [[Mxx, Mxy], [Mxy, Myy]]
    // give the squares of the semi-axes of the equivalent ellipse.
    let trace     = mxx + myy;
    let det       = mxx * myy - mxy * mxy;
    let discriminant = (trace * trace * 0.25 - det).max(0.0);
    let sqrt_disc = discriminant.sqrt();

    let lambda_max = trace * 0.5 + sqrt_disc; // semi-major axis squared
    let lambda_min = trace * 0.5 - sqrt_disc; // semi-minor axis squared

    if lambda_max <= 0.0 || lambda_min < 0.0 {
        return None;
    }

    // e = sqrt(1 - lambda_min / lambda_max)
    let ratio = (lambda_min / lambda_max).clamp(0.0, 1.0);
    let e = (1.0 - ratio).sqrt() as f32;

    Some(e.clamp(0.0, 1.0))
}

// ── Image-level eccentricity ──────────────────────────────────────────────────

pub struct EccentricityResult {
    /// Median eccentricity across all measured stars (0.0 = circular, 1.0 = line)
    pub eccentricity: f32,
    /// Number of stars that contributed to the measurement
    pub star_count: usize,
    /// Number of stars that could not be measured
    pub rejected_count: usize,
}

/// Compute median eccentricity across all detected stars.
pub fn compute_eccentricity(stars: &[StarCandidate]) -> Option<EccentricityResult> {
    if stars.is_empty() {
        return None;
    }

    let mut measurements: Vec<f32> = stars
        .iter()
        .filter_map(star_eccentricity)
        .filter(|&e| e >= 0.0 && e <= 1.0)
        .collect();

    if measurements.is_empty() {
        return None;
    }

    measurements.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = measurements.len();
    let median = if n % 2 == 1 {
        measurements[n / 2]
    } else {
        (measurements[n / 2 - 1] + measurements[n / 2]) * 0.5
    };

    Some(EccentricityResult {
        eccentricity:   median,
        star_count:     n,
        rejected_count: stars.len() - n,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::stars::StarCandidate;

    /// Circular Gaussian star — eccentricity should be near 0
    fn circular_star(sigma: f32) -> StarCandidate {
        let size = (sigma * 6.0).ceil() as usize | 1;
        let cx = (size / 2) as f32;
        let cy = (size / 2) as f32;
        let mut patch = vec![0.0f32; size * size];
        let mut peak = 0.0f32;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let val = (-(dx*dx + dy*dy) / (2.0*sigma*sigma)).exp();
                patch[y * size + x] = val;
                if val > peak { peak = val; }
            }
        }
        StarCandidate { cx, cy, peak, bbox: (0, 0, size-1, size-1), patch, pixel_count: size*size }
    }

    /// Elongated Gaussian star (sigma_x >> sigma_y) — eccentricity should be high
    fn elongated_star(sigma_x: f32, sigma_y: f32) -> StarCandidate {
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
        StarCandidate { cx, cy, peak, bbox: (0, 0, size_x-1, size_y-1), patch, pixel_count: size_x*size_y }
    }

    #[test]
    fn test_circular_star_low_eccentricity() {
        let star = circular_star(3.0);
        let e = star_eccentricity(&star).expect("should compute eccentricity");
        assert!(e < 0.15, "circular star eccentricity {} should be near 0", e);
    }

    #[test]
    fn test_elongated_star_high_eccentricity() {
        // sigma_x = 5, sigma_y = 1 → strongly elongated
        let star = elongated_star(5.0, 1.0);
        let e = star_eccentricity(&star).expect("should compute eccentricity");
        assert!(e > 0.9, "elongated star eccentricity {} should be high", e);
    }

    #[test]
    fn test_compute_eccentricity_median() {
        let stars = vec![
            circular_star(2.0),
            circular_star(2.0),
            circular_star(2.0),
        ];
        let result = compute_eccentricity(&stars).expect("should return result");
        assert!(result.eccentricity < 0.15);
        assert_eq!(result.star_count, 3);
    }

    #[test]
    fn test_compute_eccentricity_empty() {
        assert!(compute_eccentricity(&[]).is_none());
    }
}


// ----------------------------------------------------------------------
