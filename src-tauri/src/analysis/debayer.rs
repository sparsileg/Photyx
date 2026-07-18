// analysis/debayer.rs — Bilinear Bayer CFA debayering
//
// Supports RGGB, BGGR, GRBG, GBRG patterns.
// Input:  mono f32 slice (normalized 0.0–1.0), width, height, pattern
// Output: interleaved RGB f32 Vec (r, g, b per pixel, same normalization)

use std::collections::HashMap;
use crate::context::KeywordEntry;

/// Bayer CFA pattern — describes the color of the top-left 2×2 block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BayerPattern {
    RGGB,
    BGGR,
    GRBG,
    GBRG,
}

impl BayerPattern {
    /// Parse from a keyword string, case-insensitive. Defaults to RGGB.
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "BGGR" => Self::BGGR,
            "GRBG" => Self::GRBG,
            "GBRG" => Self::GBRG,
            _      => Self::RGGB,
        }
    }

    /// Returns the color channel (0=R, 1=G, 2=B) at pixel (x, y).
    pub fn channel_at(&self, x: usize, y: usize) -> u8 {
        let row = y & 1;
        let col = x & 1;
        match self {
            Self::RGGB => [[0, 1], [1, 2]][row][col],
            Self::BGGR => [[2, 1], [1, 0]][row][col],
            Self::GRBG => [[1, 0], [2, 1]][row][col],
            Self::GBRG => [[1, 2], [0, 1]][row][col],
        }
    }
}

/// True if the keyword map carries either recognized Bayer pattern
/// keyword (BAYERPAT or BAYER_PATTERN), regardless of whether its value
/// parses to a known pattern. Used by image readers to decide whether a
/// mono-layout buffer should be tagged ColorSpace::Bayer instead of
/// ColorSpace::Mono. Issue 122 — the single source of truth for "does
/// this frame carry Bayer pattern metadata," replacing three independent
/// keyword lookups that had drifted (BAYERPAT-only vs. BAYERPAT-or-
/// BAYER_PATTERN) across the reader, debayer, and stacking code paths.
pub fn has_bayer_keyword(keywords: &HashMap<String, KeywordEntry>) -> bool {
    keywords.contains_key("BAYERPAT") || keywords.contains_key("BAYER_PATTERN")
}

/// Looks up the Bayer CFA pattern from a keyword map, checking BAYERPAT
/// then BAYER_PATTERN. Returns None if neither keyword is present —
/// callers that need a pattern after already establishing the buffer is
/// Bayer (e.g. via has_bayer_keyword or a prior color-space check) are
/// responsible for their own RGGB fallback on the None case, matching
/// existing call-site behavior. Issue 122.
pub fn bayer_pattern_of(keywords: &HashMap<String, KeywordEntry>) -> Option<BayerPattern> {
    keywords.get("BAYERPAT")
        .or_else(|| keywords.get("BAYER_PATTERN"))
        .map(|kw| BayerPattern::from_str(&kw.value))
}

