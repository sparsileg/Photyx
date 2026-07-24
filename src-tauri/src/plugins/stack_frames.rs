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
    eccentricity::compute_eccentricity,
    fft_align::compute_translation,
    fwhm::compute_fwhm,
    star_align::{compose, estimate_rigid_transform, estimate_rigid_transform_triangles, AffineRigid},
    stars::detect_stars,
    stack_metrics::{ExclusionReason, FrameContribution, StackSummary},
    SigmaClipConfig, StarDetectionConfig,
};
// Issue 175: bayer_pattern_of/debayer_bilinear/BayerPattern removed —
// FrameSnapshot.bayer_pattern (below) is gone. The debayer decision now
// happens entirely on the read side (pixel_chunking::load_request, keyed
// off the file's own color_space at read time), so this file never needs
// to know or cache which Bayer pattern a frame uses.
use crate::context::{AppContext, BitDepth, ColorSpace, ImageBuffer, PixelData};
use crate::plugin::{ArgMap, ParamSpec, PhotyxPlugin, PluginError, PluginOutput};
use crate::settings::defaults::{
    CROSS_GROUP_MAX_RESIDUAL_PX, CROSS_GROUP_MIN_MATCHED, REF_MIN_STAR_FRACTION,
    MERIDIAN_FLIP_THRESHOLD, SESSION_GAP_MINUTES, ROTATOR_GROUP_TOLERANCE,
    CROSS_GROUP_VERIFY_MATCH_RADIUS_PX, STACK_SIGMA_CLIP,
};
use chrono::Utc;
use rayon::prelude::*;
use tracing::info;

/// Rotation magnitude (radians) below which we skip the affine resampler.
/// Stays local (Issue 148): a numerical guard against doing unnecessary
/// affine-resample work for a near-zero rotation, not a tuning knob — it
/// doesn't change what counts as a valid alignment.
const MIN_ROTATION_TO_APPLY: f32 = 0.001;

// ROTATOR_GROUP_TOLERANCE, SESSION_GAP_MINUTES, MERIDIAN_FLIP_THRESHOLD
// moved to settings::defaults (Issue 148) — frame-grouping thresholds a
// maintainer might plausibly retune for an unusual session cadence.

pub struct StackFrames;

//  ── Snapshot type ─────────────────────────────────────────────────────────────

struct FrameSnapshot {
    index:         usize,
    path:          String,
    width:         usize,
    height:        usize,
    channels:      usize,
    color_space:   ColorSpace,
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
        // Issue 120: guarantees set_progress("", 0, 0) runs on every exit
        // path — Ok, Err, or panic unwind — instead of only immediately
        // before the final Ok. Held for the rest of execute().
        let _progress_guard = crate::ProgressClearGuard;
        ctx.clear_stack();
        crate::set_progress("Stacking analysis", 0, 0);

        //     Light frame stacking
        let det_config = StarDetectionConfig::default();
        let (snapshots, read_failures) = collect_snapshots(ctx, &det_config)?;

        if snapshots.is_empty() {
            return Err(PluginError::new("NO_PIXELS", "No frames with pixel data available."));
        }

        let total = snapshots.len();

        let mut messages: Vec<String> = Vec::new();

        let n_groups = snapshots.iter().map(|s| s.group).max().unwrap_or(0) + 1;
        info!("StackFrames: identified {} rotational group(s)", n_groups);

        // Issue 141: the master group's reference must be selected before
        // any other group's, because non-master groups now filter their
        // reference candidate pool against the master reference's FILTER
        // value (see select_reference_in_group). Master group membership
        // itself only depends on frame counts, not on any reference having
        // been chosen yet, so it's determined first and independently.
        let master_group = (0..n_groups)
            .max_by_key(|&g| snapshots.iter().filter(|s| s.group == g).count())
            .unwrap();

        let (master_ref_idx, master_warning) = select_reference_in_group(&snapshots, master_group, None);
        if let Some(msg) = master_warning {
            info!("StackFrames: {}", msg);
            messages.push(msg);
        }

        let ref_filter = snapshots[master_ref_idx].filter.clone();

        let mut group_refs: Vec<usize> = vec![0; n_groups];
        group_refs[master_group] = master_ref_idx;
        for g in 0..n_groups {
            if g == master_group { continue; }
            let (ridx, warning) = select_reference_in_group(&snapshots, g, ref_filter.as_deref());
            if let Some(msg) = warning {
                info!("StackFrames: {}", msg);
                messages.push(msg);
            }
            group_refs[g] = ridx;
        }

        for (g, &ridx) in group_refs.iter().enumerate() {
            let count = snapshots.iter().filter(|s| s.group == g).count();
            info!("  Group {}: {} frames, reference = frame {} ({})",
                  g, count, snapshots[ridx].index, short_name(&snapshots[ridx].path));
        }

        info!("StackFrames: master group = {} (reference frame {})",
              master_group, snapshots[master_ref_idx].index);

        // Issue 139: the output canvas is defined by the master reference
        // frame — the frame every other frame's transform ultimately maps
        // into — rather than snapshots[0], which was an arbitrary
        // chronologically-first frame with no geometric significance.
        let width    = snapshots[master_ref_idx].width;
        let height   = snapshots[master_ref_idx].height;
        let channels = snapshots[master_ref_idx].channels;
        let n_pixels = width * height;

        // Issue 139: every frame must share the master reference's pixel
        // geometry before any pixel work begins. A smaller frame would
        // index out of bounds in extract_luminance/debayer downstream; a
        // larger frame would silently truncate and stack garbage at the
        // wrong row stride. Fail loudly and name the offending file rather
        // than let either happen.
        for snap in &snapshots {
            if snap.width != width || snap.height != height || snap.channels != channels {
                return Err(PluginError::new(
                    "DIMENSION_MISMATCH",
                    &format!(
                        "Frame {} ({}) is {}×{}×{}ch but the master reference {} is {}×{}×{}ch. \
                         All frames in a stack must share the same dimensions.",
                        snap.index, short_name(&snap.path), snap.width, snap.height, snap.channels,
                        snapshots[master_ref_idx].index, width, height, channels,
                    ),
                ));
            }
        }

        let ref_color_space = snapshots[master_ref_idx].color_space.clone();

        let is_color   = ref_color_space == ColorSpace::Bayer
            || ref_color_space == ColorSpace::RGB;
        let n_channels = if is_color { 3 } else { 1 };

        info!("StackFrames: output mode = {}", if is_color { "RGB (color)" } else { "Mono (grayscale)" });

        // Issue 140: bound early so every background-median divisor in this
        // function — master reference, cross-group references, within-group
        // references and targets — is computed through the same estimator
        // with the same parameters.
        let bg_sigma_config = SigmaClipConfig::default();

        let master_ref_luma_raw = load_debayered_luma(ctx, &snapshots[master_ref_idx])?;
        let master_ref_bg      = estimate_background(&master_ref_luma_raw, &bg_sigma_config).median;
        let master_ref_divisor = if master_ref_bg > 1e-6 { master_ref_bg } else { 1.0 };
        // Issue 140: normalize the master reference by its own background
        // median so compute_translation is never called with one side
        // normalized and the other raw — the asymmetry that weakened FFT
        // peak quality on frames with mismatched sky background.
        let master_ref_luma: Vec<f32> = master_ref_luma_raw.iter().map(|&v| v / master_ref_divisor).collect();
        let master_ref_stars = snapshots[master_ref_idx].stars.clone();

        let ref_path   = snapshots[master_ref_idx].path.clone();
        let ref_target = ctx.image_buffers.get(&ref_path)
            .and_then(|b| b.keywords.get("OBJECT"))
            .map(|kw| kw.value.clone());

        //  ── Solve M_cross for each non-master group ───────────────────────────
        let mut m_cross: Vec<AffineRigid> = (0..n_groups).map(|_| AffineRigid::identity()).collect();

        // Issue 128: a group whose cross-group solve is rejected (FFT
        // failure, poor verification residual, or implausible triangle-match
        // rotation) is excluded here and actually honored in the Pass 1 loop
        // below — previously the FFT-failure message claimed exclusion but
        // nothing enforced it (m_cross[g] silently stayed identity and the
        // group's frames stacked unflipped and misaligned).
        let mut group_excluded: Vec<bool> = vec![false; n_groups];

