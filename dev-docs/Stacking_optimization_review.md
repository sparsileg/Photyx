# StackFrames — Deep Review of Registration & Integration

Static review of `stack_frames.rs`, `pixel_chunking.rs`, `star_align.rs`,
`fft_align.rs`. No measurement was performed. Every finding below states
whether it is *provably redundant work* (settled by reading) or *suspected
hot* (needs a counter).

The ordering is by my estimate of expected value for **your** case — OSC
color, 3008², ~100–130 frames, one or two rotational groups. That
qualifier matters: several findings collapse to nothing on a mono or
single-group session.

---

## Prologue

Worth doing — but let me be precise about what it can and can't produce, because the failure mode is real.

**What static review can settle without measurement:** whether redundant work *exists*. Recomputing something invariant, materializing an intermediate that's immediately discarded, an O(n²) where O(n log n) is available, a cache-hostile access pattern — those are provable from the source. **What it cannot settle:** whether any of them matters. That's still principle #1, and it doesn't bend.

The useful output, then, isn't a fix list. It's a *ranked hypothesis list with the counter attached to each one* — so that when you do instrument, it's one targeted pass instead of the three progressive rounds the AnalyzeFrames investigation burned. That's a real saving, and it's work I can do while you're asleep.

The risk I'll name up front: I could spend the night producing an elegant analysis of code that turns out to be 8% of wall clock. I'll mitigate by tagging each finding with what fraction of runtime it would need to occupy to be worth acting on.

Let me read the rest of what you uploaded — only `fft_align.rs` came through in-context.

Architected comprehensive performance analysis framework with ranked optimizations

Architected comprehensive performance analysis framework with ranked optimizations

Read all four. There's substantially more here than I expected — enough that this was worth doing. Writing it up now.

Created a file, read a file

Created a file, read a file

Twelve findings, ranked, with the counter attached to each. The short version of what's in there:

**The headline is F1: Pass 1 is a fully serial per-frame loop.** Frame N's alignment doesn't depend on frame N−1 — only the Welford fold is order-dependent. Meanwhile `compute_translation` and `estimate_rigid_transform` are both entirely single-threaded, so a meaningful fraction of every frame's time runs on one core. Pass 2 already has the right shape (chunk → parallel work → sequential accumulate); Pass 1 can adopt it.

**F2 is the one I'd have bet against finding.** `compute_translation` is called once per frame with the *same reference* every time, and it re-downsamples, re-windows, and re-FFTs that reference on every call. 127 redundant forward FFTs on a 128-frame group. Same shape as the Issue 186 win — building something and throwing it away, repeatedly.

**F3 is the one that annoyed me.** The color Welford accumulation is a serial scalar loop; the mono path twelve lines below does the identical job with `par_iter_mut`. Same asymmetry again in Pass 2. You shoot an ASI533MC — colour is your only real path, so both of those have run on every frame of every stack you've ever made.

Two things I want to flag rather than bury:

**F1 and parallelizing `fft_2d` cancel each other.** Same core-hours at different levels. F1 wins because it covers RANSAC and background estimation too.

**I couldn't rank `estimate_background` or `detect_stars` at all** — `background.rs` and `stars.rs` weren't in the upload, and both sit in the per-frame path. `estimate_background` in particular is five sigma-clipping passes over 9M floats, once per frame, inside the serial loop. If it's not internally parallel it's a top-three finding and I've simply missed it.

The document opens with the measurement pass rather than the fixes, and there's one counter in it that could invalidate the whole list: time blocked in `recv()`. If the pipeline turns out to be read-bound, this is a conversation about your USB-2 enclosure, not about algorithms.

---

## 0. Read this part first: what I could not settle

Four files were not in the set, and two of them sit directly in the
per-frame path:

