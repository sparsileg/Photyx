// analysis/fft_align.rs — FFT phase correlation for frame alignment
// Stacking document §3.5 Stage 3
//
// Computes the sub-pixel translation between a reference frame and a target
// frame using phase correlation in the frequency domain.
//
// To keep FFT computation tractable on large images, both frames are
// downsampled to a registration resolution (≤ REG_SIZE pixels on the long
// axis) before the FFT. The recovered translation is then scaled back to
// full-resolution coordinates.
//
// Pipeline per frame:
//   1. Downsample both images to registration resolution
//   2. Apodize with a 2D Hann window (suppress edge artifacts)
//   3. FFT both images
//   4. Compute normalized cross-power spectrum: (F * conj(G)) / |F * conj(G)|
//   5. Inverse FFT → correlation surface
//   6. Find peak → integer translation at registration resolution
//   7. Sub-pixel refinement via 2D parabolic interpolation
//   8. Scale translation back to full-resolution coordinates

use rustfft::{FftPlanner, num_complex::Complex};

/// Target registration resolution (pixels on the long axis).
/// Images larger than this are downsampled before FFT.
const REG_SIZE: usize = 1024;

// ── Result type ───────────────────────────────────────────────────────────────

/// Translation of the target frame relative to the reference frame, in
/// full-resolution pixels. Positive dx = target shifted right; positive
/// dy = target shifted down.
#[derive(Debug, Clone)]
pub struct AlignmentTranslation {
    pub dx: f32,
    pub dy: f32,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Compute the translation between `reference` and `target` using FFT phase
/// correlation at reduced registration resolution.
///
/// Both slices must be the same dimensions (width × height), row-major,
/// normalized to 0.0–1.0.
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

    // ── Compute registration resolution ───────────────────────────────────────
    let scale = if width.max(height) > REG_SIZE {
        REG_SIZE as f32 / width.max(height) as f32
    } else {
        1.0
    };

    let reg_w = ((width  as f32 * scale).round() as usize).max(1);
    let reg_h = ((height as f32 * scale).round() as usize).max(1);

    // ── Step 1: Downsample ────────────────────────────────────────────────────
    let ref_ds = downsample(reference, width, height, reg_w, reg_h);
    let tgt_ds = downsample(target,    width, height, reg_w, reg_h);

    // ── Step 2: Apodize with 2D Hann window ──────────────────────────────────
    let window = hann_window_2d(reg_w, reg_h);

    let mut ref_complex: Vec<Complex<f32>> = ref_ds
        .iter()
        .zip(window.iter())
        .map(|(&p, &w)| Complex { re: p * w, im: 0.0 })
        .collect();

    let mut tgt_complex: Vec<Complex<f32>> = tgt_ds
        .iter()
        .zip(window.iter())
        .map(|(&p, &w)| Complex { re: p * w, im: 0.0 })
        .collect();

    // ── Step 3: FFT both images ───────────────────────────────────────────────
    fft_2d(&mut ref_complex, reg_w, reg_h, false);
    fft_2d(&mut tgt_complex, reg_w, reg_h, false);

    // ── Step 4: Normalized cross-power spectrum ───────────────────────────────
    let mut cross: Vec<Complex<f32>> = ref_complex
        .iter()
        .zip(tgt_complex.iter())
        .map(|(&r, &t)| {
            let product = r * t.conj();
            let mag = product.norm();
            if mag > 1e-10 { product / mag } else { Complex { re: 0.0, im: 0.0 } }
        })
        .collect();

    // ── Step 5: Inverse FFT → correlation surface ─────────────────────────────
    fft_2d(&mut cross, reg_w, reg_h, true);
    let correlation: Vec<f32> = cross.iter().map(|c| c.re).collect();

    // ── Step 6: Find peak ─────────────────────────────────────────────────────
    let peak_idx = correlation
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)?;

    let peak_x = peak_idx % reg_w;
    let peak_y = peak_idx / reg_w;

    // ── Step 7: Sub-pixel refinement ─────────────────────────────────────────
    let (dx_sub, dy_sub) = subpixel_refine(&correlation, reg_w, reg_h, peak_x, peak_y);

    // Convert from wrapped correlation space to signed translation
    let half_w = (reg_w / 2) as f32;
    let half_h = (reg_h / 2) as f32;

    let raw_dx = peak_x as f32 + dx_sub;
    let raw_dy = peak_y as f32 + dy_sub;

    let dx_reg = if raw_dx > half_w { raw_dx - reg_w as f32 } else { raw_dx };
    let dy_reg = if raw_dy > half_h { raw_dy - reg_h as f32 } else { raw_dy };

    // ── Step 8: Scale back to full resolution ─────────────────────────────────
    let dx = dx_reg / scale;
    let dy = dy_reg / scale;

    Some(AlignmentTranslation { dx, dy })
}

