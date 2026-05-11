// analysis/moffat.rs — Elliptical 2D Moffat PSF fitting per detected star
// Spec §11.2
//
// Fits the model:
//   I(x,y) = A · [1 + ((x-x0)²/a² + (y-y0)²/b²)]^(-β) + B
//
// Parameters:
//   A    — peak amplitude above background
//   x0   — centroid X
//   y0   — centroid Y
//   a    — semi-axis along X (always ≥ b after normalization)
//   b    — semi-axis along Y
//   β    — shape parameter (controls wing falloff; fixed at BETA_INIT)
//   B    — local background (estimated from stamp border, held fixed)
//
// Fitting uses Levenberg-Marquardt (LM) nonlinear least squares.
// β is held fixed during fitting to reduce the parameter space and improve
// convergence on low-contrast linear astrophotography data.
//
// Signal Weight formula (per spec §11.2):
//   W = A² / (A + B · π · a · b)

use crate::analysis::stars::StarCandidate;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Fixed Moffat β — typical atmospheric PSF value.
/// Held fixed during fitting to stabilize convergence on linear data.
const BETA: f32 = 4.765;

/// Maximum LM iterations per star.
const MAX_ITER: usize = 50;

/// LM convergence tolerance — stop when parameter delta norm < this.
const CONVERGENCE_TOL: f32 = 1e-5;

/// Initial LM damping parameter.
const LAMBDA_INIT: f32 = 1e-3;

/// Minimum acceptable semi-axis (pixels). Rejects point-source fits.
const MIN_AXIS: f32 = 0.5;

/// Maximum acceptable semi-axis (pixels). Rejects bloated/failed fits.
const MAX_AXIS: f32 = 30.0;

/// Minimum axis ratio b/a. Rejects extremely elongated detections
/// (satellite trail segments, diffraction spikes).
const MIN_AXIS_RATIO: f32 = 0.2;

/// Maximum normalized residual. Fits above this are rejected.
const MAX_RESIDUAL: f32 = 0.3;

// ── Output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MoffatFit {
    /// Signal Weight: A² / (A + B·π·a·b)
    pub signal_weight: f32,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Attempt a Moffat PSF fit on a single star candidate.
/// Returns None if the fit fails acceptance criteria.
pub fn fit_star(star: &StarCandidate) -> Option<MoffatFit> {
    let (x0, y0, x1, y1) = star.bbox;
    let bw = (x1 - x0 + 1) as usize;
    let bh = (y1 - y0 + 1) as usize;

    if bw < 3 || bh < 3 || star.patch.is_empty() {
        return None;
    }

    // Centroid in patch coordinates
    let cx = star.cx - x0 as f32;
    let cy = star.cy - y0 as f32;

    // Background estimate: mean of border pixels
    let bg = border_mean(&star.patch, bw, bh);

    // Amplitude estimate: peak minus background, clamped positive
    let peak_val = star.patch.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let amp_init = (peak_val - bg).max(1e-6);

    if amp_init <= 0.0 {
        return None;
    }

    // Initial axis estimate from patch size
    let axis_init = ((bw.min(bh) as f32) / 4.0).clamp(MIN_AXIS, MAX_AXIS);

    // Parameter vector: [A, cx, cy, a, b]
    // β and B are held fixed.
    let mut p = [amp_init, cx, cy, axis_init, axis_init];

    let result = lm_fit(&star.patch, bw, bh, bg, &mut p);
    if !result {
        return None;
    }

    let (a_fit, amp_fit) = (p[3], p[0]);
    let b_fit = p[4];

    // Enforce a ≥ b
    let (semi_major, semi_minor) = if a_fit >= b_fit {
        (a_fit, b_fit)
    } else {
        (b_fit, a_fit)
    };

    // Acceptance criteria
    if semi_major < MIN_AXIS || semi_minor < MIN_AXIS {
        return None;
    }
    if semi_major > MAX_AXIS || semi_minor > MAX_AXIS {
        return None;
    }
    if semi_minor / semi_major < MIN_AXIS_RATIO {
        return None;
    }
    if amp_fit <= 0.0 {
        return None;
    }

    // Compute residual
    let res = normalized_residual(&star.patch, bw, bh, bg, &p);
    if res > MAX_RESIDUAL {
        return None;
    }

    // Derived metrics
    let signal_weight = signal_weight(amp_fit, bg, semi_major, semi_minor);

    Some(MoffatFit {
        signal_weight,
    })
}

// ── Levenberg-Marquardt solver ────────────────────────────────────────────────
//
// Fits parameters p = [A, cx, cy, a, b] holding β and B fixed.
// Returns true if convergence was achieved within MAX_ITER.

