// plugins/stack_frames.rs — StackFrames built-in plugin
//
// Two-pass stacking with FFT phase correlation + RANSAC affine rigid
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
//      without a flip there is a single group.
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
//      with the FFT + RANSAC result. Because both inputs are debayered RGB
//      luma, the FFT correlation is clean.
//
//   6. Per-frame final transform: T = M_cross ∘ G
//      where G is the within-group transform (FFT + RANSAC against group ref).
//      For master-group frames, M_cross = identity, so T = G.
//
//   7. Color awareness: if the master reference frame is Bayer or RGB, the
//      stack accumulates all three RGB channels and outputs ColorSpace::RGB.
//      Mono input produces a grayscale output as before.

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
use crate::plugin::{ArgMap, ParamSpec, PhotonPlugin, PluginError, PluginOutput};
use chrono::Utc;
use rayon::prelude::*;
use tracing::info;

/// Rotation magnitude (radians) below which we skip the affine resampler.
const MIN_ROTATION_TO_APPLY: f32 = 0.001;

/// ROTATOR delta tolerance (degrees) for grouping frames.
const ROTATOR_GROUP_TOLERANCE: f32 = 10.0;

/// Gap in minutes between consecutive frames that indicates a new imaging session.
const SESSION_GAP_MINUTES: f32 = 120.0;

pub struct StackFrames;

