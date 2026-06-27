// plugins/stack_frames.rs   StackFrames built-in plugin
//
// Two-pass stacking with FFT phase correlation + triangle-based rigid
// alignment. Designed to handle meridian-flipped sessions cleanly.
//
// Architecture:
//
//   1. Per-frame debayer-first pipeline. Each frame is debayered (if Bayer)
//      to RGB, then luminance is extracted from RGB. Eliminates the Bayer
//      pattern mismatch that arises when reverse()-ing a raw Bayer buffer.
//
//   2. Frames are grouped by ROTATOR keyword. Within a session containing a
//      meridian flip there are two groups (pre-flip, post-flip). For sessions
//      without a flip there is a single group. A rotator change > 90° always
//      triggers a new group (catches same-night meridian flips regardless of
//      time gap).
//
//   3. The larger group is designated the "master group". Its best-quality
//      frame becomes the master reference for the entire stack output.
//
//   4. Each group has its own group reference (best-quality frame within
//      that group). Frames within a group align natively to their group
//      reference — no reverse() per frame, no Bayer pattern issues.
//
//   5. For each non-master group, a cross-group transform M_cross is solved
//      ONCE that maps that group's reference into master coordinates. This
//      uses an explicit 180° pre-rotation (AffineRigid::flip_180) composed
//      with the triangle match result.
//
//   6. Per-frame final transform: T = M_cross · G
//      where G is the within-group transform (triangle match against group ref).
//      For master-group frames, M_cross = identity, so T = G.
//
//   7. Color awareness: if the master reference frame is Bayer or RGB, the
//      stack accumulates all three RGB channels and outputs ColorSpace::RGB.
//      Mono input produces a grayscale output as before.
//
//   8. Calibration: if caldir= is provided, StackFrames loads master bias,
//      dark, and flat from that directory (identified by filename). Calibration
//      is applied to raw [0,1] pixel values BEFORE background normalization:
//      bias subtract → dark subtract → flat divide.
//      Background normalization (divisor) is then applied to the calibrated
//      pixels for accumulation. This ensures flat division operates on the
//      correct scale. For flat stacking, the flat master is never loaded from
//      caldir since applying a flat to flat subs makes no sense.
//
//   9. Flat stacking: raw Bayer (or mono) data is used directly without
//      debayering. Each sub is bias/dark-calibrated (respecting
//      APPLY_BIAS_IN_CALIBRATION), normalized by its scalar mean, then
//      Winsorized sigma-clipped and mean-combined. Output is always a
//      1-channel grayscale F32 master normalized to center around 1.0.

use crate::settings::defaults::{
    APPLY_BIAS_IN_CALIBRATION,
    FLAT_STACK_WINSORIZE_ITERATIONS,
    FLAT_STACK_WINSORIZE_SIGMA,
};
use crate::analysis::{
    self,
    background::estimate_background,
    debayer::{debayer_bilinear, BayerPattern},
    eccentricity::compute_eccentricity,
    fft_align::compute_translation,
    fwhm::compute_fwhm,
    star_align::{compose, estimate_rigid_transform, estimate_rigid_transform_triangles, AffineRigid},
    stars::detect_stars,
    stack_metrics::{ExclusionReason, FrameContribution, StackSummary},
    SigmaClipConfig, StarDetectionConfig,
};
use crate::context::{AppContext, BitDepth, ColorSpace, ImageBuffer, PixelData};
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use crate::plugins::image_reader::read_image_file;
use chrono::Utc;
use rayon::prelude::*;
use tracing::info;

/// Rotation magnitude (radians) below which we skip the affine resampler.
const MIN_ROTATION_TO_APPLY: f32 = 0.001;

/// ROTATOR delta tolerance (degrees) for grouping frames across sessions.
const ROTATOR_GROUP_TOLERANCE: f32 = 10.0;

/// Gap in minutes between consecutive frames that indicates a new imaging session.
const SESSION_GAP_MINUTES: f32 = 120.0;

/// Rotator change above this threshold always triggers a new group (meridian flip).
const MERIDIAN_FLIP_THRESHOLD: f32 = 90.0;

pub struct StackFrames;

//  ── Frame type detection ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum FrameType {
    Light,
    Flat,
}

/// Detect whether a frame is a light or flat sub.
/// Checks filename first (case-insensitive contains "flat"), then IMAGETYP keyword.
fn detect_frame_type(
    path:     &str,
    keywords: &std::collections::HashMap<String, crate::context::KeywordEntry>,
) -> FrameType {
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    if filename.contains("flat") {
        return FrameType::Flat;
    }

    if let Some(kw) = keywords.get("IMAGETYP") {
        if kw.value.to_lowercase().contains("flat") {
            return FrameType::Flat;
        }
    }

    FrameType::Light
}

//  ── Calibration masters ───────────────────────────────────────────────────────

struct CalibrationMasters {
    bias:          Option<Vec<f32>>,  // normalized f32 [0,1], interleaved channels
    bias_channels: usize,
    dark:          Option<Vec<f32>>,  // normalized f32 [0,1], interleaved channels
    dark_channels: usize,
    flat:          Option<Vec<f32>>,  // normalized f32 centered ~1.0, interleaved channels
    flat_channels: usize,
}

/// Scan caldir for master bias, dark, and flat files.
/// Identified by filename containing "bias", "dark", or "flat" (case-insensitive).
/// Supports .fit, .fits, .fts, .xisf extensions.
/// When load_flat=false, any flat master in caldir is ignored (used when stacking flats).
fn load_calibration_masters(caldir: &str, load_flat: bool, messages: &mut Vec<String>) -> CalibrationMasters {
    let mut bias_buf: Option<Vec<f32>> = None;
    let mut dark_buf: Option<Vec<f32>> = None;
    let mut flat_buf: Option<Vec<f32>> = None;
    let mut bias_channels = 1usize;
    let mut dark_channels = 1usize;
    let mut flat_channels = 1usize;

    let entries = match std::fs::read_dir(caldir) {
        Ok(e) => e,
        Err(e) => {
            messages.push(format!(
                "Warning: cannot read calibration directory '{}': {}", caldir, e
            ));
            return CalibrationMasters {
                bias: None, bias_channels: 1,
                dark: None, dark_channels: 1,
                flat: None, flat_channels: 1,
            };
        }
    };

    let mut files: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            matches!(ext.as_str(), "fit" | "fits" | "fts" | "xisf")
        })
        .collect();
    files.sort();

    for path in &files {
        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        let cal_type = if filename.contains("bias") {
            "bias"
        } else if filename.contains("dark") {
            "dark"
        } else if filename.contains("flat") {
            if !load_flat { continue; }
            "flat"
        } else {
            continue;
        };

        let path_str = match path.to_str() {
            Some(s) => s,
            None    => continue,
        };

        let buf = match read_image_file(path_str) {
            Ok(b)  => b,
            Err(e) => {
                messages.push(format!(
                    "Warning: cannot load {} master '{}': {}", cal_type, path_str, e
                ));
                continue;
            }
        };

        let pixels = match buf.pixels {
            Some(p) => p,
            None    => {
                messages.push(format!(
                    "Warning: {} master '{}' has no pixel data", cal_type, path_str
                ));
                continue;
            }
        };

        let c = buf.channels as usize;
        let normalized = to_f32_normalized_cal(&pixels);

        messages.push(format!(
            "Calibration: loaded {} master '{}' ({}×{} {} ch)",
            cal_type, filename, buf.width, buf.height, c
        ));

        match cal_type {
            "bias" => { bias_buf = Some(normalized); bias_channels = c; }
            "dark" => { dark_buf = Some(normalized); dark_channels = c; }
            "flat" => { flat_buf = Some(normalized); flat_channels = c; }
            _      => {}
        }
    }

    // Normalize flat master by a single global mean so channel ratios are
    // preserved. Values center around 1.0 for correct calibration use.
    if let Some(ref mut flat) = flat_buf {
        let global_mean: f32 = flat.iter().sum::<f32>() / flat.len() as f32;
        if global_mean > 1e-6 {
            for v in flat.iter_mut() {
                *v /= global_mean;
            }
        }
    }

    CalibrationMasters {
        bias: bias_buf, bias_channels,
        dark: dark_buf, dark_channels,
        flat: flat_buf, flat_channels,
    }
}

