// plugins/stack_frames.rs — StackFrames built-in plugin
// Stacking document §3.5, §3.11
//
// Stacking pipeline:
//   Pass 1: Stream frames sequentially, FFT-align each, accumulate into
//           per-pixel Welford running mean/variance buffers. Cache translations.
//   Between passes: Compute per-pixel stddev from Welford M2/count.
//   Pass 2: Stream frames again using cached translations, sigma-clip each
//           pixel against pass-1 mean/stddev, accumulate clipped sum/count.
//   Final: Divide clipped sum by count, normalize to 0.0–1.0.
//
// Rayon parallelism is used for all per-pixel work within each frame.
// Frame loops are sequential (FFT alignment requires reference luma).
// Memory: 5 f32 buffers × n_pixels — no per-frame sample accumulation.

use crate::analysis::{
    self,
    background::estimate_background,
    eccentricity::compute_eccentricity,
    fft_align::compute_translation,
    fwhm::compute_fwhm,
    stars::detect_stars,
    stack_metrics::{ExclusionReason, FrameContribution, StackSummary},
    SigmaClipConfig, StarDetectionConfig,
};
use crate::context::{AppContext, BitDepth, ColorSpace, ImageBuffer, PixelData};
use crate::plugin::{ArgMap, ParamSpec, PhotonPlugin, PluginError, PluginOutput};
use chrono::Utc;
use rayon::prelude::*;
use tracing::info;

pub struct StackFrames;

impl PhotonPlugin for StackFrames {
    fn name(&self) -> &str { "StackFrames" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str {
        "Stacks all loaded frames using two-pass sigma-clipped mean with FFT phase correlation alignment."
    }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded."));
        }

        ctx.clear_stack();

        // ── Step 1: Collect frame snapshots (metadata + stars, no pixel duplication) ──
        let det_config = StarDetectionConfig::default();

        struct FrameSnapshot {
            index:        usize,
            path:         String,
            width:        usize,
            height:       usize,
            channels:     usize,
            color_space:  ColorSpace,
            filter:       Option<String>,
            exptime:      Option<f32>,
            fwhm:         Option<f32>,
            eccentricity: Option<f32>,
            rotator:      Option<f32>,
            stars:        Vec<crate::analysis::stars::StarCandidate>,
        }

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

            let (cached_fwhm, cached_ecc) = if let Some(ar) = ctx.analysis_results.get(path) {
                (ar.fwhm, ar.eccentricity)
            } else {
                (None, None)
            };

            let filter  = buf.keywords.get("FILTER").map(|kw| kw.value.clone());
            let exptime = buf.keywords.get("EXPTIME")
                .and_then(|kw| kw.value.trim().parse::<f32>().ok());
            let rotator = buf.keywords.get("ROTATOR")
                .and_then(|kw| kw.value.trim().parse::<f32>().ok());

            let channels = buf.channels as usize;
            let width    = buf.width    as usize;
            let height   = buf.height   as usize;
            let luma     = analysis::to_luminance(pixels, channels);
            let stars    = detect_stars(&luma, width, height, &det_config);

            let fwhm        = cached_fwhm.or_else(|| compute_fwhm(&stars, None).map(|r| r.fwhm_pixels));
            let eccentricity = cached_ecc.or_else(|| compute_eccentricity(&stars).map(|r| r.eccentricity));

