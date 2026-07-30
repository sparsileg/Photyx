# Photyx Registration & Stacking Pipeline — Audit Findings

**Scope:** `analysis/fft_align.rs`, `analysis/star_align.rs`,
`plugins/stack_frames.rs`, read in full as a single pipeline. Supporting
context read: `analysis/stars.rs`, `analysis/debayer.rs`,
`analysis/session_stats.rs`, `analysis/mod.rs`, `context/mod.rs`,
`settings/defaults.rs`.

**Comparison material read:** Siril (`registration.c`, `registration.h`,
`stacking.c`, `stacking.h`) and OpenSkyStacker (`imagestacker.cpp`,
`imagestacker_p.h`, `stardetector.cpp`, `util.cpp`). Used only to inform
fixes to Photyx defects — no scope expansion proposed.

**Not reviewed:** `analysis/background.rs` (affects one inefficiency note in
Issue 02, flagged there), `analysis/fwhm.rs`, `analysis/eccentricity.rs`,
`analysis/stack_metrics.rs`, and the stack display/write commands (relevant
to Issue 07's blast radius, flagged there).

**Out of scope by instruction:** diagnostic logging volume, including the
hardcoded suspect-position probe at (1554, 1469) in the M_cross loop and the
block labeled `TEMPORARY DIAGNOSTIC (Issue 111)`.

---

## Twelve issues found

| #   | Title                                                     | Size | Class        |
| --- | --------------------------------------------------------- | ---- | ------------ |
| 01  | Dimension mismatch panics or silently corrupts output     | S    | bug          |
| 02  | FFT compares normalized target to un-normalized reference | S    | bug          |
| 03  | Filter-mismatch exemption wrong; group refs unprotected   | S    | bug          |
| 04  | Group-ref load failure aborts whole stack                 | S    | bug          |
| 05  | Pass 1 buffer duplication; star patches retained all run  | M    | performance  |
| 06  | Sigma clip uses contaminated mean and population σ        | M    | algorithm    |
| 07  | Output min/max stretch destroys photometric linearity     | M    | algorithm    |
| 08  | RANSAC accepts a transform from 4 star pairs              | S    | algorithm    |
| 09  | Scale gates contradict AffineRigid's documented contract  | S    | algorithm    |
| 10  | 13 constants absent from defaults.rs                      | S    | code-quality |
| 11  | No test pins the transform-direction contract             | M    | testing      |
| 12  | Remove validate_alignment(); reconcile TR §7              | S    | docs         |

---

## The sign-convention question, answered

The Issue 131/132 FFT sign inversion is **fully and correctly propagated**.
Traced every consumer of `AlignmentTranslation`:

- `compute_translation()` negates at Step 8, converting the cross-power
  peak to the signed translation its docstring promises.
- `estimate_rigid_transform()` Step 1 subtracts (`s.cx - fft_dx`), with a
  comment confirming this flipped from `+` as part of the same fix and that
  the old pair of errors cancelled.
- Step 6's fold-back composes the shift back in with matching sign.
- The M_cross FFT-only fallback uses `translation(-fft_t.dx, -fft_t.dy)`,
  consistent with the same convention.
- Both resamplers use `apply_inverse` for output→source mapping — the
  correct inverse-map convention, matching OpenSkyStacker's explicit
  `cv::WARP_INVERSE_MAP`.

**No compensating double-negation found anywhere.** The concern that
motivated this part of the audit is resolved.

One cross-check worth recording: Siril computes the same
reference × conj(target) product and uses the peak **unnegated**. Both can
be self-consistent — the sign depends on which argument is "reference" and
how the result is applied — and Photyx's own tests confirm its convention
matches its docstring. But the divergence from a mature reference
implementation is exactly what should be pinned by a test rather than left
to a comment. That is Issue 11.

---

## What is in good shape

Worth stating, because it bounds how aggressive the fix plan needs to be.

- **Index alignment is sound.** Every early-`continue` in Pass 1 pushes
  exactly one `FrameContribution` before continuing, so `contributions`
  stays aligned with `snapshots` — which the post-Pass-2 crop depends on.
  Verified path by path.
- **Welford accumulation is correct.** `count_buf` is per-pixel, not
  per-channel, and increments once per frame in both the mono and color
  branches. The color stddev's `flat_map_iter` correctly reuses the
  per-pixel count across all three channels.
- **Pass 2's chunking is real** and matches the documented discipline:
  sequential pixel load inside each chunk, parallel resample, sequential
  accumulate, buffers dropped per chunk.
- **RANSAC sampling is correct** — the `i1 = (i0 + 1 + lcg() % (n-1)) % n`
  construction guarantees two distinct indices.
- **The `least_squares_affine` centroid-centering fix is right**, and the
  comment explaining why the earlier uncentered formulation failed for
  triangle matching is accurate.
- **`valid_output_bounds`'s convexity argument is correct** — for an affine
  map of a convex rectangle, checking four corners is sufficient, and the
  analytic inner-box construction plus bounded inward nudge is a sound
  replacement for the seed-and-expand search it describes.
- **Issue 128's group-exclusion mechanism is properly pl umbed** — set at
  the group level, honored in Pass 1 before any pixel work, with per-frame
  reasons recorded.
- **Photyx caches star detections per frame**; OpenSkyStacker re-detects
  stars on *both* frames inside its per-frame align loop
  (`generateAlignedImage`, `util.cpp` ~line 519). Photyx is clearly better
  here.
- **Debayer-first holds under both resampler paths.** Debayering happens on
  the raw buffer before any geometric transform, so no transform path can
  introduce a Bayer-pattern mismatch. `bayer_pattern` is stored per
  snapshot and read per frame, never inherited from the reference — so a
  group at a different orientation still debayers with its own pattern.
  The Issue 131 reflection fix in `debayer.rs` is correct: reflection
  preserves coordinate parity, so a reflected sample is always the same
  Bayer channel.

---

## Suggested manual / integration tests

Against real ASI533MC Pro data (3008×3008 OSC), per project convention:

1. **Mixed-geometry session** — one cropped or binned frame among normal
   ones. Confirms Issue 01. Currently expected to panic.
2. **Mixed-background session** — frames from either side of moonrise, or
   through a transparency change. Confirms Issue 02's effect on alignment
   success rate.
3. **Meridian-flip session** — exercises the `M_cross` path, group
   exclusion, and the cross-group residual gate end to end.
4. **Two-night session with known focus difference** (the M104 case, if
   still available) — exercises Issue 09's scale gates at a real measured
   scale difference.
5. **Sparse-star session** — the one session type the pipeline has never
   been validated against. Gates both Issue 08 and the standing
   `TRI_MAX_STARS = 30` question in TR §14. If no such session exists,
   acquiring one should precede those two issues.
6. **Satellite-trail session** — a frame or two with a bright trail.
   Directly tests Issue 06's contaminated-statistics claim; the trail's
   visibility in the output is the observable.
7. **Small stack (5-10 frames)** — where the population-vs-sample σ error
   in Issue 06 is largest (~10.6% at n=5).
8. **Cross-session comparability** — stack the same target on two nights,
   compare output statistics. Tests Issue 07.

---

## Suggested sequencing

Not prescriptive, but there are real dependencies:

- **Issue 10 (constants relocation) before Issues 06, 08, 09**, which all
  propose changing values that Issue 10 moves. Otherwise the same lines get
  edited twice and a regression can't be attributed cleanly.
- **Issue 12's TR pass after** Issues 06, 07, 09, 10, which all change §7
  content further. The `validate_alignment()` deletion half has no such
  dependency and can go immediately.
- **Issues 01 and 04 are independent** and both small — good candidates to
  land first.
- **Issue 07 needs a decision before any code**, and its blast radius
  extends outside the three audited files.
- **Issue 08 is blocked on a sparse-star session** if one isn't available.

---

## Note on the audit brief

The issue that scoped this audit was written before the current source was
read, and several of its premises are now stale: it treats the Issue 131
sign fix as under active investigation (it is applied and propagated), asks
for an audit of `flip_180` composition in the cross-group solve (Issue 134
removed it), and describes `AffineRigid` as scale-fixed (it deliberately
is not). Those discrepancies are documented in Issue 12 as TR drift, since
the brief inherited them from TR §7. Nothing was skipped as a result — the
audit was performed against source, not the brief.

---

# Notes on Color Balancing

The Photyx stacked images don't seem to be quite right, color/channel
wise. When I import one of Photyx's stacked images into Pixinsight and
perform a background extraction, the result is not right - colors all
wrong, etc. Are we missing some type of balancing or check that is
causing that to happen?

Yes — there's a specific mechanism in the code that would produce
exactly this, and it's worse for background extraction than for
anything else.

The likely cause: per-frame background division destroys the color
ratios before the frames are ever combined.

In Pass 1 and Pass 2, each frame is divided by a single scalar:

rust let bg_est = estimate_background(&cal_luma, &bg_sigma_config);
let bg_level = bg_est.median; let divisor = if bg_level > 1e-6 {
bg_level } else { 1.0 }; let accum_pixels: Vec<f32> =
frame_pixels.iter().map(|&v| v / divisor).collect(); cal_luma is
extract_luminance(...) — 0.299R + 0.587G + 0.114B. So the divisor is
derived from a luminance background, then applied identically to R, G,
and B.

That's fine only if every frame's background has the same color. It
doesn't. Sky background color shifts frame to frame with airmass,
moonlight, light-pollution angle, and transparency. A frame with a
redder sky gets a divisor weighted toward its own red-heavy background
and applied to all three channels — so its R:G:B ratio comes out
different from a frame with a bluer sky. You're averaging frames whose
color balance has been differently distorted. The result is a stack
with a color cast that varies spatially, because different frames
dominate different regions after dither and rejection.

This is precisely what breaks background extraction. DBE and ABE model
a smooth background per channel and subtract it. They assume the
background gradient is a real, physical, smooth field. Here it isn't —
it's a synthetic artifact of averaging inconsistently-scaled channels,
so the per-channel models don't correspond to anything real and the
correction goes wrong.

Compounding it: the output stretch.

normalize_output() then applies a global min/max stretch across all
three channels. The comment says this preserves channel ratios, and
relative to per-channel normalization it does. But it subtracts a
global min_val from every channel, which shifts each channel's zero
point by the same absolute amount — and a uniform additive shift does
not preserve ratios. It changes the color of everything near the
background level, which is the entire region background extraction
works on. min_val and max_val are also unclipped extremes, so one hot
pixel sets the scale.

That's Issue 07 in the audit, but I scoped it as a
photometric-comparability problem. Your symptom says it's also a
color-correctness problem, which makes it more serious than I filed
it.

What's missing that mature stackers have: per-channel
normalization. Siril's normalization_coeff struct carries poffset[3],
pmul[3], pscale[3] — per-channel offset, multiplier, and scale. Photyx
computes one scalar from luminance. That's the gap.

Before I write this up, one thing I should check rather than assume. I
haven't read analysis/background.rs, so I don't know exactly what
estimate_background returns or whether it has any channel
awareness. And I'd want to confirm what the debayer path produces —
whether there's any white balance applied at all, since
debayer_bilinear does pure bilinear interpolation with no channel
scaling, meaning raw OSC channel sensitivities (green roughly 2× red
and blue on most Bayer sensors) pass straight through untouched. If
nothing downstream applies white balance, that alone would make the
stack look wrong before any of the above.

Can you upload analysis/background.rs? And it'd help to know: does the
stack look wrong before you run background extraction, or only after?
If it's wrong before, the debayer white-balance question is probably
primary. If it only goes wrong under DBE/ABE, the per-frame divisor
inconsistency is the likelier culprit.


background.rs 343 lines

rs

Stack looks fine. Based on the results of the autostretch, I presume
our stretch uses unlinked channels. Is that correct?

I can't confirm that from what I've read — AutoStretch and
get_autostretch_stack_frame live in files I haven't seen, so I'd be
guessing at whether the STF is linked or unlinked. That's worth
checking directly rather than inferring.

But your observation points somewhere useful regardless, and it's
consistent with what background.rs now confirms.

If the autostretch is unlinked, it would mask the defect rather than
cause it. An unlinked STF computes independent c0/m per channel —
which is exactly a per-channel background equalization. That's why the
stack looks fine in Photyx: the display path is silently correcting a
channel imbalance that's genuinely present in the pixel data. Export
the same buffer to PixInsight, apply a linked stretch or run DBE, and
the correction is gone — the underlying imbalance surfaces. Stack
looks fine, background extraction goes wrong. That's your symptom
exactly.

What background.rs confirms about the imbalance itself:

estimate_background takes luma: &[f32] and calls subsample(), which
strides the flattened array by 8. There's no channel awareness
anywhere in the file — it's a scalar estimator on a luminance
vector. So the stacking divisor is definitively a single
luminance-derived number applied identically to R, G, and B, as I
suspected.

Two consequences now confirmed rather than assumed:

No white balance exists anywhere in the pipeline. debayer_bilinear
interpolates and clamps; it applies no channel
scaling. extract_luminance uses fixed Rec.601 weights
(0.299/0.587/0.114) which are for display luminance, not for OSC
sensor response. On a typical Bayer sensor green is roughly 2× more
sensitive than red and blue, so the raw stack carries that imbalance
untouched. An unlinked display stretch hides it completely.  The
divisor is inconsistently colored across frames. Frames whose sky is
redder get a divisor pulled by red-heavy background; that divisor then
scales all three channels. Different frames end up with
differently-distorted R:G:B ratios before averaging.  There's also a
subtlety worth flagging: subsample() strides by 8 over the flattened
array. In Pass 1/Pass 2 it's called on cal_luma, which is already
luminance — one value per pixel — so the stride is uniform. Fine. But
if estimate_background were ever called on an interleaved RGB buffer,
a stride of 8 against a period-3 layout would sample channels in a
repeating R,G,B,R,G,B pattern — not a bug today, but a trap for anyone
who later passes RGB to it.

What I'd want before writing this up:

The AutoStretch plugin source and get_autostretch_stack_frame. If the
STF is unlinked, that's the confirmation that the display path is
masking a real data defect, and it changes how I'd frame the issue —
not "stacking produces wrong color" but "stacking produces unbalanced
channels, and the display path hides it." Those get different fixes.

A quick diagnostic you can run now, if easier than uploading: load a
Photyx stack in PixInsight and read the per-channel median from
Statistics. If green sits well above red and blue, that's the missing
white balance showing through.

Can you please write an issue to investigate this question?

I'll write it as an investigation issue — the root cause isn't
confirmed yet, and per the root-cause-before-mitigation rule this
shouldn't propose a fix.


Created a file, read a file Created a file, read a file Filed as an
investigation, not a fix — three plausible mechanisms overlap here and
fixing the wrong one would look like progress while leaving the defect
intact.

The three candidates:

No white balance anywhere. Confirmed from source — debayer_bilinear
applies no channel scaling, and nothing downstream does either. Raw
OSC green dominance passes straight through.  The per-frame divisor is
a green-weighted luminance scalar applied to all three
channels. Confirmed now that background.rs shows estimate_background
is scalar with no channel awareness. Because sky color varies frame to
frame, the frames being averaged carry differently distorted ratios —
which produces a spatially varying error, and that's specifically what
breaks DBE.  Your unlinked-stretch hypothesis. Flagged as a
hypothesis, not a finding, since I haven't read AutoStretch. If it's
unlinked, the viewer is doing per-channel equalization at render time
and hiding a real data defect — which explains "looks fine in Photyx"
exactly.  The investigation is ordered cheapest-first. Step 1 is just
reading per-channel medians in PixInsight Statistics — if green sits
well above red and blue, mechanism 1 is confirmed in about two minutes
and is probably the dominant term.

Step 4 is the one that settles mechanism 2: stack the same session
twice, once with the divisor forced to a constant. If DBE improves
markedly, the inconsistent per-frame scaling is real rather than
theoretical.

To get past step 2 I'll need the AutoStretch plugin and whichever file
holds get_autostretch_stack_frame.

One note: this overlaps with Issue 07 from the audit. That one flagged
normalize_output's global min_val subtraction as a
photometric-comparability problem. Your symptom suggests it's also a
color problem — a uniform additive shift doesn't preserve channel
ratios, and it lands right at the background level DBE works on. The
two should be assessed together.
