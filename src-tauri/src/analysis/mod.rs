// analysis/mod.rs — shared types and configuration for the analysis layer
// Spec §15 (AnalyzeFrames), §7 (pcode analysis commands)

pub mod background;
pub mod debayer;
pub mod eccentricity;
pub mod fft_align;
pub mod fwhm;
pub mod metrics;
pub mod moffat;
pub mod profiles;
pub mod session_stats;
pub mod stack_metrics;
pub mod star_align;
pub mod stars;

use serde::{Deserialize, Serialize};

// ── PXFLAG classification ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PxFlag {
    Pass,
    Reject,
}

impl PxFlag {
    pub fn as_str(&self) -> &str {
        match self {
            PxFlag::Pass   => "PASS",
            PxFlag::Reject => "REJECT",
        }
    }
}

impl std::fmt::Display for PxFlag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── Per-frame analysis result ─────────────────────────────────────────────────
// Fields are Option because results accumulate incrementally across plugin runs.
// AnalyzeFrames populates all fields in one pass.

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub filename:             String,

    // Background metrics (background.rs)
    pub background_median:    Option<f32>,

    // Star quality metrics (stars.rs)
    pub fwhm:                 Option<f32>,   // mean FWHM in pixels
    pub eccentricity:         Option<f32>,   // mean eccentricity (0 = circular, 1 = line)
    pub star_count:           Option<u32>,   // number of detected stars

    // Final classification (set by AnalyzeFrames / get_analysis_results)
    pub flag:                 Option<PxFlag>,
    pub triggered_by:         Vec<String>,

    /// True for the single frame selected as the session reference frame.
    /// Selected by highest frame_quality_score() among PASS frames
    /// (falling back to all frames if none passed); star_count used as
    /// tiebreaker. Authoritative on import.
    pub is_reference:         bool,

    // Rejection category — only populated for REJECT frames.
    // One of: "O", "T", "B", "OT", "OB", "BT", "OBT"
    // O = Optical quality (FWHM/Eccentricity)
    // T = Transparency (star count, background unchanged)
    // B = Sky brightness (background median elevated)
    // Ordering: O first (least recoverable), B before T when both present.
    pub rejection_category:   Option<String>,
}

impl AnalysisResult {
    pub fn new(filename: &str) -> Self {
        Self {
            filename: filename.to_string(),
            ..Default::default()
        }
    }
}

// ── Frame quality score ─────────────────────────────────────────────────────
// Shared by AnalyzeFrames' reference-frame selection and StackFrames'
// group/master reference selection (Issue 95) — one definition of "best
// frame" for both, instead of two formulas that could silently disagree.
// Bounded and well-behaved as eccentricity → 0, unlike a raw
// fwhm × eccentricity product (which degenerates toward zero and lets a
// bloated-but-round frame beat a sharp, moderately elongated one).

/// Higher is better. `1/FWHM` rewards sharpness (clamped to avoid
/// division blowup on a near-zero FWHM); `(1 - eccentricity)` rewards
/// roundness as a bounded penalty rather than a multiplicative gate.
/// Missing values contribute 0 to their term rather than disqualifying
/// the frame outright.
pub fn frame_quality_score(fwhm: Option<f32>, eccentricity: Option<f32>) -> f32 {
    let fwhm_score = fwhm.map(|f| 1.0 / f.max(0.1)).unwrap_or(0.0);
    let ecc_score  = eccentricity.map(|e| 1.0 - e).unwrap_or(0.0);
    fwhm_score + ecc_score
}

#[cfg(test)]
mod quality_score_tests {
    use super::*;

    #[test]
    fn sharp_moderate_eccentricity_beats_bloated_round() {
        // The exact degenerate case from Issue 95: a bloated but round
        // star field used to beat a sharp, slightly elongated one under
        // the old fwhm * eccentricity formula (0.12 vs 0.66 — backwards).
        let bloated_round = frame_quality_score(Some(6.0), Some(0.02));
        let sharp_moderate = frame_quality_score(Some(2.2), Some(0.30));
        assert!(
            sharp_moderate > bloated_round,
            "sharp moderate-eccentricity frame ({sharp_moderate}) should beat \
             bloated round frame ({bloated_round})"
        );
    }

    #[test]
    fn missing_values_score_zero_for_that_term_not_disqualified() {
        let fwhm_only = frame_quality_score(Some(3.0), None);
        let neither    = frame_quality_score(None, None);
        assert!(fwhm_only > neither);
        assert_eq!(neither, 0.0);
    }
}

