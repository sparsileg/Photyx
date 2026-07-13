// analysis/background.rs — background estimation and metrics
// Spec §15.4 (background median, background std dev, background gradient)
//
// All three metrics share a single sigma-clipped background estimator.
// The gradient metric divides the image into a grid of cells and reports
// the difference between the highest and lowest cell background medians.

use super::{BackgroundConfig, SigmaClipConfig};

// ── Sigma-clipped statistics ───────────────────────────────────────────────────

/// Result of sigma-clipped background estimation on a pixel sample.
#[derive(Debug, Clone)]
pub struct BackgroundEstimate {
    /// Sigma-clipped median of the background sample
    pub median: f32,
    /// Sigma-clipped standard deviation of the background sample
    pub stddev: f32,
}

/// Compute the median of a mutable slice (sorts in place). Retained for its
/// existing unit tests; production code now uses median_of_presorted below,
/// since sigma_clipped_background maintains a sorted working set throughout
/// rather than re-sorting a fresh clone on every call.
#[allow(dead_code)]
fn median_sorted(values: &mut Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) * 0.5
    }
}

/// Compute the median of an already-sorted-ascending slice, without sorting
/// or cloning. sigma_clipped_background sorts `working` once up front and
/// preserves sortedness across iterations (clipping removes a prefix and
/// suffix rather than filtering), so this is the only median computation
/// its hot loop needs.
fn median_of_presorted(sorted: &[f32]) -> f32 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) * 0.5
    }
}

/// Compute mean and population standard deviation of a slice.
fn mean_stddev(values: &[f32]) -> (f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let n = values.len() as f32;
    let mean = values.iter().sum::<f32>() / n;
    let variance = values.iter().map(|&x| (x - mean) * (x - mean)).sum::<f32>() / n;
    (mean, variance.sqrt())
}

/// Run sigma-clipped background estimation on an arbitrary pixel sample.
///
/// Iteratively rejects pixels beyond `config.sigma` standard deviations from
/// the current median, up to `config.iterations` times. Returns the clipped
/// median and standard deviation.
pub fn sigma_clipped_background(sample: &[f32], config: &SigmaClipConfig) -> BackgroundEstimate {
    if sample.is_empty() {
        return BackgroundEstimate { median: 0.0, stddev: 0.0 };
    }

    let mut working: Vec<f32> = sample.to_vec();
    // Sorted once, up front. Each iteration clips by removing a prefix and
    // suffix (binary-searched via partition_point) rather than filtering,
    // which preserves sortedness — so the array never needs re-sorting
    // after this, unlike the old clone-and-sort-every-iteration approach.
    working.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    for _ in 0..config.iterations {
        if working.is_empty() {
            break;
        }

        let med = median_of_presorted(&working);
        let (_, sd) = mean_stddev(&working);

        if sd == 0.0 {
            break;
        }

        let lo = med - config.sigma * sd;
        let hi = med + config.sigma * sd;

        let before = working.len();

        // working is sorted, so the surviving [lo, hi] range is a
        // contiguous slice — find its bounds by binary search instead of
        // a full linear retain scan.
        let lo_idx = working.partition_point(|&x| x < lo);
        let hi_idx = working.partition_point(|&x| x <= hi);
        working.truncate(hi_idx);
        working.drain(..lo_idx);

        // Converged — no pixels were rejected this iteration
        if working.len() == before {
            break;
        }
    }

    if working.is_empty() {
        // Pathological case: all pixels rejected — return unclipped stats
        let mut all = sample.to_vec();
        all.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let med = median_of_presorted(&all);
        let (_, sd) = mean_stddev(sample);
        return BackgroundEstimate { median: med, stddev: sd };
    }

    let median = median_of_presorted(&working);
    let (_, stddev) = mean_stddev(&working);

    BackgroundEstimate { median, stddev }
}

// ── Full-image background estimation ─────────────────────────────────────────
//
// For large images, running sigma-clip on every pixel is expensive, so we
// subsample first. Both subsample constants below are 1-D strides over the
// flattened pixel array — every Nth pixel in row-major order, NOT a 2-D N×N
// grid — sampling roughly 1/N of all pixels. Stars are bright outliers and
// will be rejected by sigma-clipping regardless of which pixels land in the
// sample.

/// Subsample stride used here, for full-image background estimation.
pub(crate) const BACKGROUND_SUBSAMPLE_STEP: usize = 8;

/// Subsample stride used by star detection's own background pre-pass
/// (detect_stars, stars.rs). Kept as a separate constant rather than
/// unified with BACKGROUND_SUBSAMPLE_STEP — changing star detection's
/// sampling density changes which stars get detected, and that hasn't been
/// empirically verified as behavior-preserving (Issue 86). Both constants
/// now live together, honestly documented, rather than scattered as
/// disagreeing magic numbers with an inaccurate shared comment.
pub(crate) const STAR_DETECTION_SUBSAMPLE_STEP: usize = 4;

fn subsample(pixels: &[f32]) -> Vec<f32> {
    pixels
        .iter()
        .enumerate()
        .filter(|(i, _)| i % BACKGROUND_SUBSAMPLE_STEP == 0)
        .map(|(_, &v)| v)
        .collect()
}

/// Estimate the background level and noise for a full luminance image.
pub fn estimate_background(luma: &[f32], config: &SigmaClipConfig) -> BackgroundEstimate {
    let sample = subsample(luma);
    sigma_clipped_background(&sample, config)
}