- **`analysis/background.rs`** — `estimate_background` is called once per
  frame in Pass 1, inside the serial loop, with `SigmaClipConfig::default()`
  (5 iterations per §6.4's convention). That is potentially five passes
  over 9M floats per frame. Whether it is parallelized internally
  determines whether it is a rounding error or a top-three finding. I
  cannot rank it without the file.
- **`analysis/stars.rs`** — `detect_stars` runs once per frame in Pass 0,
  on the main thread, serially, while the reader prefetches only 3 frames
  ahead. If detection is slower than a read+debayer, Pass 0 is
  compute-bound and the prefetch depth is irrelevant.
- `analysis/debayer.rs` and `analysis/mod.rs` (`extract_luminance`,
  `to_f32_normalized`) — known quantities post-186, but I'd want to
  confirm `debayer_bilinear` is still the un-fused path for
  `LoadKind::ColorNormalized` (it is, per `pixel_chunking.rs:259`).

**One structural note that shapes everything below:** for a color stack,
`LoadKind::ColorNormalized` is requested in both Pass 1 and Pass 2, and
that path calls `debayer_bilinear` — the *unfused* debayer that Issue 186
specifically left alone because color genuinely needs materialized RGB.
So the 8.4× fusion win from AnalyzeFrames does not transfer here, and each
buffered frame is ~108MB. That is why `PREFETCH_MAX = 4`. Keep it in mind:
memory pressure is a live constraint in this pipeline in a way it wasn't
in AnalyzeFrames.

---

## 1. The measurement pass I'd run first

Same shape as the Issue 186 investigation — wall-clock accumulators
written to a file, release build only. Eight numbers, one instrumentation
round rather than three:

**Phase totals:** Pass 0, cross-group solve, Pass 1, Pass 2, output/crop.

**Pass 1 per-stage** (this is where I most expect the answer to be):
`recv`-wait · `extract_luminance` · `estimate_background` · normalize ·
`compute_translation` · `try_rigid_refinement` · resample · Welford
accumulate.

**Pass 2 per-stage:** `recv`-wait · divide · resample · accumulate.

**The one that could invalidate everything above it:** total time blocked
in `PixelReaderHandle::recv()`, per pass. If that dominates, the pipeline
is read-bound, every finding below is noise, and the real conversation is
about your USB-2 NVMe enclosure rather than about algorithms. That counter
is cheap and it is the same class of counter that killed the
`send_blocked_ms` hypothesis last time. Run it first.

---

## 2. Findings

### F1 — Pass 1 is a fully serial per-frame loop. *(Structural. Largest.)*

`stack_frames.rs:538` — `for (i, snap) in snapshots.iter().enumerate()`.
Everything for a frame happens inside it: luminance extraction, background
estimation, normalization, FFT translation, RANSAC refinement, resample,
Welford fold.

Some inner stages are parallel (`normalized_luma` build, the resamplers,
the mono Welford). Two large ones are **entirely single-threaded**:

- `compute_translation` — `downsample`, `hann_window_2d`, and all three
  `fft_2d` calls are scalar single-threaded loops.
- `estimate_rigid_transform` — greedy matching, 50 RANSAC iterations,
  least-squares refinement, all scalar.

So on a 16-core machine, Pass 1 spends a meaningful fraction of every
frame's time using one core, then briefly fans out to sixteen for the
resample, then collapses back. That is precisely principle #2 from the
AnalyzeFrames notes, except here the serial remainder is *structural*
rather than incidental — and it repeats 128 times.

**Nothing about Pass 1 requires this.** Frame N's alignment is completely
independent of frame N−1. The only order-dependent step is the Welford
fold into `mean_buf`/`m2_buf`.

**The restructure already exists in this file.** Pass 2 (line 889) is
exactly the right shape: `for chunk in pass2_inputs.chunks(n_threads)` →
sequential recv → `chunk_ok.par_iter()` for the expensive per-frame work →
sequential accumulate. Pass 1 can adopt the identical pattern: recv a
chunk, compute background + FFT + RANSAC + resample for all `n_threads`
frames in parallel, then fold the aligned buffers into Welford
sequentially in chunk order.

**Bit-identity:** preserved, provided the fold stays sequential and in the
same frame order. Welford is order-dependent in floating point; a
*parallel merge* (Chan et al.) would give a different-but-equally-valid
result. Don't do that — keep the fold sequential. The parallel part is
only the per-frame independent work.

**What blocks it, and it's real:**

1. **Memory.** The chunk would hold `n_threads` × (frame_pixels +
   normalized_luma + aligned) simultaneously. Color, 3008², 15 threads:
   roughly 15 × (108 + 36 + 108) MB ≈ 3.8 GB peak. Pass 2 already holds
   15 × (108 + 108) ≈ 3.2 GB, so the precedent exists — but this is the
   number that made Issue 47 necessary in the first place, and it wants
   deliberate sizing rather than inheriting `n_threads` unexamined. A
   Pass 1 chunk size decoupled from `rayon_thread_count` (the way
   `PREFETCH_MAX` already is) is probably the right shape.

2. **The lazy group-reference fill** at line 632 mutates `group_ref_luma`
   and `group_ref_stars` from inside the loop. This has to be hoisted:
   load every group's reference before the loop. That's a bounded set —
   one per group — and it's arguably cleaner regardless, since the
   current version's failure path (line 648, whole-group exclusion
   discovered mid-loop) is awkward exactly because it's discovered late.