/// Convert PixelData to normalized f32 for calibration masters.
fn to_f32_normalized_cal(pixels: &PixelData) -> Vec<f32> {
    match pixels {
        PixelData::U8(v)  => v.iter().map(|&x| x as f32 / 255.0).collect(),
        PixelData::U16(v) => v.iter().map(|&x| x as f32 / 65535.0).collect(),
        PixelData::F32(v) => v.clone(),
    }
}

/// Normalize a flat master in place so each channel's mean is 1.0.
/// Retained for potential future use.
#[allow(dead_code)]
fn normalize_flat_inplace(flat: &mut Vec<f32>, channels: usize) {
    let n_pixels = flat.len() / channels.max(1);
    if n_pixels == 0 { return; }
    for ch in 0..channels {
        let mean: f32 = (0..n_pixels)
            .map(|px| flat[px * channels + ch])
            .sum::<f32>() / n_pixels as f32;
        if mean > 1e-6 {
            for px in 0..n_pixels {
                flat[px * channels + ch] /= mean;
            }
        }
    }
}

/// Apply calibration to a raw [0,1] normalized f32 frame buffer in place.
/// Order: bias subtract → dark subtract → flat divide.
/// Must be called on raw pixel values BEFORE background normalization.
/// Each calibration master uses its own channel count to avoid indexing
/// errors when masters have different channel counts (e.g. mono bias/dark
/// applied to a color frame). Values are floored at 0.0 after bias/dark;
/// no upper clamp is applied since flat division may produce values > 1.0
/// in bright regions.
/// Bias subtraction is skipped when APPLY_BIAS_IN_CALIBRATION is false
/// (e.g. when dark masters are already bias-subtracted in PixInsight).
fn apply_calibration(frame: &mut Vec<f32>, cal: &CalibrationMasters, channels: usize) {
    let n_pixels = frame.len() / channels.max(1);

    // Diagnostic: log corner and center pixels before calibration
    if n_pixels > 0 {
        let center_px = n_pixels / 2;
        let before0: Vec<f32> = (0..channels).map(|ch| frame[ch]).collect();
        let before_c: Vec<f32> = (0..channels).map(|ch| frame[center_px * channels + ch]).collect();
        let flat_px0: Vec<f32> = if let Some(ref flat) = cal.flat {
            (0..channels).map(|ch| {
                let flat_ch  = ch.min(cal.flat_channels.saturating_sub(1));
                let flat_idx = flat_ch;
                if flat_idx < flat.len() { flat[flat_idx] } else { 1.0 }
            }).collect()
        } else { vec![1.0; channels] };
        let flat_pxc: Vec<f32> = if let Some(ref flat) = cal.flat {
            (0..channels).map(|ch| {
                let flat_ch  = ch.min(cal.flat_channels.saturating_sub(1));
                let flat_idx = center_px * cal.flat_channels + flat_ch;
                if flat_idx < flat.len() { flat[flat_idx] } else { 1.0 }
            }).collect()
        } else { vec![1.0; channels] };
        info!("apply_calibration: px0 before={:?} flat={:?} | center before={:?} flat={:?}",
            before0, flat_px0, before_c, flat_pxc);
    }

    for px in 0..n_pixels {
        for ch in 0..channels {
            let idx = px * channels + ch;
            let mut val = frame[idx];

            if APPLY_BIAS_IN_CALIBRATION {
                if let Some(ref bias) = cal.bias {
                    let bias_ch  = ch.min(cal.bias_channels.saturating_sub(1));
                    let bias_idx = px * cal.bias_channels + bias_ch;
                    if bias_idx < bias.len() { val -= bias[bias_idx]; }
                }
            }
            if let Some(ref dark) = cal.dark {
                let dark_ch  = ch.min(cal.dark_channels.saturating_sub(1));
                let dark_idx = px * cal.dark_channels + dark_ch;
                if dark_idx < dark.len() { val -= dark[dark_idx]; }
            }

            val = val.max(0.0);

            if let Some(ref flat) = cal.flat {
                let flat_ch  = ch.min(cal.flat_channels.saturating_sub(1));
                let flat_idx = px * cal.flat_channels + flat_ch;
                if flat_idx < flat.len() {
                    let f = flat[flat_idx];
                    if f > 1e-6 { val /= f; }
                }
            }

            frame[idx] = val.max(0.0);
        }
    }

    // Diagnostic: log corner and center pixels after calibration
    if n_pixels > 0 {
        let center_px = n_pixels / 2;
        let after0: Vec<f32> = (0..channels).map(|ch| frame[ch]).collect();
        let after_c: Vec<f32> = (0..channels).map(|ch| frame[center_px * channels + ch]).collect();
        info!("apply_calibration: px0 after={:?} | center after={:?}", after0, after_c);
    }
}

//  ── Flat stacking ─────────────────────────────────────────────────────────────

