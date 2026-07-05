// analysis/metrics.rs — highlight clipping
// Spec §11.2

// ── Highlight clipping ────────────────────────────────────────────────────────

/// Fixed clipping threshold per spec §8.8: pixels at or above this value
/// are considered highlight-clipped.
#[allow(dead_code)]
pub const CLIP_THRESHOLD: f32 = 0.995;

/// Compute highlight clipping fraction for a luminance image.
/// Returns the fraction of pixels at or above CLIP_THRESHOLD (0.0–1.0).
#[allow(dead_code)]
pub fn highlight_clipping(luma: &[f32]) -> f32 {
    if luma.is_empty() {
        return 0.0;
    }
    let clipped = luma.iter().filter(|&&v| v >= CLIP_THRESHOLD).count();
    clipped as f32 / luma.len() as f32
}


// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipping_none() {
        let luma = vec![0.5f32; 1000];
        assert_eq!(highlight_clipping(&luma), 0.0);
    }

    #[test]
    fn test_clipping_all() {
        let luma = vec![1.0f32; 1000];
        assert_eq!(highlight_clipping(&luma), 1.0);
    }

    #[test]
    fn test_clipping_half() {
        let mut luma = vec![0.5f32; 500];
        luma.extend(vec![1.0f32; 500]);
        let clip = highlight_clipping(&luma);
        assert!((clip - 0.5).abs() < 0.001, "clipping {} should be 0.5", clip);
    }

    #[test]
    fn test_clipping_threshold() {
        let luma = vec![CLIP_THRESHOLD; 100];
        assert_eq!(highlight_clipping(&luma), 1.0);
        let luma2 = vec![CLIP_THRESHOLD - 0.001; 100];
        assert_eq!(highlight_clipping(&luma2), 0.0);
    }

    #[test]
    fn test_clipping_empty() {
        assert_eq!(highlight_clipping(&[]), 0.0);
    }
}