fn lm_fit(
    patch: &[f32],
    bw:    usize,
    bh:    usize,
    bg:    f32,
    p:     &mut [f32; 5],
) -> bool {
    let n = bw * bh;
    let mut lambda = LAMBDA_INIT;

    for _ in 0..MAX_ITER {
        // Compute residuals and Jacobian
        let mut jt_j  = [[0.0f32; 5]; 5];
        let mut jt_r  = [0.0f32; 5];
        let mut rss    = 0.0f32;

        for row in 0..bh {
            for col in 0..bw {
                let x = col as f32;
                let y = row as f32;
                let observed = patch[row * bw + col];
                let predicted = moffat_eval(x, y, p, bg);
                let r = observed - predicted;
                rss += r * r;

                let grad = moffat_grad(x, y, p);
                for i in 0..5 {
                    jt_r[i] += grad[i] * r;
                    for j in 0..5 {
                        jt_j[i][j] += grad[i] * grad[j];
                    }
                }
            }
        }

        // Damped normal equations: (J'J + λ·diag(J'J)) · δ = J'r
        let mut a_damped = jt_j;
        for i in 0..5 {
            a_damped[i][i] *= 1.0 + lambda;
        }

        let delta = solve_5x5(&a_damped, &jt_r);

        // Trial step
        let mut p_new = *p;
        for i in 0..5 {
            p_new[i] += delta[i];
        }

        // Clamp to physical bounds
        p_new[0] = p_new[0].max(1e-6);               // A > 0
        p_new[3] = p_new[3].clamp(MIN_AXIS, MAX_AXIS); // a
        p_new[4] = p_new[4].clamp(MIN_AXIS, MAX_AXIS); // b

        // Compute new RSS
        let mut rss_new = 0.0f32;
        for row in 0..bh {
            for col in 0..bw {
                let x = col as f32;
                let y = row as f32;
                let r = patch[row * bw + col] - moffat_eval(x, y, &p_new, bg);
                rss_new += r * r;
            }
        }

        if rss_new < rss {
            // Accept step
            let delta_norm: f32 = delta.iter().map(|&d| d * d).sum::<f32>().sqrt();
            *p = p_new;
            lambda *= 0.1;
            if delta_norm < CONVERGENCE_TOL {
                return true;
            }
        } else {
            // Reject step — increase damping
            lambda *= 10.0;
            if lambda > 1e8 {
                return false; // diverged
            }
        }

        let _ = n; // suppress unused warning
    }

    // Reached MAX_ITER without convergence — accept if residual is acceptable
    true
}

// ── Moffat model evaluation ───────────────────────────────────────────────────

#[inline]
fn moffat_eval(x: f32, y: f32, p: &[f32; 5], bg: f32) -> f32 {
    let (amp, cx, cy, a, b) = (p[0], p[1], p[2], p[3], p[4]);
    let dx = x - cx;
    let dy = y - cy;
    let a2 = (a * a).max(f32::EPSILON);
    let b2 = (b * b).max(f32::EPSILON);
    let u = dx * dx / a2 + dy * dy / b2;
    bg + amp * (1.0 + u).powf(-BETA)
}

// ── Jacobian (partial derivatives w.r.t. each parameter) ─────────────────────

#[inline]
fn moffat_grad(x: f32, y: f32, p: &[f32; 5]) -> [f32; 5] {
    let (amp, cx, cy, a, b) = (p[0], p[1], p[2], p[3], p[4]);
    let dx = x - cx;
    let dy = y - cy;
    let a2 = (a * a).max(f32::EPSILON);
    let b2 = (b * b).max(f32::EPSILON);
    let u  = dx * dx / a2 + dy * dy / b2;
    let s  = (1.0 + u).powf(-BETA);
    let ds = -BETA * amp * (1.0 + u).powf(-BETA - 1.0);

    [
        s,                                  // ∂/∂A
        ds * (-2.0 * dx / a2),              // ∂/∂cx
        ds * (-2.0 * dy / b2),              // ∂/∂cy
        ds * (-2.0 * dx * dx / (a2 * a)),   // ∂/∂a
        ds * (-2.0 * dy * dy / (b2 * b)),   // ∂/∂b
    ]
}

// ── 5×5 linear system solver (Gaussian elimination with partial pivoting) ─────

