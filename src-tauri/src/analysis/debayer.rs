// analysis/debayer.rs — Bilinear Bayer CFA debayering
//
// Supports RGGB, BGGR, GRBG, GBRG patterns.
// Input:  mono f32 slice (normalized 0.0–1.0), width, height, pattern
// Output: interleaved RGB f32 Vec (r, g, b per pixel, same normalization)

use std::collections::HashMap;
use crate::context::KeywordEntry;
use rayon::prelude::*;

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

// Reflect a coordinate at the image bounds rather than clamping.
// Reflection (mirror around the border index) preserves the
// coordinate's parity, so the reflected pixel is always the same
// Bayer channel as the original position — clamping instead could
// land on the pixel's own (different-channel, never-populated)
// position at a border, corrupting G/R/B interpolation there (Issue
// 131). Only ever called with sx/sy one step outside the buffer
// (immediate neighbors), so a single reflection is sufficient.
// Free function (not a closure) so the three independent
// channel-interpolation passes below can each call it without
// duplicating it or fighting over capture.
fn reflect(s: i32, len: i32) -> i32 {
    if s < 0 { -s } else if s >= len { 2 * (len - 1) - s } else { s }
}

fn sample(buf: &[f32], width: usize, height: usize, sx: i32, sy: i32) -> f32 {
    let cx = reflect(sx, width  as i32) as usize;
    let cy = reflect(sy, height as i32) as usize;
    buf[cy * width + cx]
}

/// For a green pixel, true if the red samples lie on the horizontal
/// (same-row) axis and blue on the vertical axis; false if reversed.
///
/// Derived from the pattern rather than tabulated: the horizontal
/// neighbour's own channel is the answer directly. `x + 1` is safe at
/// either edge — `channel_at` only reads coordinate parity, never
/// indexes — and the CFA's period of 2 makes it equivalent to `x - 1`.
fn red_is_horizontal_at_green(pattern: BayerPattern, x: usize, y: usize) -> bool {
    pattern.channel_at(x + 1, y) == 0
}

