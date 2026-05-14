// plugins/stack_frames.rs — StackFrames built-in plugin
// Stacking document §3.5, §3.11

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
use tracing::info;

pub struct StackFrames;

impl PhotonPlugin for StackFrames {
    fn name(&self) -> &str { "StackFrames" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str {
        "Stacks all loaded frames using sigma-clipped mean with FFT phase correlation alignment."
    }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded."));
        }

        // Clear any previous stack result before starting
        ctx.clear_stack();

        // ── Step 1: Collect frame snapshots ──────────────────────────────────
        struct FrameSnapshot {
            index:    usize,
            path:     String,
            width:    usize,
            height:   usize,
            channels: usize,
            pixels:   PixelData,
            color_space: ColorSpace,
            filter:   Option<String>,
            exptime:  Option<f32>,
            fwhm:     Option<f32>,
            eccentricity: Option<f32>,
        }

        let mut snapshots: Vec<FrameSnapshot> = Vec::new();
        for (index, path) in ctx.file_list.iter().enumerate() {
            let buf = match ctx.image_buffers.get(path) {
                Some(b) => b,
                None    => continue,
            };
            let pixels = match &buf.pixels {
                Some(p) => p.clone(),
                None    => continue,
            };

            // Pull cached analysis metrics if available
            let (cached_fwhm, cached_ecc) = if let Some(ar) = ctx.analysis_results.get(path) {
                (ar.fwhm, ar.eccentricity)
            } else {
                (None, None)
            };

            let filter = buf.keywords.get("FILTER").map(|kw| kw.value.clone());
            let exptime = buf.keywords.get("EXPTIME")
                .and_then(|kw| kw.value.trim().parse::<f32>().ok());

            snapshots.push(FrameSnapshot {
                index,
                path: path.clone(),
                width:  buf.width  as usize,
                height: buf.height as usize,
                channels: buf.channels as usize,
                pixels,
                color_space: buf.color_space.clone(),
                filter,
                exptime,
                fwhm: cached_fwhm,
                eccentricity: cached_ecc,
            });
        }

        if snapshots.is_empty() {
            return Err(PluginError::new("NO_PIXELS", "No frames with pixel data available."));
        }

        let width  = snapshots[0].width;
        let height = snapshots[0].height;
        let n_pixels = width * height;

        // ── Step 2: Compute FWHM/eccentricity for frames missing cached values ──
        let det_config = StarDetectionConfig::default();
        for snap in &mut snapshots {
            if snap.fwhm.is_none() || snap.eccentricity.is_none() {
                let luma = analysis::to_luminance(&snap.pixels, snap.channels);
                let stars = detect_stars(&luma, snap.width, snap.height, &det_config);
                if snap.fwhm.is_none() {
                    snap.fwhm = compute_fwhm(&stars, None).map(|r| r.fwhm_pixels);
                }
                if snap.eccentricity.is_none() {
                    snap.eccentricity = compute_eccentricity(&stars).map(|r| r.eccentricity);
                }
            }
        }

        // ── Step 3: Select reference frame (lowest FWHM, eccentricity as tiebreaker) ──
        let fwhm_ecc: Vec<(Option<f32>, Option<f32>)> = snapshots.iter()
            .map(|s| (s.fwhm, s.eccentricity))
            .collect();
        let ref_idx = select_reference_frame_idx(&fwhm_ecc);
        let ref_filter = snapshots[ref_idx].filter.clone();
        let ref_color_space = snapshots[ref_idx].color_space.clone();
        let is_bayer = ref_color_space == ColorSpace::Bayer;

        info!("StackFrames: reference frame index {} ({})", ref_idx, short_name(&snapshots[ref_idx].path));
        if let Some(f) = &ref_filter {
            info!("StackFrames: stack filter = {}", f);
        }

        // Extract reference luminance for FFT alignment
        let ref_luma = analysis::to_luminance(&snapshots[ref_idx].pixels, snapshots[ref_idx].channels);

        // Pull OBJECT and EXPTIME from reference frame buffer for summary
        let ref_path = snapshots[ref_idx].path.clone();
        let ref_target = ctx.image_buffers.get(&ref_path)
            .and_then(|b| b.keywords.get("OBJECT"))
            .map(|kw| kw.value.clone());