/// Stack flat frames to produce a 1-channel grayscale master flat.
///
/// Pipeline per sub:
///   1. Decode to normalized f32. No debayering — raw Bayer (or mono) data is
///      used directly as single-channel. This avoids interpolation artifacts
///      and produces a physically accurate illumination map.
///   2. Subtract bias master if APPLY_BIAS_IN_CALIBRATION is true and bias
///      master is present. Respects the same flag as light calibration to
///      prevent double-subtraction when darks are already bias-subtracted.
///   3. Subtract dark master if present.
///   4. Normalize each sub by its scalar mean so all subs contribute equally
///      regardless of exposure variation (multiplicative normalization,
///      equalize fluxes — equivalent to PI's approach).
///
/// Combining:
///   - Winsorized sigma clipping (FLAT_STACK_WINSORIZE_SIGMA,
///     FLAT_STACK_WINSORIZE_ITERATIONS) replaces outlier samples at each pixel
///     with the clipping boundary value rather than excluding them. This
///     handles hot pixels, dust motes, and any cosmic rays without discarding
///     samples, preserving the full frame count for the mean.
///   - Mean-combine surviving (Winsorized) samples per pixel.
///
/// Output: 1-channel F32 ImageBuffer normalized by global mean (~1.0 center)
/// for use as a calibration master in apply_calibration().
fn stack_flat_frames(
    ctx:      &AppContext,
    messages: &mut Vec<String>,
    cal:      &CalibrationMasters,
) -> Result<ImageBuffer, PluginError> {
    let first_path = ctx.file_list.iter()
        .find(|p| ctx.image_buffers.get(*p).and_then(|b| b.pixels.as_ref()).is_some())
        .ok_or_else(|| PluginError::new("NO_PIXELS", "No flat frames with pixel data."))?
        .clone();

    let first_buf = ctx.image_buffers.get(&first_path).unwrap();
    let width     = first_buf.width  as usize;
    let height    = first_buf.height as usize;
    let n_pixels  = width * height;
    let total     = ctx.file_list.len();

    messages.push(format!("FlatStack: {} frames, {}×{} → 1-channel grayscale master",
        total, width, height));

    // Collect normalized single-channel subs
    let mut flat_planes: Vec<Vec<f32>> = Vec::with_capacity(total);

    for (i, path) in ctx.file_list.iter().enumerate() {
        let buf = match ctx.image_buffers.get(path) {
            Some(b) => b,
            None    => { messages.push(format!("FlatStack: frame {} skipped — no buffer", i + 1)); continue; }
        };
        let pixels = match &buf.pixels {
            Some(p) => p,
            None    => { messages.push(format!("FlatStack: frame {} skipped — no pixels", i + 1)); continue; }
        };

        // Decode to normalized f32 as single channel — no debayering.
        // Raw Bayer data is treated as a grayscale illumination map.
        let mut mono = analysis::to_f32_normalized(pixels);

        // If the buffer is already multi-channel RGB (pre-debayered), collapse
        // to single channel by taking the mean across channels.
        let channels = buf.channels as usize;
        let mut raw: Vec<f32> = if channels > 1 {
            (0..n_pixels)
                .map(|px| {
                    (0..channels).map(|ch| mono[px * channels + ch]).sum::<f32>()
                        / channels as f32
                })
                .collect()
        } else {
            mono.drain(..).collect()
        };

        // Bias subtract (only if APPLY_BIAS_IN_CALIBRATION and bias master present)
        if APPLY_BIAS_IN_CALIBRATION {
            if let Some(ref bias) = cal.bias {
                let bc = cal.bias_channels.max(1);
                for px in 0..n_pixels {
                    // For a mono output we always use channel 0 of the master
                    let bias_idx = px * bc;
                    if bias_idx < bias.len() {
                        raw[px] = (raw[px] - bias[bias_idx]).max(0.0);
                    }
                }
            }
        }

        // Dark subtract
        if let Some(ref dark) = cal.dark {
            let dc = cal.dark_channels.max(1);
            for px in 0..n_pixels {
                let dark_idx = px * dc;
                if dark_idx < dark.len() {
                    raw[px] = (raw[px] - dark[dark_idx]).max(0.0);
                }
            }
        }

        // Normalize each sub by its scalar mean (multiplicative normalization,
        // equalize fluxes). Preserves the illumination pattern while removing
        // sub-to-sub brightness variation.
        let mean: f32 = raw.iter().sum::<f32>() / raw.len() as f32;
        if mean > 1e-6 {
            for v in raw.iter_mut() {
                *v /= mean;
            }
        }

        flat_planes.push(raw);

        let pct = ((i + 1) as f32 / total as f32 * 100.0).round() as u32;
        messages.push(format!("FlatStack: calibrating/normalizing frame {} / {} ({}%)…",
            i + 1, total, pct));
    }

    if flat_planes.is_empty() {
        return Err(PluginError::new("NO_FRAMES_STACKED", "No flat frames could be loaded."));
    }

    let n_frames = flat_planes.len();
    messages.push(format!(
        "FlatStack: Winsorized sigma-clipping ({:.1}σ, {} iterations) and mean-combining {} frames…",
        FLAT_STACK_WINSORIZE_SIGMA, FLAT_STACK_WINSORIZE_ITERATIONS, n_frames
    ));

    // Winsorized sigma-clip and mean-combine per pixel in parallel.
    // For each pixel, iteratively compute mean+stddev, clamp outliers to the
    // sigma boundary (Winsorize rather than exclude), then repeat. Final master
    // value is the mean of the Winsorized samples.
    let master_pixels: Vec<f32> = (0..n_pixels)
        .into_par_iter()
        .map(|px| {
            let mut samples: Vec<f32> = flat_planes.iter()
                .map(|plane| plane[px])
                .collect();

            for _ in 0..FLAT_STACK_WINSORIZE_ITERATIONS {
                let n      = samples.len() as f32;
                let mean   = samples.iter().sum::<f32>() / n;
                let var    = samples.iter().map(|&v| (v - mean).powi(2)).sum::<f32>() / n;
                let stddev = var.sqrt();
                if stddev < 1e-10 { break; }
                let lo = mean - FLAT_STACK_WINSORIZE_SIGMA * stddev;
                let hi = mean + FLAT_STACK_WINSORIZE_SIGMA * stddev;
                for v in samples.iter_mut() {
                    *v = v.clamp(lo, hi);
                }
            }

            samples.iter().sum::<f32>() / samples.len() as f32
        })
        .collect();

    // Normalize master flat to [0,1] by dividing by the global maximum so the
    // file is interchangeable with PI and other tools. Re-normalization to ~1.0
    // for calibration use happens at load time in load_calibration_masters().
    let mut master_pixels = master_pixels;
    let global_max: f32 = master_pixels.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    if global_max > 1e-6 {
        for v in master_pixels.iter_mut() {
            *v /= global_max;
        }
    }

    let timestamp_display = Utc::now().format("%Y-%m-%d %H:%M").to_string();
    let stack_label = format!(
        "MASTER FLAT \u{2014} {} frames \u{2014} {}", n_frames, timestamp_display
    );

    messages.push(format!("FlatStack: master flat created from {} frames (1-channel grayscale)", n_frames));

    Ok(ImageBuffer {
        filename:      stack_label,
        width:         width  as u32,
        height:        height as u32,
        display_width: width  as u32,
        bit_depth:     BitDepth::F32,
        color_space:   ColorSpace::Mono,
        channels:      1,
        keywords:      build_flat_keywords(width, height, n_frames),
        pixels:        Some(PixelData::F32(master_pixels)),
    })
}

//  ── Snapshot type ─────────────────────────────────────────────────────────────

struct FrameSnapshot {
    index:         usize,
    path:          String,
    width:         usize,
    height:        usize,
    channels:      usize,
    color_space:   ColorSpace,
    bayer_pattern: BayerPattern,
    filter:        Option<String>,
    exptime:       Option<f32>,
    fwhm:          Option<f32>,
    eccentricity:  Option<f32>,
    rotator:       Option<f32>,
    stars:         Vec<crate::analysis::stars::StarCandidate>,
    date_obs:      Option<f64>,
    group:         usize,
}

