// analysis/star_align.rs — RANSAC affine rigid transform alignment + triangle matching
#![allow(dead_code)]
//
// Estimates a 2D rigid transform (rotation + translation, scale fixed at 1.0)
// between a reference frame and a target frame using star centroids.
//
// The transform is expressed as a pure affine map:
//
//   [rx]   [a  -b] [fx]   [tx]
//   [ry] = [b   a] [fy] + [ty]
//
// where a = cos(θ), b = sin(θ), (tx, ty) = translation.
//
// Critically, NO assumption is made about the rotation center. The rotation
// center is implicit in (tx, ty) and is solved directly from matched star
// pairs. This is the correct approach when the physical rotation center is
// unknown or offset from the image center.
//
// Two alignment strategies are provided:
//
//   estimate_rigid_transform()          — FFT-primed RANSAC (original)
//   estimate_rigid_transform_triangles() — Triangle-based matching (new)
//
// Triangle matching does not require an FFT pre-translation and is more
// robust for cross-group (meridian flip) alignment where the rotation may
// be large and poorly constrained by local star pairs.

use crate::analysis::stars::StarCandidate;
use crate::settings::defaults::{
    MATCH_TOLERANCE, MIN_MATCHES, INLIER_TOLERANCE, MIN_INLIERS,
    MAX_ROTATION_RAD, MAX_TRANSLATION_DEVIATION,
    TRI_MAX_STARS, TRI_INLIER_TOLERANCE, TRI_MIN_INLIERS,
};
use rayon::prelude::*;
use tracing::info;

// ── Tuning constants (RANSAC) ─────────────────────────────────────────────────
// MATCH_TOLERANCE, MIN_MATCHES, INLIER_TOLERANCE, MIN_INLIERS,
// MAX_ROTATION_RAD, MAX_TRANSLATION_DEVIATION moved to settings::defaults
// (Issue 148) — user/maintainer-tunable rejection and matching thresholds.

/// RANSAC iteration count. Not a tuning knob in the usual sense — raising it
/// trades runtime for a marginally higher chance of finding the true inlier
/// set; it has no effect on what counts as a valid alignment. Stays local
/// (Issue 148).
const RANSAC_ITERATIONS: usize = 50;

// ── Tuning constants (triangle matching) ─────────────────────────────────────
// TRI_MAX_STARS, TRI_INLIER_TOLERANCE, TRI_MIN_INLIERS moved to
// settings::defaults (Issue 148) — physical pixel tolerances and counts a
// maintainer might plausibly retune.

/// Maximum descriptor distance for two triangles to be considered a match.
/// Each descriptor is (ratio1, ratio2) normalized to [0,1], so max distance
/// between any two descriptors is sqrt(2) ≈ 1.41. A tolerance of 0.02 is
/// tight enough to reject most false matches while allowing for centroid noise.
/// Stays local (Issue 148): this is a distance in a private, normalized
/// descriptor space, not a physical unit — it has no meaning outside this
/// matching step.
const TRI_DESC_TOLERANCE: f32 = 0.02;

/// Vote bin size for translation (pixels). Transforms within this distance
/// in tx/ty and TRI_THETA_BIN_RAD in theta vote for the same hypothesis.
/// Stays local (Issue 148): vote-binning granularity, not an independently
/// meaningful tolerance — changing it reshapes the voting histogram, not
/// what counts as a good match.
const TRI_TX_BIN: f32    = 3.0;
const TRI_TY_BIN: f32    = 3.0;
const TRI_THETA_BIN: f32 = 0.005; // radians (~0.29°)

// ── Public types ──────────────────────────────────────────────────────────────

/// A 2D similarity transform: rotation + uniform scale + translation.
///
/// Maps a frame point (fx, fy) to reference space:
///   rx = a·fx - b·fy + tx
///   ry = b·fx + a·fy + ty
///
/// `a` and `b` encode rotation and scale together — `theta()` recovers
/// the angle, `scale()` recovers the magnitude, where
/// `a = scale·cos θ` and `b = scale·sin θ`.
///
/// The name is retained for continuity: in practice most transforms this
/// type carries *are* rigid. Within-group alignment fits converge to
/// scale ≈ 1.0 on their own (many tight correspondences from a single
/// session), and `identity()` / `translation()` / `flip_180()` are all
/// exactly unit scale. What the type no longer does is *enforce* unit
/// scale, because a cross-group solve between two sessions can legitimately
/// need it: a real M104 two-night session measured a 1.78% scale
/// difference between nights, plausibly from a focus/backfocus or
/// temperature-driven focal-length change over the 9 days between them.
/// Forcing that to 1.0 leaves an error that grows linearly with distance
/// from the transform's fixed point (~36px at r=2000 for 1.78%), which
/// shows up as radially-varying doubled stars in the stacked output.
///
/// Note `apply_inverse()` divides by a² + b², so it is a true inverse at
/// any scale; at unit scale it reduces to the rotation-transpose form
/// used previously.
#[derive(Debug, Clone)]
pub struct AffineRigid {
    pub a:  f32,
    pub b:  f32,
    pub tx: f32,
    pub ty: f32,
}

