// analysis/fft_align.rs — FFT phase correlation for frame alignment
// Stacking document §3.5 Stage 3
//
// Computes the sub-pixel translation between a reference frame and a target
// frame using phase correlation in the frequency domain.
//
// Pipeline per frame:
//   1. Apodize both images with a 2D Hann window to suppress edge artifacts
//   2. FFT both images
//   3. Compute normalized cross-power spectrum: (F * conj(G)) / |F * conj(G)|
//   4. Inverse FFT → correlation surface
//   5. Find peak → integer translation
//   6. Sub-pixel refinement via 2D parabolic interpolation around the peak

use rustfft::{FftPlanner, num_complex::Complex};

// ── Result type ───────────────────────────────────────────────────────────────

/// Translation of the target frame relative to the reference frame, in pixels.
/// Positive dx = target is shifted right; positive dy = target is shifted down.
#[derive(Debug, Clone)]
pub struct AlignmentTranslation {
    pub dx: f32,
    pub dy: f32,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Compute the translation between `reference` and `target` using FFT phase
/// correlation. Both slices must be the same dimensions (width × height),
/// in row-major order, normalized to 0.0–1.0.
///
/// Returns None if the images are empty or the correlation peak is degenerate.
pub fn compute_translation(
    reference: &[f32],
    target:    &[f32],
    width:     usize,
    height:    usize,
) -> Option<AlignmentTranslation> {
    if width == 0 || height == 0 {
        return None;
    }
    if reference.len() < width * height || target.len() < width * height {
        return None;
    }

    let n = width * height;

    // ── Step 1: Apodize with 2D Hann window ──────────────────────────────────
    let window = hann_window_2d(width, height);

    let mut ref_complex: Vec<Complex<f32>> = reference[..n]
        .iter()
        .zip(window.iter())
        .map(|(&p, &w)| Complex { re: p * w, im: 0.0 })
        .collect();

    let mut tgt_complex: Vec<Complex<f32>> = target[..n]
        .iter()
        .zip(window.iter())
        .map(|(&p, &w)| Complex { re: p * w, im: 0.0 })
        .collect();

    // ── Step 2: FFT both images (row-major 2D via repeated 1D FFTs) ──────────
    fft_2d(&mut ref_complex, width, height, false);
    fft_2d(&mut tgt_complex, width, height, false);

    // ── Step 3: Normalized cross-power spectrum ───────────────────────────────
    let mut cross: Vec<Complex<f32>> = ref_complex
        .iter()
        .zip(tgt_complex.iter())
        .map(|(&r, &t)| {
            let product = r * t.conj();
            let mag = product.norm();
            if mag > 1e-10 {
                product / mag
            } else {
                Complex { re: 0.0, im: 0.0 }
            }
        })
        .collect();

    // ── Step 4: Inverse FFT → correlation surface ─────────────────────────────
    fft_2d(&mut cross, width, height, true);

    // Correlation surface (real part only)
    let correlation: Vec<f32> = cross.iter().map(|c| c.re).collect();

    // ── Step 5: Find peak (integer translation) ───────────────────────────────
    let peak_idx = correlation
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)?;

    let peak_x = peak_idx % width;
    let peak_y = peak_idx / width;

    // ── Step 6: Sub-pixel refinement via parabolic interpolation ─────────────
    let (dx_sub, dy_sub) = subpixel_refine(&correlation, width, height, peak_x, peak_y);

    // Convert from correlation-space (wrapped) to signed translation.
    // Peak at (px, py) in correlation space means the target is shifted by
    // (px, py) relative to the reference. Wrap around the midpoint to get
    // the signed displacement.
    let half_w = (width  / 2) as f32;
    let half_h = (height / 2) as f32;

    let raw_dx = peak_x as f32 + dx_sub;
    let raw_dy = peak_y as f32 + dy_sub;

    let dx = if raw_dx > half_w { raw_dx - width  as f32 } else { raw_dx };
    let dy = if raw_dy > half_h { raw_dy - height as f32 } else { raw_dy };

    Some(AlignmentTranslation { dx, dy })
}

// ── 2D FFT via repeated 1D FFTs ───────────────────────────────────────────────
// Performs an in-place 2D FFT on a row-major buffer by applying 1D FFTs
// along rows then columns.