impl PhotonPlugin for StackFrames {
    fn name(&self) -> &str { "StackFrames" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str {
        "Stacks loaded frames. Light frames: FFT alignment, triangle rigid refinement, \
         meridian-flip-aware group reference selection, optional calibration via caldir=. \
         Flat frames: raw single-channel (no debayer), bias/dark calibration respecting \
         APPLY_BIAS_IN_CALIBRATION, per-sub mean normalization, Winsorized sigma-clipping, \
         mean-combine, 1-channel grayscale F32 output normalized to ~1.0. \
         Frame type is detected automatically from filename or IMAGETYP keyword."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "caldir".to_string(),
                param_type:  ParamType::Path,
                required:    false,
                description: "Directory containing master calibration files. Files are \
                              identified by filename containing 'bias', 'dark', or 'flat'. \
                              Calibration is applied to raw pixels before background \
                              normalization. When stacking flats, flat masters are ignored.".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded."));
        }
        ctx.clear_stack();

        let caldir = args.get("caldir").map(|s| s.trim_matches('"').to_string());

        //  ── Detect frame type from first file ─────────────────────────────────
        let first_path = ctx.file_list.first().unwrap().clone();
        let first_keywords = ctx.image_buffers.get(&first_path)
            .map(|b| b.keywords.clone())
            .unwrap_or_default();
        let frame_type = detect_frame_type(&first_path, &first_keywords);

        info!("StackFrames: detected frame type = {:?}", frame_type);

        //  ── Load calibration masters ──────────────────────────────────────────
        let mut messages: Vec<String> = Vec::new();
        let cal = if let Some(ref dir) = caldir {
            // When stacking flats, never load a flat master as calibration.
            load_calibration_masters(dir, frame_type == FrameType::Light, &mut messages)
        } else {
            CalibrationMasters {
                bias: None, bias_channels: 1,
                dark: None, dark_channels: 1,
                flat: None, flat_channels: 1,
            }
        };

        let has_cal = cal.bias.is_some() || cal.dark.is_some() || cal.flat.is_some();

        //  ── Branch: flat vs light ─────────────────────────────────────────────
        if frame_type == FrameType::Flat {
            let flat_buf = stack_flat_frames(ctx, &mut messages, &cal)?;
            let n_frames = ctx.file_list.len();
            ctx.stack_result = Some(flat_buf);

            let full_message = messages.join("\n");
            info!("StackFrames (flat): {}", full_message);

            return Ok(PluginOutput::Data(serde_json::json!({
                "plugin":          "StackFrames",
                "frame_type":      "flat",
                "stacked_frames":  n_frames,
                "total_frames":    n_frames,
                "message":         full_message,
                "stack_available": true,
            })));
        }

        //  ── Light frame stacking ──────────────────────────────────────────────

        let det_config = StarDetectionConfig::default();
        let snapshots  = collect_snapshots(ctx, &det_config)?;

        if snapshots.is_empty() {
            return Err(PluginError::new("NO_PIXELS", "No frames with pixel data available."));
        }

        let width    = snapshots[0].width;
        let height   = snapshots[0].height;
        let n_pixels = width * height;
        let total    = snapshots.len();

        let n_groups = snapshots.iter().map(|s| s.group).max().unwrap_or(0) + 1;
        info!("StackFrames: identified {} rotational group(s)", n_groups);

        let group_refs: Vec<usize> = (0..n_groups)
            .map(|g| select_reference_in_group(&snapshots, g))
            .collect();

        for (g, &ridx) in group_refs.iter().enumerate() {
            let count = snapshots.iter().filter(|s| s.group == g).count();
            info!("  Group {}: {} frames, reference = frame {} ({})",
                g, count, snapshots[ridx].index, short_name(&snapshots[ridx].path));
        }

        let master_group   = (0..n_groups)
            .max_by_key(|&g| snapshots.iter().filter(|s| s.group == g).count())
            .unwrap();
        let master_ref_idx = group_refs[master_group];

        info!("StackFrames: master group = {} (reference frame {})",
            master_group, snapshots[master_ref_idx].index);

        let ref_filter      = snapshots[master_ref_idx].filter.clone();
        let ref_color_space = snapshots[master_ref_idx].color_space.clone();

        let is_color   = ref_color_space == ColorSpace::Bayer
                      || ref_color_space == ColorSpace::RGB;
        let n_channels = if is_color { 3 } else { 1 };

        info!("StackFrames: output mode = {}", if is_color { "RGB (color)" } else { "Mono (grayscale)" });

        let master_ref_luma  = load_debayered_luma(ctx, &snapshots[master_ref_idx])?;
        let master_ref_stars = snapshots[master_ref_idx].stars.clone();

        let ref_path   = snapshots[master_ref_idx].path.clone();
        let ref_target = ctx.image_buffers.get(&ref_path)
            .and_then(|b| b.keywords.get("OBJECT"))
            .map(|kw| kw.value.clone());

        //  ── Solve M_cross for each non-master group ───────────────────────────
        let mut m_cross: Vec<AffineRigid> = (0..n_groups).map(|_| AffineRigid::identity()).collect();

        for g in 0..n_groups {
            if g == master_group { continue; }

            let gref_snap = &snapshots[group_refs[g]];
            let gref_luma = load_debayered_luma(ctx, gref_snap)?;

            let mut flipped_luma = gref_luma.clone();
            flipped_luma.reverse();

            let fft_t = match compute_translation(&master_ref_luma, &flipped_luma, width, height) {
                Some(t) => t,
                None    => {
                    let msg = format!(
                        "StackFrames: cross-group {} FFT failed — frames from this group will be excluded", g
                    );
                    info!("{}", msg);
                    messages.push(msg);
                    continue;
                }
            };

            info!("StackFrames: cross-group {} FFT vs master ref dx={:.2} dy={:.2}",
                g, fft_t.dx, fft_t.dy);

            let flipped_stars = detect_stars(&flipped_luma, width, height, &det_config);

            let ransac = estimate_rigid_transform(
                &master_ref_stars, &flipped_stars,
                fft_t.dx, fft_t.dy, width, height,
            );

            let post_flip = match estimate_rigid_transform_triangles(
                &master_ref_stars, &flipped_stars,
            ) {
                Some(r) => {
                    let theta = r.theta();
                    info!("StackFrames: cross-group {} triangle match — tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°)",
                        g, r.tx, r.ty, theta, theta.to_degrees());
                    r
                }
                None => {
                    info!("StackFrames: cross-group {} triangle match failed — falling back to FFT-only", g);
                    AffineRigid::translation(fft_t.dx, fft_t.dy)
                }
            };

            let _ = ransac;

            let flip   = AffineRigid::flip_180(width, height);
            m_cross[g] = compose(&post_flip, &flip);

            info!("StackFrames: M_cross[{}] = a={:.4} b={:.4} tx={:.2} ty={:.2}",
                g, m_cross[g].a, m_cross[g].b, m_cross[g].tx, m_cross[g].ty);

            {
                let mut residuals: Vec<f32> = Vec::new();
                for gs in &gref_snap.stars {
                    let (mx, my) = m_cross[g].apply_forward(gs.cx, gs.cy);
                    if let Some(closest) = master_ref_stars.iter()
                        .map(|r| ((r.cx - mx).powi(2) + (r.cy - my).powi(2)).sqrt())
                        .reduce(f32::min)
                    {
                        if closest < 10.0 { residuals.push(closest); }
                    }
                }
                if residuals.is_empty() {
                    info!("StackFrames: M_cross[{}] verification — no stars matched within 10px", g);
                } else {
                    let mean = residuals.iter().sum::<f32>() / residuals.len() as f32;
                    let max  = residuals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    info!("StackFrames: M_cross[{}] verification — {} stars matched, mean residual={:.2}px, max={:.2}px",
                        g, residuals.len(), mean, max);
                }
            }
        }

        //  ── Pass 1 — Welford online mean + M2 ────────────────────────────────

        let mut mean_buf:  Vec<f32> = vec![0.0; n_pixels * n_channels];
        let mut m2_buf:    Vec<f32> = vec![0.0; n_pixels * n_channels];
        let mut count_buf: Vec<u32> = vec![0;   n_pixels];

        let mut cached_transforms: Vec<Option<AffineRigid>> = (0..snapshots.len())
            .map(|_| None).collect();

        let mut group_ref_luma:  Vec<Option<Vec<f32>>> = (0..n_groups).map(|_| None).collect();
        let mut group_ref_stars: Vec<Option<Vec<crate::analysis::stars::StarCandidate>>>
            = (0..n_groups).map(|_| None).collect();
        group_ref_luma[master_group]  = Some(master_ref_luma.clone());
        group_ref_stars[master_group] = Some(master_ref_stars.clone());

        let bg_sigma_config    = SigmaClipConfig::default();
        let mut contributions: Vec<FrameContribution> = Vec::new();
        let mut total_integration = 0.0f32;

        for (i, snap) in snapshots.iter().enumerate() {
            let mut contrib = FrameContribution::new(snap.index, &snap.path);
            contrib.filter           = snap.filter.clone();
            contrib.fwhm             = snap.fwhm;
            contrib.eccentricity     = snap.eccentricity;
            contrib.meridian_flipped = snap.group != master_group;

            // Filter validation
            if let (Some(ref rf), Some(ref sf)) = (&ref_filter, &snap.filter) {
                if rf != sf && i != master_ref_idx {
                    let msg = format!(
                        "Filter mismatch: frame {} ({}) excluded — stack filter is {}",
                        snap.index, sf, rf
                    );
                    info!("{}", msg);
                    messages.push(msg);
                    contrib.exclusion_reason = Some(ExclusionReason::FilterMismatch);
                    contributions.push(contrib);
                    continue;
                }
            }

            // Load raw [0,1] pixels
            let t_load = std::time::Instant::now();
            let mut frame_pixels = load_frame_pixels(ctx, snap, is_color);
            let ms_load = t_load.elapsed().as_secs_f64() * 1000.0;

            // Apply calibration to raw pixels BEFORE background normalization.
            // bias subtract   dark subtract   flat divide on [0,1] values.
            let t_cal = std::time::Instant::now();
            if has_cal {
                apply_calibration(&mut frame_pixels, &cal, if is_color { 3 } else { 1 });
            }
            let ms_cal = t_cal.elapsed().as_secs_f64() * 1000.0;

            // Extract calibrated luma for background estimation and FFT alignment.
            let t_luma = std::time::Instant::now();
            let cal_luma = if is_color {
                analysis::extract_luminance(&frame_pixels, width, height, 3)
            } else {
                frame_pixels.clone()
            };
            let ms_luma = t_luma.elapsed().as_secs_f64() * 1000.0;

            if group_ref_luma[snap.group].is_none() {
                let g_ref  = &snapshots[group_refs[snap.group]];
                let g_luma = load_debayered_luma(ctx, g_ref)?;
                group_ref_stars[snap.group] = Some(g_ref.stars.clone());
                group_ref_luma[snap.group]  = Some(g_luma);
            }
            let g_ref_luma  = group_ref_luma[snap.group].as_ref().unwrap();
            let g_ref_stars = group_ref_stars[snap.group].as_ref().unwrap();

            // Compute background from calibrated luma
            let t_bg = std::time::Instant::now();
            let bg_est   = estimate_background(&cal_luma, &bg_sigma_config);
            let bg_level = bg_est.median;
            let ms_bg = t_bg.elapsed().as_secs_f64() * 1000.0;
            contrib.background_level = Some(bg_level);
            let divisor = if bg_level > 1e-6 { bg_level } else { 1.0 };
            if i == 0 {
                info!("Pass 1 frame 0: bg_level={:.6} divisor={:.6} cal_luma_min={:.6} cal_luma_max={:.6}",
                    bg_level, divisor,
                    cal_luma.iter().cloned().fold(f32::INFINITY, f32::min),
                    cal_luma.iter().cloned().fold(f32::NEG_INFINITY, f32::max));
            }

            // Normalize calibrated luma by background for FFT alignment
            let normalized_luma: Vec<f32> = cal_luma.par_iter().map(|&v| v / divisor).collect();

            let t_fft = std::time::Instant::now();
            let g_transform: Option<AffineRigid> = if i == group_refs[snap.group] {
                contrib.fft_translation     = Some((0.0, 0.0));
                contrib.alignment_validated = Some(true);
                Some(AffineRigid::identity())
            } else {
                match compute_translation(g_ref_luma, &normalized_luma, width, height) {
                    Some(t) => {
                        contrib.fft_translation     = Some((t.dx, t.dy));
                        contrib.alignment_validated = Some(true);
                        info!("Frame {}: RANSAC input   {} ref stars, {} frame stars, fft=({:.2},{:.2})",
                            snap.index, g_ref_stars.len(), snap.stars.len(), t.dx, t.dy);
                        let xform = try_rigid_refinement(
                            g_ref_stars, &snap.stars,
                            t.dx, t.dy, width, height,
                            snap.index, &mut messages,
                        );
                        Some(xform)
                    }
                    None => {
                        alignment_failed(snap.index, "FFT returned no result",
                            &mut messages, &mut contrib, &mut contributions);
                        continue;
                    }
                }
            };
            let ms_fft = t_fft.elapsed().as_secs_f64() * 1000.0;

            let g_xform = g_transform.unwrap();
            let t_final = compose(&m_cross[snap.group], &g_xform);

            // Divide calibrated frame pixels by background for accumulation.
            // When calibration is applied the pedestal is already removed, so
            // background normalization would amplify near-zero values destructively.
            let accum_pixels: Vec<f32> = if has_cal {
                frame_pixels.clone()
            } else {
                frame_pixels.iter().map(|&v| v / divisor).collect()
            };

            let t_resample = std::time::Instant::now();
            let theta = t_final.theta();
            if is_color {
                let aligned_rgb = if theta.abs() >= MIN_ROTATION_TO_APPLY || t_final.a < 0.5 {
                    resample_frame_rgb_affine(&accum_pixels, width, height, &t_final)
                } else {
                    resample_frame_rgb(&accum_pixels, width, height, t_final.tx, t_final.ty)
                };

                for px in 0..n_pixels {
                    count_buf[px] += 1;
                    let count = count_buf[px] as f32;
                    for ch in 0..3 {
                        let idx    = px * 3 + ch;
                        let val    = aligned_rgb[idx];
                        let delta  = val - mean_buf[idx];
                        mean_buf[idx] += delta / count;
                        let delta2 = val - mean_buf[idx];
                        m2_buf[idx]  += delta * delta2;
                    }
                }
            } else {
                let aligned = if theta.abs() >= MIN_ROTATION_TO_APPLY || t_final.a < 0.5 {
                    resample_frame_affine(&accum_pixels, width, height, &t_final)
                } else {
                    resample_frame(&accum_pixels, width, height, t_final.tx, t_final.ty)
                };

                mean_buf.par_iter_mut()
                    .zip(m2_buf.par_iter_mut())
                    .zip(count_buf.par_iter_mut())
                    .zip(aligned.par_iter())
                    .for_each(|(((mean, m2), count), &val)| {
                        *count += 1;
                        let delta  = val - *mean;
                        *mean     += delta / *count as f32;
                        let delta2 = val - *mean;
                        *m2       += delta * delta2;
                    });
            }

            let ms_resample = t_resample.elapsed().as_secs_f64() * 1000.0;

            cached_transforms[i] = Some(t_final);

            contrib.included = true;
            if let Some(et) = snap.exptime { total_integration += et; }

            info!(
                "Pass1 frame {:3}: load={:6.1}ms cal={:6.1}ms luma={:6.1}ms bg={:6.1}ms fft={:6.1}ms resample={:6.1}ms",
                snap.index, ms_load, ms_cal, ms_luma, ms_bg, ms_fft, ms_resample
            );

            let pct = ((i + 1) as f32 / total as f32 * 100.0).round() as u32;
            messages.push(format!("Pass 1   frame {} / {} ({}%) ", i + 1, total, pct));
            contributions.push(contrib);
        }

        let stacked_count = contributions.iter().filter(|c| c.included).count();
        if stacked_count == 0 {
            return Err(PluginError::new("NO_FRAMES_STACKED", "No frames could be stacked."));
        }

        // Per-pixel stddev from Welford M2
        let stddev_buf: Vec<f32> = if is_color {
            let m2_ref = &m2_buf;
            count_buf.par_iter()
                .enumerate()
                .flat_map_iter(|(px, &count)| {
                    (0..3).map(move |ch| {
                        let idx = px * 3 + ch;
                        if count > 1 { (m2_ref[idx] / count as f32).sqrt() } else { 0.0 }
                    })
                })
                .collect()
        } else {
            count_buf.par_iter()
                .zip(m2_buf.par_iter())
                .map(|(&count, &m2)| {
                    if count > 1 { (m2 / count as f32).sqrt() } else { 0.0 }
                })
                .collect()
        };

        //     Pass 2   sigma-clipped accumulation (batched parallel)
        let sigma      = 2.5f32;
        let n_threads  = if ctx.rayon_thread_count == -1 {
            rayon::current_num_threads()
        } else {
            ctx.rayon_thread_count as usize
        }.max(1);

        let mut sum_buf:    Vec<f64> = vec![0.0; n_pixels * n_channels];
        let mut clip_count: Vec<u32> = vec![0;  n_pixels];

        // Build list of (snapshot_index, xform) for frames that have a cached transform.
        // This avoids borrowing ctx inside the parallel closure.
        struct Pass2Input {
            snap_idx:     usize,
            xform:        AffineRigid,
            pixels_f32:   Vec<f32>,
        }

        let pass2_inputs: Vec<Pass2Input> = snapshots.iter().enumerate()
            .filter_map(|(i, snap)| {
                let xform = cached_transforms[i].clone()?;
                let buf    = ctx.image_buffers.get(&snap.path)?;
                let pixels = buf.pixels.as_ref()?;
                let pixels_f32 = if is_color {
                    if snap.color_space == ColorSpace::Bayer {
                        let mono = analysis::to_f32_normalized(pixels);
                        debayer_bilinear(&mono, snap.width, snap.height, snap.bayer_pattern)
                    } else {
                        analysis::to_f32_normalized(pixels)
                    }
                } else {
                    analysis::to_luminance(pixels, snap.channels)
                };
                Some(Pass2Input { snap_idx: i, xform, pixels_f32 })
            })
            .collect();

        let mut pass2_done = 0usize;

        for chunk in pass2_inputs.chunks(n_threads) {
            // Parallel: calibrate, background estimate, resample each frame in chunk.
            let aligned_buffers: Vec<Vec<f32>> = chunk.par_iter()
                .map(|inp| {
                    let mut frame_pixels = inp.pixels_f32.clone();

                    if has_cal {
                        apply_calibration(&mut frame_pixels, &cal, if is_color { 3 } else { 1 });
                    }

                    let cal_luma = if is_color {
                        analysis::extract_luminance(&frame_pixels, width, height, 3)
                    } else {
                        frame_pixels.clone()
                    };

                    let bg_est   = estimate_background(&cal_luma, &bg_sigma_config);
                    let bg_level = bg_est.median;
                    let divisor  = if bg_level > 1e-6 { bg_level } else { 1.0 };

                    let accum_pixels: Vec<f32> = if has_cal {
                        frame_pixels
                    } else {
                        frame_pixels.iter().map(|&v| v / divisor).collect()
                    };

                    let theta = inp.xform.theta();
                    if is_color {
                        if theta.abs() >= MIN_ROTATION_TO_APPLY || inp.xform.a < 0.5 {
                            resample_frame_rgb_affine(&accum_pixels, width, height, &inp.xform)
                        } else {
                            resample_frame_rgb(&accum_pixels, width, height, inp.xform.tx, inp.xform.ty)
                        }
                    } else {
                        let normalized: Vec<f32> = cal_luma.iter().map(|&v| v / divisor).collect();
                        if theta.abs() >= MIN_ROTATION_TO_APPLY || inp.xform.a < 0.5 {
                            resample_frame_affine(&normalized, width, height, &inp.xform)
                        } else {
                            resample_frame(&normalized, width, height, inp.xform.tx, inp.xform.ty)
                        }
                    }
                })
                .collect();

            // Sequential: accumulate aligned buffers into sum_buf / clip_count.
            for (inp, aligned) in chunk.iter().zip(aligned_buffers.iter()) {
                if is_color {
                    let aligned_rgb = aligned;
                    for px in 0..n_pixels {
                        let mean_luma = mean_buf[px * 3 + 1];
                        let sd_luma   = stddev_buf[px * 3 + 1];
                        let val_luma  = aligned_rgb[px * 3 + 1];
                        let threshold = sigma * sd_luma;
                        if sd_luma < 1e-10 || (val_luma - mean_luma).abs() <= threshold {
                            clip_count[px] += 1;
                            for ch in 0..3 {
                                sum_buf[px * 3 + ch] += aligned_rgb[px * 3 + ch] as f64;
                            }
                        }
                    }
                } else {
                    sum_buf.iter_mut()
                        .zip(clip_count.iter_mut())
                        .zip(aligned.iter())
                        .zip(mean_buf.iter())
                        .zip(stddev_buf.iter())
                        .for_each(|((((sum, count), &val), &mean), &sd)| {
                            let threshold = sigma * sd;
                            if sd < 1e-10 || (val - mean).abs() <= threshold {
                                *sum   += val as f64;
                                *count += 1;
                            }
                        });
                }

                pass2_done += 1;
                let pct = (pass2_done as f32 / total as f32 * 100.0).round() as u32;
                messages.push(format!("Pass 2   frame {} / {} ({}%) ", inp.snap_idx + 1, total, pct));
            }
            // aligned_buffers dropped here, releasing chunk memory
        }

        //  ── Build output pixels ───────────────────────────────────────────────
        let raw_pixels: Vec<f32> = if is_color {
            sum_buf.par_iter()
                .enumerate()
                .map(|(flat_idx, &sum)| {
                    let px    = flat_idx / 3;
                    let count = clip_count[px];
                    if count > 0 { sum as f32 / count as f32 } else { 0.0 }
                })
                .collect()
        } else {
            sum_buf.par_iter()
                .zip(clip_count.par_iter())
                .map(|(&sum, &count)| {
                    if count > 0 { sum as f32 / count as f32 } else { 0.0 }
                })
                .collect()
        };

        let stack_pixels = normalize_output(&raw_pixels, is_color, n_pixels);

        let completed_at      = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let timestamp_display = Utc::now().format("%Y-%m-%d %H:%M").to_string();

        let stack_label = format!(
            "STACKED RESULT \u{2014} {} / {} frames \u{2014} {}",
            stacked_count, total, timestamp_display
        );

        let output_color_space = if is_color { ColorSpace::RGB } else { ColorSpace::Mono };
        let output_channels    = if is_color { 3u8 } else { 1u8 };

        let stack_buf = ImageBuffer {
            filename:      stack_label.clone(),
            width:         width  as u32,
            height:        height as u32,
            display_width: width  as u32,
            bit_depth:     BitDepth::F32,
            color_space:   output_color_space,
            channels:      output_channels,
            keywords:      build_stack_keywords(width, height, &ctx.stack_summary),
            pixels:        Some(PixelData::F32(stack_pixels)),
        };

        ctx.stack_result = Some(stack_buf);

        let mut summary = StackSummary::compute(&contributions, &completed_at);
        summary.target              = ref_target;
        summary.filter              = ref_filter;
        summary.integration_seconds = total_integration;

        ctx.stack_contributions = contributions;
        ctx.stack_summary       = Some(summary.clone());

        let cal_note = if caldir.is_some() {
            format!("  Calibration:           applied ({}{}{})",
                if cal.bias.is_some() { "bias " } else { "" },
                if cal.dark.is_some() { "dark " } else { "" },
                if cal.flat.is_some() { "flat" }  else { "" },
            )
        } else {
            "  Calibration:           none".to_string()
        };

        let quality_summary = format!(
            "Stack Quality Summary:\n  Frames stacked:        {} / {}\n  SNR improvement:       ~{:.1}x (vs single frame)\n  Alignment success:     {:.1}%\n  Background uniformity: {}\n  Output mode:           {}\n{}",
            summary.stacked_frames, summary.total_frames,
            summary.snr_improvement, summary.alignment_success_rate * 100.0,
            summary.background_uniformity,
            if is_color { "RGB color" } else { "Grayscale" },
            cal_note,
        );

        messages.push(format!(
            "Stacking complete \u{2014} {} / {} frames stacked", stacked_count, total
        ));
        messages.push(quality_summary);

        let full_message = messages.join("\n");
        info!("StackFrames: {}", full_message);

        Ok(PluginOutput::Data(serde_json::json!({
            "plugin":          "StackFrames",
            "frame_type":      "light",
            "stacked_frames":  stacked_count,
            "total_frames":    total,
            "message":         full_message,
            "stack_available": true,
        })))
    }
}