/// Bilinear Bayer debayer.
///
/// `mono` — row-major f32 slice, normalized 0.0–1.0
/// Returns interleaved RGB f32 Vec: [r0, g0, b0, r1, g1, b1, ...]
///
/// Parallelized across rayon's shared pool (Issue 185 follow-up):
/// timing showed this function, not disk decode, dominates per-frame
/// reader-thread cost (~10x decode time on real OSC sessions). Each of
/// the three RGB channels is fully separable — computing R never reads
/// G or B data — so Steps 1 and 2 below run as three independent
/// per-channel passes, each parallelized over disjoint rows, rather
/// than the original single interleaved pass. Step 2 reads only from
/// Step 1's now-immutable buffers and writes into fresh `_interp`
/// buffers (seeded as clones of their Step-1 source) rather than
/// mutating in place, since row-parallel writes into a buffer also
/// being read by neighboring rows is unsound. Trades roughly 3x one
/// channel-buffer's worth of transient memory (proportional to frame
/// size) for the parallelism. Output is numerically identical to the
/// prior sequential version — same formulas, same neighbor sampling,
/// just reorganized by channel and row.
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

    // Step 1: copy known channel values into their respective buffers.
    // Three independent passes — each scans every pixel but only ever
    // writes its own channel (the other positions are no-op skips,
    // left at the zero-init default, same as the original single-pass
    // version's unmatched match arms).
    rayon::join(
        || r_buf.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
            for x in 0..width {
                if pattern.channel_at(x, y) == 0 {
                    row[x] = mono[y * width + x];
                }
            }
        }),
        || rayon::join(
            || g_buf.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
                for x in 0..width {
                    if pattern.channel_at(x, y) == 1 {
                        row[x] = mono[y * width + x];
                    }
                }
            }),
            || b_buf.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
                for x in 0..width {
                    if pattern.channel_at(x, y) == 2 {
                        row[x] = mono[y * width + x];
                    }
                }
            }),
        ),
    );

    // Step 2: interpolate missing values with bilinear averaging.
    // Each pass reads only its own channel's Step-1 buffer (the
    // original per-pixel formulas never mix channels for a single
    // output channel) and writes into a fresh buffer seeded as a clone
    // of that source, so already-genuine samples are already correct
    // without this pass needing to touch them.
    let mut r_interp = r_buf.clone();
    let mut g_interp = g_buf.clone();
    let mut b_interp = b_buf.clone();

    rayon::join(
        || r_interp.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
            for x in 0..width {
                let ch = pattern.channel_at(x, y);
                if ch == 0 { continue; } // already genuine — clone is correct
                let ix = x as i32;
                let iy = y as i32;
                row[x] = if ch == 2 {
                    // Blue pixel: R via diagonal average
                    (sample(&r_buf, width, height, ix-1, iy-1) +
                     sample(&r_buf, width, height, ix+1, iy-1) +
                     sample(&r_buf, width, height, ix-1, iy+1) +
                     sample(&r_buf, width, height, ix+1, iy+1)) * 0.25
                } else if red_is_horizontal_at_green(pattern, x, y) {
                    (sample(&r_buf, width, height, ix-1, iy) + sample(&r_buf, width, height, ix+1, iy)) * 0.5
                } else {
                    (sample(&r_buf, width, height, ix, iy-1) + sample(&r_buf, width, height, ix, iy+1)) * 0.5
                };
            }
        }),
        || rayon::join(
            || g_interp.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
                for x in 0..width {
                    let ch = pattern.channel_at(x, y);
                    if ch == 1 { continue; } // already genuine — clone is correct
                    let ix = x as i32;
                    let iy = y as i32;
                    // Both R and B pixels interpolate G the same way: cardinal average
                    row[x] = (sample(&g_buf, width, height, ix-1, iy) +
                              sample(&g_buf, width, height, ix+1, iy) +
                              sample(&g_buf, width, height, ix, iy-1) +
                              sample(&g_buf, width, height, ix, iy+1)) * 0.25;
                }
            }),
            || b_interp.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
                for x in 0..width {
                    let ch = pattern.channel_at(x, y);
                    if ch == 2 { continue; } // already genuine — clone is correct
                    let ix = x as i32;
                    let iy = y as i32;
                    row[x] = if ch == 0 {
                        // Red pixel: B via diagonal average
                        (sample(&b_buf, width, height, ix-1, iy-1) +
                         sample(&b_buf, width, height, ix+1, iy-1) +
                         sample(&b_buf, width, height, ix-1, iy+1) +
                         sample(&b_buf, width, height, ix+1, iy+1)) * 0.25
                    } else if red_is_horizontal_at_green(pattern, x, y) {
                        // Green pixel: B is on the axis opposite R
                        (sample(&b_buf, width, height, ix, iy-1) + sample(&b_buf, width, height, ix, iy+1)) * 0.5
                    } else {
                        (sample(&b_buf, width, height, ix-1, iy) + sample(&b_buf, width, height, ix+1, iy)) * 0.5
                    };
                }
            }),
        ),
    );

    // Step 3: interleave into RGB output, parallelized per pixel.
    let mut out = vec![0.0f32; n * 3];
    out.par_chunks_mut(3).enumerate().for_each(|(i, px)| {
        px[0] = r_interp[i].clamp(0.0, 1.0);
        px[1] = g_interp[i].clamp(0.0, 1.0);
        px[2] = b_interp[i].clamp(0.0, 1.0);
    });
    out
}