3. **`contributions` positional alignment** (the Issue 174 comment at
   line 490 is explicit about this). The `continue` paths push in loop
   order; a chunked version must reassemble in the same order. Mechanical
   but it's the kind of thing that silently corrupts the crop
   intersection at line 1075 if it's got wrong.

4. **The reader's prefetch depth** would need to change from
   `PREFETCH_SEQUENTIAL_DEPTH` (3) to `prefetch_capacity_chunked` — Pass 1
   would no longer be a single-frame-sequential consumer.

**Worth doing if:** Pass 1 is more than ~25% of wall clock. Given it does
per frame what Pass 2 does *plus* FFT, RANSAC, and background estimation,
I'd be surprised if it isn't the largest phase.

---

### F2 — The reference frame's FFT is recomputed for every single frame. *(Provably redundant.)*

`compute_translation(g_ref_luma, &normalized_luma, width, height)` is
called once per frame with **the same first argument for every frame in
the group**. Inside `fft_align.rs`, that argument gets:

- `downsample()` — 3008² → 1024², scalar, single-threaded (line 84)
- `hann_window_2d()` — allocate and fill a 1024² buffer (line 88)
- the windowed complex buffer build (line 90)
- a full forward `fft_2d` (line 103)

All four are identical across every frame in a group. With 128 frames,
that's 127 redundant downsamples, 127 redundant window constructions, and
127 redundant forward FFTs.

Per call, `compute_translation` does three 2D FFTs (ref forward, target
forward, cross inverse), two downsamples, and one window build. Caching
the reference side removes one of three FFTs, one of two downsamples, and
the window entirely — call it **~40% of `compute_translation`, on every
frame**.

This is the same shape as the Issue 186 win: building something and
throwing it away, repeatedly.

**The fix is an API change, not a micro-optimization.** A
`PhaseCorrelator::new(reference, width, height)` that owns the
downsampled, windowed, forward-transformed reference spectrum plus the
reusable FFT plans, exposing `.translation_to(&target) -> Option<AlignmentTranslation>`.
Construct one per group reference; the existing free function stays as a
thin wrapper for the one-off call sites.

**Bit-identical:** yes. Identical arithmetic, just not repeated.

**Bonus, free once the struct exists:** the cross-group solve
(`stack_frames.rs:319`) re-downsamples and re-FFTs `master_ref_luma` once
per group. Small, but it comes along for the ride.

---

### F3 — The color Welford accumulation is a serial scalar loop. *(Provably suboptimal. Your path specifically.)*

`stack_frames.rs:750–761`:

```
for px in 0..n_pixels {
    count_buf[px] += 1;
    for ch in 0..3 { ... }
}
```

Fully serial, ~27M float operations per frame. Twelve lines below it, the
**mono** path does the identical job with `par_iter_mut` (line 769). There
is no stated reason for the asymmetry, and I can't construct one — each
pixel is independent in both cases.

The same asymmetry repeats in Pass 2: color accumulation at line 1004 is a
serial `for px in 0..n_pixels`; mono at line 1017 uses an iterator chain
(serial too, but at least vectorizable and not doing `px * 3 + ch` index
arithmetic three times per pixel).

You shoot an ASI533MC. **Colour is your only real path.** These two loops
run on every frame of every stack you have ever made, and both are
sitting on one core.