//  ── Output normalization ──────────────────────────────────────────────────────

fn normalize_output(raw: &[f32], is_color: bool, _n_pixels: usize) -> Vec<f32> {
    if !is_color {
        let max_val = raw.par_iter().cloned().reduce(|| f32::NEG_INFINITY, f32::max);
        let min_val = raw.par_iter().cloned().reduce(|| f32::INFINITY,     f32::min);
        let range   = (max_val - min_val).max(1e-6);
        return raw.par_iter().map(|&v| ((v - min_val) / range).clamp(0.0, 1.0)).collect();
    }

    // Normalize all channels together using a single global min/max so that
    // relative channel ratios are preserved. Per-channel normalization would
    // destroy color balance by stretching each channel independently.
    let max_val = raw.par_iter().cloned().reduce(|| f32::NEG_INFINITY, f32::max);
    let min_val = raw.par_iter().cloned().reduce(|| f32::INFINITY,     f32::min);
    let range   = (max_val - min_val).max(1e-6);
    raw.par_iter().map(|&v| ((v - min_val) / range).clamp(0.0, 1.0)).collect()
}

//  ── Snapshot collection ───────────────────────────────────────────────────────

fn collect_snapshots(
    ctx:        &AppContext,
    det_config: &StarDetectionConfig,
) -> Result<Vec<FrameSnapshot>, PluginError> {
    let mut snapshots: Vec<FrameSnapshot> = Vec::new();

    for (index, path) in ctx.file_list.iter().enumerate() {
        let buf = match ctx.image_buffers.get(path) {
            Some(b) => b,
            None    => continue,
        };
        let pixels = match &buf.pixels {
            Some(p) => p,
            None    => continue,
        };

        let width    = buf.width  as usize;
        let height   = buf.height as usize;
        let channels = buf.channels as usize;

        let bayer_pattern = buf.keywords.get("BAYERPAT")
            .map(|kw| BayerPattern::from_str(&kw.value))
            .unwrap_or(BayerPattern::RGGB);

        let luma = if buf.color_space == ColorSpace::Bayer {
            let mono = analysis::to_f32_normalized(pixels);
            let rgb  = debayer_bilinear(&mono, width, height, bayer_pattern);
            analysis::extract_luminance(&rgb, width, height, 3)
        } else {
            analysis::to_luminance(pixels, channels)
        };

        let stars        = detect_stars(&luma, width, height, det_config);
        let fwhm         = if stars.len() >= 5 { compute_fwhm(&stars, None).map(|r| r.fwhm_pixels) } else { None };
        let eccentricity = if stars.len() >= 5 { compute_eccentricity(&stars).map(|r| r.eccentricity) } else { None };

        let filter   = buf.keywords.get("FILTER").map(|kw| kw.value.clone());
        let exptime  = buf.keywords.get("EXPTIME").and_then(|kw| kw.value.parse::<f32>().ok());
        let rotator  = buf.keywords.get("ROTATOR").and_then(|kw| kw.value.parse::<f32>().ok());
        let date_obs = buf.keywords.get("DATE-OBS").and_then(|kw| parse_date_obs(&kw.value));

        snapshots.push(FrameSnapshot {
            index,
            path: path.clone(),
            width, height, channels,
            color_space:   buf.color_space.clone(),
            bayer_pattern,
            filter, exptime, fwhm, eccentricity, rotator, stars, date_obs,
            group: 0,
        });
    }

    if !snapshots.is_empty() {
        assign_groups(&mut snapshots);
    }

    Ok(snapshots)
}

