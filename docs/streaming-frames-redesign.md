# Issue 170 — Recommendation: Streaming frame load (Option A, full redesign)

## Decision

**Option A — full redesign**, approved by Stan in discussion. The load
pipeline will no longer require full-session raw-pixel residency for
anything. Session size becomes unbounded by RAM; resident memory scales
with per-frame *metadata* (unavoidable, small) plus fixed-size bounded
working sets, not with raw pixel data.

## Tester constraint — confirmed assumption

Working assumption per Stan: the tester's 64 GB genuinely was not enough
to hold all frames in the desired session — RAM exhaustion, not the
32 GB `buffer_pool_memory_limit` preference ceiling alone. No separate
small preference-ceiling fix is planned; the redesign supersedes it.

## Design

### 1. `ctx.image_buffers` becomes a metadata registry + viewing LRU

The structure stays. `ImageBuffer.pixels` is already
`Option<PixelData>` — after load, pixels are `None` for all frames
except a small recently-viewed set.

- **Metadata resident for every frame**: keywords, dimensions, color
  space, bit depth, channels. Grouping, keyword queries, and session
  listing all work unchanged. Memory scales with session size but only
  at metadata cost — accepted; a session can't be truly unlimited anyway.
- **Viewing LRU stored in-place**: up to N frames keep `pixels =
  Some(...)`. Eviction = set `pixels` back to `None` (drops the
  allocation). A small side structure on `AppContext` (`Vec<String>`
  of paths, most-recent-first, ~20 lines with touch/evict helpers)
  provides recency order, since HashMap has none.
- **LRU capacity = `ctx.rayon_thread_count`** (Stan's preference:
  reuse the existing bound). *Noted alternative for the record*: a
  standalone constant in defaults.rs (e.g. 4) would decouple viewing
  cache size from the parallelism knob; rejected for now in favor of
  fewer constants. Post-171, rayon_thread_count is always a concrete
  positive value, so this bound is always sane.
- **LRU stores raw decoded pixels — hard requirement, not an option.**
  `get_pixel` (Info Panel), `get_histogram`, and DebayerImage's manual
  preview read actual pixel values from `image_buffers`. Because the
  LRU lives in the same structure, all these consumers work with
  ZERO changes. This is the main payoff of the in-place design.

### 2. Click-to-view reads from disk

Selecting a frame in the session list triggers: disk read → decode →
(debayer if needed) → store raw pixels into the frame's `image_buffers`
entry (touching the LRU, evicting oldest if over capacity) → downsample
+ JPEG encode for display, as today. Recently-viewed frames are warm
(instant, as today); cold frames cost one disk read + decode.

### 3. Blink caches generated during load

`AddFiles`/`ReadImages` change from "read everything, keep resident"
to: read each frame sequentially → record metadata into
`image_buffers` (pixels = None) → downsample directly to the 12.5%/25%
blink JPEGs → discard the raw buffer → next frame. The separate
post-load `CacheFrames`/`start_background_cache` pass becomes
unnecessary for the load path (its plumbing may survive for the
resolution-change rebuild case — implementation issue decides).

`display_cache` (per-frame display-resolution JPEGs, ~hundreds of KB
each) also scales per frame. Bounded but nontrivial at very large
sessions (thousands of frames → high hundreds of MB). The
implementation issue must decide: keep build-all (simple, known cost)
vs. on-demand/LRU (smaller resident set, more disk churn). No
recommendation forced here; either is compatible with the design.

### 4. AnalyzeFrames + StackFrames read pixels from disk, per chunk

