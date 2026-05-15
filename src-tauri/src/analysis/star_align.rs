// analysis/star_align.rs — RANSAC affine rigid transform alignment
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

use crate::analysis::stars::StarCandidate;

// ── Tuning constants ──────────────────────────────────────────────────────────

const MATCH_TOLERANCE: f32 = 15.0;
const MIN_MATCHES: usize = 4;
const INLIER_TOLERANCE: f32 = 2.0;
const MIN_INLIERS: usize = 4;
const RANSAC_ITERATIONS: usize = 50;
const MAX_ROTATION_RAD: f32 = 0.52; // ~30 degrees
const MAX_TRANSLATION_DEVIATION: f32 = 20.0;

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
        // (x,y) → (W-1-x, H-1-y) = (-1)·(x,y) + (W-1, H-1)
        // In rigid form: a = cos(180°) = -1, b = sin(180°) = 0
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
    ///
    /// For a 2×2 orthogonal matrix [a -b; b a], the inverse is the transpose
    /// [a b; -b a]. So src = Aᵀ · (out - t).
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
///
/// If `inner` maps frame coords → intermediate coords, and `outer` maps
/// intermediate coords → reference coords, then `compose(outer, inner)`
/// maps frame coords → reference coords directly.
///
/// Math: applying first inner then outer to a point p:
///   p1 = A_in·p + t_in
///   p2 = A_out·p1 + t_out
///      = A_out·A_in·p + A_out·t_in + t_out
///
/// So composed.A  = A_out · A_in
///    composed.t  = A_out · t_in + t_out
pub fn compose(outer: &AffineRigid, inner: &AffineRigid) -> AffineRigid {
    // 2x2 matrix product: [a -b; b a] · [a -b; b a]
    let a = outer.a * inner.a - outer.b * inner.b;
    let b = outer.b * inner.a + outer.a * inner.b;
    // A_out · t_in
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

// ── Public entry point ────────────────────────────────────────────────────────

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
    // Convention: resampler uses (out - dx, out - dy) to find source. So a
    // frame star at (fx, fy) maps to reference position (fx - dx, fy - dy).
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

    // For pre-translated input, RANSAC translation should be near zero.
    if refined.tx.abs() > MAX_TRANSLATION_DEVIATION
        || refined.ty.abs() > MAX_TRANSLATION_DEVIATION
    {
        return None;
    }

    Some(refined)
}

// ── 2-pair solve ─────────────────────────────────────────────────────────────

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
        // flip applied twice should give identity
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
        // Inner: 180° flip of 3008x3008
        // Outer: translate by (10, -5)
        // Composed should: rotate 180° then shift
        let w = 3008;
        let h = 3008;
        let flip = AffineRigid::flip_180(w, h);
        let trans = AffineRigid::translation(10.0, -5.0);
        let composed = compose(&trans, &flip);

        // Verify: (0,0) → flip → (3007, 3007) → translate → (3017, 3002)
        let (rx, ry) = composed.apply_forward(0.0, 0.0);
        assert!((rx - 3017.0).abs() < 1e-3, "rx={}", rx);
        assert!((ry - 3002.0).abs() < 1e-3, "ry={}", ry);
    }
}