//  ── Group assignment ──────────────────────────────────────────────────────────

fn assign_groups(snapshots: &mut Vec<FrameSnapshot>) {
    snapshots.sort_by(|a, b| {
        a.date_obs.partial_cmp(&b.date_obs).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut group = 0usize;
    snapshots[0].group = 0;

    for i in 1..snapshots.len() {
        let time_gap = match (snapshots[i - 1].date_obs, snapshots[i].date_obs) {
            (Some(a), Some(b)) => (b - a) / 60.0,
            _ => 0.0,
        };
        let rot_diff = match (snapshots[i - 1].rotator, snapshots[i].rotator) {
            (Some(a), Some(b)) => (b - a).abs(),
            _ => 0.0,
        };
        // A rotation > MERIDIAN_FLIP_THRESHOLD always triggers a new group
        // regardless of time gap (catches same-night meridian flips).
        // A long time gap combined with any significant rotation also splits.
        if rot_diff > MERIDIAN_FLIP_THRESHOLD
            || (time_gap > SESSION_GAP_MINUTES as f64 && rot_diff > ROTATOR_GROUP_TOLERANCE)
        {
            group += 1;
        }
        snapshots[i].group = group;
    }
}

//  ── Reference frame selection ─────────────────────────────────────────────────

fn select_reference_in_group(snapshots: &[FrameSnapshot], group: usize) -> usize {
    snapshots.iter()
        .enumerate()
        .filter(|(_, s)| s.group == group)
        .max_by(|(_, a), (_, b)| {
            let score_a = quality_score(a);
            let score_b = quality_score(b);
            score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn quality_score(snap: &FrameSnapshot) -> f32 {
    let fwhm_score = snap.fwhm.map(|f| 1.0 / f.max(0.1)).unwrap_or(0.0);
    let ecc_score  = snap.eccentricity.map(|e| 1.0 - e).unwrap_or(0.0);
    fwhm_score + ecc_score
}

//  ── Frame pixel loading ───────────────────────────────────────────────────────

fn load_frame_pixels(ctx: &AppContext, snap: &FrameSnapshot, is_color: bool) -> Vec<f32> {
    let buf    = ctx.image_buffers.get(&snap.path).unwrap();
    let pixels = buf.pixels.as_ref().unwrap();

    if is_color {
        if snap.color_space == ColorSpace::Bayer {
            let mono = analysis::to_f32_normalized(pixels);
            debayer_bilinear(&mono, snap.width, snap.height, snap.bayer_pattern)
        } else {
            analysis::to_f32_normalized(pixels)
        }
    } else {
        analysis::to_luminance(pixels, snap.channels)
    }
}

fn load_debayered_luma(ctx: &AppContext, snap: &FrameSnapshot) -> Result<Vec<f32>, PluginError> {
    let buf = ctx.image_buffers.get(&snap.path)
        .ok_or_else(|| PluginError::new("NO_BUFFER", "Frame buffer missing."))?;
    let pixels = buf.pixels.as_ref()
        .ok_or_else(|| PluginError::new("NO_PIXELS", "Frame has no pixel data."))?;

    let luma = if snap.color_space == ColorSpace::Bayer {
        let mono = analysis::to_f32_normalized(pixels);
        let rgb  = debayer_bilinear(&mono, snap.width, snap.height, snap.bayer_pattern);
        analysis::extract_luminance(&rgb, snap.width, snap.height, 3)
    } else {
        analysis::to_luminance(pixels, snap.channels)
    };
    Ok(luma)
}

//  ── Alignment helpers ─────────────────────────────────────────────────────────

fn try_rigid_refinement(
    ref_stars:  &[crate::analysis::stars::StarCandidate],
    frm_stars:  &[crate::analysis::stars::StarCandidate],
    fft_dx:     f32,
    fft_dy:     f32,
    width:      usize,
    height:     usize,
    frame_idx:  usize,
    messages:   &mut Vec<String>,
) -> AffineRigid {
    let ransac = estimate_rigid_transform(ref_stars, frm_stars, fft_dx, fft_dy, width, height);
    match estimate_rigid_transform_triangles(ref_stars, frm_stars) {
        Some(tri) => {
            let theta = tri.theta();
            info!("Frame {}: triangle match — tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°)",
                frame_idx, tri.tx, tri.ty, theta, theta.to_degrees());
            let _ = ransac;
            tri
        }
        None => {
            let msg = format!(
                "Frame {}: triangle match failed — using FFT translation only", frame_idx
            );
            info!("{}", msg);
            messages.push(msg);
            AffineRigid::translation(fft_dx, fft_dy)
        }
    }
}

fn alignment_failed(
    frame_idx:     usize,
    reason:        &str,
    messages:      &mut Vec<String>,
    contrib:       &mut FrameContribution,
    contributions: &mut Vec<FrameContribution>,
) {
    let msg = format!("Frame {}: alignment failed — {} — excluded", frame_idx, reason);
    info!("{}", msg);
    messages.push(msg);
    contrib.exclusion_reason    = Some(ExclusionReason::AlignmentFailed);
    contrib.alignment_validated = Some(false);
    contributions.push(contrib.clone());
}

//  ── DATE-OBS parsing ─────────────────────────────────────────────────────────

fn parse_date_obs(s: &str) -> Option<f64> {
    // ISO 8601: "2026-06-15T22:59:16" or "2026-06-15T22:59:16.000"
    let dt = chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%dT%H:%M:%S%.f"))
        .ok()?;
    Some(dt.and_utc().timestamp() as f64)
}

//  ── Frame resampling (translation only) ──────────────────────────────────────

fn resample_frame(
    pixels: &[f32],
    width:  usize,
    height: usize,
    dx:     f32,
    dy:     f32,
) -> Vec<f32> {
    (0..height * width)
        .into_par_iter()
        .map(|idx| {
            let out_y = idx / width;
            let out_x = idx % width;
            let src_x = out_x as f32 - dx;
            let src_y = out_y as f32 - dy;
            let x0 = src_x.floor() as i32;
            let y0 = src_y.floor() as i32;
            let fx = src_x - x0 as f32;
            let fy = src_y - y0 as f32;
            bilinear(pixels, width, height, x0, y0, x0 + 1, y0 + 1, fx, fy)
        })
        .collect()
}

fn resample_frame_affine(
    normalized: &[f32],
    width:      usize,
    height:     usize,
    xform:      &AffineRigid,
) -> Vec<f32> {
    (0..height * width)
        .into_par_iter()
        .map(|idx| {
            let out_y = idx / width;
            let out_x = idx % width;
            let (src_x, src_y) = xform.apply_inverse(out_x as f32, out_y as f32);
            let x0 = src_x.floor() as i32;
            let y0 = src_y.floor() as i32;
            let fx = src_x - x0 as f32;
            let fy = src_y - y0 as f32;
            bilinear(normalized, width, height, x0, y0, x0 + 1, y0 + 1, fx, fy)
        })
        .collect()
}

//  ── Frame resampling (RGB) ────────────────────────────────────────────────────

fn resample_frame_rgb(
    rgb:    &[f32],
    width:  usize,
    height: usize,
    dx:     f32,
    dy:     f32,
) -> Vec<f32> {
    (0..height * width)
        .into_par_iter()
        .flat_map_iter(|idx| {
            let out_y = idx / width;
            let out_x = idx % width;
            let src_x = out_x as f32 - dx;
            let src_y = out_y as f32 - dy;
            let x0 = src_x.floor() as i32;
            let y0 = src_y.floor() as i32;
            let fx  = src_x - x0 as f32;
            let fy  = src_y - y0 as f32;
            (0..3).map(move |ch| {
                bilinear_rgb(rgb, width, height, x0, y0, x0 + 1, y0 + 1, fx, fy, ch)
            })
        })
        .collect()
}

fn resample_frame_rgb_affine(
    rgb:    &[f32],
    width:  usize,
    height: usize,
    xform:  &AffineRigid,
) -> Vec<f32> {
    (0..height * width)
        .into_par_iter()
        .flat_map_iter(|idx| {
            let out_y = idx / width;
            let out_x = idx % width;
            let (src_x, src_y) = xform.apply_inverse(out_x as f32, out_y as f32);
            let x0 = src_x.floor() as i32;
            let y0 = src_y.floor() as i32;
            let fx  = src_x - x0 as f32;
            let fy  = src_y - y0 as f32;
            (0..3).map(move |ch| {
                bilinear_rgb(rgb, width, height, x0, y0, x0 + 1, y0 + 1, fx, fy, ch)
            })
        })
        .collect()
}

//  ── Bilinear interpolation ────────────────────────────────────────────────────

fn bilinear(
    pixels: &[f32],
    width:  usize,
    height: usize,
    x0:     i32,
    y0:     i32,
    x1:     i32,
    y1:     i32,
    fx:     f32,
    fy:     f32,
) -> f32 {
    let w = width  as i32;
    let h = height as i32;
    let clamp = |x: i32, max: i32| x.clamp(0, max - 1) as usize;
    let p00 = pixels[clamp(y0, h) * width + clamp(x0, w)];
    let p10 = pixels[clamp(y0, h) * width + clamp(x1, w)];
    let p01 = pixels[clamp(y1, h) * width + clamp(x0, w)];
    let p11 = pixels[clamp(y1, h) * width + clamp(x1, w)];
    let top    = p00 * (1.0 - fx) + p10 * fx;
    let bottom = p01 * (1.0 - fx) + p11 * fx;
    top * (1.0 - fy) + bottom * fy
}

fn bilinear_rgb(
    pixels: &[f32],
    width:  usize,
    height: usize,
    x0:     i32,
    y0:     i32,
    x1:     i32,
    y1:     i32,
    fx:     f32,
    fy:     f32,
    ch:     usize,
) -> f32 {
    let w = width  as i32;
    let h = height as i32;
    let clamp = |x: i32, max: i32| x.clamp(0, max - 1) as usize;
    let p00 = pixels[(clamp(y0, h) * width + clamp(x0, w)) * 3 + ch];
    let p10 = pixels[(clamp(y0, h) * width + clamp(x1, w)) * 3 + ch];
    let p01 = pixels[(clamp(y1, h) * width + clamp(x0, w)) * 3 + ch];
    let p11 = pixels[(clamp(y1, h) * width + clamp(x1, w)) * 3 + ch];
    let top    = p00 * (1.0 - fx) + p10 * fx;
    let bottom = p01 * (1.0 - fx) + p11 * fx;
    top * (1.0 - fy) + bottom * fy
}

//  ── Helpers ───────────────────────────────────────────────────────────────────

fn short_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn build_stack_keywords(
    width:   usize,
    height:  usize,
    summary: &Option<crate::analysis::stack_metrics::StackSummary>,
) -> std::collections::HashMap<String, crate::context::KeywordEntry> {
    use crate::context::KeywordEntry;
    let mut kw = std::collections::HashMap::new();
    let mut insert = |name: &str, value: &str, comment: &str| {
        kw.insert(name.to_string(), KeywordEntry::new(name, value, Some(comment)));
    };
    insert("SIMPLE",   "T",                 "file conforms to FITS standard");
    insert("BITPIX",   "-32",               "32-bit floating point");
    insert("NAXIS",    "2",                 "number of axes");
    insert("NAXIS1",   &width.to_string(),  "image width in pixels");
    insert("NAXIS2",   &height.to_string(), "image height in pixels");
    insert("BZERO",    "0",                 "offset for unsigned integers");
    insert("BSCALE",   "1",                 "default scaling factor");
    insert("ROWORDER", "TOP-DOWN",          "row order");
    insert("CREATOR",  "Photyx",            "software that created this file");
    if let Some(s) = summary {
        if let Some(ref target) = s.target  { insert("OBJECT",   target,                                    "target object name"); }
        if let Some(ref filter) = s.filter  { insert("FILTER",   filter,                                    "filter used"); }
        if s.integration_seconds > 0.0      { insert("EXPTIME",  &format!("{:.1}", s.integration_seconds), "total integration time in seconds"); }
        insert("STACKCNT", &s.stacked_frames.to_string(), "number of frames stacked");
    }
    kw
}

fn build_flat_keywords(
    width:    usize,
    height:   usize,
    n_frames: usize,
) -> std::collections::HashMap<String, crate::context::KeywordEntry> {
    use crate::context::KeywordEntry;
    let mut kw = std::collections::HashMap::new();
    let mut insert = |name: &str, value: &str, comment: &str| {
        kw.insert(name.to_string(), KeywordEntry::new(name, value, Some(comment)));
    };
    insert("SIMPLE",   "T",                    "file conforms to FITS standard");
    insert("BITPIX",   "-32",                  "32-bit floating point");
    insert("NAXIS",    "2",                    "number of axes");
    insert("NAXIS1",   &width.to_string(),     "image width in pixels");
    insert("NAXIS2",   &height.to_string(),    "image height in pixels");
    insert("BZERO",    "0",                    "offset for unsigned integers");
    insert("BSCALE",   "1",                    "default scaling factor");
    insert("ROWORDER", "TOP-DOWN",             "row order");
    insert("CREATOR",  "Photyx",               "software that created this file");
    insert("IMAGETYP", "Flat Field",           "frame type");
    insert("STACKCNT", &n_frames.to_string(),  "number of frames combined");
    kw
}

//  ── Alignment validation (retained for future use) ────────────────────────────

#[allow(dead_code)]
fn validate_alignment(
    frame_stars:    &[crate::analysis::stars::StarCandidate],
    ref_stars:      &[crate::analysis::stars::StarCandidate],
    dx:             f32,
    dy:             f32,
    tolerance:      f32,
    min_match_rate: f32,
) -> bool {
    let sample: Vec<_> = frame_stars.iter().take(20).collect();
    if sample.is_empty() { return false; }
    let matched = sample.iter().filter(|s| {
        let pred_x = s.cx + dx;
        let pred_y = s.cy + dy;
        ref_stars.iter().any(|r| {
            let d = ((r.cx - pred_x).powi(2) + (r.cy - pred_y).powi(2)).sqrt();
            d <= tolerance
        })
    }).count();
    (matched as f32 / sample.len() as f32) >= min_match_rate
}

// ----------------------------------------------------------------------
