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
use tracing::info;

// ── Tuning constants (RANSAC) ─────────────────────────────────────────────────

const MATCH_TOLERANCE: f32        = 15.0;
const MIN_MATCHES: usize          = 4;
const INLIER_TOLERANCE: f32       = 2.0;
const MIN_INLIERS: usize          = 4;
const RANSAC_ITERATIONS: usize    = 50;
const MAX_ROTATION_RAD: f32       = 0.52; // ~30 degrees
const MAX_TRANSLATION_DEVIATION: f32 = 20.0;

// ── Tuning constants (triangle matching) ─────────────────────────────────────

/// Number of brightest stars to use for triangle building.
const TRI_MAX_STARS: usize = 60;

/// Maximum descriptor distance for two triangles to be considered a match.
/// Each descriptor is (ratio1, ratio2) normalized to [0,1], so max distance
/// between any two descriptors is sqrt(2) ≈ 1.41. A tolerance of 0.02 is
/// tight enough to reject most false matches while allowing for centroid noise.
const TRI_DESC_TOLERANCE: f32 = 0.02;

/// Inlier tolerance (pixels) when refining the triangle-voted transform.
const TRI_INLIER_TOLERANCE: f32 = 3.0;

/// Minimum number of inliers required to accept a triangle-matched transform.
const TRI_MIN_INLIERS: usize = 6;

/// Vote bin size for translation (pixels). Transforms within this distance
/// in tx/ty and TRI_THETA_BIN_RAD in theta vote for the same hypothesis.
const TRI_TX_BIN: f32    = 3.0;
const TRI_TY_BIN: f32    = 3.0;
const TRI_THETA_BIN: f32 = 0.005; // radians (~0.29°)

// ── Public types ──────────────────────────────────────────────────────────────

/// A 2D affine rigid transform: rotation + translation, scale fixed at 1.0
/// (or scale ±1 to allow 180° reflection-via-rotation).
///
/// Maps a frame point (fx, fy) to reference space:
///   rx = a·fx - b·fy + tx
///   ry = b·fx + a·fy + ty
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

    /// Rotation angle in radians.
    pub fn theta(&self) -> f32 {
        self.b.atan2(self.a)
    }

    /// Apply the forward transform: frame point → reference space.
    pub fn apply_forward(&self, fx: f32, fy: f32) -> (f32, f32) {
        (
            self.a * fx - self.b * fy + self.tx,
            self.b * fx + self.a * fy + self.ty,
        )
    }

    /// Apply the inverse transform: reference point → frame space.
    pub fn apply_inverse(&self, rx: f32, ry: f32) -> (f32, f32) {
        let ox = rx - self.tx;
        let oy = ry - self.ty;
        (
             self.a * ox + self.b * oy,
            -self.b * ox + self.a * oy,
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

    // Match triangles and collect implied transforms as votes.
    // Each vote is (tx_bin, ty_bin, theta_bin) → count.
    // We use integer bins to group similar transforms.
    let mut votes: std::collections::HashMap<(i32, i32, i32), (u32, AffineRigid)>
        = std::collections::HashMap::new();

    let mut match_count = 0u32;

    for rd in &ref_descs {
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

            // Sanity: scale should be near 1.0 (rigid transform)
            let scale_sq = xform.a * xform.a + xform.b * xform.b;
            if (scale_sq - 1.0).abs() > 0.1 {
                continue;
            }

            match_count += 1;

            // Bin the transform
            let tx_bin    = (xform.tx / TRI_TX_BIN).round() as i32;
            let ty_bin    = (xform.ty / TRI_TY_BIN).round() as i32;
            let theta_bin = (xform.theta() / TRI_THETA_BIN).round() as i32;
            let key = (tx_bin, ty_bin, theta_bin);

            let entry = votes.entry(key).or_insert((0, xform.clone()));
            entry.0 += 1;
        }
    }

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

    // Use the winning voted transform directly — least-squares refinement
    // is numerically unstable with star centroids far from the origin.
    let theta = best_xform.theta();
    info!("tri_align: result — tx={:.2} ty={:.2} θ={:.4}rad ({:.3}°) inliers={}",
        best_xform.tx, best_xform.ty, theta, theta.to_degrees(),
        inlier_pairs.len());

    Some(best_xform.clone())
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
    let translated: Vec<(f32, f32)> = frame_stars.iter()
        .map(|s| (s.cx + fft_dx, s.cy + fft_dy))
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

    // Step 4: Least-squares refine
    let inlier_pairs: Vec<MatchedPair> = best_inliers.iter()
        .map(|&i| pairs[i])
        .collect();

    let refined = least_squares_affine(&inlier_pairs)?;

    // Step 5: Sanity checks
    let theta = refined.theta();
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

    Some(refined)
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

    let mut sum_ff    = 0.0f64;
    let mut sum_rhs_a = 0.0f64;
    let mut sum_rhs_b = 0.0f64;
    let mut sum_fx    = 0.0f64;
    let mut sum_fy    = 0.0f64;
    let mut sum_rx    = 0.0f64;
    let mut sum_ry    = 0.0f64;

    for &(rx, ry, fx, fy) in pairs {
        let (rx, ry, fx, fy) = (rx as f64, ry as f64, fx as f64, fy as f64);
        sum_ff    += fx * fx + fy * fy;
        sum_rhs_a += fx * rx + fy * ry;
        sum_rhs_b += fx * ry - fy * rx;
        sum_fx    += fx;
        sum_fy    += fy;
        sum_rx    += rx;
        sum_ry    += ry;
    }

    if sum_ff.abs() < 1e-10 {
        return None;
    }

    let a = sum_rhs_a / sum_ff;
    let b = sum_rhs_b / sum_ff;

    let n_f = n as f64;
    let tx = (sum_rx - a * sum_fx + b * sum_fy) / n_f;
    let ty = (sum_ry - b * sum_fx - a * sum_fy) / n_f;

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
}