            snapshots.push(FrameSnapshot {
                index,
                path: path.clone(),
                width,
                height,
                channels,
                color_space: buf.color_space.clone(),
                filter,
                exptime,
                rotator,
                fwhm,
                eccentricity,
                stars,
            });
        }

        if snapshots.is_empty() {
            return Err(PluginError::new("NO_PIXELS", "No frames with pixel data available."));
        }

        let width    = snapshots[0].width;
        let height   = snapshots[0].height;
        let n_pixels = width * height;

        // ── Step 2: Select reference frame ───────────────────────────────────
        let fwhm_ecc: Vec<(Option<f32>, Option<f32>)> = snapshots.iter()
            .map(|s| (s.fwhm, s.eccentricity))
            .collect();
        let ref_idx         = select_reference_frame_idx(&fwhm_ecc);
        let ref_filter      = snapshots[ref_idx].filter.clone();
        let ref_color_space = snapshots[ref_idx].color_space.clone();
        let is_bayer        = ref_color_space == ColorSpace::Bayer;

        info!("StackFrames: reference frame index {} ({})", ref_idx, short_name(&snapshots[ref_idx].path));
        if let Some(f) = &ref_filter {
            info!("StackFrames: stack filter = {}", f);
        }

        // Load reference luma from buffer pool
        let ref_luma = ctx.image_buffers.get(&snapshots[ref_idx].path)
            .and_then(|b| b.pixels.as_ref())
            .map(|p| analysis::to_luminance(p, snapshots[ref_idx].channels))
            .ok_or_else(|| PluginError::new("NO_REF", "Reference frame pixel data unavailable."))?;

        let ref_stars = &snapshots[ref_idx].stars;

        let ref_path   = snapshots[ref_idx].path.clone();
        let ref_target = ctx.image_buffers.get(&ref_path)
            .and_then(|b| b.keywords.get("OBJECT"))
            .map(|kw| kw.value.clone());

        // ── Step 3: Pass 1 — Welford accumulation ────────────────────────────
        let mut mean_buf:  Vec<f32> = vec![0.0; n_pixels];
        let mut m2_buf:    Vec<f32> = vec![0.0; n_pixels];
        let mut count_buf: Vec<u32> = vec![0;   n_pixels];

        // Cache per-frame results for pass 2
        let mut cached_translations: Vec<Option<(f32, f32)>> = vec![None; snapshots.len()];

        let bg_sigma_config   = SigmaClipConfig::default();
        let mut contributions: Vec<FrameContribution> = Vec::new();
        let mut messages:      Vec<String>            = Vec::new();
        let     total          = snapshots.len();
        let mut total_integration = 0.0f32;

        // Flip reference state
        let mut flip_ref_luma:  Option<Vec<f32>>                                   = None;
        let mut flip_ref_stars: Option<Vec<crate::analysis::stars::StarCandidate>> = None;
        let mut flip_ref_dx:    f32 = 0.0;
        let mut flip_ref_dy:    f32 = 0.0;

        for (i, snap) in snapshots.iter().enumerate() {
            let mut contrib = FrameContribution::new(snap.index, &snap.path);
            contrib.filter       = snap.filter.clone();
            contrib.fwhm         = snap.fwhm;
            contrib.eccentricity = snap.eccentricity;

            // ── Filter validation ─────────────────────────────────────────────
            if let (Some(ref rf), Some(ref sf)) = (&ref_filter, &snap.filter) {
                if rf != sf && i != ref_idx {
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

            // ── Load frame luma from buffer pool ──────────────────────────────
            let raw_luma = match ctx.image_buffers.get(&snap.path)
                .and_then(|b| b.pixels.as_ref())
                .map(|p| analysis::to_luminance(p, snap.channels))
            {
                Some(l) => l,
                None    => continue,
            };

            // ── Meridian flip detection ───────────────────────────────────────
            let (luma, frame_stars) = if i != ref_idx {
                let ref_rotator = snapshots[ref_idx].rotator;
                match (ref_rotator, snap.rotator) {
                    (Some(ref_rot), Some(frame_rot)) => {
                        let delta   = (frame_rot - ref_rot).rem_euclid(360.0);
                        let flipped = (delta - 180.0).abs() < 10.0;
                        if flipped {
                            contrib.meridian_flipped = true;
                            let msg = format!(
                                "Meridian flip detected: frame {} (ROTATOR {:.1}° vs ref {:.1}°) — pre-rotated 180°",
                                snap.index, frame_rot, ref_rot
                            );
                            info!("{}", msg);
                            messages.push(msg);
                            let mut fl = raw_luma;
                            fl.reverse();
                            let fs = detect_stars(&fl, snap.width, snap.height, &det_config);
                            (fl, fs)
                        } else {
                            (raw_luma, snap.stars.clone())
                        }
                    }
                    (None, _) | (_, None) => {
                        if snap.rotator.is_none() {
                            let msg = format!(
                                "ROTATOR keyword missing: frame {} — assuming no flip",
                                snap.index
                            );
                            info!("{}", msg);
                            messages.push(msg);
                        }
                        (raw_luma, snap.stars.clone())
                    }
                }
            } else {
                (raw_luma, snap.stars.clone())
            };

            // ── Normalize by sigma-clipped background ─────────────────────────
            let bg_est   = estimate_background(&luma, &bg_sigma_config);
            let bg_level = bg_est.median;
            contrib.background_level = Some(bg_level);
            let divisor  = if bg_level > 1e-6 { bg_level } else { 1.0 };

            let normalized: Vec<f32> = luma.par_iter().map(|&v| v / divisor).collect();

            // ── FFT alignment ─────────────────────────────────────────────────
            let translation: Option<(f32, f32)> = if i == ref_idx {
                contrib.fft_translation    = Some((0.0, 0.0));
                contrib.alignment_validated = Some(true);
                Some((0.0, 0.0))

            } else if contrib.meridian_flipped {
                match flip_ref_luma.as_ref() {
                    None => {
                        match compute_translation(&ref_luma, &normalized, width, height) {
                            Some(t) => {
                                info!(
                                    "Frame {} (first flip): FFT vs main ref dx={:.2} dy={:.2}",
                                    snap.index, t.dx, t.dy
                                );
                                contrib.fft_translation    = Some((t.dx, t.dy));
                                contrib.alignment_validated = Some(true);
                                flip_ref_luma   = Some(normalized.clone());
                                flip_ref_stars  = Some(frame_stars.clone());
                                flip_ref_dx     = t.dx;
                                flip_ref_dy     = t.dy;
                                Some((t.dx, t.dy))
                            }
                            None => {
                                let msg = format!(
                                    "Alignment failed: frame {} — FFT returned no result, skipped",
                                    snap.index
                                );
                                info!("{}", msg);
                                messages.push(msg);
                                contrib.exclusion_reason = Some(ExclusionReason::AlignmentFailed);
                                contributions.push(contrib);
                                continue;
                            }
                        }
                    }
                    Some(flip_ref_norm) => {
                        match compute_translation(flip_ref_norm, &normalized, width, height) {
                            Some(t) => {
                                let total_dx = flip_ref_dx + t.dx;
                                let total_dy = flip_ref_dy + t.dy;
                                info!(
                                    "Frame {} (flip): FFT vs flip-ref dx={:.2} dy={:.2} → total dx={:.2} dy={:.2}",
                                    snap.index, t.dx, t.dy, total_dx, total_dy
                                );
                                let frs       = flip_ref_stars.as_deref().unwrap_or(&[]);
                                let validated = validate_alignment(
                                    &frame_stars, frs, t.dx, t.dy, 3.0, 0.5
                                );
                                contrib.fft_translation    = Some((total_dx, total_dy));
                                contrib.alignment_validated = Some(validated);
                                if validated {
                                    Some((total_dx, total_dy))
                                } else {
                                    let msg = format!(
                                        "Alignment failed: frame {} — star position check failed, skipped",
                                        snap.index
                                    );
                                    info!("{}", msg);
                                    messages.push(msg);
                                    contrib.exclusion_reason = Some(ExclusionReason::AlignmentFailed);
                                    contributions.push(contrib);
                                    continue;
                                }
                            }
                            None => {
                                let msg = format!(
                                    "Alignment failed: frame {} — FFT returned no result, skipped",
                                    snap.index
                                );
                                info!("{}", msg);
                                messages.push(msg);
                                contrib.exclusion_reason = Some(ExclusionReason::AlignmentFailed);
                                contributions.push(contrib);
                                continue;
                            }
                        }
                    }
                }

            } else {
                match compute_translation(&ref_luma, &normalized, width, height) {
                    Some(t) => {
                        let validated = validate_alignment(
                            &frame_stars, ref_stars, t.dx, t.dy, 3.0, 0.5
                        );
                        contrib.fft_translation    = Some((t.dx, t.dy));
                        contrib.alignment_validated = Some(validated);
                        if validated {
                            Some((t.dx, t.dy))
                        } else {
                            let msg = format!(
                                "Alignment failed: frame {} — star position check failed, skipped",
                                snap.index
                            );
                            info!("{}", msg);
                            messages.push(msg);
                            contrib.exclusion_reason = Some(ExclusionReason::AlignmentFailed);
                            contributions.push(contrib);
                            continue;
                        }
                    }
                    None => {
                        let msg = format!(
                            "Alignment failed: frame {} — FFT returned no result, skipped",
                            snap.index
                        );
                        info!("{}", msg);
                        messages.push(msg);
                        contrib.exclusion_reason = Some(ExclusionReason::AlignmentFailed);
                        contributions.push(contrib);
                        continue;
                    }
                }
            };

            // ── Resample and Welford-accumulate (Rayon parallel across pixels) ─
            let (dx, dy) = translation.unwrap();
            let aligned  = resample_frame(&normalized, width, height, dx, dy);

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

            cached_translations[i] = Some((dx, dy));

            contrib.included = true;
            if let Some(et) = snap.exptime {
                total_integration += et;
            }

            let pct = ((i + 1) as f32 / total as f32 * 100.0).round() as u32;
            messages.push(format!("Pass 1 — frame {} / {} ({}%)…", i + 1, total, pct));
            contributions.push(contrib);
        }

        let stacked_count = contributions.iter().filter(|c| c.included).count();

        if stacked_count == 0 {
            return Err(PluginError::new(
                "NO_FRAMES_STACKED",
                "No frames could be stacked — all were excluded by filter mismatch or alignment failure.",
            ));
        }

        // ── Between passes: compute per-pixel stddev from Welford M2 ─────────
        let stddev_buf: Vec<f32> = count_buf.par_iter()
            .zip(m2_buf.par_iter())
            .map(|(&count, &m2)| {
                if count > 1 { (m2 / count as f32).sqrt() } else { 0.0 }
            })
            .collect();

        // ── Step 4: Pass 2 — sigma-clipped accumulation ───────────────────────
        let sigma          = 2.5f32;
        let mut sum_buf:   Vec<f64> = vec![0.0; n_pixels];
        let mut clip_count: Vec<u32> = vec![0;  n_pixels];

        for (i, snap) in snapshots.iter().enumerate() {
            let (dx, dy) = match cached_translations[i] {
                Some(t) => t,
                None    => continue,
            };

            // Load luma and apply flip if needed
            let raw_luma = match ctx.image_buffers.get(&snap.path)
                .and_then(|b| b.pixels.as_ref())
                .map(|p| analysis::to_luminance(p, snap.channels))
            {
                Some(l) => l,
                None    => continue,
            };

            let is_flipped = contributions.iter()
                .find(|c| c.frame_index == snap.index && c.included)
                .map(|c| c.meridian_flipped)
                .unwrap_or(false);

            let luma = if is_flipped {
                let mut fl = raw_luma;
                fl.reverse();
                fl
            } else {
                raw_luma
            };

            let bg_est   = estimate_background(&luma, &bg_sigma_config);
            let bg_level = bg_est.median;
            let divisor  = if bg_level > 1e-6 { bg_level } else { 1.0 };

            let normalized: Vec<f32> = luma.par_iter().map(|&v| v / divisor).collect();
            let aligned = resample_frame(&normalized, width, height, dx, dy);

            // Sigma-clip and accumulate — Rayon parallel across pixels
            sum_buf.par_iter_mut()
                .zip(clip_count.par_iter_mut())
                .zip(aligned.par_iter())
                .zip(mean_buf.par_iter())
                .zip(stddev_buf.par_iter())
                .for_each(|((((sum, count), &val), &mean), &sd)| {
                    let threshold = sigma * sd;
                    if sd < 1e-10 || (val - mean).abs() <= threshold {
                        *sum   += val as f64;
                        *count += 1;
                    }
                });

            let pct = ((i + 1) as f32 / total as f32 * 100.0).round() as u32;
            messages.push(format!("Pass 2 — frame {} / {} ({}%)…", i + 1, total, pct));
        }

        // ── Step 5: Build final pixel buffer ──────────────────────────────────
        let raw_pixels: Vec<f32> = sum_buf.par_iter()
            .zip(clip_count.par_iter())
            .map(|(&sum, &count)| {
                if count > 0 { sum as f32 / count as f32 } else { 0.0 }
            })
            .collect();

        let max_val = raw_pixels.par_iter().cloned().reduce(|| f32::NEG_INFINITY, f32::max);
        let min_val = raw_pixels.par_iter().cloned().reduce(|| f32::INFINITY,     f32::min);
        let range   = (max_val - min_val).max(1e-6);

        let stack_pixels: Vec<f32> = raw_pixels.par_iter()
            .map(|&v| ((v - min_val) / range).clamp(0.0, 1.0))
            .collect();

        // ── Step 6: Build output ImageBuffer ─────────────────────────────────
        let completed_at      = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let timestamp_display = Utc::now().format("%Y-%m-%d %H:%M").to_string();

        let stack_label = format!(
            "STACKED RESULT \u{2014} {} / {} frames \u{2014} {}",
            stacked_count, total, timestamp_display
        );

        let output_color_space = if is_bayer { ColorSpace::Mono } else { ColorSpace::Mono };

        let stack_buf = ImageBuffer {
            filename:      stack_label.clone(),
            width:         width  as u32,
            height:        height as u32,
            display_width: width  as u32,
            bit_depth:     BitDepth::F32,
            color_space:   output_color_space,
            channels:      1,
            keywords:      std::collections::HashMap::new(),
            pixels:        Some(PixelData::F32(stack_pixels)),
        };

        ctx.stack_result = Some(stack_buf);

        // ── Step 7: Compute and store summary ────────────────────────────────
        let mut summary = StackSummary::compute(&contributions, &completed_at);
        summary.target              = ref_target;
        summary.filter              = ref_filter;
        summary.integration_seconds = total_integration;

        ctx.stack_contributions = contributions;
        ctx.stack_summary       = Some(summary.clone());

        // ── Step 8: Build console output ─────────────────────────────────────
        let quality_summary = format!(
            "Stack Quality Summary:\n  Frames stacked:        {} / {}\n  SNR improvement:       ~{:.1}x (vs single frame)\n  Alignment success:     {:.1}%\n  Background uniformity: {}",
            summary.stacked_frames,
            summary.total_frames,
            summary.snr_improvement,
            summary.alignment_success_rate * 100.0,
            summary.background_uniformity,
        );

        messages.push(format!(
            "Stacking complete \u{2014} {} / {} frames stacked",
            stacked_count, total
        ));
        messages.push(quality_summary);

        let full_message = messages.join("\n");
        info!("StackFrames: {}", full_message);

        Ok(PluginOutput::Data(serde_json::json!({
            "plugin":          "StackFrames",
            "stacked_frames":  stacked_count,
            "total_frames":    total,
            "message":         full_message,
            "stack_available": true,
        })))
    }
}