// ── Background median metric ──────────────────────────────────────────────────

/// Compute the background median for a luminance image.
/// Returns a value in the 0.0–1.0 normalized range.
#[allow(dead_code)]
pub fn background_median(luma: &[f32], config: &BackgroundConfig) -> f32 {
    estimate_background(luma, &config.sigma_clip).median
}

// ── Background std dev metric ─────────────────────────────────────────────────

/// Compute the background standard deviation for a luminance image.
/// Returns a value in the 0.0–1.0 normalized range.
#[allow(dead_code)]
pub fn background_stddev(luma: &[f32], config: &BackgroundConfig) -> f32 {
    estimate_background(luma, &config.sigma_clip).stddev
}

// ── Background gradient metric ────────────────────────────────────────────────
//
// Divides the image into a grid_size × grid_size grid of cells.
// Computes the sigma-clipped background median for each cell.
// Returns the difference between the maximum and minimum cell medians.
// A high value indicates a strong gradient (light pollution, vignetting, etc.).

/// Compute the background gradient for a luminance image.
/// Returns a value in the 0.0–1.0 normalized range (max_cell_median - min_cell_median).
pub fn background_gradient(
    luma:   &[f32],
    width:  usize,
    height: usize,
    config: &BackgroundConfig,
) -> f32 {
    let grid = config.gradient_grid_size as usize;
    if grid == 0 || width == 0 || height == 0 {
        return 0.0;
    }

    let cell_w = width  / grid;
    let cell_h = height / grid;

    if cell_w == 0 || cell_h == 0 {
        return 0.0;
    }

    let mut cell_medians: Vec<f32> = Vec::with_capacity(grid * grid);

    for row in 0..grid {
        for col in 0..grid {
            let x0 = col * cell_w;
            let y0 = row * cell_h;
            let x1 = (x0 + cell_w).min(width);
            let y1 = (y0 + cell_h).min(height);

            // Collect pixels for this cell
            let mut cell_pixels: Vec<f32> = Vec::with_capacity(cell_w * cell_h);
            for y in y0..y1 {
                for x in x0..x1 {
                    cell_pixels.push(luma[y * width + x]);
                }
            }

            // Subsample within cell before sigma-clipping (cells can be large)
            let sample = subsample(&cell_pixels);
            let est = sigma_clipped_background(&sample, &config.sigma_clip);
            cell_medians.push(est.median);
        }
    }

    if cell_medians.is_empty() {
        return 0.0;
    }

    let max = cell_medians.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min = cell_medians.iter().cloned().fold(f32::INFINITY,     f32::min);

    (max - min).max(0.0)
}

// ── Combined background analysis ──────────────────────────────────────────────

/// Run all three background metrics in one pass (avoids triple subsampling).
pub struct BackgroundMetrics {
    pub median:   f32,
    pub stddev:   f32,
    pub gradient: f32,
}

pub fn compute_background_metrics(
    luma:   &[f32],
    width:  usize,
    height: usize,
    config: &BackgroundConfig,
) -> BackgroundMetrics {
    let est = estimate_background(luma, &config.sigma_clip);
    let gradient = background_gradient(luma, width, height, config);

    BackgroundMetrics {
        median:   est.median,
        stddev:   est.stddev,
        gradient,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> BackgroundConfig {
        BackgroundConfig::default()
    }

    #[test]
    fn test_median_sorted_odd() {
        let mut v = vec![3.0f32, 1.0, 2.0];
        assert_eq!(median_sorted(&mut v), 2.0);
    }

    #[test]
    fn test_median_sorted_even() {
        let mut v = vec![4.0f32, 1.0, 3.0, 2.0];
        assert_eq!(median_sorted(&mut v), 2.5);
    }

    #[test]
    fn test_sigma_clip_rejects_outliers() {
        // 100 background pixels around 0.1, plus 10 bright star pixels at 0.9
        let mut sample: Vec<f32> = (0..100).map(|i| 0.1 + (i as f32) * 0.0005).collect();
        sample.extend(vec![0.9f32; 10]);

        let config = SigmaClipConfig::default();
        let est = sigma_clipped_background(&sample, &config);

        // Stars should be clipped; median should be close to 0.1
        assert!(est.median < 0.15, "median {} should be near 0.1", est.median);
        assert!(est.stddev < 0.05, "stddev {} should be small", est.stddev);
    }

    #[test]
    fn test_background_median_flat_image() {
        let luma = vec![0.05f32; 100 * 100];
        let config = make_config();
        let med = background_median(&luma, &config);
        assert!((med - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_background_gradient_flat_image() {
        // Flat image should have zero gradient
        let luma = vec![0.05f32; 200 * 200];
        let config = make_config();
        let grad = background_gradient(&luma, 200, 200, &config);
        assert!(grad < 0.001, "gradient {} should be ~0 for flat image", grad);
    }

    #[test]
    fn test_background_gradient_ramp() {
        // Left half dark, right half bright — should produce non-zero gradient
        let width = 200usize;
        let height = 200usize;
        let mut luma = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                luma[y * width + x] = if x < width / 2 { 0.05 } else { 0.30 };
            }
        }
        let config = make_config();
        let grad = background_gradient(&luma, width, height, &config);
        assert!(grad > 0.1, "gradient {} should be significant for ramp image", grad);
    }

}


// ----------------------------------------------------------------------
