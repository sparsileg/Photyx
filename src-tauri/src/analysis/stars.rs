// analysis/stars.rs — shared star detection for FWHM, eccentricity, and star count
// Spec §15.4, §7.8 (ComputeFWHM, CountStars, ComputeEccentricity)
//
// Pipeline:
//   1. Sigma-clipped background estimate (reuses background.rs)
//   2. Background subtract → working image
//   3. Threshold at detection_threshold × σ_bg
//   4. Local maximum test within peak_radius
//   5. Connected-component flood fill down to flood_threshold × σ_bg
//   6. Reject saturated stars
//   7. Return Vec<StarCandidate>

use super::{StarDetectionConfig, background::sigma_clipped_background};

// ── Star candidate ────────────────────────────────────────────────────────────

/// A detected star with centroid, bounding box, and pixel patch.
/// FWHM and eccentricity plugins consume this struct directly.
#[derive(Debug, Clone)]
pub struct StarCandidate {
    /// Sub-pixel centroid X (image coordinates, 0-based)
    pub cx: f32,
    /// Sub-pixel centroid Y (image coordinates, 0-based)
    pub cy: f32,
    /// Peak pixel value (background-subtracted, normalized 0.0–1.0)
    pub peak: f32,
    /// Bounding box: (x0, y0, x1, y1) in pixel coordinates (inclusive)
    pub bbox: (usize, usize, usize, usize),
    /// Pixel patch extracted from the background-subtracted image,
    /// row-major, same dimensions as bbox
    pub patch: Vec<f32>,
    /// Number of pixels in the connected component
    pub pixel_count: usize,
}

// ── Detection ─────────────────────────────────────────────────────────────────

/// Detect stars in a luminance image (f32, normalized 0.0–1.0).
///
/// Returns a `Vec<StarCandidate>` sorted by descending peak brightness.
pub fn detect_stars(
    luma:   &[f32],
    width:  usize,
    height: usize,
    config: &StarDetectionConfig,
) -> Vec<StarCandidate> {
    if width == 0 || height == 0 || luma.len() < width * height {
        return Vec::new();
    }

    // ── Step 1: sigma-clipped background estimate ─────────────────────────────
    // Subsample for speed (every 4th pixel)
    let sample: Vec<f32> = luma.iter().cloned().step_by(4).collect();
    let bg_est = sigma_clipped_background(&sample, &config.sigma_clip);
    let bg      = bg_est.median;
    let bg_sd   = bg_est.stddev.max(1e-6); // guard against zero stddev

    let detect_thresh = bg + config.detection_threshold * bg_sd;
    let flood_thresh  = bg + config.flood_threshold     * bg_sd;

    // ── Step 2: background subtract ──────────────────────────────────────────
    let bgsub: Vec<f32> = luma.iter().map(|&v| (v - bg).max(0.0)).collect();

    // ── Step 3 & 4: find local maxima above detection threshold ──────────────
    let r = config.peak_radius as usize;
    let mut peaks: Vec<(usize, usize, f32)> = Vec::new(); // (x, y, value)

    for y in r..height.saturating_sub(r) {
        for x in r..width.saturating_sub(r) {
            let val = luma[y * width + x];
            if val < detect_thresh {
                continue;
            }
            // Reject saturated stars
            if val >= config.saturation_threshold {
                continue;
            }
            // Local maximum test: must be brightest pixel in peak_radius neighbourhood
            if is_local_max(&luma, width, height, x, y, r) {
                peaks.push((x, y, bgsub[y * width + x]));
            }
        }
    }

    // ── Step 5: flood fill from each peak ────────────────────────────────────
    let mut visited = vec![false; width * height];
    let mut candidates: Vec<StarCandidate> = Vec::new();

    for (px, py, peak_val) in peaks {
        let idx = py * width + px;
        if visited[idx] {
            continue; // already consumed by a brighter star
        }

        let component = flood_fill(
            &bgsub,
            &mut visited,
            width,
            height,
            px,
            py,
            flood_thresh - bg, // flood threshold in background-subtracted space
        );

        if component.is_empty() {
            continue;
        }

        // Bounding box
        let x0 = component.iter().map(|&(x, _)| x).min().unwrap();
        let x1 = component.iter().map(|&(x, _)| x).max().unwrap();
        let y0 = component.iter().map(|&(_, y)| y).min().unwrap();
        let y1 = component.iter().map(|&(_, y)| y).max().unwrap();

        // Weighted centroid (intensity-weighted center of mass)
        let (mut sum_xw, mut sum_yw, mut sum_w) = (0.0f64, 0.0f64, 0.0f64);
        for &(x, y) in &component {
            let w = bgsub[y * width + x] as f64;
            sum_xw += x as f64 * w;
            sum_yw += y as f64 * w;
            sum_w  += w;
        }
        let (cx, cy) = if sum_w > 0.0 {
            ((sum_xw / sum_w) as f32, (sum_yw / sum_w) as f32)
        } else {
            (px as f32, py as f32)
        };

        // Extract pixel patch (bbox region from bgsub image)
        let bw = x1 - x0 + 1;
        let bh = y1 - y0 + 1;
        let mut patch = vec![0.0f32; bw * bh];
        for &(x, y) in &component {
            patch[(y - y0) * bw + (x - x0)] = bgsub[y * width + x];
        }

        candidates.push(StarCandidate {
            cx,
            cy,
            peak: peak_val,
            bbox: (x0, y0, x1, y1),
            patch,
            pixel_count: component.len(),
        });
    }

    // Sort by descending peak brightness
    candidates.sort_unstable_by(|a, b| {
        b.peak.partial_cmp(&a.peak).unwrap_or(std::cmp::Ordering::Equal)
    });

    candidates
}