/// Bilinear Bayer debayer.
///
/// `mono` — row-major f32 slice, normalized 0.0–1.0
/// Returns interleaved RGB f32 Vec: [r0, g0, b0, r1, g1, b1, ...]
pub fn debayer_bilinear(
    mono:    &[f32],
    width:   usize,
    height:  usize,
    pattern: BayerPattern,
) -> Vec<f32> {
    let n = width * height;
    let mut r_buf = vec![0.0f32; n];
    let mut g_buf = vec![0.0f32; n];
    let mut b_buf = vec![0.0f32; n];

    // Step 1: Copy known channel values into their respective buffers
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let val = mono[idx];
            match pattern.channel_at(x, y) {
                0 => r_buf[idx] = val,
                1 => g_buf[idx] = val,
                2 => b_buf[idx] = val,
                _ => {}
            }
        }
    }

    // Step 2: Interpolate missing values with bilinear averaging
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let ch  = pattern.channel_at(x, y);

            // Helper: sample a buffer at (sx, sy), reflecting at image
            // bounds rather than clamping. Reflection (mirror around the
            // border index) preserves the coordinate's parity, so the
            // reflected pixel is always the same Bayer channel as the
            // original position — clamping instead could land on the
            // pixel's own (different-channel, never-populated) position
            // at a border, corrupting G/R/B interpolation there (Issue 131:
            // test_debayer_flat_image caught this via a flat 0.5 image
            // reading back 0.25 at border pixels). Only ever called with
            // sx/sy one step outside the buffer (immediate neighbors), so
            // a single reflection is sufficient — no need for iterative
            // wraparound.
            let reflect = |s: i32, len: i32| -> i32 {
                if s < 0 { -s } else if s >= len { 2 * (len - 1) - s } else { s }
            };
            let sample = |buf: &[f32], sx: i32, sy: i32| -> f32 {
                let cx = reflect(sx, width  as i32) as usize;
                let cy = reflect(sy, height as i32) as usize;
                buf[cy * width + cx]
            };

            let ix = x as i32;
            let iy = y as i32;

            match ch {
                // Red pixel — interpolate G and B
                0 => {
                    // G: average of 4 cardinal neighbors
                    g_buf[idx] = (sample(&g_buf, ix-1, iy) +
                                  sample(&g_buf, ix+1, iy) +
                                  sample(&g_buf, ix, iy-1) +
                                  sample(&g_buf, ix, iy+1)) * 0.25;
                    // B: average of 4 diagonal neighbors
                    b_buf[idx] = (sample(&b_buf, ix-1, iy-1) +
                                  sample(&b_buf, ix+1, iy-1) +
                                  sample(&b_buf, ix-1, iy+1) +
                                  sample(&b_buf, ix+1, iy+1)) * 0.25;
                }
                // Blue pixel — interpolate G and R
                2 => {
                    g_buf[idx] = (sample(&g_buf, ix-1, iy) +
                                  sample(&g_buf, ix+1, iy) +
                                  sample(&g_buf, ix, iy-1) +
                                  sample(&g_buf, ix, iy+1)) * 0.25;
                    r_buf[idx] = (sample(&r_buf, ix-1, iy-1) +
                                  sample(&r_buf, ix+1, iy-1) +
                                  sample(&r_buf, ix-1, iy+1) +
                                  sample(&r_buf, ix+1, iy+1)) * 0.25;
                }
                // Green pixel — determine if on R row or B row
                1 => {
                    let row = y & 1;
                    let col = x & 1;
                    // Which axis has the same-color neighbors?
                    let (horiz_ch, _vert_ch) = match pattern {
                        BayerPattern::RGGB | BayerPattern::BGGR => {
                            if row == 0 { (0u8, 2u8) } else { (2u8, 0u8) }
                        }
                        BayerPattern::GRBG | BayerPattern::GBRG => {
                            if col == 0 { (0u8, 2u8) } else { (2u8, 0u8) }
                        }
                    };
                    let interp = |buf: &[f32], horizontal: bool| -> f32 {
                        if horizontal {
                            (sample(buf, ix-1, iy) + sample(buf, ix+1, iy)) * 0.5
                        } else {
                            (sample(buf, ix, iy-1) + sample(buf, ix, iy+1)) * 0.5
                        }
                    };
                    if horiz_ch == 0 {
                        r_buf[idx] = interp(&r_buf, true);
                        b_buf[idx] = interp(&b_buf, false);
                    } else {
                        r_buf[idx] = interp(&r_buf, false);
                        b_buf[idx] = interp(&b_buf, true);
                    }
                }
                _ => {}
            }
        }
    }

    // Step 3: Interleave into RGB output
    let mut out = vec![0.0f32; n * 3];
    for i in 0..n {
        out[i * 3]     = r_buf[i].clamp(0.0, 1.0);
        out[i * 3 + 1] = g_buf[i].clamp(0.0, 1.0);
        out[i * 3 + 2] = b_buf[i].clamp(0.0, 1.0);
    }
    out
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rggb_channel_at() {
        let p = BayerPattern::RGGB;
        assert_eq!(p.channel_at(0, 0), 0); // R
        assert_eq!(p.channel_at(1, 0), 1); // G
        assert_eq!(p.channel_at(0, 1), 1); // G
        assert_eq!(p.channel_at(1, 1), 2); // B
    }

    #[test]
    fn test_debayer_output_size() {
        let mono = vec![0.5f32; 4 * 4];
        let out  = debayer_bilinear(&mono, 4, 4, BayerPattern::RGGB);
        assert_eq!(out.len(), 4 * 4 * 3);
    }

    #[test]
    fn test_debayer_flat_image() {
        // A flat gray image should produce equal R, G, B at every pixel
        let mono = vec![0.5f32; 8 * 8];
        let out  = debayer_bilinear(&mono, 8, 8, BayerPattern::RGGB);
        for i in 0..64 {
            let r = out[i * 3];
            let g = out[i * 3 + 1];
            let b = out[i * 3 + 2];
            assert!((r - 0.5).abs() < 0.01, "R off at {}: {}", i, r);
            assert!((g - 0.5).abs() < 0.01, "G off at {}: {}", i, g);
            assert!((b - 0.5).abs() < 0.01, "B off at {}: {}", i, b);
        }
    }
}