/// Bayer CFA → luminance in one pass, without materializing RGB.
///
/// Equivalent to `extract_luminance(&debayer_bilinear(mono, ..), .., 3)`,
/// but computes each pixel's R, G, B as three scalars from a 3x3 window
/// on `mono` and combines them immediately. None of `debayer_bilinear`'s
/// six intermediate full-frame buffers (three channel buffers, three
/// interpolation clones) nor its interleaved RGB output are allocated,
/// and `extract_luminance`'s separate full-frame pass disappears. For a
/// 9 MPx frame this takes the path from ~11 channel-buffers' worth of
/// allocation to one, and the working set from seven full frames to a
/// three-row sliding window.
///
/// Valid because `debayer_bilinear`'s interpolation step only ever
/// samples *genuine* CFA positions of the channel being interpolated,
/// and `reflect` preserves coordinate parity so border reflections land
/// on the same channel. At every position it reads, the channel buffer
/// still holds exactly `mono`'s value. Sample order, addition order, the
/// per-channel clamp, and the Rec.601 weights are all preserved, so
/// output is bit-identical to the two-call sequence.
///
/// Used by `LoadKind::Luma`. `debayer_bilinear` remains the path for
/// callers that genuinely need RGB (`LoadKind::ColorNormalized`,
/// `DebayerImage`).
pub fn debayer_to_luma(
    mono:    &[f32],
    width:   usize,
    height:  usize,
    pattern: BayerPattern,
) -> Vec<f32> {
    let mut luma = vec![0.0f32; width * height];

    luma.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        for x in 0..width {
            let ix = x as i32;
            let iy = y as i32;

            let cardinal = || {
                (sample(mono, width, height, ix-1, iy) +
                 sample(mono, width, height, ix+1, iy) +
                 sample(mono, width, height, ix, iy-1) +
                 sample(mono, width, height, ix, iy+1)) * 0.25
            };
            let diagonal = || {
                (sample(mono, width, height, ix-1, iy-1) +
                 sample(mono, width, height, ix+1, iy-1) +
                 sample(mono, width, height, ix-1, iy+1) +
                 sample(mono, width, height, ix+1, iy+1)) * 0.25
            };
            let horizontal = || {
                (sample(mono, width, height, ix-1, iy) +
                 sample(mono, width, height, ix+1, iy)) * 0.5
            };
            let vertical = || {
                (sample(mono, width, height, ix, iy-1) +
                 sample(mono, width, height, ix, iy+1)) * 0.5
            };

            let own = mono[y * width + x];

            let (r, g, b) = match pattern.channel_at(x, y) {
                0 => (own, cardinal(), diagonal()),
                2 => (diagonal(), cardinal(), own),
                _ => {
                    if red_is_horizontal_at_green(pattern, x, y) {
                        (horizontal(), own, vertical())
                    } else {
                        (vertical(), own, horizontal())
                    }
                }
            };

            row[x] = 0.299 * r.clamp(0.0, 1.0)
                   + 0.587 * g.clamp(0.0, 1.0)
                   + 0.114 * b.clamp(0.0, 1.0);
        }
    });

    luma
}

// ── Tests ─────────────────────────────────────────────────────────────────────

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

    fn pseudo_random_image(n: usize, lo: f32, hi: f32) -> Vec<f32> {
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let unit = ((state >> 40) as f32) / (u32::pow(2, 24) as f32);
                lo + unit * (hi - lo)
            })
            .collect()
    }

    fn assert_luma_paths_match(width: usize, height: usize, lo: f32, hi: f32) {
        let mono = pseudo_random_image(width * height, lo, hi);
        for pattern in [
            BayerPattern::RGGB,
            BayerPattern::BGGR,
            BayerPattern::GRBG,
            BayerPattern::GBRG,
        ] {
            let rgb      = debayer_bilinear(&mono, width, height, pattern);
            let expected = crate::analysis::extract_luminance(&rgb, width, height, 3);
            let actual   = debayer_to_luma(&mono, width, height, pattern);

            assert_eq!(actual.len(), expected.len(), "{:?}: length mismatch", pattern);
            for i in 0..expected.len() {
                assert_eq!(
                    actual[i], expected[i],
                    "{:?}: mismatch at index {} (x={}, y={}): got {}, expected {}",
                    pattern, i, i % width, i / width, actual[i], expected[i]
                );
            }
        }
    }

    /// debayer_to_luma must be bit-identical to
    /// extract_luminance(debayer_bilinear(..), .., 3) — it exists purely
    /// to avoid materializing RGB, not to approximate it. Odd, unequal
    /// dimensions exercise border reflection on all four edges and both
    /// coordinate parities.
    #[test]
    fn test_debayer_to_luma_matches_rgb_path() {
        assert_luma_paths_match(37, 23, 0.0, 1.0);
    }

    /// The per-channel clamp happens before the Rec.601 combine in both
    /// paths. F32 source frames bypass normalization entirely
    /// (to_f32_normalized clones them), so out-of-range input is
    /// reachable in production and the two paths must still agree.
    #[test]
    fn test_debayer_to_luma_matches_rgb_path_out_of_range() {
        assert_luma_paths_match(23, 37, -0.5, 1.5);
    }
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
