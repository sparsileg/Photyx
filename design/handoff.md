# Photyx Stacking — Session Handoff (May 2026)

## What Works

The stacking pipeline is largely functional. Read `stacking_development.md`
section 10 carefully before touching anything. Key facts:

- 56/56 frames stack successfully (validation currently disabled)
- M64 galaxy clearly visible, well-registered
- Meridian flip correctly detected via ROTATOR keyword
- Two rotational groups: Group 0 (10 pre-flip frames), Group 1 (46 post-flip,
  master group)
- Debayer-first pipeline eliminates Bayer pattern mismatch from reverse()
- M_cross correctly encodes 180° flip: `a≈-1.0, b≈-0.002, tx≈3024, ty≈3002`
- XISF export reads cleanly in Siril and PixInsight

## The One Remaining Problem

A faint residual rotation ghost (~0.112°) is visible on brighter stars —
a secondary image offset from the primary, most visible in zoomed crops.
This comes from M_cross having `b≈-0.002` (θ≈0.112°), meaning the 10
pre-flip frames are placed with a small rotational offset relative to the
46 post-flip frames.

**We do not know if this rotation is real or a RANSAC artifact.**

## First Task: Determine If The Rotation Is Real

Add one line after M_cross is computed to zero out the rotation component:

```rust
// TEMPORARY DIAGNOSTIC: force pure translation to test if b is real
m_cross[g].b = 0.0;
m_cross[g].a = -1.0; // keep the flip, remove the rotation
```

Stack and examine the result:
- If the ghost **disappears or improves**: `b` was a RANSAC artifact. Fix by
  either zeroing `b` permanently or improving the cross-group RANSAC.
- If the ghost **stays or gets worse**: the rotation is real physical field
  rotation. Fix by ensuring within-group RANSAC fires reliably on pre-flip
  frames to correct per-frame rotation independently.

## Current Code State

### stack_frames.rs — key facts
- `validate_alignment` is **disabled** (all frames accepted, no rejection gate)
- Within-group RANSAC: `try_rigid_refinement` called with `snap.stars` and
  `g_ref_stars` (both from debayered luma at snapshot time — consistent)
- RANSAC residual sanity check: `rigid.tx.abs() > 10.0 || rigid.ty.abs() > 10.0`
- RANSAC fires on almost no within-group frames (returns None for most)
- Frame 50 consistently gets bad RANSAC solve (~52px residual), correctly rejected

### star_align.rs — key facts
- `MATCH_TOLERANCE = 15.0px` — may be too tight for within-group matching
- Pre-translation sign: `(s.cx + fft_dx, s.cy + fft_dy)` — POSITIVE (critical)
- RANSAC residual sanity in star_align: `refined.tx.abs() > MAX_TRANSLATION_DEVIATION`
  where `MAX_TRANSLATION_DEVIATION = 20.0`

### Sign conventions (CRITICAL — many bugs came from getting these wrong)
- `compute_translation(reference, target)` → positive dx = target shifted RIGHT
- Resampler: `src_x = out_x - dx` (subtracts)
- Star pre-translation for RANSAC: `cx + fft_dx` (adds — frame star → ref space)
- `validate_alignment` predicted position: `cx + dx` (adds)
- RANSAC residual reconstruction in `try_rigid_refinement`:
  `tx = aft_x + rigid.tx` where `aft_x = cos_t * fft_dx - sin_t * fft_dy`
  (POSITIVE aft, not negated)

## What Has Been Tried And Failed

See `stacking_development.md` section 10.3 for the full list. Most importantly:

- Inline debayered star detection during alignment → made residual worse, reverted
- RANSAC pre-translate with `-fft` → broke everything
- validate_alignment with `min_match_rate=0.3, tolerance=5.0` → more frames
  failed, worse result
- MATCH_TOLERANCE raised to 25px → made residual worse, reverted to 15px
- Centroid remapping `(W-1-cx, H-1-cy)` for flip frames → broke validation

## Files To Upload At Start Of Session

- `stacking_development.md` (updated — section 10 is the key reference)
- `stack_frames.rs` (current version)
- `star_align.rs` (current version)
- `fft_align.rs` (needed to understand sign conventions from source)
- `project-onboarding.md`

## Do Not Re-Litigate

The following are settled and correct:
- Debayer-first approach
- AffineRigid + compose() math
- M_cross architecture (two groups, one cross-group solve)
- Sign conventions (see above)
- `reverse()` for the flip in M_cross solve (on debayered luma, safe)

Start with the diagnostic test. Don't change anything else until you know
whether the rotation is real.
