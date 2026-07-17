// plugins/stack_frames.rs — StackFrames built-in plugin
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
//      where G is the within-group transform (FFT-primed RANSAC against
//      group ref, sanity-bounded to reject implausible rotation/translation).
//      For master-group frames, M_cross = identity, so T = G.
//
//   7. Color awareness: if the master reference frame is Bayer or RGB, the
//      stack accumulates all three RGB channels and outputs ColorSpace::RGB.
//      Mono input produces a grayscale output as before.

use crate::analysis::{
    self,
    background::estimate_background,
    debayer::{bayer_pattern_of, debayer_bilinear, BayerPattern},
    eccentricity::compute_eccentricity,
    fft_align::compute_translation,
    fwhm::compute_fwhm,
    star_align::{compose, estimate_rigid_transform, estimate_rigid_transform_triangles, AffineRigid},
    stars::detect_stars,
    stack_metrics::{ExclusionReason, FrameContribution, StackSummary},
    SigmaClipConfig, StarDetectionConfig,
};
use crate::context::{AppContext, BitDepth, ColorSpace, ImageBuffer, PixelData};
use crate::plugin::{ArgMap, ParamSpec, PhotyxPlugin, PluginError, PluginOutput};
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

impl PhotyxPlugin for StackFrames {
    fn name(&self) -> &str { "StackFrames" }
    fn version(&self) -> &str { "1.1.0" }
    fn description(&self) -> &str {
        "Stacks loaded frames using FFT alignment, triangle rigid refinement, \
         meridian-flip-aware group reference selection, and two-pass \
         sigma-clipped mean combination."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![]
    }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded."));
        }
        ctx.clear_stack();
        crate::set_progress("Stacking analysis", 0, 0);

        //     Light frame stacking
        let det_config = StarDetectionConfig::default();
        let snapshots  = collect_snapshots(ctx, &det_config)?;

        if snapshots.is_empty() {
            return Err(PluginError::new("NO_PIXELS", "No frames with pixel data available."));
        }

        let width    = snapshots[0].width;
        let height   = snapshots[0].height;
        let n_pixels = width * height;
        let total    = snapshots.len();

        let mut messages: Vec<String> = Vec::new();

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
            let frame_pixels = match load_frame_pixels(ctx, snap, is_color) {
                Ok(p) => p,
                Err(e) => {
                    buffer_unavailable(snap.index, &e.message, &mut messages, &mut contrib, &mut contributions);
                    continue;
                }
            };

            // Extract luma for background estimation and FFT alignment.
            let cal_luma = if is_color {
                analysis::extract_luminance(&frame_pixels, width, height, 3)
            } else {
                frame_pixels.clone()
            };

            if group_ref_luma[snap.group].is_none() {
                let g_ref  = &snapshots[group_refs[snap.group]];
                let g_luma = load_debayered_luma(ctx, g_ref)?;
                group_ref_stars[snap.group] = Some(g_ref.stars.clone());
                group_ref_luma[snap.group]  = Some(g_luma);
            }
            let g_ref_luma  = group_ref_luma[snap.group].as_ref().unwrap();
            let g_ref_stars = group_ref_stars[snap.group].as_ref().unwrap();

            // Background estimation for normalization
            let bg_est   = estimate_background(&cal_luma, &bg_sigma_config);
            let bg_level = bg_est.median;
            contrib.background_level = Some(bg_level);
            let divisor = if bg_level > 1e-6 { bg_level } else { 1.0 };

            // Normalize luma by background for FFT alignment
            let normalized_luma: Vec<f32> = cal_luma.par_iter().map(|&v| v / divisor).collect();

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

            let g_xform = g_transform.unwrap();
            let t_final = compose(&m_cross[snap.group], &g_xform);

            let accum_pixels: Vec<f32> = frame_pixels.iter().map(|&v| v / divisor).collect();

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

            cached_transforms[i] = Some(t_final);
            contrib.included = true;
            if let Some(et) = snap.exptime { total_integration += et; }
            crate::set_progress("Registering for stack", (i + 1) as u32, total as u32);
            contributions.push(contrib);
        }

        let registered_count = contributions.iter().filter(|c| c.included).count();
        if registered_count == 0 {
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

        //  ── Pass 2 — sigma-clipped accumulation (batched parallel) ────────────

        let sigma      = 2.5f32;
        let n_threads  = if ctx.rayon_thread_count == -1 {
            rayon::current_num_threads()
        } else {
            ctx.rayon_thread_count as usize
        }.max(1);

        let mut sum_buf:    Vec<f64> = vec![0.0; n_pixels * n_channels];
        let mut clip_count: Vec<u32> = vec![0;  n_pixels];

        struct Pass2Input {
            snap_idx: usize,
            xform:    AffineRigid,
        }

        // Pixel data is loaded lazily inside the chunk loop to avoid
        // pre-allocating all frames simultaneously. Peak Pass 2 memory is
        // bounded to one batch at a time.
        let pass2_inputs: Vec<Pass2Input> = snapshots.iter().enumerate()
            .filter_map(|(i, _snap)| {
                let xform = cached_transforms[i].clone()?;
                Some(Pass2Input { snap_idx: i, xform })
            })
            .collect();

        let mut pass2_done = 0usize;

        for chunk in pass2_inputs.chunks(n_threads) {
            // Sequential: load one batch worth of pixel data. A frame whose
            // buffer disappeared since Pass 1 (the snapshot/re-lookup-by-path
            // structure means this is looked up again here rather than reusing
            // what Pass 1 already loaded) is excluded rather than panicking —
            // its contribution entry is updated in place so the final counts
            // stay accurate.
            let mut chunk_ok:     Vec<&Pass2Input> = Vec::with_capacity(chunk.len());
            let mut chunk_pixels: Vec<Vec<f32>>    = Vec::with_capacity(chunk.len());

            for inp in chunk.iter() {
                let snap = &snapshots[inp.snap_idx];
                let loaded = ctx.image_buffers.get(&snap.path)
                    .and_then(|buf| buf.pixels.as_ref().map(|pixels| (buf, pixels)));

                match loaded {
                    Some((_buf, pixels)) => {
                        let px = if is_color {
                            if snap.color_space == ColorSpace::Bayer {
                                let mono = analysis::to_f32_normalized(pixels);
                                debayer_bilinear(&mono, snap.width, snap.height, snap.bayer_pattern)
                            } else {
                                analysis::to_f32_normalized(pixels)
                            }
                        } else {
                            analysis::to_luminance(pixels, snap.channels)
                        };
                        chunk_ok.push(inp);
                        chunk_pixels.push(px);
                    }
                    None => {
                        let msg = format!(
                            "Frame {}: buffer unavailable during stacking — excluded", snap.index
                        );
                        info!("{}", msg);
                        messages.push(msg);
                        if let Some(c) = contributions.get_mut(inp.snap_idx) {
                            c.included         = false;
                            c.exclusion_reason = Some(ExclusionReason::BufferUnavailable);
                        }
                    }
                }
            }

            // Parallel: background estimate and resample each frame in chunk.
            let aligned_buffers: Vec<Vec<f32>> = chunk_ok.par_iter()
                .zip(chunk_pixels.into_par_iter())
                .map(|(inp, frame_pixels)| {
                    let cal_luma = if is_color {
                        analysis::extract_luminance(&frame_pixels, width, height, 3)
                    } else {
                        frame_pixels.clone()
                    };

                    let bg_est   = estimate_background(&cal_luma, &bg_sigma_config);
                    let bg_level = bg_est.median;
                    let divisor  = if bg_level > 1e-6 { bg_level } else { 1.0 };

                    let accum_pixels: Vec<f32> = frame_pixels.iter().map(|&v| v / divisor).collect();

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
            for (_inp, aligned) in chunk_ok.iter().zip(aligned_buffers.iter()) {
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
                crate::set_progress("Integrating stack", pass2_done as u32, total as u32);
            }
            // aligned_buffers dropped here, releasing chunk memory
        }

        // Recompute after Pass 2: a frame counted as registered after Pass 1
        // may have been excluded above if its buffer vanished before Pass 2
        // could load it. Everything downstream (messages, JSON response,
        // StackSummary::compute) must reflect this final count, not the
        // pre-Pass-2 one.
        let stacked_count = contributions.iter().filter(|c| c.included).count();
        if stacked_count == 0 {
            return Err(PluginError::new("NO_FRAMES_STACKED", "No frames could be stacked."));
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

        //  ── Crop to common-overlap region (Issue 111, Option 3) ───────────────
        // Every included frame's final transform is already known. Compute
        // the axis-aligned region free of clamp-fabricated data for each one
        // and intersect across all frames that actually made it into the
        // final average (contributions[i].included, post-Pass-2) — a frame
        // excluded before Pass 2 (e.g. buffer vanished) must not constrain
        // the crop, since its data never entered sum_buf.
        let crop = cached_transforms.iter()
            .enumerate()
            .filter_map(|(i, xform)| {
                if contributions.get(i).map(|c| c.included).unwrap_or(false) {
                    xform.as_ref().map(|x| (i, x))
                } else {
                    None
                }
            })
            .map(|(i, xform)| {
                let bounds = valid_output_bounds(width, height, xform);
                // TEMPORARY DIAGNOSTIC (Issue 111) — remove once the zero-area
                // collapse on rotated transforms is understood and fixed.
                info!(
                    "StackFrames: valid_output_bounds frame {} \u{2014} a={:.4} b={:.4} (\u{03b8}={:.4}rad) tx={:.2} ty={:.2} \u{2192} bounds=({}, {}, {}, {})",
                    snapshots[i].index, xform.a, xform.b, xform.theta(), xform.tx, xform.ty,
                    bounds.0, bounds.1, bounds.2, bounds.3
                );
                bounds
            })
            .fold((0usize, 0usize, width, height), |(ax0, ay0, ax1, ay1), (bx0, by0, bx1, by1)| {
                (ax0.max(bx0), ay0.max(by0), ax1.min(bx1), ay1.min(by1))
            });

        let (crop_x0, crop_y0, crop_x1, crop_y1) = crop;
        let cropped_w = crop_x1.saturating_sub(crop_x0);
        let cropped_h = crop_y1.saturating_sub(crop_y0);

        let (final_pixels, final_w, final_h) = if cropped_w > 0 && cropped_h > 0
            && (cropped_w < width || cropped_h < height)
        {
            let msg = format!(
                "cropped to common-overlap region {}\u{00d7}{} \u{2192} {}\u{00d7}{} (origin {},{})",
                width, height, cropped_w, cropped_h, crop_x0, crop_y0
            );
            info!("StackFrames: {}", msg);
            messages.push(msg);
            (
                crop_buffer(&stack_pixels, width, n_channels, crop_x0, crop_y0, cropped_w, cropped_h),
                cropped_w,
                cropped_h,
            )
        } else {
            if cropped_w == 0 || cropped_h == 0 {
                let msg = "common-overlap crop degenerated to zero area \u{2014} using full uncropped canvas instead".to_string();
                info!("StackFrames: {}", msg);
                messages.push(msg);
            }
            (stack_pixels, width, height)
        };

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
            width:         final_w  as u32,
            height:        final_h as u32,
            display_width: final_w  as u32,
            bit_depth:     BitDepth::F32,
            color_space:   output_color_space,
            channels:      output_channels,
            keywords:      build_stack_keywords(final_w, final_h, &ctx.stack_summary),
            pixels:        Some(PixelData::F32(final_pixels)),
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
            "Stacking complete \u{2014} {} / {} frames stacked", stacked_count, total
        ));
        messages.push(quality_summary);

        let full_message = messages.join("\n");
        info!("StackFrames: {}", full_message);

        crate::set_progress("", 0, 0);

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

        let bayer_pattern = bayer_pattern_of(&buf.keywords)
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
    crate::analysis::frame_quality_score(snap.fwhm, snap.eccentricity)
}

//  ── Frame pixel loading ───────────────────────────────────────────────────────

fn load_frame_pixels(ctx: &AppContext, snap: &FrameSnapshot, is_color: bool) -> Result<Vec<f32>, PluginError> {
    let buf = ctx.image_buffers.get(&snap.path)
        .ok_or_else(|| PluginError::new("NO_BUFFER", "Frame buffer missing."))?;
    let pixels = buf.pixels.as_ref()
        .ok_or_else(|| PluginError::new("NO_PIXELS", "Frame has no pixel data."))?;

    let result = if is_color {
        if snap.color_space == ColorSpace::Bayer {
            let mono = analysis::to_f32_normalized(pixels);
            debayer_bilinear(&mono, snap.width, snap.height, snap.bayer_pattern)
        } else {
            analysis::to_f32_normalized(pixels)
        }
    } else {
        analysis::to_luminance(pixels, snap.channels)
    };

    Ok(result)
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
    match estimate_rigid_transform(ref_stars, frm_stars, fft_dx, fft_dy, width, height) {
        Some(refined) => {
            let theta = refined.theta();
            info!("Frame {}: RANSAC match — tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°)",
                frame_idx, refined.tx, refined.ty, theta, theta.to_degrees());
            refined
        }
        None => {
            let msg = format!(
                "Frame {}: RANSAC match failed — using FFT translation only", frame_idx
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

fn buffer_unavailable(
    frame_idx:     usize,
    reason:        &str,
    messages:      &mut Vec<String>,
    contrib:       &mut FrameContribution,
    contributions: &mut Vec<FrameContribution>,
) {
    let msg = format!("Frame {}: buffer unavailable — {} — excluded", frame_idx, reason);
    info!("{}", msg);
    messages.push(msg);
    contrib.exclusion_reason = Some(ExclusionReason::BufferUnavailable);
    contributions.push(contrib.clone());
}

//  ── DATE-OBS parsing ─────────────────────────────────────────────────────────

fn parse_date_obs(s: &str) -> Option<f64> {
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

//  ── Common-overlap crop (Issue 111, Option 3) ────────────────────────────────
// For a single frame's transform, determines the axis-aligned output region
// where every pixel's bilinear footprint lies entirely inside the source
// frame — i.e. free of clamp-fabricated edge data. Uses the exact same
// apply_inverse math the real resampler uses, evaluated directly per frame
// rather than inferred from an accumulated coverage count (the approach an
// earlier attempt at this feature used, which failed because a clamping
// resampler makes every pixel look fully covered regardless of frame shift).

/// Returns (x_min, y_min, x_max_exclusive, y_max_exclusive) — the largest
/// axis-aligned rectangle, found by scanning inward from each canvas edge,
/// within which this frame's resample has no fabricated (clamped) data.
fn valid_output_bounds(width: usize, height: usize, xform: &AffineRigid) -> (usize, usize, usize, usize) {
    let w = width  as i32;
    let h = height as i32;

    // Point-membership test for the valid region V = T(source_rect) — true
    // iff this OUTPUT pixel's bilinear footprint is entirely inside the
    // source frame (no clamp-fabricated data). Weight-aware: a corner only
    // needs to be in-bounds if its interpolation weight is nonzero.
    let is_valid = |ox: usize, oy: usize| -> bool {
        let (src_x, src_y) = xform.apply_inverse(ox as f32, oy as f32);
        let x0f = src_x.floor() as i32;
        let y0f = src_y.floor() as i32;
        let fx = src_x - x0f as f32;
        let fy = src_y - y0f as f32;
        let x0_ok = x0f >= 0 && x0f < w;
        let x1_ok = fx <= 0.0 || (x0f + 1 < w);
        let y0_ok = y0f >= 0 && y0f < h;
        let y1_ok = fy <= 0.0 || (y0f + 1 < h);
        x0_ok && x1_ok && y0_ok && y1_ok
    };

    // V is convex — T is a rigid rotation+translation, so it maps the
    // convex source rectangle to a convex (merely rotated) rectangle in
    // output space. For two convex shapes, an axis-aligned candidate
    // rectangle lies entirely inside V iff all four of its corners do —
    // so a candidate can be checked in O(1), not O(pixels). This
    // replaces an earlier full-row/column scan, which was only correct
    // when the invalid region was a pure horizontal or vertical band —
    // never true once translation has both x and y components, which is
    // why every non-identity frame previously collapsed to zero area.
    let corners_valid = |x0: usize, y0: usize, x1: usize, y1: usize| -> bool {
        if x1 <= x0 || y1 <= y0 { return false; }
        is_valid(x0, y0) && is_valid(x1 - 1, y0) && is_valid(x0, y1 - 1) && is_valid(x1 - 1, y1 - 1)
    };

    // The valid region is, to sub-pixel accuracy, the FORWARD image of
    // the source rectangle under the transform — a parallelogram, since
    // the transform is affine. So instead of searching for it, compute
    // it: map the four source corners into output space and take the
    // inner axis-aligned box (per axis, the two middle values of the
    // four sorted corner coordinates). For the near-axis-aligned
    // transforms here (true rotation ≤ ~0.5°; the 180° flip merely
    // relabels which corner is which), this box is inscribed in the
    // parallelogram, and the rotation cross-term is captured exactly by
    // the corner positions. This replaces a seed-and-expand search whose
    // greedy per-edge expansion found inclusion-maximal but badly
    // sub-optimal rectangles: one edge would grab a pixel that was only
    // valid near the seed row/column, permanently blocking the
    // perpendicular axis from expanding past where the rotation
    // cross-term invalidated that pixel (e.g. 1 column of width traded
    // for ~1100 rows of height).
    let src_max_x = width  as f32 - 1.0;
    let src_max_y = height as f32 - 1.0;
    let corners = [
        xform.apply_forward(0.0,       0.0),
        xform.apply_forward(src_max_x, 0.0),
        xform.apply_forward(0.0,       src_max_y),
        xform.apply_forward(src_max_x, src_max_y),
    ];
    let mut xs: Vec<f32> = corners.iter().map(|c| c.0).collect();
    let mut ys: Vec<f32> = corners.iter().map(|c| c.1).collect();
    xs.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
    ys.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));

    // Inner box from the two middle coordinates per axis, rounded
    // inward and clamped to the canvas. x1/y1 are exclusive bounds.
    let mut x0 = xs[1].ceil().max(0.0) as usize;
    let mut y0 = ys[1].ceil().max(0.0) as usize;
    let mut x1 = (xs[2].floor() + 1.0).clamp(0.0, width  as f32) as usize;
    let mut y1 = (ys[2].floor() + 1.0).clamp(0.0, height as f32) as usize;

    // Boundary rounding and the bilinear-footprint edge cases can leave
    // the analytic box off by a pixel — verify against the exact
    // pixel-level predicate and nudge inward a few times if needed. If
    // it still fails, fall through with the degenerate-checking
    // self-check below reporting (0,0,0,0).
    for _ in 0..4 {
        if x1 <= x0 || y1 <= y0 { break; }
        if corners_valid(x0, y0, x1, y1) { break; }
        x0 += 1;
        y0 += 1;
        x1 = x1.saturating_sub(1);
        y1 = y1.saturating_sub(1);
    }


    // Final self-check: don't trust the search implicitly. If the
    // converged rectangle somehow fails its own containment test,
    // report degenerate rather than a silently-wrong crop.
    if corners_valid(x0, y0, x1, y1) {
        (x0, y0, x1, y1)
    } else {
        (0, 0, 0, 0)
    }
}

/// Crops a row-major pixel buffer (mono or interleaved-channel) to the
/// given rectangle within a `src_w`-wide source buffer.
fn crop_buffer(
    src:      &[f32],
    src_w:    usize,
    channels: usize,
    x0:       usize,
    y0:       usize,
    crop_w:   usize,
    crop_h:   usize,
) -> Vec<f32> {
    let mut out = vec![0.0f32; crop_w * crop_h * channels];
    for row in 0..crop_h {
        let src_start = ((y0 + row) * src_w + x0) * channels;
        let dst_start = row * crop_w * channels;
        let row_len   = crop_w * channels;
        out[dst_start..dst_start + row_len]
            .copy_from_slice(&src[src_start..src_start + row_len]);
    }
    out
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