**Bit-identical:** yes for the Welford — parallelizing *across pixels*
doesn't change any pixel's arithmetic sequence. (Parallelizing across
*frames* would; that's F1's constraint, not this one.) Pass 2's is a plain
`f64` sum, also per-pixel independent.

**Cost to fix:** low. `par_chunks_mut(3)` on `mean_buf`/`m2_buf` zipped
against `count_buf.par_iter_mut()` and `aligned_rgb.par_chunks(3)`.

---

### F4 — The resamplers do two integer divisions per pixel and recompute the affine map per pixel. *(Provably redundant. Highest raw pixel count in the pipeline.)*

All four resamplers (`resample_frame`, `resample_frame_affine`,
`resample_frame_rgb`, `resample_frame_rgb_affine`) share this shape:

```
(0..height * width).into_par_iter().map(|idx| {
    let out_y = idx / width;
    let out_x = idx % width;
    ...
})
```

That is 9M integer divisions and 9M modulos per resample. Integer division
is ~20–40 cycles and does not vectorize. Switching to
`par_chunks_mut(width).enumerate()` over output rows gives `out_y` for
free and `out_x` as the inner loop index — both disappear.

Row-wise structure then unlocks two more things:

- **Incremental affine.** For an affine transform,
  `apply_inverse(x+1, y) − apply_inverse(x, y)` is a constant vector. The
  whole row's source coordinates can be a running add instead of two
  multiply-adds and a division-by-determinant per pixel. Classic DDA.
  (`apply_inverse` divides by `det = a² + b²` per call — that division is
  also loop-invariant and should be a reciprocal computed once.)
- **Interior fast path.** `bilinear` performs four `clamp` calls per
  sample. For the overwhelming majority of pixels no clamping is needed;
  a per-row span calculation splits each row into a clamped prefix, an
  unclamped interior, and a clamped suffix. Standard resampler structure.

And separately, **`bilinear_rgb` is called three times per pixel** (line
1814, `(0..3).map(...)`), each call recomputing the same four `clamp`s and
the same four base indices. That's 12 clamps per pixel where 4 would do,
and three redundant index computations. Compute the four corner indices
once, loop the channels inside.

**Why this matters more than it looks:** the resamplers run **twice per
frame** (Pass 1 and Pass 2), and for color each run touches 27M output
values with four source samples each. This is the single largest
pixel-touch count in the whole pipeline. Even a modest per-pixel constant
reduction is multiplied by ~256 full-frame resamples on a 128-frame color
stack.

**Bit-identical:** the index restructure and the clamp hoist, yes.
Incremental affine, **no** — accumulated floating-point error along a row
differs from independent evaluation. At 3008 px per row with f32 that
error is small but real. Either use f64 accumulators for the running
coordinate (cheap, keeps it effectively exact) or treat it as a change
requiring validation. I'd use f64 and keep it in the bit-identical bucket.

---

### F5 — `fft_2d` rebuilds its planner per call and its column pass is cache-hostile. *(Provably suboptimal.)*

Three separate problems in `fft_align.rs:225`:

**(a) `FftPlanner::new()` inside the function.** Called three times per
`compute_translation`, so three times per frame. rustfft caches computed
plans *within a planner instance*; a fresh instance discards that cache
and regenerates twiddle-factor tables for size 1024. The plan objects are
`Arc<dyn Fft<f32>>` and are `Send + Sync`, so they can live as fields on
F2's `PhaseCorrelator` and be reused for the entire run.

**(b) The column pass is a strided gather/scatter.** Lines 236–244: for
each of 1024 columns, copy 1024 elements strided by 1024 into `col_buf`,
transform, write back strided. Each strided access touches its own cache
line; you effectively stream the entire 8MB buffer twice per column with
near-zero reuse, and it doesn't fit in L2. The standard fix is
**transpose → row FFT → transpose**, with a blocked transpose (32×32
tiles). For this size that's typically a 3–8× improvement on the column
pass alone.

**(c) No parallelism.** Row FFTs are embarrassingly parallel
(`par_chunks_mut(width)`), and after a transpose the column pass is too.