impl AffineRigid {
    /// Identity transform: no rotation, no translation.
    pub fn identity() -> Self {
        Self { a: 1.0, b: 0.0, tx: 0.0, ty: 0.0 }
    }

    /// Pure translation.
    pub fn translation(tx: f32, ty: f32) -> Self {
        Self { a: 1.0, b: 0.0, tx, ty }
    }

    /// 180° rotation around the image center.
    /// Maps (x, y) → (width - 1 - x, height - 1 - y).
    ///
    /// This is the exact transform applied by `Vec::reverse()` on a row-major
    /// buffer of dimensions width × height. Used to encode meridian flip
    /// orientation in a composable affine form.
    pub fn flip_180(width: usize, height: usize) -> Self {
        Self {
            a:  -1.0,
            b:   0.0,
            tx:  (width  as f32) - 1.0,
            ty:  (height as f32) - 1.0,
        }
    }

    /// Rotation angle in radians. Independent of scale.
    pub fn theta(&self) -> f32 {
        self.b.atan2(self.a)
    }

    /// Uniform scale factor. 1.0 for a pure rigid transform; may differ
    /// for a cross-group solve between sessions with a real focal-length
    /// or backfocus difference (see the struct doc).
    pub fn scale(&self) -> f32 {
        (self.a * self.a + self.b * self.b).sqrt()
    }

    /// Apply the forward transform: frame point → reference space.
    pub fn apply_forward(&self, fx: f32, fy: f32) -> (f32, f32) {
        (
            self.a * fx - self.b * fy + self.tx,
            self.b * fx + self.a * fy + self.ty,
        )
    }

    /// Apply the inverse transform: reference point → frame space.
    ///
    /// The previous formulation used the rotation matrix transpose,
    /// which is the true inverse only when a² + b² = 1. Now that a/b may
    /// carry a real uniform scale, the transpose must be divided by the
    /// squared scale to be a genuine inverse. At unit scale this reduces
    /// exactly to the old expression, so rigid-transform behaviour is
    /// unchanged.
    pub fn apply_inverse(&self, rx: f32, ry: f32) -> (f32, f32) {
        let ox = rx - self.tx;
        let oy = ry - self.ty;
        let det = self.a * self.a + self.b * self.b;
        if det < 1e-12 {
            return (0.0, 0.0);
        }
        (
            ( self.a * ox + self.b * oy) / det,
            (-self.b * ox + self.a * oy) / det,
        )
    }
}

/// Compose two rigid transforms: T = outer ∘ inner.
pub fn compose(outer: &AffineRigid, inner: &AffineRigid) -> AffineRigid {
    let a = outer.a * inner.a - outer.b * inner.b;
    let b = outer.b * inner.a + outer.a * inner.b;
    let mx = outer.a * inner.tx - outer.b * inner.ty;
    let my = outer.b * inner.tx + outer.a * inner.ty;
    AffineRigid {
        a,
        b,
        tx: mx + outer.tx,
        ty: my + outer.ty,
    }
}

// ── Internal types ────────────────────────────────────────────────────────────

type MatchedPair = (f32, f32, f32, f32); // (ref_x, ref_y, frame_x, frame_y)

// ── Triangle matching ─────────────────────────────────────────────────────────

/// Scale-invariant triangle descriptor.
/// Given three stars, compute the three side lengths, sort them descending
/// so sides[0] is longest. Descriptor = (sides[1]/sides[0], sides[2]/sides[0]).
/// Both values are in [0, 1]. Orientation tracks whether the triangle vertices
/// in their original order are clockwise (true) or counter-clockwise (false).
#[derive(Debug, Clone)]
struct TriDescriptor {
    r1:          f32,   // second-longest / longest
    r2:          f32,   // shortest / longest
    clockwise:   bool,
    /// Indices into the original star slice, in descriptor order
    /// (longest-edge-opposite vertex first).
    i0: usize, i1: usize, i2: usize,
}