// ── Sigma-clipping configuration ──────────────────────────────────────────────
// PixInsight defaults: 3.0σ, 5 iterations.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigmaClipConfig {
    pub sigma:       f32,
    pub iterations:  u32,
}

impl Default for SigmaClipConfig {
    fn default() -> Self {
        Self { sigma: 3.0, iterations: 5 }
    }
}

// ── Star detection configuration ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarDetectionConfig {
    pub detection_threshold:  f32,
    pub peak_radius:          u32,
    pub flood_threshold:      f32,
    pub saturation_threshold: f32,
    pub sigma_clip:           SigmaClipConfig,
}

impl Default for StarDetectionConfig {
    fn default() -> Self {
        Self {
            detection_threshold:  5.0,
            peak_radius:          3,
            flood_threshold:      2.0,
            saturation_threshold: 0.98,
            sigma_clip:           SigmaClipConfig::default(),
        }
    }
}

// ── Background estimation configuration ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundConfig {
    pub sigma_clip:         SigmaClipConfig,
    pub gradient_grid_size: u32,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            sigma_clip:         SigmaClipConfig::default(),
            gradient_grid_size: 8,
        }
    }
}

// ── Luminance extraction ──────────────────────────────────────────────────────

pub fn extract_luminance(
    pixels:   &[f32],
    width:    usize,
    height:   usize,
    channels: usize,
) -> Vec<f32> {
    let n = width * height;
    match channels {
        1 => pixels[..n].to_vec(),
        3 => {
            let mut luma = Vec::with_capacity(n);
            for i in 0..n {
                let r = pixels[i * 3];
                let g = pixels[i * 3 + 1];
                let b = pixels[i * 3 + 2];
                luma.push(0.299 * r + 0.587 * g + 0.114 * b);
            }
            luma
        }
        _ => (0..n).map(|i| pixels[i * channels]).collect(),
    }
}

// ── Pixel normalization ───────────────────────────────────────────────────────

use crate::context::PixelData;

pub fn to_f32_normalized(pixels: &PixelData) -> Vec<f32> {
    match pixels {
        PixelData::U8(v)  => v.iter().map(|&x| x as f32 / 255.0).collect(),
        PixelData::U16(v) => v.iter().map(|&x| x as f32 / 65535.0).collect(),
        PixelData::F32(v) => v.clone(),
    }
}

/// Combined normalize + luminance extraction in a single pass.
pub fn to_luminance(pixels: &PixelData, channels: usize) -> Vec<f32> {
    match (pixels, channels) {
        (PixelData::U8(v), 1)  => v.iter().map(|&x| x as f32 / 255.0).collect(),
        (PixelData::U16(v), 1) => v.iter().map(|&x| x as f32 / 65535.0).collect(),
        (PixelData::F32(v), 1) => v.clone(),
        (PixelData::U8(v), 3) => {
            let n = v.len() / 3;
            let mut luma = Vec::with_capacity(n);
            for i in 0..n {
                let r = v[i * 3]     as f32 / 255.0;
                let g = v[i * 3 + 1] as f32 / 255.0;
                let b = v[i * 3 + 2] as f32 / 255.0;
                luma.push(0.299 * r + 0.587 * g + 0.114 * b);
            }
            luma
        }
        (PixelData::U16(v), 3) => {
            let n = v.len() / 3;
            let mut luma = Vec::with_capacity(n);
            for i in 0..n {
                let r = v[i * 3]     as f32 / 65535.0;
                let g = v[i * 3 + 1] as f32 / 65535.0;
                let b = v[i * 3 + 2] as f32 / 65535.0;
                luma.push(0.299 * r + 0.587 * g + 0.114 * b);
            }
            luma
        }
        (PixelData::F32(v), 3) => {
            let n = v.len() / 3;
            let mut luma = Vec::with_capacity(n);
            for i in 0..n {
                let r = v[i * 3];
                let g = v[i * 3 + 1];
                let b = v[i * 3 + 2];
                luma.push(0.299 * r + 0.587 * g + 0.114 * b);
            }
            luma
        }
        (PixelData::U8(v), _)  => v.iter().step_by(channels).map(|&x| x as f32 / 255.0).collect(),
        (PixelData::U16(v), _) => v.iter().step_by(channels).map(|&x| x as f32 / 65535.0).collect(),
        (PixelData::F32(v), _) => v.iter().step_by(channels).cloned().collect(),
    }
}


// ----------------------------------------------------------------------