        // ── Step 4: Per-frame: filter validate, normalize, align, accumulate ──
        let bg_sigma_config = SigmaClipConfig::default();

        // Accumulation buffers: sum and count per pixel
        let mut sum_buf   = vec![0.0f64; n_pixels];
        let mut count_buf = vec![0u32;   n_pixels];

        // For sigma clipping we collect per-pixel sample vectors.
        // At 60-180 frames and typical image sizes this is the memory-intensive
        // step. We use a flat Vec<Vec<f32>> indexed by pixel position.
        // Each inner Vec grows as frames are accumulated.
        let mut pixel_samples: Vec<Vec<f32>> = vec![Vec::new(); n_pixels];

        let mut contributions: Vec<FrameContribution> = Vec::new();
        let mut messages: Vec<String> = Vec::new();
        let total = snapshots.len();
        let mut stacked_count = 0usize;
        let mut total_integration = 0.0f32;

        for (i, snap) in snapshots.iter().enumerate() {
            let mut contrib = FrameContribution::new(snap.index, &snap.path);
            contrib.filter     = snap.filter.clone();
            contrib.fwhm       = snap.fwhm;
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

            // ── Normalize by sigma-clipped background ─────────────────────────
            let luma = analysis::to_luminance(&snap.pixels, snap.channels);
            let bg_est = estimate_background(&luma, &bg_sigma_config);
            let bg_level = bg_est.median;
            contrib.background_level = Some(bg_level);

            let divisor = if bg_level > 1e-6 { bg_level } else { 1.0 };
            let normalized: Vec<f32> = luma.iter().map(|&v| v / divisor).collect();

            // ── FFT alignment ─────────────────────────────────────────────────
            let translation = if i == ref_idx {
                // Reference frame has zero translation by definition
                contrib.fft_translation    = Some((0.0, 0.0));
                contrib.alignment_validated = Some(true);
                Some((0.0f32, 0.0f32))
            } else {
                match compute_translation(&ref_luma, &normalized, width, height) {
                    Some(t) => {
                        contrib.fft_translation = Some((t.dx, t.dy));

                        // ── Alignment validation via star positions ───────────
                        let validated = validate_alignment(
                            &ref_luma,
                            &normalized,
                            width,
                            height,
                            t.dx,
                            t.dy,
                            &det_config,
                        );
                        contrib.alignment_validated = Some(validated);

                        if validated {
                            Some((t.dx, t.dy))
                        } else {
                            let msg = format!(
                                "Alignment failed: frame {} — skipped",
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

            // ── Accumulate into per-pixel sample vectors ──────────────────────
            let (dx, dy) = translation.unwrap();
            accumulate_frame(&normalized, &mut pixel_samples, width, height, dx, dy);

            contrib.included = true;
            if let Some(et) = snap.exptime {
                total_integration += et;
            }
            stacked_count += 1;

            let pct = ((i + 1) as f32 / total as f32 * 100.0).round() as u32;
            let msg = format!("Stacking frame {} / {} ({}%)\u{2026}", i + 1, total, pct);
            messages.push(msg);

            contributions.push(contrib);
        }

        if stacked_count == 0 {
            return Err(PluginError::new(
                "NO_FRAMES_STACKED",
                "No frames could be stacked — all were excluded by filter mismatch or alignment failure.",
            ));
        }

        // ── Step 5: Sigma-clipped mean per pixel ──────────────────────────────
        let sigma = 2.5f32;
        for (i, samples) in pixel_samples.iter().enumerate() {
            if samples.is_empty() {
                continue;
            }
            let clipped_mean = sigma_clipped_mean(samples, sigma);
            sum_buf[i]   = clipped_mean as f64;
            count_buf[i] = 1; // sentinel — pixel is valid
        }

        // Normalize output to 0.0–1.0 range
        let stack_pixels: Vec<f32> = (0..n_pixels)
            .map(|i| if count_buf[i] > 0 { sum_buf[i] as f32 } else { 0.0 })
            .collect();

        // ── Step 6: Build output ImageBuffer ─────────────────────────────────
        let completed_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let timestamp_display = Utc::now().format("%Y-%m-%d %H:%M").to_string();

        let stack_label = format!(
            "STACKED RESULT \u{2014} {} frames \u{2014} {}",
            stacked_count, timestamp_display
        );

        let output_color_space = if is_bayer {
            // Bayer stacked as mono → debayer result is RGB, but we keep mono
            // for now and note it in the label. Full debayer is a future enhancement.
            ColorSpace::Mono
        } else {
            ColorSpace::Mono
        };

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
        summary.target               = ref_target;
        summary.filter               = ref_filter;
        summary.integration_seconds  = total_integration;

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
// Lowest FWHM wins; eccentricity as tiebreaker.
// Frames with no FWHM measurement are sorted last.

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

// ── Alignment validation ──────────────────────────────────────────────────────
// After computing an FFT translation, verify it by checking that a sample of
// bright stars in the target frame land within tolerance of their predicted
// positions after applying the translation.

fn validate_alignment(
    ref_luma:   &[f32],
    tgt_luma:   &[f32],
    width:      usize,
    height:     usize,
    dx:         f32,
    dy:         f32,
    det_config: &StarDetectionConfig,
) -> bool {
    let tolerance_px = 3.0f32;
    let min_validated = 3usize;

    // Detect stars in reference and target
    let ref_stars = detect_stars(ref_luma, width, height, det_config);
    let tgt_stars = detect_stars(tgt_luma, width, height, det_config);

    if ref_stars.is_empty() || tgt_stars.is_empty() {
        // No stars detectable — can't validate; accept the translation
        return true;
    }

    // Take up to 20 brightest reference stars
    let sample: Vec<_> = ref_stars.iter().take(20).collect();

    let mut matched = 0usize;
    for rs in &sample {
        // Predicted position of this star in the target frame
        let pred_x = rs.cx + dx;
        let pred_y = rs.cy + dy;

        // Find nearest target star to predicted position
        let nearest = tgt_stars.iter().min_by(|a, b| {
            let da = (a.cx - pred_x).hypot(a.cy - pred_y);
            let db = (b.cx - pred_x).hypot(b.cy - pred_y);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });

        if let Some(ns) = nearest {
            let dist = (ns.cx - pred_x).hypot(ns.cy - pred_y);
            if dist <= tolerance_px {
                matched += 1;
            }
        }
    }

    matched >= min_validated.min(sample.len())
}

// ── Frame accumulation with sub-pixel translation ─────────────────────────────
// Resamples the normalized frame into the stack accumulation buffer using
// bilinear interpolation at the computed (dx, dy) offset.

fn accumulate_frame(
    normalized:    &[f32],
    pixel_samples: &mut Vec<Vec<f32>>,
    width:         usize,
    height:        usize,
    dx:            f32,
    dy:            f32,
) {
    for out_y in 0..height {
        for out_x in 0..width {
            // Source coordinates in the input frame
            let src_x = out_x as f32 - dx;
            let src_y = out_y as f32 - dy;

            // Bilinear interpolation
            let x0 = src_x.floor() as i32;
            let y0 = src_y.floor() as i32;
            let x1 = x0 + 1;
            let y1 = y0 + 1;

            let fx = src_x - x0 as f32;
            let fy = src_y - y0 as f32;

            let sample = bilinear(normalized, width, height, x0, y0, x1, y1, fx, fy);
            pixel_samples[out_y * width + out_x].push(sample);
        }
    }
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

// ── Sigma-clipped mean ────────────────────────────────────────────────────────

fn sigma_clipped_mean(samples: &[f32], sigma: f32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    if samples.len() == 1 {
        return samples[0];
    }

    let mut working: Vec<f32> = samples.to_vec();

    for _ in 0..5 {
        if working.is_empty() {
            break;
        }
        let n    = working.len() as f32;
        let mean = working.iter().sum::<f32>() / n;
        let var  = working.iter().map(|&x| (x - mean) * (x - mean)).sum::<f32>() / n;
        let sd   = var.sqrt();

        if sd < 1e-10 {
            break;
        }

        let lo = mean - sigma * sd;
        let hi = mean + sigma * sd;
        let before = working.len();
        working.retain(|&x| x >= lo && x <= hi);
        if working.len() == before {
            break;
        }
    }

    if working.is_empty() {
        return samples.iter().sum::<f32>() / samples.len() as f32;
    }

    working.iter().sum::<f32>() / working.len() as f32
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn short_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