// ── Reference frame selection ─────────────────────────────────────────────────

fn select_reference_frame_idx(fwhm_ecc: &[(Option<f32>, Option<f32>)]) -> usize {
    fwhm_ecc
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let fa = a.0.unwrap_or(f32::MAX);
            let fb = b.0.unwrap_or(f32::MAX);
            fa.partial_cmp(&fb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ea = a.1.unwrap_or(f32::MAX);
                    let eb = b.1.unwrap_or(f32::MAX);
                    ea.partial_cmp(&eb).unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

// ── Frame resampling with sub-pixel translation ───────────────────────────────
// Returns an aligned output buffer using bilinear interpolation.
// Rayon parallel across pixels.

fn resample_frame(
    normalized: &[f32],
    width:      usize,
    height:     usize,
    dx:         f32,
    dy:         f32,
) -> Vec<f32> {
    (0..height * width)
        .into_par_iter()
        .map(|idx| {
            let out_y = idx / width;
            let out_x = idx % width;
            let src_x = out_x as f32 - dx;
            let src_y = out_y as f32 - dy;
            let x0    = src_x.floor() as i32;
            let y0    = src_y.floor() as i32;
            let fx    = src_x - x0 as f32;
            let fy    = src_y - y0 as f32;
            bilinear(normalized, width, height, x0, y0, x0 + 1, y0 + 1, fx, fy)
        })
        .collect()
}

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

// ── Alignment validation ──────────────────────────────────────────────────────

fn validate_alignment(
    frame_stars:    &[crate::analysis::stars::StarCandidate],
    ref_stars:      &[crate::analysis::stars::StarCandidate],
    dx:             f32,
    dy:             f32,
    tolerance:      f32,
    min_match_rate: f32,
) -> bool {
    let sample: Vec<_> = frame_stars.iter().take(20).collect();
    if sample.is_empty() {
        return false;
    }

    let matched = sample.iter().filter(|s| {
        let predicted_x = s.cx + dx;
        let predicted_y = s.cy + dy;
        ref_stars.iter().any(|r| {
            let dist = ((r.cx - predicted_x).powi(2) + (r.cy - predicted_y).powi(2)).sqrt();
            dist <= tolerance
        })
    }).count();

    (matched as f32 / sample.len() as f32) >= min_match_rate
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn short_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
