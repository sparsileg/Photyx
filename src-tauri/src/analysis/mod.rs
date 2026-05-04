// analysis/mod.rs — shared types and configuration for the analysis layer
// Spec §15 (AnalyzeFrames), §7 (pcode analysis commands)

pub mod background;
pub mod eccentricity;
pub mod fwhm;
pub mod metrics;
pub mod profiles;
pub mod session_stats;
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

    // Signal metrics — SNR is retained as diagnostic information only.
    // It does NOT drive PXFLAG rejection. See session_stats::classify_frame.
    pub snr_estimate:         Option<f32>,

    // Star quality metrics (stars.rs)
    pub fwhm:                 Option<f32>,   // mean FWHM in pixels
    pub eccentricity:         Option<f32>,   // mean eccentricity (0 = circular, 1 = line)
    pub star_count:           Option<u32>,   // number of detected stars

    // Final classification (set by AnalyzeFrames / get_analysis_results)
    pub flag:                 Option<PxFlag>,
    pub triggered_by:         Vec<String>,

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