Confirmed against source this session: BOTH are buffer-dependent today.
`pixel_chunking::snapshot_pixel_chunk` (AnalyzeFrames, CacheFrames)
clones chunks out of `image_buffers`; StackFrames Pass 2's chunk loop
re-looks-up pixel data by path from `image_buffers` (its "rolling
buffer" bounds the extra copy, not the base residency). Both switch
their pixel source to disk:

- A shared disk-sourced chunk loader replaces/extends
  `pixel_chunking.rs`: given a chunk of paths, read + decode each from
  disk into owned `FramePixelSnapshot`s. Chunk size logic unchanged
  (rayon_thread_count, always concrete post-171).
- **Analysis chunks DO NOT route through `image_buffers`.** Rayon
  closures can't borrow `&mut AppContext`, so owned extraction is
  required regardless — a stop in the map would cost an extra full
  copy per chunk and churn the viewing LRU with analysis frames,
  evicting what the user was actually looking at. Disk → owned
  snapshots directly.
- **The disk loader ERRORS on an unreadable/missing file — never
  silently skips.** Today's snapshot_pixel_chunk skips pixel-less
  frames; under this design that behavior is dangerous (an accidental
  buffer-sourced pass would process only the ~N resident frames and
  "succeed"). Hard rule: nothing outside the viewing path may depend
  on pixels being present in `image_buffers`.
- Both paths (viewing and chunk loading) share ONE low-level
  per-format read/decode function — cfitsio's sequential-access rule
  is enforced once, there.
- StackFrames cost note: it touches pixels in both passes, so a
  disk-sourced stack = two full read passes over the session per run,
  FITS serialized within each. This is the accepted price of unlimited
  session size.

### 5. Chunk prefetch (pipelined read-ahead) — follow-up, not v1

While Rayon workers process chunk N, one background reader thread
loads chunk N+1 from disk. cfitsio safety falls out for free: the
prefetch reader is a single sequential thread, so FITS reads never
run concurrently. Transient memory doubles to 2 × threads × frame
size — still bounded and small. Built once in the shared chunk
loader; AnalyzeFrames, CacheFrames, and both StackFrames passes all
inherit it.

**Must land as a separate step after the synchronous disk-read
version is proven correct** — prefetch adds a thread-coordination
surface and should not be debugged simultaneously with the source
switch.

## Memory accounting (best estimate; ASI533MC Pro 3008×3008 U16 basis)

Per-frame resident (scales with session size):
- Metadata (keywords, dims, etc.): ~1–2 KB/frame — negligible
- blink_cache_12 + blink_cache_25 JPEGs: ~tens of KB/frame
- display_cache JPEGs (if build-all retained): ~100–300 KB/frame

Bounded transients (do NOT scale with session size):
- Chunk pixels: threads × ~18 MB/frame raw (×2 with prefetch)
- Viewing LRU: up to threads × ~18–54 MB/frame (raw, possibly
  debayered to RGB f32 depending on path — implementation to pin down)
- StackFrames accumulation buffers (sum/stddev/count over full canvas,
  RGB f64/f32): roughly low hundreds of MB per run at this resolution
- FFT working buffers during alignment: tens of MB, transient

Small residuals: full_res_cache (bounded by use), stack_result (one
frame), analysis results (~KB/frame).

Known unknowns, labeled as such: exact debayered-vs-raw storage form
in the LRU; display_cache policy; real blink/display JPEG sizes on
representative data (measure, don't estimate, before quoting numbers).

### `buffer_pool_memory_limit` — redefined

The old meaning ("can the whole session fit") is gone. New effective
meaning per Stan: resident budget ≈ metadata + (pixels × threads) +
blink caches [+ display_cache per policy decision]. Whether the
preference survives as a user-visible knob (e.g. capping concurrent
chunk memory on low-RAM machines), gets repurposed, or is retired is
delegated to the load-path implementation issue. §8.4 of the technical
reference needs updating whichever way it lands.

## Constraints carried forward (from Issue 170's own text)

- cfitsio not thread-safe → all FITS reads sequential, in every pass,
  regardless of thread count. Enforced structurally by the single
  shared reader (and single prefetch thread).
- OS page-cache benefit between passes is opportunistic only — no
  fallback logic, no cache-priming, no perf claims without a
  drop-caches before/after measurement on a representative session.
- Fresh uploads required before any implementation code, per file,
  per standing methodology.

## Follow-on issues (to be filed; no code lands under 170)

1. **Load path redesign** — blink-cache-during-load;
   `image_buffers` → metadata registry; viewing LRU + click-to-read;
   display_cache policy decision; `buffer_pool_memory_limit`
   redefinition. The largest of the three.
2. **Disk-sourced chunking** — shared disk chunk loader (synchronous);
   AnalyzeFrames + CacheFrames + StackFrames (both passes) switch
   pixel source; error-not-skip semantics; single shared per-format
   read function. Depends on #1's loader/registry shape.
3. **Chunk prefetch** — pipelined read-ahead in the shared loader.
   Depends on #2 proven correct. Smallest, highest
   thread-coordination risk.

## Acceptance criteria status (Issue 170)

- [x] Written recommendation (this document) — Option A, tradeoffs
      accounted for
- [x] Tester constraint addressed (assumed genuine RAM exhaustion,
      per Stan)
- [ ] Follow-on issues filed with concrete acceptance criteria
      (drafts next, pending Stan's review of this document)
- [x] No code changes land under this issue number