// ── Local maximum test ────────────────────────────────────────────────────────

fn is_local_max(
    luma:   &[f32],
    width:  usize,
    height: usize,
    cx:     usize,
    cy:     usize,
    radius: usize,
) -> bool {
    let val = luma[cy * width + cx];
    let x0 = cx.saturating_sub(radius);
    let x1 = (cx + radius + 1).min(width);
    let y0 = cy.saturating_sub(radius);
    let y1 = (cy + radius + 1).min(height);

    for y in y0..y1 {
        for x in x0..x1 {
            if x == cx && y == cy {
                continue;
            }
            if luma[y * width + x] >= val {
                return false;
            }
        }
    }
    true
}

// ── Flood fill ────────────────────────────────────────────────────────────────
// 4-connected flood fill collecting all pixels above flood_threshold.
// Uses an iterative stack rather than recursion to avoid stack overflow on large stars.

fn flood_fill(
    bgsub:           &[f32],
    visited:         &mut Vec<bool>,
    width:           usize,
    height:          usize,
    start_x:         usize,
    start_y:         usize,
    flood_threshold: f32,   // in background-subtracted (bgsub) space
) -> Vec<(usize, usize)> {
    let mut component: Vec<(usize, usize)> = Vec::new();
    let mut stack: Vec<(usize, usize)> = vec![(start_x, start_y)];

    while let Some((x, y)) = stack.pop() {
        let idx = y * width + x;
        if visited[idx] {
            continue;
        }
        if bgsub[idx] < flood_threshold {
            continue;
        }
        visited[idx] = true;
        component.push((x, y));

        // 4-connected neighbours
        if x > 0          { stack.push((x - 1, y)); }
        if x + 1 < width  { stack.push((x + 1, y)); }
        if y > 0          { stack.push((x, y - 1)); }
        if y + 1 < height { stack.push((x, y + 1)); }
    }

    component
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_with_stars(width: usize, height: usize, stars: &[(usize, usize, f32)]) -> Vec<f32> {
        let mut img = vec![0.05f32; width * height];
        for &(x, y, brightness) in stars {
            // Gaussian-ish blob: set center and immediate neighbours
            img[y * width + x] = brightness;
            if x > 0          { img[y * width + x - 1] = brightness * 0.5; }
            if x + 1 < width  { img[y * width + x + 1] = brightness * 0.5; }
            if y > 0          { img[(y-1) * width + x] = brightness * 0.5; }
            if y + 1 < height { img[(y+1) * width + x] = brightness * 0.5; }
        }
        img
    }

    #[test]
    fn test_detects_single_star() {
        let (w, h) = (100, 100);
        let img = flat_with_stars(w, h, &[(50, 50, 0.8)]);
        let config = StarDetectionConfig::default();
        let stars = detect_stars(&img, w, h, &config);
        assert!(!stars.is_empty(), "should detect at least one star");
        // Centroid should be near (50, 50)
        let s = &stars[0];
        assert!((s.cx - 50.0).abs() < 2.0, "cx {} not near 50", s.cx);
        assert!((s.cy - 50.0).abs() < 2.0, "cy {} not near 50", s.cy);
    }

    #[test]
    fn test_detects_multiple_stars() {
        let (w, h) = (200, 200);
        let img = flat_with_stars(w, h, &[
            (50,  50,  0.8),
            (150, 50,  0.7),
            (50,  150, 0.75),
            (150, 150, 0.65),
        ]);
        let config = StarDetectionConfig::default();
        let stars = detect_stars(&img, w, h, &config);
        assert!(stars.len() >= 4, "expected ≥4 stars, got {}", stars.len());
    }

    #[test]
    fn test_rejects_saturated_stars() {
        let (w, h) = (100, 100);
        // One saturated star (value ≥ 0.98), one normal star
        let img = flat_with_stars(w, h, &[(50, 50, 0.99), (20, 20, 0.7)]);
        let config = StarDetectionConfig::default();
        let stars = detect_stars(&img, w, h, &config);
        // Saturated star at (50,50) should be rejected
        for s in &stars {
            assert!(
                (s.cx - 50.0).abs() > 2.0 || (s.cy - 50.0).abs() > 2.0,
                "saturated star should not be in results"
            );
        }
    }

    #[test]
    fn test_empty_image_returns_empty() {
        let config = StarDetectionConfig::default();
        let stars = detect_stars(&[], 0, 0, &config);
        assert!(stars.is_empty());
    }

    #[test]
    fn test_flat_image_no_stars() {
        let img = vec![0.05f32; 100 * 100];
        let config = StarDetectionConfig::default();
        let stars = detect_stars(&img, 100, 100, &config);
        assert!(stars.is_empty(), "flat image should have no stars, got {}", stars.len());
    }
}

// ----------------------------------------------------------------------