**Important — (c) conflicts with F1.** If Pass 1 becomes chunk-parallel
across frames, the cores are already saturated and adding inner
parallelism to the FFT just adds contention and scheduling overhead. These
are **alternatives, not additive.** Pick the outer one (F1) — it
parallelizes RANSAC and background estimation too, not just the FFT.
Items (a) and (b) remain worth doing either way, since they reduce
absolute work rather than distributing it.

**Bit-identical:** (a) and (b) yes — same arithmetic, different plan
lifetime and different memory traversal order. (c) yes, same reason.

---

### F6 — The Hann window is materialized but it's separable. *(Provably redundant. Small.)*

`hann_window_2d` builds `hann_row` and `hann_col`, then fills a full
`width × height` buffer with their outer product (line 262) — a 4MB
allocation and fill per call — which is then consumed in exactly one
streaming pass (line 90). Rank-1 data materialized as a full 2D array,
used once, discarded.

Fold `hann_row[x] * hann_col[y]` directly into the complex-buffer
construction. One allocation and one full memory pass disappear. Under F2
the two 1D vectors are computed once per group anyway.

This is principle #3 in miniature — smaller than the debayer fusion by
orders of magnitude, but the same reasoning applies and it's nearly free.

---

### F7 — `downsample` is single-threaded with a per-source-pixel branch. *(Suspected hot.)*

`fft_align.rs:126`. Runs 3008² → 1024² per call — twice per
`compute_translation` today, once after F2. Scalar, single-threaded,
recomputes `x0/x1/y0/y1` from float multiplies per output pixel, and
tests `v.is_finite()` on every one of ~9M source pixels.

