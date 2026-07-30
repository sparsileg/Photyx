# Pipeline Optimization — What Worked, and Where Else It Applies

AnalyzeFrames went from 28s to 9.4s on a 105-frame OSC session. Nothing about
the algorithm changed; every metric is bit-identical. What changed was how much
memory the pipeline touched to produce the same numbers.

The techniques below are ordered by how much they returned, and each notes
where else in Photyx the same conditions hold.

---

## 1. Instrument the pipeline, not the function

The first hypothesis was wrong. A plausible story — the prefetch channel capped
at 4 while chunks needed 15 — survived code reading and collapsed the moment a
counter was added: `send_blocked_ms` was zero on every run, meaning the reader
never once waited for channel space. There was no backlog to relieve.

The instrumentation that mattered was cheap and specific: wall-clock accumulators
around each stage, written to a file rather than a log (release builds don't
carry debug logging, and the timings only exist in release). Three numbers per
frame — normalize, debayer, luminance — were enough to redirect the entire effort.

**Rule:** a hypothesis about where time goes is worth exactly one counter. Add
the counter before adding the fix. In this case the "obvious" bottleneck was
responsible for none of the time, and the stage nobody had questioned was
responsible for two-thirds of it.

---

## 2. The serial remainder sets the ceiling

The first optimization attempt parallelized `debayer_bilinear` across 15 cores
and returned 20%. The reason: it was one of three stages, and the other two —
`to_f32_normalized` (17ms) and `extract_luminance` (31ms) — stayed
single-threaded. 48ms of untouched serial work inside a 220ms stage caps the
achievable speedup near 4x no matter how well the middle stage scales.

**Rule:** before parallelizing a stage, measure the stages you aren't touching.
If they sum to a third of the total, that third is your floor.

---

## 3. Fuse stages and delete the intermediates

This was the win: 146ms became 17.5ms, an 8.4x improvement, by not building
data that was going to be thrown away.

The original path was `normalize → debayer → extract luminance`. Debayering
produced a full interleaved RGB frame — three floats per pixel — and luminance
extraction immediately collapsed it back to one float per pixel. The RGB frame
existed for one pass and was discarded.

Luminance at a pixel depends only on that pixel's 3×3 neighbourhood in the
source. So R, G and B can be computed as three scalars from a sliding window
and combined immediately. Nothing intermediate needs to exist.

Per 9-megapixel frame, allocation went from ~400MB to ~72MB:

| | before | after |
|---|---|---|
| three channel buffers | 109MB | — |
| three interpolation clones | 109MB | — |
| interleaved RGB output | 109MB | — |
| mono + luma | 72MB | 72MB |

The working set went from seven full frames to a three-row window that fits in
L2 cache. That is why the fused version parallelizes well while the previous one
didn't: the earlier version wasn't compute-bound, it was memory-bound, and no
number of cores fixes bandwidth.

**Where this applies:** any A → B → C chain where B's output is consumed once
and is larger than either endpoint. The tell is an intermediate buffer whose
size is a multiple of the input and whose lifetime is a single pass.

In Photyx specifically:

- **Session load** (`build_blink_jpegs`) reads, normalizes, stretches,
  downsamples and JPEG-encodes each frame at two resolutions. The full-resolution
  stretched buffer exists only to be downsampled to 12.5% and 25%. Stretching
  after downsampling — or fusing the stretch into the resize — would skip a
  full-frame intermediate twice per frame.
- **Stacking Pass 1/Pass 2** normalizes by background median, resamples, then
  accumulates. Each frame materializes a full aligned buffer that is read once
  by the Welford update. Resample-and-accumulate in one pass over output tiles
  would remove it.
- **`debayer_bilinear` still carries three `clone()` calls** (~109MB/frame) on
  the `ColorNormalized` path, where genuine samples are copied and then skipped.
  Writing them in the same loop removes the copy outright.

---

## 4. Allocation is compute

With `M_MMAP_THRESHOLD` set to 1MB — necessary here to stop heap fragmentation —
every large buffer becomes its own mapping. Each first touch is a kernel
zero-fill page fault, and each release is an unmap plus TLB shootdown across
every core that touched it.

This shows up as a distinctive signature: **low user-space CPU across many
cores**. During the parallel-but-slow phase, most cores sat at 25-35% while a
few peaked at 60-70%. That isn't a scheduling problem, it's threads waiting on
the kernel. Arithmetic parallelizes; page faults don't.

**Rule:** if parallel code shows low per-core utilization and the work involves
large short-lived buffers, count the allocations before touching the scheduler.
The fix is usually to allocate less, or to reuse buffers across iterations
rather than allocating per iteration.

---

## 5. Prove equivalence, don't assert it

The fused function was justified by an algebraic argument: the interpolation
only ever reads genuine sensor positions, so reading the source directly is the
same read. The argument was correct for two of the four Bayer patterns.

A test asserting the fused path bit-identical to the old two-call sequence, run
across all four patterns and both odd and even dimensions, failed immediately on
BGGR — and in doing so exposed a pre-existing bug that had been shipping: for
BGGR and GBRG frames, red and blue were zero at every green pixel, corrupting
half of all pixels in every debayered image.

That bug had survived because the only test used a flat grey image, where wrong
answers still look right, and because the developer's own sensor is RGGB.

**Rule:** when refactoring numerics, the test is not "does the new version look
reasonable" but "is it identical to the old one." Use adversarial input —
pseudo-random values, odd dimensions to exercise borders, out-of-range values,
every enum variant. Exact equality is the right assertion when both paths
perform the same operations in the same order.

---

## 6. Negative results worth recording

- **Raising the prefetch channel capacity** — the reader never blocked on it.
- **Dropping buffers per-worker rather than per-chunk** (`into_par_iter`) — no
  measurable change, despite a clean mechanism story about synchronized unmap
  bursts at chunk boundaries. The six slow frames per run remain unexplained.
- **Splitting the thread pool** between reader and consumer — considered at
  length, then made irrelevant by the fusion. The reader stopped being the
  bottleneck.

Two of the three were plausible enough to have been implemented on reasoning
alone. Measurement rejected all three for the cost of a few counters.

---

## Remaining, in order of value

1. `to_f32_normalized` is still a single-threaded 17ms per frame and is now the
   largest non-decode cost — roughly 1.4s of the 9.4s.
2. Decode (21ms/frame) is serial by necessity: cfitsio is not thread-safe, and
   one reader thread is a structural invariant, not a tuning choice.
3. The six outlier frames per run cost ~2s combined and are not understood.