/// Build all triangle descriptors from a star list.
/// Uses only the top TRI_MAX_STARS brightest stars.
fn build_descriptors(stars: &[StarCandidate]) -> Vec<TriDescriptor> {
    // Sort by peak brightness descending, take top N
    let mut sorted: Vec<(usize, &StarCandidate)> = stars.iter().enumerate().collect();
    sorted.sort_by(|(_, a), (_, b)| b.peak.partial_cmp(&a.peak)
        .unwrap_or(std::cmp::Ordering::Equal));
    sorted.truncate(TRI_MAX_STARS);

    let n = sorted.len();
    let mut descs = Vec::with_capacity(n * n * n / 6);

    for ii in 0..n {
        for jj in (ii + 1)..n {
            for kk in (jj + 1)..n {
                let (oi, si) = sorted[ii];
                let (oj, sj) = sorted[jj];
                let (ok, sk) = sorted[kk];

                let dij = dist(si.cx, si.cy, sj.cx, sj.cy);
                let dik = dist(si.cx, si.cy, sk.cx, sk.cy);
                let djk = dist(sj.cx, sj.cy, sk.cx, sk.cy);

                // Degenerate triangle — skip
                if dij < 1.0 || dik < 1.0 || djk < 1.0 {
                    continue;
                }

                // Sort sides descending: (longest, mid, shortest)
                // Track which vertex is opposite each side.
                // Side opposite vertex i = djk, opposite j = dik, opposite k = dij
                let mut sides = [(djk, oi, oj, ok),
                                 (dik, oj, oi, ok),
                                 (dij, ok, oi, oj)];
                sides.sort_by(|a, b| b.0.partial_cmp(&a.0)
                    .unwrap_or(std::cmp::Ordering::Equal));

                let longest = sides[0].0;
                if longest < 1.0 { continue; }

                let r1 = sides[1].0 / longest;
                let r2 = sides[2].0 / longest;

                // Orientation: cross product of (j-i) × (k-i)
                let cross = (sj.cx - si.cx) * (sk.cy - si.cy)
                          - (sj.cy - si.cy) * (sk.cx - si.cx);
                let clockwise = cross < 0.0;

                // The vertex opposite the longest side is sides[0].1
                // We order the other two vertices as sides[0].2, sides[0].3
                descs.push(TriDescriptor {
                    r1,
                    r2,
                    clockwise,
                    i0: sides[0].1,
                    i1: sides[0].2,
                    i2: sides[0].3,
                });
            }
        }
    }

    descs
}

/// Given a matched triangle pair (ref triangle, frame triangle), compute the
/// implied AffineRigid transform using the two base vertices of each triangle
/// (i1 and i2, which are opposite the longest side and thus well-separated).
fn transform_from_triangle_pair(
    ref_stars:   &[StarCandidate],
    frame_stars: &[StarCandidate],
    rd: &TriDescriptor,
    fd: &TriDescriptor,
) -> Option<AffineRigid> {
    // Use vertices i1 and i2 from each triangle (i0 is the apex opposite
    // the longest side; i1 and i2 are the base vertices, well-separated).
    let r0 = &ref_stars[rd.i1];
    let r1 = &ref_stars[rd.i2];
    let f0 = &frame_stars[fd.i1];
    let f1 = &frame_stars[fd.i2];

    let p_ref   = (r0.cx, r0.cy, f0.cx, f0.cy);
    let p_ref2  = (r1.cx, r1.cy, f1.cx, f1.cy);

    solve_affine_2pairs(p_ref, p_ref2)
}