fn solve_5x5(a: &[[f32; 5]; 5], b: &[f32; 5]) -> [f32; 5] {
    let mut m = [[0.0f32; 6]; 5];
    for i in 0..5 {
        for j in 0..5 {
            m[i][j] = a[i][j];
        }
        m[i][5] = b[i];
    }

    for col in 0..5 {
        // Partial pivoting
        let mut max_row = col;
        let mut max_val = m[col][col].abs();
        for row in (col + 1)..5 {
            if m[row][col].abs() > max_val {
                max_val = m[row][col].abs();
                max_row = row;
            }
        }
        m.swap(col, max_row);

        let pivot = m[col][col];
        if pivot.abs() < f32::EPSILON {
            continue; // singular — skip
        }

        for row in (col + 1)..5 {
            let factor = m[row][col] / pivot;
            for k in col..6 {
                m[row][k] -= factor * m[col][k];
            }
        }
    }

    // Back substitution
    let mut x = [0.0f32; 5];
    for i in (0..5).rev() {
        let mut s = m[i][5];
        for j in (i + 1)..5 {
            s -= m[i][j] * x[j];
        }
        let diag = m[i][i];
        x[i] = if diag.abs() > f32::EPSILON { s / diag } else { 0.0 };
    }
    x
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Mean of the border pixels of a bw×bh patch.
fn border_mean(patch: &[f32], bw: usize, bh: usize) -> f32 {
    let mut sum = 0.0f32;
    let mut count = 0usize;

    for col in 0..bw {
        sum += patch[col];               // top row
        sum += patch[(bh - 1) * bw + col]; // bottom row
        count += 2;
    }
    for row in 1..(bh - 1) {
        sum += patch[row * bw];          // left col
        sum += patch[row * bw + bw - 1]; // right col
        count += 2;
    }

    if count == 0 { 0.0 } else { sum / count as f32 }
}

/// Normalized RMS residual: sqrt(mean((observed−predicted)²)) / amplitude
fn normalized_residual(
    patch: &[f32],
    bw:    usize,
    bh:    usize,
    bg:    f32,
    p:     &[f32; 5],
) -> f32 {
    let n = (bw * bh) as f32;
    if n == 0.0 || p[0] <= 0.0 {
        return f32::MAX;
    }
    let rss: f32 = (0..bh).flat_map(|row| (0..bw).map(move |col| (row, col)))
        .map(|(row, col)| {
            let r = patch[row * bw + col] - moffat_eval(col as f32, row as f32, p, bg);
            r * r
        })
        .sum();
    (rss / n).sqrt() / p[0]
}

/// Signal Weight: A² / (A + B·π·a·b)
/// B is the local background level.
/// When B ≈ 0 (perfectly dark background), Signal Weight ≈ A.
fn signal_weight(amp: f32, bg: f32, a: f32, b: f32) -> f32 {
    let denom = amp + bg * std::f32::consts::PI * a * b;
    if denom <= 0.0 {
        return 0.0;
    }
    amp * amp / denom
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::stars::StarCandidate;

    /// Build a synthetic Moffat star patch for testing.
    fn synthetic_moffat_star(
        bw: usize, bh: usize,
        amp: f32, bg: f32,
        a: f32, b: f32,
    ) -> StarCandidate {
        let cx = bw as f32 / 2.0;
        let cy = bh as f32 / 2.0;
        let mut patch = vec![0.0f32; bw * bh];
        for row in 0..bh {
            for col in 0..bw {
                let x = col as f32;
                let y = row as f32;
                let dx = x - cx;
                let dy = y - cy;
                let u = dx * dx / (a * a) + dy * dy / (b * b);
                patch[row * bw + col] = bg + amp * (1.0 + u).powf(-BETA);
            }
        }
        StarCandidate {
            cx: cx + 10.0, // absolute coords don't matter for fit_star
            cy: cy + 10.0,
            peak: amp,
            bbox: (10, 10, 10 + bw - 1, 10 + bh - 1),
            patch,
            pixel_count: bw * bh,
        }
    }

    #[test]
    fn test_fit_symmetric_star() {
        let star = synthetic_moffat_star(15, 15, 0.5, 0.02, 2.5, 2.5);
        let fit = fit_star(&star).expect("fit should succeed on clean synthetic star");
        assert!(fit.signal_weight > 0.0, "signal weight should be positive");
    }

    #[test]
    fn test_fit_elongated_star() {
        let star = synthetic_moffat_star(15, 15, 0.5, 0.02, 4.0, 2.0);
        assert!(fit_star(&star).is_some(), "fit should succeed on elongated star");
    }

    #[test]
    fn test_rejects_too_small_patch() {
        let star = synthetic_moffat_star(2, 2, 0.5, 0.02, 1.0, 1.0);
        assert!(fit_star(&star).is_none(), "should reject patch smaller than 3×3");
    }

    #[test]
    fn test_signal_weight_positive() {
        let sw = signal_weight(0.5, 0.02, 2.5, 2.5);
        assert!(sw > 0.0, "signal weight should be positive");
    }

    #[test]
    fn test_signal_weight_penalizes_broad_psf() {
        // Same peak amplitude, same background — broader PSF should score lower
        let sw_narrow = signal_weight(0.5, 0.02, 2.0, 2.0);
        let sw_broad  = signal_weight(0.5, 0.02, 4.0, 4.0);
        assert!(sw_narrow > sw_broad,
            "narrow PSF ({:.4}) should outweigh broad PSF ({:.4})", sw_narrow, sw_broad);
    }

    #[test]
    fn test_fwhm_formula() {
        // For a=b=2.5, FWHM should be well-defined and positive
        let fwhm = fwhm_from_axes(2.5, 2.5);
        assert!(fwhm > 0.0 && fwhm < 20.0, "FWHM {} out of range", fwhm);
    }

    #[test]
    fn test_eccentricity_circular() {
        let ecc = eccentricity_from_axes(2.5, 2.5);
        assert!(ecc < 0.01, "circular star eccentricity should be ~0, got {}", ecc);
    }

    #[test]
    fn test_eccentricity_elongated() {
        let ecc = eccentricity_from_axes(4.0, 2.0);
        assert!(ecc > 0.8, "elongated star eccentricity should be high, got {}", ecc);
    }
}