fn fft_2d(buf: &mut Vec<Complex<f32>>, width: usize, height: usize, inverse: bool) {
    let mut planner = FftPlanner::<f32>::new();

    // Row-wise FFTs
    let row_fft = if inverse {
        planner.plan_fft_inverse(width)
    } else {
        planner.plan_fft_forward(width)
    };
    for row in 0..height {
        let start = row * width;
        row_fft.process(&mut buf[start..start + width]);
    }

    // Column-wise FFTs — extract column, FFT, write back
    let col_fft = if inverse {
        planner.plan_fft_inverse(height)
    } else {
        planner.plan_fft_forward(height)
    };
    let mut col_buf = vec![Complex { re: 0.0f32, im: 0.0 }; height];
    for col in 0..width {
        for row in 0..height {
            col_buf[row] = buf[row * width + col];
        }
        col_fft.process(&mut col_buf);
        for row in 0..height {
            buf[row * width + col] = col_buf[row];
        }
    }

    // Normalize inverse FFT
    if inverse {
        let scale = 1.0 / (width * height) as f32;
        for v in buf.iter_mut() {
            v.re *= scale;
            v.im *= scale;
        }
    }
}

// ── 2D Hann window ────────────────────────────────────────────────────────────

fn hann_window_2d(width: usize, height: usize) -> Vec<f32> {
    let hann_row: Vec<f32> = (0..width)
        .map(|x| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * x as f32 / (width  - 1) as f32).cos()))
        .collect();
    let hann_col: Vec<f32> = (0..height)
        .map(|y| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * y as f32 / (height - 1) as f32).cos()))
        .collect();

    let mut window = vec![0.0f32; width * height];
    for y in 0..height {
        for x in 0..width {
            window[y * width + x] = hann_row[x] * hann_col[y];
        }
    }
    window
}

// ── Sub-pixel refinement ──────────────────────────────────────────────────────
// Fits a parabola through the peak and its immediate neighbours in each axis.
// Returns the sub-pixel offset (dx, dy) relative to the integer peak.

fn subpixel_refine(
    correlation: &[f32],
    width:       usize,
    height:      usize,
    peak_x:      usize,
    peak_y:      usize,
) -> (f32, f32) {
    // X axis
    let dx = if peak_x > 0 && peak_x + 1 < width {
        let l = correlation[peak_y * width + peak_x - 1];
        let c = correlation[peak_y * width + peak_x];
        let r = correlation[peak_y * width + peak_x + 1];
        let denom = 2.0 * c - l - r;
        if denom.abs() > 1e-10 { (l - r) / (2.0 * denom) } else { 0.0 }
    } else {
        0.0
    };

    // Y axis
    let dy = if peak_y > 0 && peak_y + 1 < height {
        let u = correlation[(peak_y - 1) * width + peak_x];
        let c = correlation[peak_y       * width + peak_x];
        let d = correlation[(peak_y + 1) * width + peak_x];
        let denom = 2.0 * c - u - d;
        if denom.abs() > 1e-10 { (u - d) / (2.0 * denom) } else { 0.0 }
    } else {
        0.0
    };

    (dx, dy)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_with_star(width: usize, height: usize, cx: usize, cy: usize) -> Vec<f32> {
        let mut img = vec![0.05f32; width * height];
        // Simple Gaussian-ish blob
        for dy in -3i32..=3 {
            for dx in -3i32..=3 {
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                    let r2 = (dx * dx + dy * dy) as f32;
                    img[y as usize * width + x as usize] += 0.5 * (-r2 / 4.0).exp();
                }
            }
        }
        img
    }

    fn shift_image(src: &[f32], width: usize, height: usize, dx: i32, dy: i32) -> Vec<f32> {
        let mut out = vec![0.05f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let sx = x as i32 - dx;
                let sy = y as i32 - dy;
                if sx >= 0 && sx < width as i32 && sy >= 0 && sy < height as i32 {
                    out[y * width + x] = src[sy as usize * width + sx as usize];
                }
            }
        }
        out
    }

    #[test]
    fn test_zero_translation() {
        let (w, h) = (128, 128);
        let img = flat_with_star(w, h, 64, 64);
        let result = compute_translation(&img, &img, w, h).unwrap();
        assert!(result.dx.abs() < 0.5, "dx {} should be near 0", result.dx);
        assert!(result.dy.abs() < 0.5, "dy {} should be near 0", result.dy);
    }

    #[test]
    fn test_known_integer_translation() {
        let (w, h) = (128, 128);
        let reference = flat_with_star(w, h, 64, 64);
        let target    = shift_image(&reference, w, h, 5, -3);
        let result    = compute_translation(&reference, &target, w, h).unwrap();
        assert!((result.dx - 5.0).abs() < 1.0, "dx {} should be near 5", result.dx);
        assert!((result.dy + 3.0).abs() < 1.0, "dy {} should be near -3", result.dy);
    }

    #[test]
    fn test_empty_returns_none() {
        assert!(compute_translation(&[], &[], 0, 0).is_none());
    }
}