/// Triangle-based rigid transform estimation.
///
/// Does NOT require an FFT pre-translation. Builds triangle descriptors for
/// both star sets, matches triangles by descriptor similarity, votes on the
/// implied transform, then refines with least-squares over inliers.
///
/// Returns None if insufficient matches or refinement fails.
pub fn estimate_rigid_transform_triangles(
    ref_stars:   &[StarCandidate],
    frame_stars: &[StarCandidate],
) -> Option<AffineRigid> {
    if ref_stars.len() < 3 || frame_stars.len() < 3 {
        info!("tri_align: too few stars (ref={}, frame={})", ref_stars.len(), frame_stars.len());
        return None;
    }

    let ref_descs   = build_descriptors(ref_stars);
    let frame_descs = build_descriptors(frame_stars);

    info!("tri_align: {} ref triangles, {} frame triangles",
        ref_descs.len(), frame_descs.len());

    if ref_descs.is_empty() || frame_descs.is_empty() {
        return None;
    }

    // Match triangles and collect implied transforms as votes in parallel.
    // Each rayon thread accumulates its own local HashMap, then we merge
    // them into a single votes map. This avoids any shared mutable state.
    type VoteMap = std::collections::HashMap<(i32, i32, i32), (u32, AffineRigid)>;

    let (votes, match_count): (VoteMap, u32) = ref_descs
        .par_iter()
        .fold(
            || (VoteMap::new(), 0u32),
            |mut acc, rd| {
                for fd in &frame_descs {
                    // Orientations must match
                    if rd.clockwise != fd.clockwise {
                        continue;
                    }

                    // Descriptor distance
                    let dr1 = rd.r1 - fd.r1;
                    let dr2 = rd.r2 - fd.r2;
                    let desc_dist = (dr1 * dr1 + dr2 * dr2).sqrt();

                    if desc_dist > TRI_DESC_TOLERANCE {
                        continue;
                    }

                    // Compute implied transform
                    let xform = match transform_from_triangle_pair(
                        ref_stars, frame_stars, rd, fd
                    ) {
                        Some(x) => x,
                        None    => continue,
                    };

                    // Redundant with solve_affine_2pairs()'s own ±5% gate
                    // (called by transform_from_triangle_pair() just above)
                    // — that check runs first and is tighter, so nothing
                    // reaches this ±10% check that didn't already pass a
                    // stricter one. Not a rigidity requirement: AffineRigid
                    // deliberately permits non-unit scale for cross-group
                    // solves (see the struct doc) — this is just a coarse
                    // early-out to avoid voting on a degenerate hypothesis.
                    // See Issue 147 for the open question of whether the
                    // upstream gate is too tight for legitimate cross-night
                    // scale drift.
                    let scale_sq = xform.a * xform.a + xform.b * xform.b;
                    if (scale_sq - 1.0).abs() > 0.1 {
                        continue;
                    }

                    acc.1 += 1;

                    // Bin the transform
                    let tx_bin    = (xform.tx / TRI_TX_BIN).round() as i32;
                    let ty_bin    = (xform.ty / TRI_TY_BIN).round() as i32;
                    let theta_bin = (xform.theta() / TRI_THETA_BIN).round() as i32;
                    let key = (tx_bin, ty_bin, theta_bin);

                    let entry = acc.0.entry(key).or_insert((0, xform.clone()));
                    entry.0 += 1;
                }
                acc
            },
        )
        .reduce(
            || (VoteMap::new(), 0u32),
            |mut a, b| {
                // Merge vote counts from two partial results
                for (key, (count, xform)) in b.0 {
                    let entry = a.0.entry(key).or_insert((0, xform));
                    entry.0 += count;
                }
                a.1 += b.1;
                a
            },
        );

    info!("tri_align: {} triangle matches, {} vote bins", match_count, votes.len());

    if votes.is_empty() {
        info!("tri_align: no votes — returning None");
        return None;
    }

    // Find the winning bin
    let (_, (best_count, best_xform)) = votes.iter()
        .max_by_key(|(_, (count, _))| *count)?;

    info!("tri_align: winning bin has {} votes, tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°)",
        best_count, best_xform.tx, best_xform.ty,
        best_xform.theta(), best_xform.theta().to_degrees());

    // Collect inliers under the winning transform
    let inlier_pairs: Vec<MatchedPair> = collect_inliers(
        ref_stars, frame_stars, best_xform, TRI_INLIER_TOLERANCE
    );

    info!("tri_align: {} inlier pairs from winning transform", inlier_pairs.len());

    if inlier_pairs.len() < TRI_MIN_INLIERS {
        info!("tri_align: insufficient inliers ({} < {}), returning None",
            inlier_pairs.len(), TRI_MIN_INLIERS);
        return None;
    }

    // Refine over the full inlier set (same least_squares_affine already
    // used by RANSAC's estimate_rigid_transform on comparably-shaped pairs).
    // Falls back to the raw winning-bin transform only if refinement
    // itself fails outright — should not normally happen given the inlier
    // count has already cleared TRI_MIN_INLIERS.
    let refined = least_squares_affine(&inlier_pairs).unwrap_or_else(|| best_xform.clone());

    let theta = refined.theta();
    info!("tri_align: result — tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°) inliers={}",
        refined.tx, refined.ty, theta, theta.to_degrees(),
        inlier_pairs.len());

    Some(refined)
}

/// Collect matched star pairs that are inliers under a given transform.
/// Uses greedy nearest-neighbour matching after applying the transform.
fn collect_inliers(
    ref_stars:   &[StarCandidate],
    frame_stars: &[StarCandidate],
    xform:       &AffineRigid,
    tolerance:   f32,
) -> Vec<MatchedPair> {
    let mut used_ref = vec![false; ref_stars.len()];
    let mut pairs    = Vec::new();

    for fs in frame_stars {
        let (px, py) = xform.apply_forward(fs.cx, fs.cy);

        let best = ref_stars.iter().enumerate()
            .filter(|(j, _)| !used_ref[*j])
            .map(|(j, r)| (j, dist(r.cx, r.cy, px, py)))
            .filter(|(_, d)| *d <= tolerance)
            .min_by(|(_, da), (_, db)| da.partial_cmp(db)
                .unwrap_or(std::cmp::Ordering::Equal));

        if let Some((j, _)) = best {
            used_ref[j] = true;
            pairs.push((ref_stars[j].cx, ref_stars[j].cy, fs.cx, fs.cy));
        }
    }

    pairs
}

// ── Original RANSAC entry point (kept for within-group alignment) ─────────────