Parallelize over output rows (`par_chunks_mut(dst_w)`); hoist the y-range
per row; precompute the x-ranges once into a small table (there are only
`dst_w` of them, and they're identical for every row).

The `is_finite` guard is a behavior question rather than a performance
one: whether NaN can reach here post-normalization is your call, not mine,
and I'd leave the guard alone unless you're confident. The other two are
free.

---

### F8 — Greedy star matching is O(N_ref × N_frame) with a `sqrt` inside the filter, once per frame. *(Suspected hot — needs a star-count number.)*

`star_align.rs:509`, inside `estimate_rigid_transform`, which runs once
per frame in Pass 1:

```
for &(fx, fy) in &translated {
    let best = ref_stars.iter().enumerate()
        .filter(|(j, _)| !used_ref[*j])
        .map(|(j, r)| (j, dist(r.cx, r.cy, fx, fy)))
        ...
}
```

Full scan of every reference star for every frame star, computing a
`sqrt` for each, then discarding all but the nearest within
`MATCH_TOLERANCE` (15px). With ~1000 stars a side that's 10⁶ distance
computations and 10⁶ `sqrt`s **per frame**, single-threaded.

Two independent fixes:

- **Drop the `sqrt`.** Compare squared distances against
  `MATCH_TOLERANCE²`, take the square root only of the winner (or not at
  all — nothing downstream uses the distance value). Free, bit-identical
  in outcome.
- **Spatial hash grid.** Bin reference stars into cells of side
  `MATCH_TOLERANCE`; each frame star then examines 9 cells instead of
  1000 stars. O(N) expected instead of O(N²). Care needed to preserve the
  exact greedy order and tie-breaking so the matched set is unchanged.

The same pattern appears in `collect_inliers` (line 456) and in the
cross-group residual verification (`stack_frames.rs:388`) — both run far
less often, so they're cheap follow-ons rather than reasons in themselves.

**I need your typical detected-star count to rank this properly.** At 200
stars a side it's negligible; at 2000 it's 4M distance computations per
frame and it belongs near the top.

---

### F9 — Triangle descriptor matching is O(n²) in descriptors where a sort makes it near-linear. *(Provably improvable. Session-shape dependent.)*

`star_align.rs:328` — `ref_descs.par_iter()` with an inner
`for fd in &frame_descs`. At `TRI_MAX_STARS = 30` there are C(30,3) = 4060
descriptors a side, so **~16.5M descriptor comparisons per call**, of which
the overwhelming majority are rejected immediately by the
`desc_dist > TRI_DESC_TOLERANCE` test (tolerance 0.02 in a [0,1]² space).

Descriptors are 2D points. Sorting `frame_descs` by `r1` and binary
searching the ±0.02 band reduces the inner loop from 4060 candidates to
roughly 4060 × 0.04 ≈ 160 — a **~25× reduction in comparisons**. A 2D grid
hash on (r1, r2) with cell size 0.02 does better still and is barely more
code.

This is the one genuine algorithmic improvement in `star_align.rs`, as
opposed to constant-factor work.

**But it only fires on multi-group sessions.** `estimate_rigid_transform_triangles`
is called from exactly two places: the cross-group `M_cross` solve (once
per non-master group), and `resolve_rot_diff` during group assignment
(only when `ROTATOR` is missing on a frame). **On a single-group session
with ROTATOR present throughout, this code never runs at all.** Issue
43/47 optimized it hard because your M82 benchmark had a meridian flip;
if your current benchmark doesn't, this is dead weight in the ranking.

Note also that `resolve_rot_diff` rebuilds descriptors for the same frame
twice (once as `curr` for pair (i−1, i), once as `prev` for pair (i, i+1))
when it runs at all.

---

### F10 — Pass 1 and Pass 2 each read, decode, debayer and resample every frame. *(The structural one. Highest ceiling, highest risk.)*

This is the double-work the earlier optimization notes flagged and never
investigated. Per frame, twice: full disk read, full decode, full
`debayer_bilinear` to RGB (~108MB), divide by divisor, full resample.

The reason is stated in TR §7.3 and it's correct: caching Pass 1's aligned
buffers would be 128 × 108MB ≈ 13.8GB. That was rejected on sound grounds
and I'm not reopening it.

What's worth naming is the option space, because none of it is free and
the right answer depends on the split the instrumentation gives you:

- **Disk-backed scratch (mmap).** Write Pass 1's aligned buffers to a temp
  file, mmap in Pass 2. Trades 13.8GB RAM for 13.8GB of disk I/O. On your
  USB-2 NVMe enclosure that is almost certainly *worse* than recomputing.
  On the NTFS spinning path, marginal. I'd not pursue this.

- **Partial retention under a memory budget.** Keep aligned buffers for as
  many frames as fit in, say, 4GB (~37 color frames), re-read the rest.
  Saves ~29% of Pass 2's read+decode on a 128-frame session. The ratio
  isn't good enough to justify the complexity.

- **Decimated Pass 1.** The mean and σ maps exist *only* to gate Pass 2's
  clip decision. If Pass 1 accumulated on a 2× or 4× decimated aligned
  frame and Pass 2 upsampled the mean/σ maps, Pass 1's resample and
  accumulate cost would drop 4–16×.
  
  **I want to be careful not to oversell the third one.** It changes
  output, so it needs real-data validation, not `cargo test`. And there's
  a specific reason to be suspicious of it: the outliers this clip is
  meant to catch are *not* all large-scale. Satellite trails and aircraft
  are — decimation would handle those fine. **Cosmic ray hits are one to a
  few pixels**, and decimating the σ map smears a single hot pixel's
  contribution across its neighbours, which could plausibly let the hit
  through its own threshold *and* raise the threshold for the surrounding
  clean pixels. That's the same self-inclusion failure mode Issue 144
  already documents as a known limitation, made worse rather than better.
  I'd test this on a session with known cosmic ray hits before believing
  anything about it.

**My honest read:** measure the Pass 1 / Pass 2 split first. If Pass 2 is
dominated by read+decode, this is an I/O conversation. If it's dominated
by resample, F4 addresses it directly and with no output change, and F10
isn't needed.

---

### F11 — Diagnostic logging in the hot path. *(Cheap to test, possibly free.)*

Several blocks that look like leftovers from specific investigations:

- `stack_frames.rs:281–300` — sorts all group-reference stars by pixel
  count, clones the counts vec for a median, emits 11 log lines. Marked
  "galaxy-contamination investigation".
- `stack_frames.rs:363–437` — builds `ResidualSample` vectors, sorts
  twice, emits up to 27 log lines. Marked "companion to the max-residual
  gate issue".
- `stack_frames.rs:1086–1092` — **explicitly marked
  `TEMPORARY DIAGNOSTIC (Issue 111) — remove once ... understood and
  fixed`**, and it emits one `info!` per included frame.
- `stack_frames.rs:703–704` — per-frame `info!` in the Pass 1 loop.
- `star_align.rs` — 6 `info!` calls per `estimate_rigid_transform_triangles`
  invocation and 3 per `estimate_rigid_transform` (so 3 per frame).

Per-frame formatted logging with a synchronous rolling-file appender is
not obviously free at 128 frames × several lines. It's also not obviously
expensive. This is the cheapest possible test: run the benchmark once with
the log level raised above `info` and compare wall clock. One run, no code
change, definitive answer.

The Issue 111 one at line 1086 should probably just go regardless — it's
self-described as temporary and Issue 111 closed.

---

### F12 — Minor, listed for completeness

- `stack_frames.rs:487` — `master_ref_luma.clone()` and
  `master_ref_stars.clone()`, ~36MB, once per run. Trivial.
- `percentile_bounds` (line 1229) — `data.to_vec()` on the full
  interleaved buffer, ~108MB allocation, once per run. Uses
  `select_nth_unstable_by` correctly (O(n), no full sort). Fine.
- `resample_frame_rgb*` use `flat_map_iter` into `collect()` rather than
  writing into a preallocated buffer. Folds into F4's restructure.

---

## 3. What conflicts with what

Worth being explicit, because two of these actively cancel each other:

|                              | Combines with           | Conflicts with                            |
| ---------------------------- | ----------------------- | ----------------------------------------- |
| **F1** (parallel Pass 1)     | F2, F3, F4, F5(a,b), F8 | **F5(c)** — inner FFT parallelism         |
| **F5(c)** (parallel FFT)     | F5(a), F5(b)            | **F1**                                    |
| **F2** (cached ref spectrum) | everything              | —                                         |
| **F10** (decimated Pass 1)   | —                       | changes output; validate before combining |

**F1 and F5(c) are the same core-hours spent at different levels.** F1 is
strictly better because it parallelizes RANSAC and background estimation
as well, not just the FFT. If F1 lands, explicitly do *not* parallelize
`fft_2d`'s rows.

---

## 4. Suggested sequence

**Round 1 — measure.** The counters in §1, one instrumented release run on
your benchmark session. Everything below is conditional on what it says.

**Round 2 — the bit-identical set**, in this order, each independently
verifiable against a pre-change stack output:

1. F3 (color Welford + color Pass 2 accumulate) — smallest change,
   immediate, and it's on your actual path.
2. F2 (`PhaseCorrelator`, cached reference spectrum) — self-contained in
   `fft_align.rs` plus call-site changes.
3. F5(a) + F5(b) (planner hoist, transposed column pass) — falls naturally
   out of F2's struct.
4. F4 (row-wise resamplers, interior fast path, index hoist in
   `bilinear_rgb`).
5. F8 (squared distances; grid only if the star count justifies it).

**Round 3 — the structural one.** F1, as its own issue, discussed before
any code. It touches `contributions` ordering and the group-reference
lifecycle, which is exactly the kind of change that produces a subtly
wrong crop rather than a loud failure.

**Deferred pending evidence.** F9 (only if your benchmark has a group
split), F10 (only if Pass 2 is resample-dominated rather than
decode-dominated, and only with real-data validation on a session with
known cosmic ray hits).

---

## 5. What I need from you

1. **Which session is the benchmark**, and its current release-build wall
   clock. The M82 128-frame set at 3m24s is the last number I have and it
   predates Issues 174/175 entirely.
2. **`background.rs` and `stars.rs`** — both sit in the per-frame path and
   I can't rank them blind.
3. **Typical detected star count** on your frames — this is the single
   number that moves F8 between "free cleanup" and "top three."
4. Whether the benchmark session has a **meridian flip / multiple
   rotational groups**. F9's entire value depends on it.

One caveat on all of the above: this is a static review. Every "provably
redundant" claim means the work is definitely being done twice, not that
removing it will definitely show up on a stopwatch. The counter comes
first.
