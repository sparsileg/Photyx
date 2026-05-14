// analysis/stack_metrics.rs — per-frame contribution metrics for StackFrames
// Spec §3.8 (stacking document)

use serde::{Deserialize, Serialize};

// ── Exclusion reason ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExclusionReason {
    FilterMismatch,
    AlignmentFailed,
}

impl ExclusionReason {
    pub fn as_str(&self) -> &str {
        match self {
            ExclusionReason::FilterMismatch  => "filter_mismatch",
            ExclusionReason::AlignmentFailed => "alignment_failed",
        }
    }
}

impl std::fmt::Display for ExclusionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── Per-frame contribution record ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameContribution {
    /// Zero-based index in the session file list
    pub frame_index: usize,

    /// Full source file path
    pub filename: String,

    /// FILTER keyword value from the frame header, if present
    pub filter: Option<String>,

    /// Whether this frame was included in the final stack
    pub included: bool,

    /// Why the frame was excluded, if applicable
    pub exclusion_reason: Option<ExclusionReason>,

    /// Sigma-clipped background level before normalization (0.0–1.0)
    pub background_level: Option<f32>,

    /// FFT translation computed vs. reference frame (pixels, sub-pixel)
    pub fft_translation: Option<(f32, f32)>,

    /// Whether the star position validation check passed
    pub alignment_validated: Option<bool>,

    /// Per-frame FWHM — from cached AnalysisResult or recomputed
    pub fwhm: Option<f32>,

    /// Per-frame eccentricity — from cached AnalysisResult or recomputed
    pub eccentricity: Option<f32>,

    /// Whether a 180° meridian flip was detected and corrected for this frame
    pub meridian_flipped: bool,
}

impl FrameContribution {
    pub fn new(frame_index: usize, filename: &str) -> Self {
        Self {
            frame_index,
            filename: filename.to_string(),
            filter: None,
            included: false,
            exclusion_reason: None,
            background_level: None,
            fft_translation: None,
            alignment_validated: None,
            fwhm: None,
            eccentricity: None,
            meridian_flipped: false,
        }
    }
}

// ── Stack run summary ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StackSummary {
    /// Total frames in the session at stack time
    pub total_frames: usize,

    /// Frames excluded due to filter mismatch
    pub filter_excluded: usize,

    /// Frames excluded due to alignment failure
    pub alignment_excluded: usize,

    /// Frames successfully stacked
    pub stacked_frames: usize,

    /// Theoretical SNR improvement vs single frame (sqrt of stacked_frames)
    pub snr_improvement: f32,

    /// Alignment success rate as a fraction (0.0–1.0)
    pub alignment_success_rate: f32,

    /// Background uniformity label derived from variance of per-frame background levels
    pub background_uniformity: BackgroundUniformity,

    /// UTC timestamp when the stack completed (ISO 8601)
    pub completed_at: String,

    /// OBJECT keyword from the reference frame, if present
    pub target: Option<String>,

    /// FILTER keyword from the reference frame, if present
    pub filter: Option<String>,

    /// Sum of EXPTIME keyword values across stacked frames, in seconds
    pub integration_seconds: f32,
}

impl StackSummary {
    pub fn compute(contributions: &[FrameContribution], completed_at: &str) -> Self {
        let total_frames      = contributions.len();
        let filter_excluded   = contributions.iter()
            .filter(|c| c.exclusion_reason == Some(ExclusionReason::FilterMismatch))
            .count();
        let alignment_excluded = contributions.iter()
            .filter(|c| c.exclusion_reason == Some(ExclusionReason::AlignmentFailed))
            .count();
        let stacked_frames = contributions.iter().filter(|c| c.included).count();

        let snr_improvement = (stacked_frames as f32).sqrt();

        let attempted = total_frames - filter_excluded;
        let alignment_success_rate = if attempted > 0 {
            (stacked_frames as f32) / (attempted as f32)
        } else {
            0.0
        };

        let bg_levels: Vec<f32> = contributions.iter()
            .filter(|c| c.included)
            .filter_map(|c| c.background_level)
            .collect();
        let background_uniformity = BackgroundUniformity::from_levels(&bg_levels);

        Self {
            total_frames,
            filter_excluded,
            alignment_excluded,
            stacked_frames,
            snr_improvement,
            alignment_success_rate,
            background_uniformity,
            completed_at: completed_at.to_string(),
            target: None,
            filter: None,
            integration_seconds: 0.0,
        }
    }
}

// ── Background uniformity classification ──────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum BackgroundUniformity {
    #[default]
    Good,
    Moderate,
    Poor,
}

impl BackgroundUniformity {
    /// Classify based on coefficient of variation of background levels.
    /// CV = stddev / mean. Thresholds are empirically chosen.
    pub fn from_levels(levels: &[f32]) -> Self {
        if levels.len() < 2 {
            return Self::Good;
        }
        let n   = levels.len() as f32;
        let mean = levels.iter().sum::<f32>() / n;
        if mean == 0.0 {
            return Self::Good;
        }
        let variance = levels.iter().map(|&x| (x - mean) * (x - mean)).sum::<f32>() / n;
        let cv = variance.sqrt() / mean;

        if cv < 0.05 {
            Self::Good
        } else if cv < 0.15 {
            Self::Moderate
        } else {
            Self::Poor
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Good     => "good",
            Self::Moderate => "moderate",
            Self::Poor     => "poor",
        }
    }
}

impl std::fmt::Display for BackgroundUniformity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