/// Estimate the affine rigid transform between `ref_stars` and `frame_stars`.
///
/// `fft_dx` / `fft_dy` is the translation already computed by FFT phase
/// correlation. Frame star positions are pre-translated by this offset before
/// matching, so RANSAC solves for any small residual rotation around whatever
/// center the geometry dictates.
///
/// Returns `None` on failure (too few matches, bad sanity check, etc.) —
/// caller should fall back to FFT translation only.
pub fn estimate_rigid_transform(
    ref_stars:   &[StarCandidate],
    frame_stars: &[StarCandidate],
    fft_dx:      f32,
    fft_dy:      f32,
    _width:      usize,
    _height:     usize,
) -> Option<AffineRigid> {
    // Step 1: Pre-translate frame stars by FFT offset.
    // Issue 132: compute_translation now returns the correctly-signed
    // translation (positive fft_dx = target shifted right, matching its
    // docstring). A frame star at (cx, cy) that is itself shifted right
    // by fft_dx relative to the reference needs fft_dx SUBTRACTED to land
    // back on the reference star's position — this flipped from "+" to
    // "-" as part of the same fix (previously this added fft_dx, which
    // only produced correct alignment because compute_translation's
    // pre-fix sign was already inverted; the two cancelled).
    let translated: Vec<(f32, f32)> = frame_stars.iter()
        .map(|s| (s.cx - fft_dx, s.cy - fft_dy))
        .collect();

    // Step 2: Build candidate matches (greedy nearest-neighbour, one-to-one).
    let mut used_ref = vec![false; ref_stars.len()];
    let mut pairs:   Vec<MatchedPair> = Vec::new();

    for &(fx, fy) in &translated {
        let best = ref_stars.iter().enumerate()
            .filter(|(j, _)| !used_ref[*j])
            .map(|(j, r)| (j, dist(r.cx, r.cy, fx, fy)))
            .filter(|(_, d)| *d <= MATCH_TOLERANCE)
            .min_by(|(_, da), (_, db)| da.partial_cmp(db).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((j, _)) = best {
            used_ref[j] = true;
            pairs.push((ref_stars[j].cx, ref_stars[j].cy, fx, fy));
        }
    }

    if pairs.len() < MIN_MATCHES {
        info!(
            "star_align: only {} pair(s) matched (need {}), returning None",
            pairs.len(), MIN_MATCHES
        );
        return None;
    }

    // Step 3: RANSAC
    let n = pairs.len();
    let mut best_inliers: Vec<usize> = Vec::new();

    let mut rng = 0x12345678u64;
    let mut lcg = move || -> usize {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (rng >> 33) as usize
    };

    for _ in 0..RANSAC_ITERATIONS {
        let i0 = lcg() % n;
        let i1 = (i0 + 1 + lcg() % (n - 1)) % n;

        let hyp = match solve_affine_2pairs(pairs[i0], pairs[i1]) {
            Some(h) => h,
            None    => continue,
        };

        let inliers: Vec<usize> = pairs.iter().enumerate()
            .filter(|(_, &(rx, ry, fx, fy))| {
                let (px, py) = hyp.apply_forward(fx, fy);
                dist(px, py, rx, ry) <= INLIER_TOLERANCE
            })
            .map(|(i, _)| i)
            .collect();

        if inliers.len() > best_inliers.len() {
            best_inliers = inliers;
        }
    }

    if best_inliers.len() < MIN_INLIERS {
        info!(
            "star_align: best inlier count {} below minimum {}, returning None",
            best_inliers.len(), MIN_INLIERS
        );
        return None;
    }

    // Issue 146: log candidate matches and accepted inliers on the success
    // path — previously only the failure branches above logged these
    // counts, so there was no way to measure headroom above MIN_MATCHES/
    // MIN_INLIERS on frames that already pass. Pure logging addition, no
    // behavior change.
    info!(
        "star_align: accepted {} inlier(s) from {} candidate match(es) (min {}/{})",
        best_inliers.len(), pairs.len(), MIN_MATCHES, MIN_INLIERS
    );

    // Step 4: Least-squares refine (residual on top of the FFT pre-translation)
    let inlier_pairs: Vec<MatchedPair> = best_inliers.iter()
        .map(|&i| pairs[i])
        .collect();

    let refined = least_squares_affine(&inlier_pairs)?;

    // Step 5: Sanity checks — against the residual, not the folded-back
    // total, since MAX_TRANSLATION_DEVIATION bounds how far the refined
    // solve is allowed to deviate from the already-known-good FFT estimate,
    // not the frame's total dither offset.
    let theta = refined.theta();

    // Issue 146: log the residual rotation regardless of outcome, to
    // measure real within-group theta distribution against
    // MAX_ROTATION_RAD (currently ~30°, suspected far looser than the
    // real signal — see issue discussion). Pure logging addition.
    info!(
        "star_align: residual rotation θ={:.4}rad ({:.3}°) (limit {:.3}°)",
        theta, theta.to_degrees(), MAX_ROTATION_RAD.to_degrees()
    );

    if theta.abs() > MAX_ROTATION_RAD {
        return None;
    }

    if refined.tx.abs() > MAX_TRANSLATION_DEVIATION
        || refined.ty.abs() > MAX_TRANSLATION_DEVIATION
    {
        info!(
            "star_align: sanity check failed — refined tx={:.2} ty={:.2} (limit {})",
            refined.tx, refined.ty, MAX_TRANSLATION_DEVIATION
        );
        return None;
    }

    // Step 6: Fold the FFT pre-translation back in. `refined` was fit
    // against frame coordinates already shifted by (-fft_dx, -fft_dy) — see
    // Step 1 — so its tx/ty are only the residual correction on top of that
    // shift, not a transform that correctly maps raw frame coordinates.
    // Compose the shift back in (matching Step 1's sign, Issue 132) so the
    // returned transform is usable directly against raw (unshifted) frame
    // pixels, matching every caller's expectation (composition with
    // M_cross, apply_inverse during resampling).
    let final_tx = -(refined.a * fft_dx - refined.b * fft_dy) + refined.tx;
    let final_ty = -(refined.b * fft_dx + refined.a * fft_dy) + refined.ty;

    Some(AffineRigid {
        a:  refined.a,
        b:  refined.b,
        tx: final_tx,
        ty: final_ty,
    })
}

// ── 2-pair solve ──────────────────────────────────────────────────────────────

fn solve_affine_2pairs(p0: MatchedPair, p1: MatchedPair) -> Option<AffineRigid> {
    let (rx0, ry0, fx0, fy0) = p0;
    let (rx1, ry1, fx1, fy1) = p1;

    let dfx = fx0 - fx1;
    let dfy = fy0 - fy1;
    let det = dfx * dfx + dfy * dfy;

    if det < 100.0 {
        return None;
    }

    let drx = rx0 - rx1;
    let dry = ry0 - ry1;

    let a = ( dfx * drx + dfy * dry) / det;
    let b = (-dfy * drx + dfx * dry) / det;

    // Issue 147 (deferred, not fixed): this ±5% gate runs for every RANSAC
    // and triangle-pair hypothesis, including cross-group solves — but
    // AffineRigid deliberately permits non-unit scale for exactly that
    // case (real focus/backfocus/temperature-driven focal-length drift
    // between nights; see the struct doc). A real M104 two-night session
    // measured ~1.78% scale drift, which clears this gate with only ~30%
    // margin — a modestly larger drift would be silently rejected here,
    // before ever reaching the empirical CROSS_GROUP_MAX_RESIDUAL_PX/
    // CROSS_GROUP_MIN_MATCHED verification gate that was actually built to
    // judge cross-group solves. Left as-is: the exposure is narrow (only
    // multi-night sessions with group splits, and only when true drift
    // exceeds what's been observed in practice), and today's fallback
    // degrades gracefully rather than corrupting anything — a scale-
    // rejected triangle match just falls through to a logged
    // "triangle match failed, falling back to FFT-translation-only",
    // which is a worse solve (no rotation/scale correction) but not a
    // silently wrong one. Revisit if a session is ever seen where that
    // fallback's degraded accuracy is a real problem.
    let scale_sq = a * a + b * b;
    if (scale_sq - 1.0).abs() > 0.05 {
        return None;
    }

    let tx = rx0 - a * fx0 + b * fy0;
    let ty = ry0 - b * fx0 - a * fy0;

    Some(AffineRigid { a, b, tx, ty })
}

// ── Least-squares from N pairs ────────────────────────────────────────────────

fn least_squares_affine(pairs: &[MatchedPair]) -> Option<AffineRigid> {
    let n = pairs.len();
    if n < 2 {
        return None;
    }
    let n_f = n as f64;

    // Center both point sets on their centroids before solving for
    // rotation. The previous uncentered formulation implicitly assumed
    // tx = ty = 0 while solving for (a, b) — a valid approximation only
    // when the true translation is small relative to the coordinate
    // magnitudes (true for RANSAC's FFT-pre-shifted residual fits, false
    // for triangle matching's raw, unshifted star coordinates). Centering
    // makes the fit exact regardless of translation size, and the
    // translation is then recovered exactly from the centroids.
    let mut sum_fx = 0.0f64;
    let mut sum_fy = 0.0f64;
    let mut sum_rx = 0.0f64;
    let mut sum_ry = 0.0f64;
    for &(rx, ry, fx, fy) in pairs {
        sum_fx += fx as f64;
        sum_fy += fy as f64;
        sum_rx += rx as f64;
        sum_ry += ry as f64;
    }
    let fx_mean = sum_fx / n_f;
    let fy_mean = sum_fy / n_f;
    let rx_mean = sum_rx / n_f;
    let ry_mean = sum_ry / n_f;

    let mut sum_ff    = 0.0f64;
    let mut sum_rhs_a = 0.0f64;
    let mut sum_rhs_b = 0.0f64;

    for &(rx, ry, fx, fy) in pairs {
        let fxc = fx as f64 - fx_mean;
        let fyc = fy as f64 - fy_mean;
        let rxc = rx as f64 - rx_mean;
        let ryc = ry as f64 - ry_mean;
        sum_ff    += fxc * fxc + fyc * fyc;
        sum_rhs_a += fxc * rxc + fyc * ryc;
        sum_rhs_b += fxc * ryc - fyc * rxc;
    }

    if sum_ff.abs() < 1e-10 {
        return None;
    }

    // Solved as a similarity transform: a and b are free parameters, so
    // they encode rotation *and* uniform scale together. This is
    // deliberate — see the AffineRigid struct doc. Within-group fits
    // converge to scale ≈ 1.0 naturally (hundreds of tight
    // correspondences from the same session), so they are rigid in
    // practice without being constrained to it. Cross-group fits between
    // sessions may carry a real scale difference (focus/backfocus shift,
    // temperature, focal-length change between nights) which this
    // formulation captures rather than discards.
    //
    // An earlier revision renormalized a/b onto the unit circle here to
    // enforce rigidity. That removed a genuine 1.78% scale difference in
    // a real M104 two-night session, and because the translation is
    // recovered from the centroids using whatever rotation is passed in,
    // the renormalized rotation produced a translation solving a
    // different problem than the one the correspondences described — a
    // globally offset transform that pushed 92% of stars outside the
    // verification match radius. Rotation, scale, and translation are a
    // matched set from a single solve and must stay consistent with each
    // other.
    let a = sum_rhs_a / sum_ff;
    let b = sum_rhs_b / sum_ff;

    // Translation recovered from the centroids: ref_mean = M * frame_mean + t
    let tx = rx_mean - (a * fx_mean - b * fy_mean);
    let ty = ry_mean - (b * fx_mean + a * fy_mean);

    Some(AffineRigid {
        a:  a  as f32,
        b:  b  as f32,
        tx: tx as f32,
        ty: ty as f32,
    })
}

// ── Distance helper ───────────────────────────────────────────────────────────

#[inline]
fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_star(cx: f32, cy: f32) -> StarCandidate {
        StarCandidate {
            cx, cy,
            peak: 0.8,
            bbox: (0, 0, 1, 1),
            patch: vec![0.8],
            pixel_count: 1,
        }
    }

    #[test]
    fn test_identity_compose() {
        let id = AffineRigid::identity();
        let t  = AffineRigid::translation(5.0, -3.0);
        let c1 = compose(&id, &t);
        let c2 = compose(&t, &id);
        assert!((c1.tx - 5.0).abs() < 1e-5);
        assert!((c1.ty + 3.0).abs() < 1e-5);
        assert!((c2.tx - 5.0).abs() < 1e-5);
        assert!((c2.ty + 3.0).abs() < 1e-5);
    }

    #[test]
    fn test_flip_180_roundtrip() {
        let w = 3008;
        let h = 3008;
        let flip = AffineRigid::flip_180(w, h);
        let twice = compose(&flip, &flip);
        assert!((twice.a - 1.0).abs() < 1e-5);
        assert!(twice.b.abs() < 1e-5);
        assert!(twice.tx.abs() < 1e-3);
        assert!(twice.ty.abs() < 1e-3);
    }

    #[test]
    fn test_flip_180_maps_corner_to_corner() {
        let w = 3008;
        let h = 3008;
        let flip = AffineRigid::flip_180(w, h);
        let (rx, ry) = flip.apply_forward(0.0, 0.0);
        assert!((rx - (w as f32 - 1.0)).abs() < 1e-3);
        assert!((ry - (h as f32 - 1.0)).abs() < 1e-3);
        let (rx, ry) = flip.apply_forward(100.0, 200.0);
        assert!((rx - (w as f32 - 101.0)).abs() < 1e-3);
        assert!((ry - (h as f32 - 201.0)).abs() < 1e-3);
    }

    #[test]
    fn test_compose_flip_then_translate() {
        let w = 3008;
        let h = 3008;
        let flip  = AffineRigid::flip_180(w, h);
        let trans = AffineRigid::translation(10.0, -5.0);
        let composed = compose(&trans, &flip);
        let (rx, ry) = composed.apply_forward(0.0, 0.0);
        assert!((rx - 3017.0).abs() < 1e-3, "rx={}", rx);
        assert!((ry - 3002.0).abs() < 1e-3, "ry={}", ry);
    }

    #[test]
    fn test_triangle_matching_pure_translation() {
        // Build a synthetic star field and shift it by a known translation.
        // Triangle matching should recover it.
        let ref_stars: Vec<StarCandidate> = vec![
            make_star(100.0, 100.0), make_star(500.0, 150.0), make_star(300.0, 400.0),
            make_star(800.0, 200.0), make_star(150.0, 600.0), make_star(700.0, 500.0),
            make_star(400.0, 250.0), make_star(600.0, 350.0), make_star(200.0, 350.0),
            make_star(900.0, 400.0),
        ];

        let tx = 47.0f32;
        let ty = -23.0f32;

        let frame_stars: Vec<StarCandidate> = ref_stars.iter()
            .map(|s| make_star(s.cx - tx, s.cy - ty))
            .collect();

        let result = estimate_rigid_transform_triangles(&ref_stars, &frame_stars);
        assert!(result.is_some(), "triangle matching should succeed");
        let r = result.unwrap();
        assert!((r.tx - tx).abs() < 2.0, "tx error: got {:.2}, expected {:.2}", r.tx, tx);
        assert!((r.ty - ty).abs() < 2.0, "ty error: got {:.2}, expected {:.2}", r.ty, ty);
    }

    #[test]
    fn test_triangle_matching_rotation() {
        // Build a star field, rotate it by a known angle, verify recovery.
        let ref_stars: Vec<StarCandidate> = vec![
            make_star(100.0, 100.0), make_star(500.0, 150.0), make_star(300.0, 400.0),
            make_star(800.0, 200.0), make_star(150.0, 600.0), make_star(700.0, 500.0),
            make_star(400.0, 250.0), make_star(600.0, 350.0), make_star(200.0, 350.0),
            make_star(900.0, 400.0),
        ];

        // Rotate by 0.5° around origin with a small translation
        let theta = 0.5f32.to_radians();
        let a = theta.cos();
        let b = theta.sin();
        let tx = 20.0f32;
        let ty = -10.0f32;
        let xform = AffineRigid { a, b, tx, ty };

        let frame_stars: Vec<StarCandidate> = ref_stars.iter()
            .map(|s| {
                // inverse of xform to get frame coords from ref coords
                let (fx, fy) = xform.apply_inverse(s.cx, s.cy);
                make_star(fx, fy)
            })
            .collect();

let result = estimate_rigid_transform_triangles(&ref_stars, &frame_stars);
        assert!(result.is_some(), "triangle matching should succeed for rotation");
        let r = result.unwrap();
        let theta_err = (r.theta() - theta).abs();
        assert!(theta_err < 0.01, "theta error: got {:.4}rad, expected {:.4}rad",
            r.theta(), theta);
    }

    #[test]
    fn test_estimate_rigid_transform_sign_contract() {
        // Issue 149: pins Step 1 (the fft_dx/fft_dy pre-translation sign)
        // and Step 6 (folding that pre-translation back into the returned
        // transform) together — a single-sided sign flip in either would
        // still let this compile and run, but would fail the assertions
        // below rather than just producing a slightly-off number.
        let ref_stars: Vec<StarCandidate> = vec![
            make_star(100.0, 100.0), make_star(400.0, 120.0), make_star(250.0, 300.0),
            make_star(380.0, 380.0), make_star(120.0, 350.0), make_star(300.0, 150.0),
            make_star(200.0, 250.0), make_star(350.0, 200.0), make_star(150.0, 150.0),
            make_star(300.0, 400.0),
        ];

        // Known ground-truth transform, frame -> reference space.
        let theta_true = 0.3f32.to_radians();
        let tx_true = 12.0f32;
        let ty_true = -7.0f32;
        let xform_true = AffineRigid {
            a: theta_true.cos(),
            b: theta_true.sin(),
            tx: tx_true,
            ty: ty_true,
        };

        let frame_stars: Vec<StarCandidate> = ref_stars.iter()
            .map(|s| {
                let (fx, fy) = xform_true.apply_inverse(s.cx, s.cy);
                make_star(fx, fy)
            })
            .collect();

        // fft_dx/fft_dy as if compute_translation had already found the
        // (rotation-free) translation component exactly — this isolates
        // estimate_rigid_transform's own Step 1/Step 6 sign handling from
        // compute_translation's Step 8 negation, which test_end_to_end_*
        // in stack_frames.rs exercises together instead. Per Step 1's
        // documented convention (positive fft_dx = target/frame shifted
        // right), a frame shifted by approximately -tx_true relative to
        // the reference corresponds to fft_dx = -tx_true.
        let fft_dx = -tx_true;
        let fft_dy = -ty_true;

        let result = estimate_rigid_transform(&ref_stars, &frame_stars, fft_dx, fft_dy, 512, 512);
        assert!(result.is_some(), "should recover a transform from clean synthetic data");
        let recovered = result.unwrap();

        // Step 6 fold-back: the returned transform's parameters should
        // match the known ground truth directly, not just its residual on
        // top of the FFT pre-translation.
        assert!((recovered.tx - tx_true).abs() < 1.0,
            "tx error: got {:.2}, expected {:.2}", recovered.tx, tx_true);
        assert!((recovered.ty - ty_true).abs() < 1.0,
            "ty error: got {:.2}, expected {:.2}", recovered.ty, ty_true);
        assert!((recovered.theta() - theta_true).abs() < 0.01,
            "theta error: got {:.4}rad, expected {:.4}rad", recovered.theta(), theta_true);

        // And directly: apply_forward on each frame star should land on
        // its corresponding reference star, within sub-pixel tolerance —
        // the actual contract every caller relies on.
        for (rs, fs) in ref_stars.iter().zip(frame_stars.iter()) {
            let (px, py) = recovered.apply_forward(fs.cx, fs.cy);
            let d = dist(px, py, rs.cx, rs.cy);
            assert!(d < 1.0,
                "frame star ({:.1},{:.1}) mapped to ({:.2},{:.2}), expected near ({:.1},{:.1}) — dist={:.2}",
                fs.cx, fs.cy, px, py, rs.cx, rs.cy, d);
        }
    }
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