        for g in 0..n_groups {
            if g == master_group { continue; }

            let gref_snap = &snapshots[group_refs[g]];
            let gref_luma_raw = match load_debayered_luma(ctx, gref_snap) {
                Ok(luma) => luma,
                Err(e) => {
                    let msg = format!(
                        "StackFrames: cross-group {} reference buffer unavailable ({}) — frames from this group will be excluded",
                        g, e.message
                    );
                    info!("{}", msg);
                    messages.push(msg);
                    group_excluded[g] = true;
                    continue;
                }
            };
            // Issue 140: normalize this group's reference by its own
            // background median — same reasoning as the master reference
            // above, and arguably more relevant here, since cross-group
            // references are exactly the frames most likely to differ in
            // sky background (different night, different conditions).
            let gref_bg      = estimate_background(&gref_luma_raw, &bg_sigma_config).median;
            let gref_divisor = if gref_bg > 1e-6 { gref_bg } else { 1.0 };
            let gref_luma: Vec<f32> = gref_luma_raw.iter().map(|&v| v / gref_divisor).collect();

            // Diagnostic logging (galaxy-contamination investigation): dumps
            // the largest-by-pixel_count detected candidates for this
            // group's reference frame, plus explicitly flags any candidate
            // near a known suspect position. pixel_count is the flood-fill
            // component size — a real stellar PSF should stay small and
            // compact; a galaxy bulge/core feeding the same flood-fill from
            // a much brighter, broader base would plausibly pull in far
            // more connected pixels before dropping below the flood
            // threshold. Purely diagnostic; does not change star selection,
            // matching, or gating.
            {
                let mut by_size: Vec<&crate::analysis::stars::StarCandidate> =
                    gref_snap.stars.iter().collect();
                by_size.sort_by(|a, b| b.pixel_count.cmp(&a.pixel_count));

                let counts: Vec<usize> = gref_snap.stars.iter().map(|s| s.pixel_count).collect();
                let median = if counts.is_empty() { 0 } else {
                    let mut sorted = counts.clone();
                    sorted.sort_unstable();
                    sorted[sorted.len() / 2]
                };
                info!("StackFrames: M_cross[{}] candidate size check — {} total, median pixel_count={}",
                      g, gref_snap.stars.len(), median);

                for (i, s) in by_size.iter().take(10).enumerate() {
                    let (x0, y0, x1, y1) = s.bbox;
                    info!("StackFrames: M_cross[{}] largest {} — star at ({:.1},{:.1}) pixel_count={} bbox=({},{})-({},{}) peak={:.3}",
                          g, i, s.cx, s.cy, s.pixel_count, x0, y0, x1, y1, s.peak);
                }
            }

            // Issue 134: previously this compared a 180°-reversed copy of
            // the group reference against the master reference, assuming
            // every non-master group was a meridian flip. Real sessions
            // showed that assumption fails across most of the angular
            // range — M104 (~105° cross-night rotation, no flip at all)
            // and Sh2-101 (a same-orientation group misread as 180° of
            // "drift") both had valid, well-matched cross-group transforms
            // discarded because they weren't near 180°. Triangle matching
            // is rotation-invariant and finds the true relative
            // orientation directly (confirmed: it recovered M104's real
            // ~105° unassisted before this fix discarded the answer), so
            // this now compares the group reference to the master
            // reference unflipped and uses whatever transform is found —
            // 0°, 105°, 179°, or anything else — with no pre-composed
            // assumption. `gref_snap.stars` is reused directly from
            // `collect_snapshots()` rather than re-detecting on a
            // reversed buffer.
            let fft_t = match compute_translation(&master_ref_luma, &gref_luma, width, height) {
                Some(t) => t,
                None    => {
                    let msg = format!(
                        "StackFrames: cross-group {} FFT failed — frames from this group will be excluded", g
                    );
                    info!("{}", msg);
                    messages.push(msg);
                    group_excluded[g] = true;
                    continue;
                }
            };

            info!("StackFrames: cross-group {} FFT vs master ref dx={:.2} dy={:.2}",
                  g, fft_t.dx, fft_t.dy);

            let cross_transform = match estimate_rigid_transform_triangles(
                &master_ref_stars, &gref_snap.stars,
            ) {
                Some(r) => {
                    let theta = r.theta();
                    info!("StackFrames: cross-group {} triangle match — tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°)",
                          g, r.tx, r.ty, theta, theta.to_degrees());
                    r
                }
                None => {
                    info!("StackFrames: cross-group {} triangle match failed — falling back to FFT-only", g);
                    // Issue 132: matches the sign convention used throughout
                    // estimate_rigid_transform (frame point - fft offset =
                    // reference point). Unchanged by Issue 134 — this sign
                    // relationship holds regardless of which buffer was
                    // passed as the FFT target.
                    AffineRigid::translation(-fft_t.dx, -fft_t.dy)
                }
            };

            m_cross[g] = cross_transform;

            info!("StackFrames: M_cross[{}] = a={:.4} b={:.4} tx={:.2} ty={:.2} θ={:.3}° scale={:.5}",
                  g, m_cross[g].a, m_cross[g].b, m_cross[g].tx, m_cross[g].ty,
                  m_cross[g].theta().to_degrees(), m_cross[g].scale());

            let mut residual_count = 0usize;
            let mut residual_mean  = 0.0f32;
            {
                // Diagnostic logging (companion to the max-residual gate
                // issue): each sample keeps the group-reference star's own
                // unflipped position alongside the residual vector to its
                // closest master-reference match, not just the scalar
                // distance already logged. Purely diagnostic; does not
                // change what gates the solve or the meaning of
                // residual_count/residual_mean (still matched-only, as the
                // existing CROSS_GROUP_MIN_MATCHED/MAX_RESIDUAL_PX gate
                // expects).

                struct ResidualSample { gx: f32, gy: f32, dx: f32, dy: f32, dist: f32 }
                let mut residuals: Vec<ResidualSample> = Vec::new();
                // Stars whose closest master-reference candidate is still
                // >= CROSS_GROUP_VERIFY_MATCH_RADIUS_PX away — i.e. found no
                // match at all. These never appear in the mean/max above,
                // but a star whose true displacement under M_cross exceeds
                // the search radius entirely is arguably the more important
                // signal: it silently drops out of the matched population
                // rather than showing up as a bad residual, which is
                // exactly the kind of thing a mean-only check (and a human
                // skimming only the worst matched residuals) can miss.
                let mut unmatched: Vec<ResidualSample> = Vec::new();
                for gs in &gref_snap.stars {
                    let (mx, my) = m_cross[g].apply_forward(gs.cx, gs.cy);
                    let closest = master_ref_stars.iter()
                        .map(|r| (r.cx, r.cy, ((r.cx - mx).powi(2) + (r.cy - my).powi(2)).sqrt()))
                        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
                    if let Some((rx, ry, dist)) = closest {
                        let sample = ResidualSample { gx: gs.cx, gy: gs.cy, dx: rx - mx, dy: ry - my, dist };
                        if dist < CROSS_GROUP_VERIFY_MATCH_RADIUS_PX {
                            residuals.push(sample);
                        } else {
                            unmatched.push(sample);
                        }
                    }
                }

                let total = gref_snap.stars.len();
                if residuals.is_empty() {
                    info!("StackFrames: M_cross[{}] verification — no stars matched within {}px ({} total)", g, CROSS_GROUP_VERIFY_MATCH_RADIUS_PX, total);
                } else {
                    let mean = residuals.iter().map(|r| r.dist).sum::<f32>() / residuals.len() as f32;
                    let max  = residuals.iter().map(|r| r.dist).fold(f32::NEG_INFINITY, f32::max);
                    let unmatched_pct = 100.0 * unmatched.len() as f32 / total as f32;
                    info!("StackFrames: M_cross[{}] verification — {}/{} stars matched, mean residual={:.2}px, max={:.2}px, {} unmatched ({:.1}%)",
                          g, residuals.len(), total, mean, max, unmatched.len(), unmatched_pct);
                    residual_count = residuals.len();
                    residual_mean  = mean;

                    // Worst matched outliers (up to 10) — position + residual
                    // vector; angle=0° is "up", increasing clockwise.
                    let mut sorted = residuals;
                    sorted.sort_by(|a, b| b.dist.partial_cmp(&a.dist).unwrap_or(std::cmp::Ordering::Equal));
                    for (i, r) in sorted.iter().take(10).enumerate() {
                        let angle_deg = r.dx.atan2(-r.dy).to_degrees();
                        info!("StackFrames: M_cross[{}] outlier {} — star at ({:.1},{:.1}) residual=({:.2},{:.2}) dist={:.2}px angle={:.1}\u{00b0} (0\u{00b0}=up, clockwise)",
                              g, i, r.gx, r.gy, r.dx, r.dy, r.dist, angle_deg);
                    }

                    // Closest-miss unmatched stars (up to 15), sorted by
                    // attempted distance ascending — the near-boundary
                    // misses are the most informative for seeing where a
                    // rigid-only model starts breaking down spatially.
                    if !unmatched.is_empty() {
                        let mut sorted_unmatched = unmatched;
                        sorted_unmatched.sort_by(|a, b| a.dist.partial_cmp(&b.dist).unwrap_or(std::cmp::Ordering::Equal));
                        for (i, r) in sorted_unmatched.iter().take(15).enumerate() {
                            let angle_deg = r.dx.atan2(-r.dy).to_degrees();
                            info!("StackFrames: M_cross[{}] unmatched {} — star at ({:.1},{:.1}) nearest-miss=({:.2},{:.2}) dist={:.2}px angle={:.1}\u{00b0} (0\u{00b0}=up, clockwise)",
                                  g, i, r.gx, r.gy, r.dx, r.dy, r.dist, angle_deg);
                        }
                    }
                }
            }


            // ── Issue 128 / Issue 134: verification gate ────────────────────────
            // A garbage cross-group solve was previously logged (loudly) and
            // then stacked anyway (Issue 128). This angle-agnostic residual/
            // matched-star check — too few matched stars, or a mean residual
            // far above what a real solve produces (healthy sessions run
            // 0.28–0.47px) — is now the sole gate. Issue 134 removed the
            // companion rotation-plausibility check that used to run
            // alongside it: that check only made sense under the retired
            // 180°-flip assumption (see header note above) and had no valid
            // generalization once arbitrary relative orientations are
            // accepted — a spurious match is expected to already fail this
            // residual check on its own, since the transform is applied to
            // the same points it was fit from.
            let exclude_reason: Option<String> =
                if residual_count < CROSS_GROUP_MIN_MATCHED || residual_mean > CROSS_GROUP_MAX_RESIDUAL_PX {
                    Some(format!(
                        "verification matched {} stars (min {}), mean residual {:.2}px (max allowed {:.2}px)",
                        residual_count, CROSS_GROUP_MIN_MATCHED, residual_mean, CROSS_GROUP_MAX_RESIDUAL_PX
                    ))
                } else {
                    None
                };

            if let Some(reason) = exclude_reason {
                let frame_count = snapshots.iter().filter(|s| s.group == g).count();
                let msg = format!(
                    "StackFrames: cross-group {} solve rejected ({}) \u{2014} {} frame(s) from this group excluded",
                    g, reason, frame_count
                );
                info!("{}", msg);
                messages.push(msg);
                group_excluded[g] = true;
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

        // Issue 174: contributions stays positionally aligned with snapshots
        // through Pass 1 / Pass 2 (several sites index it by snapshot
        // position — see contributions.get(i) / get_mut(inp.snap_idx)), so
        // read-failed frames are NOT seeded here. They are appended after the
        // last positional access, just before StackSummary::compute, so the
        // summary reflects them without disturbing the aligned region.
        let mut contributions: Vec<FrameContribution> = Vec::new();
        let mut total_integration = 0.0f32;

        // Issue 175: per-frame pixel loads (below, replacing
        // load_frame_pixels) move onto the background reader. The request
        // list must include EXACTLY the frames that will reach the load —
        // group_excluded and the filter-mismatch check below both `continue`
        // BEFORE ever loading pixels, and both conditions are fully
        // determined by this point (nothing computed later in this loop
        // affects them). Requesting a load for a frame the loop will skip
        // would desync every recv() after it, since the channel is a
        // straight FIFO with no per-frame addressing. This precomputed
        // needs_pixel_load mirrors the two `continue` conditions below
        // exactly — if either of those conditions ever changes, this must
        // change with it.
        let needs_pixel_load: Vec<bool> = snapshots.iter().enumerate()
            .map(|(i, snap)| {
                if group_excluded[snap.group] {
                    return false;
                }
                filters_match(ref_filter.as_deref(), snap.filter.as_deref())
                    || i == group_refs[snap.group]
            })
            .collect();

        let pass1_kind = if is_color {
            crate::plugins::pixel_chunking::LoadKind::ColorNormalized
        } else {
            crate::plugins::pixel_chunking::LoadKind::Luma
        };
        let pass1_requests: Vec<crate::plugins::pixel_chunking::LoadRequest> = snapshots.iter()
            .zip(needs_pixel_load.iter())
            .filter(|(_, &needed)| needed)
            .map(|(snap, _)| crate::plugins::pixel_chunking::LoadRequest {
                path: snap.path.clone(),
                kind: pass1_kind,
            })
            .collect();
        let mut pass1_reader = crate::plugins::pixel_chunking::PixelReaderHandle::spawn_disk_reader(
            pass1_requests, crate::plugins::pixel_chunking::PREFETCH_SEQUENTIAL_DEPTH,
        );

        for (i, snap) in snapshots.iter().enumerate() {
            let mut contrib = FrameContribution::new(snap.index, &snap.path);
            contrib.filter           = snap.filter.clone();
            contrib.fwhm             = snap.fwhm;
            contrib.eccentricity     = snap.eccentricity;
            contrib.meridian_flipped = snap.group != master_group;

            // Issue 128: this frame's group failed cross-group validation —
            // exclude before any pixel loading or alignment work.
            if group_excluded[snap.group] {
                cross_group_failed(snap.index, &mut messages, &mut contrib, &mut contributions);
                continue;
            }

            // Filter validation (Issue 141). Comparison is trimmed and
            // case-folded via filters_match — "Ha", "ha", and "Ha " are the
            // same filter. A frame (or the master reference) with no FILTER
            // keyword is deliberately treated as matching any filter;
            // missing metadata should not disqualify a frame.
            //
            // The exemption is against this frame's own group reference,
            // not just the master reference: select_reference_in_group
            // already prefers a filter-matching candidate for every group
            // (falling back only when none exists), so this exemption is
            // now the safety net for that fallback case — without it, a
            // group whose only viable reference has a mismatched filter
            // would have that reference excluded here after it has already
            // been used to solve every other frame's transform in the
            // group (the original form of this bug).
            if !filters_match(ref_filter.as_deref(), snap.filter.as_deref())
                && i != group_refs[snap.group]
            {
                let msg = format!(
                    "Filter mismatch: frame {} ({}) excluded — stack filter is {}",
                    snap.index,
                    snap.filter.as_deref().unwrap_or("(none)"),
                    ref_filter.as_deref().unwrap_or("(none)")
                );
                info!("{}", msg);
                messages.push(msg);
                contrib.exclusion_reason = Some(ExclusionReason::FilterMismatch);
                contributions.push(contrib);
                continue;
            }

            // Load raw [0,1] pixels. Issue 175: sourced from pass1_reader
            // (spawned above) instead of a synchronous load_frame_pixels
            // call — this recv() lines up with pass1_requests 1:1 because
            // needs_pixel_load[i] (used to build that request list) mirrors
            // the two `continue`s above exactly, so every iteration that
            // reaches this point has a corresponding request already
            // in flight (or already delivered) on the reader thread.
            let frame_pixels: Vec<f32> = match pass1_reader.recv() {
                Some(crate::plugins::pixel_chunking::LoadOutcome::Loaded(loaded)) => {
                    match (is_color, loaded) {
                        (true, crate::plugins::pixel_chunking::LoadedFrame::ColorNormalized(cs)) => cs.rgb,
                        (false, crate::plugins::pixel_chunking::LoadedFrame::Luma(ls)) => ls.luma,
                        (_, _) => {
                            // Unreachable in practice: pass1_kind is fixed
                            // for the whole run and matches is_color.
                            buffer_unavailable(
                                snap.index,
                                "internal error — unexpected LoadedFrame kind for this stack's color mode",
                                &mut messages, &mut contrib, &mut contributions,
                            );
                            continue;
                        }
                    }
                }
                Some(crate::plugins::pixel_chunking::LoadOutcome::Missing { path }) => {
                    buffer_unavailable(
                        snap.index, &format!("source file missing: {}", path),
                        &mut messages, &mut contrib, &mut contributions,
                    );
                    continue;
                }
                Some(crate::plugins::pixel_chunking::LoadOutcome::Unreadable { path, error }) => {
                    buffer_unavailable(
                        snap.index, &format!("source file unreadable: {} ({})", path, error),
                        &mut messages, &mut contrib, &mut contributions,
                    );
                    continue;
                }
                None => {
                    // Shouldn't happen — pass1_requests was built 1:1 with
                    // the frames that reach this point, in loop order.
                    buffer_unavailable(
                        snap.index, "background reader closed early",
                        &mut messages, &mut contrib, &mut contributions,
                    );
                    continue;
                }
            };

            if group_ref_luma[snap.group].is_none() {
                let g_ref = &snapshots[group_refs[snap.group]];
                match load_debayered_luma(ctx, g_ref) {
                    Ok(g_luma_raw) => {
                        // Issue 140: normalize this group's reference by its
                        // own background median — matches the master
                        // reference (above execute()) and cross-group
                        // reference normalization, so every reference luma
                        // compute_translation sees in this file now uses
                        // the same convention as its target.
                        let g_bg      = estimate_background(&g_luma_raw, &bg_sigma_config).median;
                        let g_divisor = if g_bg > 1e-6 { g_bg } else { 1.0 };
                        let g_luma: Vec<f32> = g_luma_raw.iter().map(|&v| v / g_divisor).collect();
                        group_ref_stars[snap.group] = Some(g_ref.stars.clone());
                        group_ref_luma[snap.group]  = Some(g_luma);
                    }
                    Err(e) => {
                        // Issue 142: a group reference that fails to load
                        // once will not load later in the same run, so the
                        // whole group is excluded here rather than retried
                        // frame by frame — mirrors the cross-group solve's
                        // own reference-load failure handling above.
                        let msg = format!(
                            "StackFrames: group {} reference buffer unavailable ({}) — frames from this group will be excluded",
                            snap.group, e.message
                        );
                        info!("{}", msg);
                        messages.push(msg);
                        group_excluded[snap.group] = true;
                        cross_group_failed(snap.index, &mut messages, &mut contrib, &mut contributions);
                        continue;
                    }
                }
            }
            let g_ref_luma  = group_ref_luma[snap.group].as_ref().unwrap();
            let g_ref_stars = group_ref_stars[snap.group].as_ref().unwrap();

            // Background estimation, and the buffer used for FFT alignment
            // (Issue 143). Color: a genuine luminance extraction from RGB
            // (cal_luma) is needed for background estimation and alignment,
            // and is dropped as soon as normalized_luma is built from it —
            // nothing after this point reads cal_luma again. Mono: luma and
            // raw pixels are the same data, so frame_pixels is background-
            // estimated and normalized directly, with no separate clone —
            // the previous mono path cloned frame_pixels into cal_luma for
            // no reason, then normalized both it and frame_pixels
            // separately into two buffers holding identical values.
            let (bg_level, normalized_luma): (f32, Vec<f32>) = if is_color {
                let cal_luma = analysis::extract_luminance(&frame_pixels, width, height, 3);
                let bg = estimate_background(&cal_luma, &bg_sigma_config).median;
                let divisor = if bg > 1e-6 { bg } else { 1.0 };
                let normalized: Vec<f32> = cal_luma.par_iter().map(|&v| v / divisor).collect();
                (bg, normalized)
            } else {
                let bg = estimate_background(&frame_pixels, &bg_sigma_config).median;
                let divisor = if bg > 1e-6 { bg } else { 1.0 };
                let normalized: Vec<f32> = frame_pixels.par_iter().map(|&v| v / divisor).collect();
                (bg, normalized)
            };
            contrib.background_level = Some(bg_level);
            let divisor = if bg_level > 1e-6 { bg_level } else { 1.0 };

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
                        match try_rigid_refinement(
                            g_ref_stars, &snap.stars,
                            t.dx, t.dy, width, height,
                            snap.index, &mut messages,
                        ) {
                            Some(xform) => Some(xform),
                            // Issue 133 (Fix 2): RANSAC's own sanity checks
                            // rejected the match — exclude the frame rather
                            // than silently falling back to an unvalidated
                            // translation-only transform.
                            None => {
                                alignment_failed(snap.index, "RANSAC sanity check rejected the match",
                                                 &mut messages, &mut contrib, &mut contributions);
                                continue;
                            }
                        }
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
            let theta   = t_final.theta();

            // Issue 143: mono accumulates straight off normalized_luma —
            // it is already the same background-normalized data the old
            // accum_pixels duplicated from frame_pixels. Color still needs
            // a genuine second buffer here (full RGB, vs. normalized_luma's
            // single channel); frame_pixels is dropped immediately after
            // building it, since nothing below this point reads it again.
            if is_color {
                let accum_pixels: Vec<f32> = frame_pixels.iter().map(|&v| v / divisor).collect();
                drop(frame_pixels);

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
                    resample_frame_affine(&normalized_luma, width, height, &t_final)
                } else {
                    resample_frame(&normalized_luma, width, height, t_final.tx, t_final.ty)
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

        // Per-pixel stddev from Welford M2. Issue 144: uses the unbiased
        // sample variance M2 / (n - 1), not the population form M2 / n —
        // dividing by n systematically underestimates sigma, worst at small
        // frame counts (~10.6% low at n=5, ~5.1% at n=10, ~1.7% at n=30),
        // which meant a small stack was effectively clipping tighter than
        // its nominal STACK_SIGMA_CLIP threshold. The existing count > 1
        // guard already establishes n >= 2, so count - 1 is safe here.
        let stddev_buf: Vec<f32> = if is_color {
            let m2_ref = &m2_buf;
            count_buf.par_iter()
                .enumerate()
                .flat_map_iter(|(px, &count)| {
                    (0..3).map(move |ch| {
                        let idx = px * 3 + ch;
                        if count > 1 { (m2_ref[idx] / (count - 1) as f32).sqrt() } else { 0.0 }
                    })
                })
                .collect()
        } else {
            count_buf.par_iter()
                .zip(m2_buf.par_iter())
                .map(|(&count, &m2)| {
                    if count > 1 { (m2 / (count - 1) as f32).sqrt() } else { 0.0 }
                })
                .collect()
        };

        // Issue 144: number of output pixels that received data from fewer
        // than two contributing frames in Pass 1 — exactly the pixels
        // stddev_buf just set to 0.0 above, which means Pass 2's
        // sd_luma < 1e-10 fallback accepts them unconditionally with no
        // outlier protection at all. Most common at frame edges under
        // significant dither, or whenever the Issue 111 common-overlap crop
        // degenerates to the full uncropped canvas rather than trimming
        // low-coverage edges away. Computed here from count_buf (Pass 1
        // output, unmodified by Pass 2) rather than clip_count, which is a
        // different count — post-clip acceptances, not raw coverage.
        let low_coverage_pixels = count_buf.iter().filter(|&&c| c < 2).count();

        //  ── Pass 2 — sigma-clipped accumulation (batched parallel) ────────────

        let sigma      = STACK_SIGMA_CLIP;
        let n_threads  = (ctx.rayon_thread_count as usize).max(1);

        let mut sum_buf:    Vec<f64> = vec![0.0; n_pixels * n_channels];
        let mut clip_count: Vec<u32> = vec![0;  n_pixels];

        struct Pass2Input {
            snap_idx: usize,
            xform:    AffineRigid,
            divisor:  f32,
        }

        // Pixel data is loaded lazily inside the chunk loop to avoid
        // pre-allocating all frames simultaneously. Peak Pass 2 memory is
        // bounded to one batch at a time.
        let pass2_inputs: Vec<Pass2Input> = snapshots.iter().enumerate()
            .filter_map(|(i, _snap)| {
                let xform = cached_transforms[i].clone()?;
                // Issue 140: reuse the background-median divisor already
                // computed and recorded in Pass 1 (contrib.background_level)
                // instead of recomputing estimate_background a second time
                // on the same pixel data in Pass 2.
                let divisor = contributions.get(i)
                    .and_then(|c| c.background_level)
                    .map(|bg| if bg > 1e-6 { bg } else { 1.0 })
                    .unwrap_or(1.0);
                Some(Pass2Input { snap_idx: i, xform, divisor })
            })
            .collect();

        // Issue 175: folds Pass 2's own read step onto the shared
        // background reader instead of the private read_frame_from_disk
        // loop it used to run inline below. pass2_inputs' order IS the
        // request order — no separate needs-load precomputation required
        // here (unlike Pass 1), since pass2_inputs is already exactly the
        // filtered set of frames this pass will read, in the order it
        // will read them.
        let pass2_kind = if is_color {
            crate::plugins::pixel_chunking::LoadKind::ColorNormalized
        } else {
            crate::plugins::pixel_chunking::LoadKind::Luma
        };
        let pass2_requests: Vec<crate::plugins::pixel_chunking::LoadRequest> = pass2_inputs.iter()
            .map(|inp| crate::plugins::pixel_chunking::LoadRequest {
                path: snapshots[inp.snap_idx].path.clone(),
                kind: pass2_kind,
            })
            .collect();
        let mut pass2_reader = crate::plugins::pixel_chunking::PixelReaderHandle::spawn_disk_reader(
            pass2_requests, crate::plugins::pixel_chunking::prefetch_capacity_chunked(ctx),
        );

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
                // Issue 175: sourced from pass2_reader (spawned above)
                // instead of a synchronous read_frame_from_disk call —
                // this recv() lines up with pass2_requests 1:1 because
                // both were built from pass2_inputs in the same order, and
                // this loop (flattened across all chunks) visits
                // pass2_inputs in that same order too.
                let outcome = pass2_reader.recv();
                match outcome {
                    Some(crate::plugins::pixel_chunking::LoadOutcome::Loaded(loaded)) => {
                        let px = match (is_color, loaded) {
                            (true, crate::plugins::pixel_chunking::LoadedFrame::ColorNormalized(cs)) => Some(cs.rgb),
                            (false, crate::plugins::pixel_chunking::LoadedFrame::Luma(ls)) => Some(ls.luma),
                            (_, _) => None, // unreachable: pass2_kind is fixed for the whole run
                        };
                        match px {
                            Some(px) => {
                                chunk_ok.push(inp);
                                chunk_pixels.push(px);
                            }
                            None => {
                                let msg = format!(
                                    "Frame {}: internal error — unexpected LoadedFrame kind for this stack's color mode during stacking — excluded",
                                    snap.index
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
                    other => {
                        // Missing/Unreadable/None (reader closed early) all
                        // route to the same exclude-and-continue pathway,
                        // matching Issue 174's exclude-and-continue policy
                        // for StackFrames — a degraded stack is still
                        // useful.
                        let reason = match other {
                            Some(crate::plugins::pixel_chunking::LoadOutcome::Missing { path }) =>
                                format!("source file missing: {}", path),
                            Some(crate::plugins::pixel_chunking::LoadOutcome::Unreadable { path, error }) =>
                                format!("source file unreadable: {} ({})", path, error),
                            None =>
                                "background reader closed early".to_string(),
                            Some(crate::plugins::pixel_chunking::LoadOutcome::Loaded(_)) =>
                                unreachable!("Loaded is handled in the arm above"),
                        };
                        let msg = format!(
                            "Frame {}: {} during stacking — excluded", snap.index, reason
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

            // Parallel: resample each frame in chunk, reusing the
            // background-median divisor Pass 1 already computed for this
            // frame (Issue 140) rather than recomputing estimate_background
            // a second time on identical pixel data.
            let aligned_buffers: Vec<Vec<f32>> = chunk_ok.par_iter()
                .zip(chunk_pixels.into_par_iter())
                .map(|(inp, frame_pixels)| {
                    let divisor = inp.divisor;
                    let theta   = inp.xform.theta();

                    // Issue 143: cal_luma was computed unconditionally here
                    // (a real extract_luminance in the color case, a full
                    // clone in the mono case) but only ever consumed by the
                    // mono branch's `normalized` — the color branch used
                    // accum_pixels (built from frame_pixels directly) and
                    // never read cal_luma at all, so the color case paid
                    // for a full luminance extraction and threw the result
                    // away. Both branches now build exactly one buffer,
                    // directly from frame_pixels.
                    if is_color {
                        let accum_pixels: Vec<f32> = frame_pixels.iter().map(|&v| v / divisor).collect();
                        if theta.abs() >= MIN_ROTATION_TO_APPLY || inp.xform.a < 0.5 {
                            resample_frame_rgb_affine(&accum_pixels, width, height, &inp.xform)
                        } else {
                            resample_frame_rgb(&accum_pixels, width, height, inp.xform.tx, inp.xform.ty)
                        }
                    } else {
                        let normalized: Vec<f32> = frame_pixels.iter().map(|&v| v / divisor).collect();
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

        // Issue 174: append the frames that failed to read in
        // collect_snapshots as excluded contributions. This happens after
        // the last positional access to `contributions` (the crop loop
        // above), so appending cannot misalign snapshot-indexed lookups. The
        // frames carry their true file_list index and are marked
        // BufferUnavailable, matching the exclusion reason used when a read
        // fails in the later passes — so a source file that is missing or
        // unreadable is surfaced in the stack summary regardless of which
        // pass first tried to read it, never silently dropped.
        for f in &read_failures {
            let mut contrib = FrameContribution::new(f.index, &f.path);
            contrib.included         = false;
            contrib.exclusion_reason = Some(ExclusionReason::BufferUnavailable);
            let msg = format!(
                "Frame {}: {} — excluded from stack", f.index, f.reason
            );
            info!("StackFrames: {}", msg);
            messages.push(msg);
            contributions.push(contrib);
        }

        let mut summary = StackSummary::compute(&contributions, &completed_at, low_coverage_pixels);
        summary.target              = ref_target;
        summary.filter              = ref_filter;
        summary.integration_seconds = total_integration;

        ctx.stack_contributions = contributions;
        ctx.stack_summary       = Some(summary.clone());

        // Issue 144: only add the low-coverage line when it's actually
        // nonzero, so a normal well-overlapped stack's summary doesn't grow
        // a permanent "0 pixels" line that nobody needs to read.
        let low_coverage_line = if summary.low_coverage_pixels > 0 {
            format!("\n  Low-coverage pixels:   {} (unclipped, <2 contributing frames)", summary.low_coverage_pixels)
        } else {
            String::new()
        };

        let quality_summary = format!(
            "Stack Quality Summary:\n  Frames stacked:        {} / {}\n  SNR improvement:       ~{:.1}x (vs single frame)\n  Alignment success:     {:.1}%\n  Background uniformity: {}\n  Output mode:           {}{}",
            summary.stacked_frames, summary.total_frames,
            summary.snr_improvement, summary.alignment_success_rate * 100.0,
            summary.background_uniformity,
            if is_color { "RGB color" } else { "Grayscale" },
            low_coverage_line,
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

//     Output normalization

/// Computes low/high pixel-value bounds at the given percentiles (0–100)
/// via partial selection — O(n) per call on a scratch clone, no full sort
/// needed. Used by `normalize_output` (Issue 145) to derive a display range
/// robust to a handful of extreme pixels, rather than the frame's absolute
/// min/max.
fn percentile_bounds(data: &[f32], low_pct: f32, high_pct: f32) -> (f32, f32) {
    let n = data.len();
    if n == 0 {
        return (0.0, 1.0);
    }
    let mut work: Vec<f32> = data.to_vec();
    let idx_low  = (((low_pct  / 100.0) * (n - 1) as f32).round() as usize).min(n - 1);
    let idx_high = (((high_pct / 100.0) * (n - 1) as f32).round() as usize).min(n - 1);

    let cmp = |a: &f32, b: &f32| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal);
    let low_val  = *work.select_nth_unstable_by(idx_low, cmp).1;
    let high_val = *work.select_nth_unstable_by(idx_high, cmp).1;
    (low_val, high_val)
}

/// Stretches raw stacked pixel data into the [0.0, 1.0] display range for
/// Photyx's own preview (Issue 145). This is a display-normalized preview
/// stretch, not linear data suitable for photometric reuse or cross-session
/// comparison — Photyx's stack result is a quick-look validation step
/// before real processing happens in PixInsight from the original frames,
/// not an image intended for further downstream processing itself.
///
/// Bounds are the 0.1st and 99.99th percentiles rather than the frame's
/// absolute min/max, so a handful of hot-pixel or cosmic-ray defect pixels
/// can't single-handedly compress the rest of the frame into a sliver of
/// the display range — the failure mode this function existed to prevent
/// in the previous absolute-extremes version, where a single surviving
/// defect pixel set the scale for the entire image. Values outside the
/// percentile window (genuine bright star cores as well as defects) are
/// pushed to the clamp bounds, which is expected.
///
/// `is_color` and `_n_pixels` are unused: percentile bounds are computed
/// across the full interleaved buffer either way, matching the previous
/// version's reasoning for the color case (a single global bound across
/// all three channels preserves color ratios; per-channel normalization
/// would destroy color balance) — the two branches were already doing
/// identical work before this change, just written twice.
fn normalize_output(raw: &[f32], _is_color: bool, _n_pixels: usize) -> Vec<f32> {
    const NORMALIZE_LOW_PERCENTILE:  f32 = 0.1;
    const NORMALIZE_HIGH_PERCENTILE: f32 = 99.99;

    let (min_val, max_val) = percentile_bounds(raw, NORMALIZE_LOW_PERCENTILE, NORMALIZE_HIGH_PERCENTILE);
    let range = (max_val - min_val).max(1e-6);
    raw.par_iter().map(|&v| ((v - min_val) / range).clamp(0.0, 1.0)).collect()
}

//  ── Snapshot collection ───────────────────────────────────────────────────────

/// A frame that could not be read from disk during collect_snapshots.
/// Carries the true file_list index and path so execute() can seed it as an
/// excluded contribution (ExclusionReason::BufferUnavailable) once the
/// contributions vec exists — collect_snapshots runs before that vec is
/// built, so it cannot record the exclusion itself.
struct FrameReadFailure {
    index:  usize,
    path:   String,
    reason: String,
}

fn collect_snapshots(
    _ctx:       &AppContext,
    det_config: &StarDetectionConfig,
) -> Result<(Vec<FrameSnapshot>, Vec<FrameReadFailure>), PluginError> {
    let mut snapshots: Vec<FrameSnapshot> = Vec::new();
    let mut read_failures: Vec<FrameReadFailure> = Vec::new();

    // Issue 175: kind=Luma — this pass only ever needed luminance (for
    // star detection), never the raw pixel buffer, so the debayer +
    // extract_luminance work that used to happen inline below now happens
    // on the reader thread instead, overlapping with THIS frame's star
    // detection on the main thread. Request order matches _ctx.file_list
    // order 1:1, so each loop iteration's recv() corresponds to that
    // iteration's (index, path) — the reader is never asked to reorder.
    let requests: Vec<crate::plugins::pixel_chunking::LoadRequest> = _ctx.file_list.iter()
        .map(|path| crate::plugins::pixel_chunking::LoadRequest {
            path: path.clone(),
            kind: crate::plugins::pixel_chunking::LoadKind::Luma,
        })
        .collect();
    let mut reader = crate::plugins::pixel_chunking::PixelReaderHandle::spawn_disk_reader(
        requests, crate::plugins::pixel_chunking::PREFETCH_SEQUENTIAL_DEPTH,
    );

    for (index, path) in _ctx.file_list.iter().enumerate() {
        // Issue 174 (exclude-and-continue policy unchanged by 175): a
        // missing or unreadable file is recorded as a read-failure so
        // execute() can seed it as an excluded contribution surfaced in
        // the stack summary, rather than silently skipped.
        let outcome = match reader.recv() {
            Some(o) => o,
            // Shouldn't happen under normal operation (one send per
            // request, checked 1:1 against file_list above) — handled as
            // a read failure rather than left to silently under-populate
            // snapshots, consistent with collect_snapshots' own
            // exclude-and-continue policy for every other failure mode.
            None => {
                let reason = "background reader closed early".to_string();
                info!("StackFrames: collect_snapshots — {} ({})", path, reason);
                read_failures.push(FrameReadFailure { index, path: path.clone(), reason });
                continue;
            }
        };

        let snap = match outcome {
            crate::plugins::pixel_chunking::LoadOutcome::Loaded(
                crate::plugins::pixel_chunking::LoadedFrame::Luma(snap)
            ) => snap,
            crate::plugins::pixel_chunking::LoadOutcome::Loaded(_) => {
                // Unreachable in practice: this reader was spawned with
                // LoadKind::Luma requests only.
                let reason = "internal error — unexpected non-Luma LoadedFrame for a Luma request".to_string();
                info!("StackFrames: collect_snapshots — {} ({})", path, reason);
                read_failures.push(FrameReadFailure { index, path: path.clone(), reason });
                continue;
            }
            crate::plugins::pixel_chunking::LoadOutcome::Missing { path: _ } => {
                let reason = "source file missing".to_string();
                info!("StackFrames: collect_snapshots — {} ({})", path, reason);
                read_failures.push(FrameReadFailure { index, path: path.clone(), reason });
                continue;
            }
            crate::plugins::pixel_chunking::LoadOutcome::Unreadable { path: _, error } => {
                info!("StackFrames: collect_snapshots — {} ({})", path, error);
                read_failures.push(FrameReadFailure { index, path: path.clone(), reason: error });
                continue;
            }
        };

        let width    = snap.width;
        let height   = snap.height;
        let channels = snap.channels;

        // luma arrives pre-converted (debayered + extracted, or a
        // straight to_luminance pass) from the reader thread. Issue 175:
        // no bayer_pattern to derive here anymore — the debayer decision
        // now happens entirely on the read side, keyed off the file's own
        // color_space at read time (pixel_chunking::load_request), so
        // FrameSnapshot no longer caches a pattern for later use.

        let mut stars    = detect_stars(&snap.luma, width, height, det_config);
        let fwhm         = if stars.len() >= 5 { compute_fwhm(&stars, None).map(|r| r.fwhm_pixels) } else { None };
        let eccentricity = if stars.len() >= 5 { compute_eccentricity(&stars).map(|r| r.eccentricity) } else { None };

        // Issue 143 (unchanged): StarCandidate.patch is consumed only by
        // compute_fwhm/compute_eccentricity above — dropped here rather
        // than retained for the life of the plugin. See original comment
        // for the full memory-retention rationale.
        for s in stars.iter_mut() {
            s.patch = Vec::new();
        }

        let filter   = snap.keywords.get("FILTER").map(|kw| kw.value.clone());
        let exptime  = snap.keywords.get("EXPTIME").and_then(|kw| kw.value.parse::<f32>().ok());
        let rotator  = snap.keywords.get("ROTATOR").and_then(|kw| kw.value.parse::<f32>().ok());
        let date_obs = snap.keywords.get("DATE-OBS").and_then(|kw| parse_date_obs(&kw.value));

        snapshots.push(FrameSnapshot {
            index,
            path: path.clone(),
            width, height, channels,
            color_space:   snap.color_space,
            filter, exptime, fwhm, eccentricity, rotator, stars, date_obs,
            group: 0,
        });
    }

    if !snapshots.is_empty() {
        assign_groups(&mut snapshots);
    }

    Ok((snapshots, read_failures))
}

//  ── Group assignment ──────────────────────────────────────────────────────────

/// Resolves the rotator delta (degrees) between two consecutive frames for
/// grouping purposes.
///
/// When both frames carry a `ROTATOR` keyword, this is the literal keyword
/// delta — unchanged behavior. When either is missing (Issue 133), falls
/// back to triangle-based star matching as a substitute rotation signal:
/// `estimate_rigid_transform_triangles()` is scale/rotation-invariant and
/// needs no FFT pre-translation, unlike the RANSAC path used elsewhere in
/// this file, so it works without any prior orientation hint.
///
/// Returns `None` only when the substitute check itself is inconclusive
/// (too few stars, no confident triangle match) — the caller treats `None`
/// as "force a new group": an inconclusive signal defaults to splitting
/// rather than risking a silent cross-orientation merge.
fn resolve_rot_diff(prev: &FrameSnapshot, curr: &FrameSnapshot) -> Option<f32> {
    if let (Some(a), Some(b)) = (prev.rotator, curr.rotator) {
        return Some((b - a).abs());
    }
    estimate_rigid_transform_triangles(&prev.stars, &curr.stars)
        .map(|xform| xform.theta().to_degrees().abs())
}

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

        let rotator_missing = snapshots[i - 1].rotator.is_none() || snapshots[i].rotator.is_none();
        let rot_diff = resolve_rot_diff(&snapshots[i - 1], &snapshots[i]);

        let split = match rot_diff {
            Some(diff) => {
                diff > MERIDIAN_FLIP_THRESHOLD
                    || (time_gap > SESSION_GAP_MINUTES as f64 && diff > ROTATOR_GROUP_TOLERANCE)
            }
            // Issue 133: ROTATOR missing on at least one side and the
            // triangle-match substitute was itself inconclusive — split
            // rather than risk silently merging two different
            // orientations into one group.
            None => true,
        };

        if split {
            group += 1;
            if rotator_missing {
                info!(
                    "StackFrames: group split at frame {} — ROTATOR missing, \
                     resolved via triangle-match substitute (rot_diff={:?})",
                    snapshots[i].index, rot_diff
                );
            }
        }
        snapshots[i].group = group;
    }
}

//  ── Reference frame selection ─────────────────────────────────────────────────

/// True if `a` and `b` should be treated as the same filter (Issue 141): a
/// missing FILTER keyword on either side is deliberately treated as
/// "matches anything" — missing metadata should not disqualify a frame —
/// otherwise the comparison is trimmed and case-folded, so "Ha", "ha", and
/// "Ha " are all the same filter.
fn filters_match(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.trim().eq_ignore_ascii_case(b.trim()),
        _ => true,
    }
}

/// Selects the best-quality frame within a group to serve as its reference,
/// restricted to candidates whose star count is at least REF_MIN_STAR_FRACTION
/// of the group's own median star count (Issue 127). Prevents a cloud-obscured
/// frame — which can measure deceptively tight FWHM because only the
/// brightest stars punch through the murk — from winning reference selection
/// on FWHM/eccentricity alone despite having a fraction of the real star
/// population. The gate is per-group, not session-wide, since pre-flip and
/// post-flip populations legitimately differ in size.
///
/// `required_filter`, when given, further narrows the candidate pool to
/// frames whose FILTER matches it via `filters_match` (Issue 141) — pass the
/// master reference's filter when selecting a non-master group's reference,
/// so a group's reference is chosen to match the stack's filter whenever
/// possible. Pass `None` when selecting the master group's own reference,
/// since there is no stack filter yet to compare against. If no candidate in
/// the group matches the required filter, selection falls back to the
/// pre-filter pool and warns, rather than leaving the group without a
/// reference — the Pass 1 filter-exclusion check exempts whichever frame
/// this function returns, so a mismatched fallback reference still stacks,
/// it just isn't silently treated as filter-clean.
///
/// Returns the chosen index plus an optional warning message (for the caller
/// to log/surface) when either gate fell back to a wider pool. If both the
/// star-count gate and the filter gate fell back, both messages are
/// returned joined together rather than one silently dropping the other.
fn select_reference_in_group(
    snapshots: &[FrameSnapshot],
    group: usize,
    required_filter: Option<&str>,
) -> (usize, Option<String>) {
    let member_indices: Vec<usize> = snapshots.iter()
        .enumerate()
        .filter(|(_, s)| s.group == group)
        .map(|(i, _)| i)
        .collect();

    if member_indices.is_empty() {
        return (0, None);
    }

    let mut star_counts: Vec<usize> = member_indices.iter()
        .map(|&i| snapshots[i].stars.len())
        .collect();
    star_counts.sort_unstable();
    let median_stars = star_counts[star_counts.len() / 2] as f32;
    let min_stars     = median_stars * REF_MIN_STAR_FRACTION as f32;

    let candidate_indices: Vec<usize> = member_indices.iter()
        .cloned()
        .filter(|&i| snapshots[i].stars.len() as f32 >= min_stars)
        .collect();
    let passed = candidate_indices.len();

    let (pool, star_warning): (Vec<usize>, Option<String>) = if candidate_indices.is_empty() {
        let msg = format!(
            "Group {}: reference candidacy gate found no frame with star count \u{2265} {:.0} \
             (median {:.0} \u{00d7} {:.2}) \u{2014} falling back to best-available by quality score \
             across all {} frames",
            group, min_stars, median_stars, REF_MIN_STAR_FRACTION, member_indices.len()
        );
        (member_indices.clone(), Some(msg))
    } else {
        (candidate_indices, None)
    };

    info!(
        "StackFrames: Group {} reference candidacy — {}/{} frames passed median star-count gate \
         (median={:.0}, min={:.0})",
        group, passed, member_indices.len(), median_stars, min_stars
    );

    // Issue 141: further narrow to frames whose filter matches the stack
    // filter, so a group's reference — which every other frame in the group
    // aligns against, and which the Pass 1 filter check exempts from
    // exclusion — is never itself the frame that mismatches the stack
    // filter unless nothing in the group's candidate pool qualifies.
    let (final_pool, filter_warning): (Vec<usize>, Option<String>) = if let Some(rf) = required_filter {
        let filtered: Vec<usize> = pool.iter()
            .cloned()
            .filter(|&i| filters_match(Some(rf), snapshots[i].filter.as_deref()))
            .collect();
        if filtered.is_empty() {
            let msg = format!(
                "Group {}: no candidate matches stack filter \u{201c}{}\u{201d} \u{2014} falling back \
                 to best-available by quality score, ignoring filter",
                group, rf
            );
            (pool, Some(msg))
        } else {
            (filtered, None)
        }
    } else {
        (pool, None)
    };

    let warning = match (star_warning, filter_warning) {
        (Some(a), Some(b)) => Some(format!("{}; {}", a, b)),
        (Some(a), None)    => Some(a),
        (None, Some(b))    => Some(b),
        (None, None)       => None,
    };

    let chosen = final_pool.iter()
        .cloned()
        .max_by(|&a, &b| {
            quality_score(&snapshots[a]).partial_cmp(&quality_score(&snapshots[b]))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(member_indices[0]);

    (chosen, warning)
}


fn quality_score(snap: &FrameSnapshot) -> f32 {
    crate::analysis::frame_quality_score(snap.fwhm, snap.eccentricity)
}

//  ── Frame pixel loading ───────────────────────────────────────────────────────

// Issue 175: read_frame_from_disk removed — it's now fully unused.
// collect_snapshots (Pass 0), Pass 1's per-frame loads, and Pass 2's
// per-frame loads were migrated to the shared background reader in
// earlier Issue 175 steps; load_debayered_luma (below) — the one
// remaining caller — now routes through pixel_chunking::load_request
// directly instead. As of this change, pixel_chunking.rs is the only
// place in the codebase that calls image_reader::read_image_file for
// these pipelines — the single-reader-of-FITS-files property is now
// structural (one module), not just conventional (one function per
// file, trusted not to be duplicated elsewhere).

fn load_debayered_luma(_ctx: &AppContext, snap: &FrameSnapshot) -> Result<Vec<f32>, PluginError> {
    // Issue 175: routed through pixel_chunking::load_request — the same
    // per-file loader PixelReaderHandle's background reader uses — rather
    // than a second, independent copy of the debayer-or-luminance
    // branching logic. Called synchronously here, NOT through
    // PixelReaderHandle: this is a bounded, one-off load (at most a
    // handful of calls per run — master reference, per-group cross-group
    // reference, per-group Pass 1 lazy-fill — never a loop over many
    // frames), so there's no concurrent compute to overlap the read
    // against, and spinning up a reader thread here would be pure
    // overhead rather than the overlap the prefetch design targets.
    match crate::plugins::pixel_chunking::load_request(
        &snap.path, crate::plugins::pixel_chunking::LoadKind::Luma,
    ) {
        crate::plugins::pixel_chunking::LoadOutcome::Loaded(
            crate::plugins::pixel_chunking::LoadedFrame::Luma(luma_snap)
        ) => Ok(luma_snap.luma),
        crate::plugins::pixel_chunking::LoadOutcome::Loaded(_) => {
            // Unreachable in practice: load_request was called with
            // LoadKind::Luma above.
            Err(PluginError::new(
                "INTERNAL_ERROR",
                "load_debayered_luma: unexpected non-Luma LoadedFrame for a Luma request",
            ))
        }
        crate::plugins::pixel_chunking::LoadOutcome::Missing { path } => {
            // Same error code + message shape read_frame_from_disk used
            // to produce, preserved so nothing downstream that matched on
            // e.code or logged e.message sees a behavior change — none of
            // load_debayered_luma's three call sites branch on the code
            // today (all three just propagate or log e.message), but
            // matching the prior contract exactly is cheap insurance.
            Err(PluginError::new(
                "SOURCE_FILE_MISSING",
                &format!("source file missing: {}", path),
            ))
        }
        crate::plugins::pixel_chunking::LoadOutcome::Unreadable { path, error } => {
            Err(PluginError::new(
                "SOURCE_FILE_UNREADABLE",
                &format!("source file unreadable: {} ({})", path, error),
            ))
        }
    }
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
) -> Option<AffineRigid> {
    match estimate_rigid_transform(ref_stars, frm_stars, fft_dx, fft_dy, width, height) {
        Some(refined) => {
            let theta = refined.theta();
            info!("Frame {}: RANSAC match — tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°)",
                  frame_idx, refined.tx, refined.ty, theta, theta.to_degrees());
            Some(refined)
        }
        None => {
            // Issue 133 (Fix 2): previously silently fell back to a
            // translation-only transform when RANSAC's own sanity checks
            // rejected the refined match — this could stack a frame using
            // an unvalidated FFT translation with no rotation applied,
            // even when the true motion included rotation RANSAC couldn't
            // confirm. Now excluded via the caller's alignment_failed()
            // path instead, consistent with the FFT-failure case just
            // above it.
            let msg = format!(
                "Frame {}: RANSAC match failed — excluding frame", frame_idx
            );
            info!("{}", msg);
            messages.push(msg);
            None
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

fn cross_group_failed(
    frame_idx:     usize,
    messages:      &mut Vec<String>,
    contrib:       &mut FrameContribution,
    contributions: &mut Vec<FrameContribution>,
) {
    // Reason detail already logged once at the group level when the solve
    // was rejected — kept terse here to avoid repeating it per frame.
    let msg = format!(
        "Frame {}: excluded \u{2014} this frame's group failed cross-group validation", frame_idx
    );
    info!("{}", msg);
    messages.push(msg);
    contrib.exclusion_reason = Some(ExclusionReason::CrossGroupFailed);
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


//  ── Tests (Issue 149) ───────────────────────────────────────────────────────
// These pin the sign convention shared across fft_align::compute_translation,
// star_align::estimate_rigid_transform, and this file's resample_frame_affine
// (Issue 131/132) — the specific failure mode was two compensating sign
// errors producing correct output, so a partial revert of the fix would
// restore working alignment while leaving the code semantically wrong.
// Test data here is synthetic rather than real, deliberately: the whole
// point is an exactly-known ground-truth transform, which real frames
// cannot provide.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::stars::StarCandidate;

    fn make_test_star(cx: f32, cy: f32) -> StarCandidate {
        StarCandidate { cx, cy, peak: 0.8, bbox: (0, 0, 1, 1), patch: vec![], pixel_count: 1 }
    }

    /// Flat background with Gaussian blob "stars" at the given sub-pixel
    /// positions, matching the pattern fft_align.rs's own tests use.
    fn synth_image(width: usize, height: usize, stars: &[(f32, f32)]) -> Vec<f32> {
        let mut img = vec![0.05f32; width * height];
        for &(cx, cy) in stars {
            let x0 = (cx as i32 - 6).max(0);
            let x1 = (cx as i32 + 6).min(width as i32 - 1);
            let y0 = (cy as i32 - 6).max(0);
            let y1 = (cy as i32 + 6).min(height as i32 - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let r2 = dx * dx + dy * dy;
                    img[y as usize * width + x as usize] += 0.6 * (-r2 / 5.0).exp();
                }
            }
        }
        img
    }

    /// Builds the "frame" image a sensor would see under `xform` (frame ->
    /// reference space, matching AffineRigid::apply_forward's documented
    /// direction): for each frame-space pixel (fx, fy), samples `reference`
    /// at apply_forward(fx, fy). This is the mirror operation of
    /// resample_frame_affine, which warps frame data into reference-space
    /// output via apply_inverse — used only to construct test input, not
    /// production code.
    fn warp_forward(reference: &[f32], width: usize, height: usize, xform: &AffineRigid) -> Vec<f32> {
        let mut out = vec![0.05f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let (rx, ry) = xform.apply_forward(x as f32, y as f32);
                let x0 = rx.floor() as i32;
                let y0 = ry.floor() as i32;
                let fx = rx - x0 as f32;
                let fy = ry - y0 as f32;
                out[y * width + x] = bilinear(reference, width, height, x0, y0, x0 + 1, y0 + 1, fx, fy);
            }
        }
        out
    }

    #[test]
    fn test_end_to_end_translation_rotation_round_trip() {
        let (w, h) = (512usize, 512usize);

        // Ten well-separated star positions, matching star_align.rs's own
        // test convention, with enough margin from the canvas edges that
        // the translation below never pushes a star's sampling footprint
        // out of bounds.
        let ref_positions: [(f32, f32); 10] = [
            (100.0, 100.0), (400.0, 120.0), (250.0, 300.0), (380.0, 380.0),
            (120.0, 350.0), (300.0, 150.0), (200.0, 250.0), (350.0, 200.0),
            (150.0, 150.0), (300.0, 400.0),
        ];
        let reference = synth_image(w, h, &ref_positions);

        // Known ground-truth transform, frame -> reference space. Rotation
        // magnitude matches the real within-group regime measured in
        // Issue 146 (worst observed residual ~0.01°, so 0.3° gives a
        // comfortable margin above the noise floor while staying well
        // inside MAX_ROTATION_RAD); translation matches a typical dither
        // offset.
        let theta_true = 0.3f32.to_radians();
        let xform_true = AffineRigid {
            a: theta_true.cos(),
            b: theta_true.sin(),
            tx: 12.0,
            ty: -7.0,
        };

        let frame = warp_forward(&reference, w, h, &xform_true);

        let ref_stars: Vec<StarCandidate> = ref_positions.iter()
            .map(|&(x, y)| make_test_star(x, y))
            .collect();
        // Frame-space star positions are the inverse of the known
        // transform applied to the reference positions — same convention
        // star_align.rs's own rotation test (test_triangle_matching_rotation)
        // uses to build frame_stars from a known ref->frame relationship.
        let frame_stars: Vec<StarCandidate> = ref_positions.iter()
            .map(|&(x, y)| {
                let (fx, fy) = xform_true.apply_inverse(x, y);
                make_test_star(fx, fy)
            })
            .collect();

        // Full chain under test: FFT translation estimate -> RANSAC-refined
        // rigid transform -> resample.
        let fft_t = compute_translation(&reference, &frame, w, h)
            .expect("FFT should find a translation estimate");
        let recovered = estimate_rigid_transform(
            &ref_stars, &frame_stars, fft_t.dx, fft_t.dy, w, h,
        ).expect("RANSAC should recover a transform from clean synthetic data");

        let resampled = resample_frame_affine(&frame, w, h, &recovered);

        // Each star should land back at its original reference-space
        // position, within sub-pixel tolerance. A single-sided sign error
        // anywhere in the chain (the Step 8 negation in compute_translation,
        // the Step 1 pre-translation sign in estimate_rigid_transform, or
        // resample_frame_affine's apply_inverse convention) would place
        // stars at roughly double the true offset or in the wrong
        // direction entirely — failing this by several pixels, not a
        // fraction of one.
for &(rx, ry) in &ref_positions {
            let x0 = rx.floor() as i32;
            let y0 = ry.floor() as i32;
            let fx = rx - x0 as f32;
            let fy = ry - y0 as f32;
            let val = bilinear(&resampled, w, h, x0, y0, x0 + 1, y0 + 1, fx, fy);
            assert!(
                val > 0.4,
                "star at ({:.1},{:.1}) not recovered in resampled frame — \
                 sampled value {:.3} (expected > 0.4; a low value here \
                 indicates a possible sign inversion somewhere in the \
                 compute_translation -> estimate_rigid_transform -> \
                 resample_frame_affine chain)",
                rx, ry, val
            );
        }
    }

    #[test]
    fn test_resample_frame_affine_uses_inverse_map() {
        // Issue 149: isolates resample_frame_affine's apply_inverse
        // convention specifically, using a pure translation (no rotation)
        // so the two candidate failure locations below are unambiguous
        // and don't overlap with any rotation-related error.
        let (w, h) = (64usize, 64usize);
        let mut frame = vec![0.0f32; w * h];
        let (src_x, src_y) = (20usize, 20usize);
        frame[src_y * w + src_x] = 1.0;

        let xform = AffineRigid::translation(10.0, -5.0);
        let resampled = resample_frame_affine(&frame, w, h, &xform);

        // Correct (apply_inverse): the frame's bright pixel, mapped
        // forward by the transform, should appear at (30, 15) in the
        // output — this is the actual production contract, matching how
        // t_final (frame -> reference-canvas space) is used everywhere
        // else in this file.
        let (expect_x, expect_y) = (30usize, 15usize);
        assert!(
            resampled[expect_y * w + expect_x] > 0.9,
            "expected bright pixel at ({}, {}) [apply_forward(src)], got {:.3}",
            expect_x, expect_y, resampled[expect_y * w + expect_x]
        );

        // Wrong-direction alternative (what a apply_forward-instead-of-
        // apply_inverse bug would produce): the pixel would land at
        // apply_inverse(src) = (10, 25) instead. Confirms the test
        // actually discriminates direction, not just presence.
        let (wrong_x, wrong_y) = (10usize, 25usize);
        assert!(
            resampled[wrong_y * w + wrong_x] < 0.1,
            "found bright pixel at ({}, {}) — this is apply_inverse(src), \
             the wrong-direction result a forward/inverse swap in \
             resample_frame_affine would produce",
            wrong_x, wrong_y
        );
    }
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