// ── Downsample ────────────────────────────────────────────────────────────────
// Box-filter downsample from (src_w × src_h) to (dst_w × dst_h).

fn downsample(
    src:   &[f32],
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
) -> Vec<f32> {
    if dst_w == src_w && dst_h == src_h {
        return src.to_vec();
    }

    let x_ratio = src_w as f32 / dst_w as f32;
    let y_ratio = src_h as f32 / dst_h as f32;

    let mut out = vec![0.0f32; dst_w * dst_h];

    for dy in 0..dst_h {
        for dx in 0..dst_w {
            let x0 = (dx as f32 * x_ratio) as usize;
            let y0 = (dy as f32 * y_ratio) as usize;
            let x1 = (((dx + 1) as f32 * x_ratio) as usize).min(src_w);
            let y1 = (((dy + 1) as f32 * y_ratio) as usize).min(src_h);

            let mut sum   = 0.0f32;
            let mut count = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let v = src[sy * src_w + sx];
                    if v.is_finite() { sum += v; count += 1; }
                }
            }
            out[dy * dst_w + dx] = if count > 0 { sum / count as f32 } else { 0.0 };
        }
    }
    out
}

// ── 2D FFT via repeated 1D FFTs ───────────────────────────────────────────────

fn fft_2d(buf: &mut Vec<Complex<f32>>, width: usize, height: usize, inverse: bool) {
    let mut planner = FftPlanner::<f32>::new();

    // Row-wise FFTs
    let row_fft = if inverse { planner.plan_fft_inverse(width) } else { planner.plan_fft_forward(width) };
    for row in 0..height {
        let start = row * width;
        row_fft.process(&mut buf[start..start + width]);
    }

    // Column-wise FFTs
    let col_fft = if inverse { planner.plan_fft_inverse(height) } else { planner.plan_fft_forward(height) };
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

fn subpixel_refine(
    correlation: &[f32],
    width:       usize,
    height:      usize,
    peak_x:      usize,
    peak_y:      usize,
) -> (f32, f32) {
    let dx = if peak_x > 0 && peak_x + 1 < width {
        let l = correlation[peak_y * width + peak_x - 1];
        let c = correlation[peak_y * width + peak_x];
        let r = correlation[peak_y * width + peak_x + 1];
        let denom = 2.0 * c - l - r;
        if denom.abs() > 1e-10 { (l - r) / (2.0 * denom) } else { 0.0 }
    } else { 0.0 };

    let dy = if peak_y > 0 && peak_y + 1 < height {
        let u = correlation[(peak_y - 1) * width + peak_x];
        let c = correlation[peak_y       * width + peak_x];
        let d = correlation[(peak_y + 1) * width + peak_x];
        let denom = 2.0 * c - u - d;
        if denom.abs() > 1e-10 { (u - d) / (2.0 * denom) } else { 0.0 }
    } else { 0.0 };

    (dx, dy)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_with_star(width: usize, height: usize, cx: usize, cy: usize) -> Vec<f32> {
        let mut img = vec![0.05f32; width * height];
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
        let (w, h) = (256, 256);
        let img = flat_with_star(w, h, 128, 128);
        let result = compute_translation(&img, &img, w, h).unwrap();
        assert!(result.dx.abs() < 1.0, "dx {} should be near 0", result.dx);
        assert!(result.dy.abs() < 1.0, "dy {} should be near 0", result.dy);
    }

    #[test]
    fn test_known_translation() {
        let (w, h) = (512, 512);
        let reference = flat_with_star(w, h, 256, 256);
        let target    = shift_image(&reference, w, h, 20, -15);
        let result    = compute_translation(&reference, &target, w, h).unwrap();
        assert!((result.dx - 20.0).abs() < 2.0, "dx {} should be near 20", result.dx);
        assert!((result.dy + 15.0).abs() < 2.0, "dy {} should be near -15", result.dy);
    }

    #[test]
    fn test_empty_returns_none() {
        assert!(compute_translation(&[], &[], 0, 0).is_none());
    }

    #[test]
    fn test_large_image_downsamples() {
        // 3008×3008 image — verify it completes in reasonable time
        let (w, h) = (3008, 3008);
        let img = flat_with_star(w, h, 1504, 1504);
        let shifted = shift_image(&img, w, h, 50, -30);
        let result = compute_translation(&img, &shifted, w, h).unwrap();
        // At this scale factor (~5.9x) expect accuracy within ~6 pixels
        assert!((result.dx - 50.0).abs() < 7.0, "dx {} should be near 50", result.dx);
        assert!((result.dy + 30.0).abs() < 7.0, "dy {} should be near -30", result.dy);
    }
}