// ── Snapshot type ─────────────────────────────────────────────────────────────

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
        "Stacks loaded frames with FFT alignment, RANSAC rigid refinement, and \
         meridian-flip-aware group reference selection. Debayers per-frame before alignment. \
         Outputs RGB for OSC/color sessions, grayscale for mono sessions."
    }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded."));
        }
        ctx.clear_stack();

        // ── Step 1: Snapshot collection (debayer-first) ───────────────────────
        let det_config = StarDetectionConfig::default();
        let snapshots  = collect_snapshots(ctx, &det_config)?;

        if snapshots.is_empty() {
            return Err(PluginError::new("NO_PIXELS", "No frames with pixel data available."));
        }

        let width    = snapshots[0].width;
        let height   = snapshots[0].height;
        let n_pixels = width * height;
        let total    = snapshots.len();

        // ── Step 2: Group frames by ROTATOR ───────────────────────────────────
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

        // ── Step 3: Designate master group (largest) ──────────────────────────
        let master_group = (0..n_groups)
            .max_by_key(|&g| snapshots.iter().filter(|s| s.group == g).count())
            .unwrap();
        let master_ref_idx = group_refs[master_group];

        info!("StackFrames: master group = {} (reference frame {})",
            master_group, snapshots[master_ref_idx].index);

        let ref_filter      = snapshots[master_ref_idx].filter.clone();
        let ref_color_space = snapshots[master_ref_idx].color_space.clone();

        // Determine output color mode from the master reference frame.
        // Bayer and RGB input → color (RGB) output.
        // Mono input → grayscale output.
        let is_color = ref_color_space == ColorSpace::Bayer
                    || ref_color_space == ColorSpace::RGB;
        let n_channels = if is_color { 3 } else { 1 };

        info!("StackFrames: output mode = {}", if is_color { "RGB (color)" } else { "Mono (grayscale)" });

        let master_ref_luma  = load_debayered_luma(ctx, &snapshots[master_ref_idx])?;
        let master_ref_stars = snapshots[master_ref_idx].stars.clone();

        let ref_path   = snapshots[master_ref_idx].path.clone();
        let ref_target = ctx.image_buffers.get(&ref_path)
            .and_then(|b| b.keywords.get("OBJECT"))
            .map(|kw| kw.value.clone());

        // ── Step 4: Solve M_cross for each non-master group ───────────────────
        let mut m_cross: Vec<AffineRigid> = (0..n_groups).map(|_| AffineRigid::identity()).collect();
        let mut messages: Vec<String> = Vec::new();

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
                        "StackFrames: cross-group {} FFT failed — frames from this group will be excluded",
                        g
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

            let _ = ransac; // triangle solver preferred; RANSAC retained as fallback above

            let flip = AffineRigid::flip_180(width, height);
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

        // ── Step 5: Pass 1 — Welford online mean + M2 ────────────────────────
        //
        // For mono: mean_buf / m2_buf / count_buf are n_pixels long.
        // For color: mean_buf / m2_buf are n_pixels * 3 long (interleaved RGB);
        //            count_buf is n_pixels long (coverage is identical per channel).

        let mut mean_buf:  Vec<f32> = vec![0.0; n_pixels * n_channels];
        let mut m2_buf:    Vec<f32> = vec![0.0; n_pixels * n_channels];
        let mut count_buf: Vec<u32> = vec![0;   n_pixels];

        let mut cached_transforms: Vec<Option<AffineRigid>> = (0..snapshots.len())
            .map(|_| None).collect();

        let mut group_ref_luma: Vec<Option<Vec<f32>>>  = (0..n_groups).map(|_| None).collect();
        let mut group_ref_stars: Vec<Option<Vec<crate::analysis::stars::StarCandidate>>>
            = (0..n_groups).map(|_| None).collect();
        group_ref_luma[master_group]  = Some(master_ref_luma.clone());
        group_ref_stars[master_group] = Some(master_ref_stars.clone());

        let bg_sigma_config = SigmaClipConfig::default();
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

            // Load luma for alignment (always mono for FFT/RANSAC)
            let luma = match load_debayered_luma(ctx, snap) {
                Ok(l) => l,
                Err(_) => continue,
            };

            // Ensure group reference luma is loaded
            if group_ref_luma[snap.group].is_none() {
                let g_ref = &snapshots[group_refs[snap.group]];
                let g_luma = load_debayered_luma(ctx, g_ref)?;
                group_ref_stars[snap.group] = Some(g_ref.stars.clone());
                group_ref_luma[snap.group]  = Some(g_luma);
            }
            let g_ref_luma  = group_ref_luma[snap.group].as_ref().unwrap();
            let g_ref_stars = group_ref_stars[snap.group].as_ref().unwrap();

            // Background normalize luma for alignment
            let bg_est   = estimate_background(&luma, &bg_sigma_config);
            let bg_level = bg_est.median;
            contrib.background_level = Some(bg_level);
            let divisor  = if bg_level > 1e-6 { bg_level } else { 1.0 };
            let normalized_luma: Vec<f32> = luma.par_iter().map(|&v| v / divisor).collect();

            // Within-group transform G
            let g_transform: Option<AffineRigid> = if i == group_refs[snap.group] {
                contrib.fft_translation     = Some((0.0, 0.0));
                contrib.alignment_validated = Some(true);
                Some(AffineRigid::identity())
            } else {
                match compute_translation(g_ref_luma, &normalized_luma, width, height) {
                    Some(t) => {
                        contrib.fft_translation     = Some((t.dx, t.dy));
                        contrib.alignment_validated = Some(true);
                        info!("Frame {}: RANSAC input — {} ref stars, {} frame stars, fft=({:.2},{:.2})",
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

            // Compose: T = M_cross ∘ G
            let g_xform = g_transform.unwrap();
            let t_final = compose(&m_cross[snap.group], &g_xform);

            // Load pixel data for accumulation — mono luma or full RGB
            let frame_pixels = load_frame_pixels(ctx, snap, is_color, divisor);

            // Resample and accumulate
            let theta = t_final.theta();
            if is_color {
                // Resample all three channels using the same spatial transform
                let aligned_rgb = if theta.abs() >= MIN_ROTATION_TO_APPLY || t_final.a < 0.5 {
                    resample_frame_rgb_affine(&frame_pixels, width, height, &t_final)
                } else {
                    resample_frame_rgb(&frame_pixels, width, height, t_final.tx, t_final.ty)
                };

                // Welford accumulation across all channels
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
                    resample_frame_affine(&frame_pixels, width, height, &t_final)
                } else {
                    resample_frame(&frame_pixels, width, height, t_final.tx, t_final.ty)
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

            cached_transforms[i] = Some(t_final);

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
            return Err(PluginError::new("NO_FRAMES_STACKED", "No frames could be stacked."));
        }

        // Per-pixel (per-channel) stddev from Welford M2
        let stddev_buf: Vec<f32> = if is_color {
            // stddev per channel per pixel — same length as mean_buf (n_pixels * 3)
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

        // ── Pass 2 — sigma-clipped accumulation ───────────────────────────────
        let sigma           = 2.5f32;
        let mut sum_buf:    Vec<f64> = vec![0.0; n_pixels * n_channels];
        let mut clip_count: Vec<u32> = vec![0;  n_pixels];

        for (i, snap) in snapshots.iter().enumerate() {
            let xform = match cached_transforms[i].as_ref() {
                Some(x) => x,
                None    => continue,
            };

            let luma = match load_debayered_luma(ctx, snap) {
                Ok(l) => l,
                Err(_) => continue,
            };

            let bg_est   = estimate_background(&luma, &bg_sigma_config);
            let bg_level = bg_est.median;
            let divisor  = if bg_level > 1e-6 { bg_level } else { 1.0 };

            let frame_pixels = load_frame_pixels(ctx, snap, is_color, divisor);

            let theta = xform.theta();
            if is_color {
                let aligned_rgb = if theta.abs() >= MIN_ROTATION_TO_APPLY || xform.a < 0.5 {
                    resample_frame_rgb_affine(&frame_pixels, width, height, xform)
                } else {
                    resample_frame_rgb(&frame_pixels, width, height, xform.tx, xform.ty)
                };

                // Sigma clip decision uses green channel (≈ luma); apply same
                // accept/reject to all three channels per pixel.
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
                let normalized: Vec<f32> = luma.par_iter().map(|&v| v / divisor).collect();
                let aligned = if theta.abs() >= MIN_ROTATION_TO_APPLY || xform.a < 0.5 {
                    resample_frame_affine(&normalized, width, height, xform)
                } else {
                    resample_frame(&normalized, width, height, xform.tx, xform.ty)
                };

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
            }

            let pct = ((i + 1) as f32 / total as f32 * 100.0).round() as u32;
            messages.push(format!("Pass 2 — frame {} / {} ({}%)…", i + 1, total, pct));
        }

        // ── Build output pixels ───────────────────────────────────────────────
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

        // Normalize output to [0, 1] per-channel for color, globally for mono
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

        let quality_summary = format!(
            "Stack Quality Summary:\n  Frames stacked:        {} / {}\n  SNR improvement:       ~{:.1}x (vs single frame)\n  Alignment success:     {:.1}%\n  Background uniformity: {}\n  Output mode:           {}",
            summary.stacked_frames, summary.total_frames,
            summary.snr_improvement, summary.alignment_success_rate * 100.0,
            summary.background_uniformity,
            if is_color { "RGB color" } else { "Grayscale" },
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

// ── Output normalization ──────────────────────────────────────────────────────

/// Normalize stacked pixels to [0, 1].
/// Mono: single global min/max.
/// Color: per-channel min/max to preserve color balance.
fn normalize_output(raw: &[f32], is_color: bool, n_pixels: usize) -> Vec<f32> {
    if !is_color {
        let max_val = raw.par_iter().cloned().reduce(|| f32::NEG_INFINITY, f32::max);
        let min_val = raw.par_iter().cloned().reduce(|| f32::INFINITY,     f32::min);
        let range   = (max_val - min_val).max(1e-6);
        return raw.par_iter().map(|&v| ((v - min_val) / range).clamp(0.0, 1.0)).collect();
    }

    // Color: normalize each channel independently
    let mut out = vec![0.0f32; raw.len()];
    for ch in 0..3 {
        let ch_vals: Vec<f32> = (0..n_pixels).map(|px| raw[px * 3 + ch]).collect();
        let max_val = ch_vals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min_val = ch_vals.iter().cloned().fold(f32::INFINITY,     f32::min);
        let range   = (max_val - min_val).max(1e-6);
        for px in 0..n_pixels {
            out[px * 3 + ch] = ((raw[px * 3 + ch] - min_val) / range).clamp(0.0, 1.0);
        }
    }
    out
}

// ── Frame pixel loader ────────────────────────────────────────────────────────

/// Load a frame's pixels for accumulation.
/// - Mono sessions: returns background-normalized luma (n_pixels f32).
/// - Color sessions: returns background-normalized interleaved RGB (n_pixels * 3 f32).
///   Bayer frames are debayered; already-RGB frames are used directly.
fn load_frame_pixels(
    ctx:      &AppContext,
    snap:     &FrameSnapshot,
    is_color: bool,
    divisor:  f32,
) -> Vec<f32> {
    let buf = match ctx.image_buffers.get(&snap.path) {
        Some(b) => b,
        None    => return vec![0.0; if is_color { snap.width * snap.height * 3 } else { snap.width * snap.height }],
    };
    let pixels = match &buf.pixels {
        Some(p) => p,
        None    => return vec![0.0; if is_color { snap.width * snap.height * 3 } else { snap.width * snap.height }],
    };

    if !is_color {
        // Reuse existing luma path, already normalized by caller via divisor
        let luma = extract_or_debayer_luma(
            pixels, snap.width, snap.height, snap.channels,
            &snap.color_space, snap.bayer_pattern,
        );
        luma.into_iter().map(|v| v / divisor).collect()
    } else {
        // Produce interleaved RGB f32
        let rgb = match snap.color_space {
            ColorSpace::Bayer => {
                // Debayer raw mono → RGB
                let mono: Vec<f32> = match pixels {
                    PixelData::U8(v)  => v.iter().map(|&p| p as f32 / 255.0).collect(),
                    PixelData::U16(v) => v.iter().map(|&p| p as f32 / 65535.0).collect(),
                    PixelData::F32(v) => v.clone(),
                };
                debayer_bilinear(&mono, snap.width, snap.height, snap.bayer_pattern)
            }
            ColorSpace::RGB => {
                // Already RGB — normalize to f32
                match pixels {
                    PixelData::U8(v)  => v.iter().map(|&p| p as f32 / 255.0).collect(),
                    PixelData::U16(v) => v.iter().map(|&p| p as f32 / 65535.0).collect(),
                    PixelData::F32(v) => v.clone(),
                }
            }
            ColorSpace::Mono => {
                // Shouldn't reach here in color mode but handle gracefully:
                // replicate mono across R, G, B
                let mono: Vec<f32> = match pixels {
                    PixelData::U8(v)  => v.iter().map(|&p| p as f32 / 255.0).collect(),
                    PixelData::U16(v) => v.iter().map(|&p| p as f32 / 65535.0).collect(),
                    PixelData::F32(v) => v.clone(),
                };
                mono.iter().flat_map(|&v| [v, v, v]).collect()
            }
        };
        // Background-normalize all channels by the same divisor (computed from luma)
        rgb.into_iter().map(|v| v / divisor).collect()
    }
}

// ── Snapshot collection (debayer-first) ───────────────────────────────────────

fn collect_snapshots(
    ctx: &AppContext,
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

        let (cached_fwhm, cached_ecc) = if let Some(ar) = ctx.analysis_results.get(path) {
            (ar.fwhm, ar.eccentricity)
        } else {
            (None, None)
        };

        let filter   = buf.keywords.get("FILTER").map(|kw| kw.value.clone());
        let exptime  = buf.keywords.get("EXPTIME")
            .and_then(|kw| kw.value.trim().parse::<f32>().ok());
        let rotator  = buf.keywords.get("ROTATOR")
            .and_then(|kw| kw.value.trim().parse::<f32>().ok());
        let date_obs = buf.keywords.get("DATE-OBS")
            .and_then(|kw| parse_date_obs(&kw.value));

        let width    = buf.width    as usize;
        let height   = buf.height   as usize;
        let channels = buf.channels as usize;

        let bayer_pattern = buf.keywords.get("BAYERPAT")
            .or_else(|| buf.keywords.get("BAYER_PATTERN"))
            .map(|kw| BayerPattern::from_str(&kw.value))
            .unwrap_or(BayerPattern::RGGB);

        let luma = extract_or_debayer_luma(pixels, width, height, channels,
            &buf.color_space, bayer_pattern);

        let stars = detect_stars(&luma, width, height, det_config);

        let fwhm         = cached_fwhm.or_else(|| compute_fwhm(&stars, None).map(|r| r.fwhm_pixels));
        let eccentricity = cached_ecc.or_else(|| compute_eccentricity(&stars).map(|r| r.eccentricity));

        snapshots.push(FrameSnapshot {
            index,
            path: path.clone(),
            width,
            height,
            channels,
            color_space: buf.color_space.clone(),
            bayer_pattern,
            filter,
            exptime,
            rotator,
            date_obs,
            fwhm,
            eccentricity,
            stars,
            group: 0,
        });
    }

    // Assign rotational groups
    let mut group_anchors:        Vec<f32>         = Vec::new();
    let mut group_last_date_obs:  Vec<Option<f64>> = Vec::new();

    for snap in snapshots.iter_mut() {
        let rot     = snap.rotator.unwrap_or(0.0);
        let obs_sec = snap.date_obs;

        let mut found = None;
        for (g, &anchor) in group_anchors.iter().enumerate() {
            let delta = circular_delta(rot, anchor);
            if delta > ROTATOR_GROUP_TOLERANCE { continue; }
            let within_session = match (obs_sec, group_last_date_obs[g]) {
                (Some(t), Some(last)) => {
                    let gap_minutes = (t - last).abs() as f32 / 60.0;
                    gap_minutes <= SESSION_GAP_MINUTES
                }
                _ => true,
            };
            if within_session { found = Some(g); break; }
        }

        snap.group = match found {
            Some(g) => {
                if obs_sec.is_some() { group_last_date_obs[g] = obs_sec; }
                g
            }
            None => {
                group_anchors.push(rot);
                group_last_date_obs.push(obs_sec);
                group_anchors.len() - 1
            }
        };
    }

    Ok(snapshots)
}

/// Parse a DATE-OBS string (ISO 8601) to Unix seconds.
fn parse_date_obs(s: &str) -> Option<f64> {
    let s = s.trim();
    let (date_part, time_part) = if let Some(idx) = s.find('T') {
        (&s[..idx], &s[idx+1..])
    } else {
        return None;
    };

    let date_parts: Vec<&str> = date_part.split('-').collect();
    if date_parts.len() != 3 { return None; }
    let year:  i64 = date_parts[0].parse().ok()?;
    let month: i64 = date_parts[1].parse().ok()?;
    let day:   i64 = date_parts[2].parse().ok()?;

    let time_parts: Vec<&str> = time_part.split(':').collect();
    if time_parts.len() < 2 { return None; }
    let hour: i64 = time_parts[0].parse().ok()?;
    let min:  i64 = time_parts[1].parse().ok()?;
    let sec:  f64 = time_parts.get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let a = (14 - month) / 12;
    let y = year + 4800 - a;
    let m = month + 12 * a - 3;
    let jdn = day + (153 * m + 2) / 5 + 365 * y + y / 4 - y / 100 + y / 400 - 32045;
    let days_since_epoch = jdn - 2440588;
    let unix = days_since_epoch as f64 * 86400.0
        + hour as f64 * 3600.0
        + min  as f64 * 60.0
        + sec;

    Some(unix)
}

/// Smallest absolute circular difference between two angles in degrees.
fn circular_delta(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 { 360.0 - d } else { d }
}

// ── Per-frame luma loader (debayered) ─────────────────────────────────────────

fn load_debayered_luma(
    ctx:  &AppContext,
    snap: &FrameSnapshot,
) -> Result<Vec<f32>, PluginError> {
    let buf = ctx.image_buffers.get(&snap.path)
        .ok_or_else(|| PluginError::new("NO_BUF", &format!("Buffer missing: {}", snap.path)))?;
    let pixels = buf.pixels.as_ref()
        .ok_or_else(|| PluginError::new("NO_PIXELS", &format!("No pixels: {}", snap.path)))?;

    Ok(extract_or_debayer_luma(
        pixels, snap.width, snap.height, snap.channels,
        &snap.color_space, snap.bayer_pattern,
    ))
}

fn extract_or_debayer_luma(
    pixels:        &PixelData,
    width:         usize,
    height:        usize,
    channels:      usize,
    color_space:   &ColorSpace,
    bayer_pattern: BayerPattern,
) -> Vec<f32> {
    if *color_space == ColorSpace::Bayer {
        let mono: Vec<f32> = match pixels {
            PixelData::U8(v)  => v.iter().map(|&p| p as f32 / 255.0).collect(),
            PixelData::U16(v) => v.iter().map(|&p| p as f32 / 65535.0).collect(),
            PixelData::F32(v) => v.clone(),
        };
        let rgb = debayer_bilinear(&mono, width, height, bayer_pattern);
        analysis::extract_luminance(&rgb, width, height, 3)
    } else {
        analysis::to_luminance(pixels, channels)
    }
}

// ── Reference frame selection within a group ──────────────────────────────────

fn select_reference_in_group(snapshots: &[FrameSnapshot], group: usize) -> usize {
    snapshots.iter()
        .enumerate()
        .filter(|(_, s)| s.group == group)
        .min_by(|(_, a), (_, b)| {
            let fa = a.fwhm.unwrap_or(f32::MAX);
            let fb = b.fwhm.unwrap_or(f32::MAX);
            fa.partial_cmp(&fb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let ea = a.eccentricity.unwrap_or(f32::MAX);
                    let eb = b.eccentricity.unwrap_or(f32::MAX);
                    ea.partial_cmp(&eb).unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

// ── RANSAC helper ─────────────────────────────────────────────────────────────

fn try_rigid_refinement(
    ref_stars:   &[crate::analysis::stars::StarCandidate],
    frame_stars: &[crate::analysis::stars::StarCandidate],
    fft_dx:      f32,
    fft_dy:      f32,
    width:       usize,
    height:      usize,
    frame_index: usize,
    messages:    &mut Vec<String>,
) -> AffineRigid {
    match estimate_rigid_transform(ref_stars, frame_stars, fft_dx, fft_dy, width, height) {
        Some(rigid) => {
            let cos_t = rigid.a;
            let sin_t = rigid.b;
            let aft_x = cos_t * fft_dx - sin_t * fft_dy;
            let aft_y = sin_t * fft_dx + cos_t * fft_dy;
            let final_xform = AffineRigid {
                a:  rigid.a,
                b:  rigid.b,
                tx: aft_x + rigid.tx,
                ty: aft_y + rigid.ty,
            };
            if rigid.tx.abs() > 15.0 || rigid.ty.abs() > 15.0 {
                info!("Frame {}: rigid refinement rejected (residual {:.1},{:.1} too large) — using FFT only",
                    frame_index, rigid.tx, rigid.ty);
                return AffineRigid::translation(fft_dx, fft_dy);
            }
            let theta = final_xform.theta();
            if theta.abs() >= MIN_ROTATION_TO_APPLY {
                let msg = format!(
                    "Frame {}: rigid alignment — tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°)",
                    frame_index, final_xform.tx, final_xform.ty,
                    theta, theta.to_degrees()
                );
                info!("{}", msg);
                messages.push(msg);
            }
            final_xform
        }
        None => {
            info!("Frame {}: rigid refinement not available — using FFT translation only", frame_index);
            AffineRigid::translation(fft_dx, fft_dy)
        }
    }
}

// ── Alignment failure helper ──────────────────────────────────────────────────

fn alignment_failed(
    frame_index:   usize,
    reason:        &str,
    messages:      &mut Vec<String>,
    contrib:       &mut FrameContribution,
    contributions: &mut Vec<FrameContribution>,
) {
    let msg = format!("Alignment failed: frame {} — {}, skipped", frame_index, reason);
    info!("{}", msg);
    messages.push(msg);
    contrib.exclusion_reason = Some(ExclusionReason::AlignmentFailed);
    contributions.push(contrib.clone());
}

// ── Frame resampling (mono) ───────────────────────────────────────────────────

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
            let x0 = src_x.floor() as i32;
            let y0 = src_y.floor() as i32;
            let fx = src_x - x0 as f32;
            let fy = src_y - y0 as f32;
            bilinear(normalized, width, height, x0, y0, x0 + 1, y0 + 1, fx, fy)
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

// ── Frame resampling (RGB) ────────────────────────────────────────────────────

/// Resample interleaved RGB pixels by (dx, dy) translation.
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

/// Resample interleaved RGB pixels using an affine transform.
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

// ── Bilinear interpolation ────────────────────────────────────────────────────

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

/// Bilinear interpolation for a single channel of interleaved RGB.
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

// ── Helpers ───────────────────────────────────────────────────────────────────

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

    insert("SIMPLE",   "T",                    "file conforms to FITS standard");
    insert("BITPIX",   "-32",                  "32-bit floating point");
    insert("NAXIS",    "2",                    "number of axes");
    insert("NAXIS1",   &width.to_string(),     "image width in pixels");
    insert("NAXIS2",   &height.to_string(),    "image height in pixels");
    insert("BZERO",    "0",                    "offset for unsigned integers");
    insert("BSCALE",   "1",                    "default scaling factor");
    insert("ROWORDER", "TOP-DOWN",             "row order");
    insert("CREATOR",  "Photyx",               "software that created this file");

    if let Some(s) = summary {
        if let Some(ref target) = s.target {
            insert("OBJECT", target, "target object name");
        }
        if let Some(ref filter) = s.filter {
            insert("FILTER", filter, "filter used");
        }
        if s.integration_seconds > 0.0 {
            insert("EXPTIME",
                &format!("{:.1}", s.integration_seconds),
                "total integration time in seconds");
        }
        insert("STACKCNT",
            &s.stacked_frames.to_string(),
            "number of frames stacked");
    }

    kw
}

// ── Alignment validation (retained for future use) ────────────────────────────

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
