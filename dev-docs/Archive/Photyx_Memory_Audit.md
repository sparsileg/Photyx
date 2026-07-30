# Photyx Memory Audit

**Image size:** 3008 × 3008 pixels  
**Date:** May 2026

---

## Per-Frame Raw Buffer Size

| Format   | Bytes per pixel | Raw buffer size |
| -------- | --------------- | --------------- |
| Mono U16 | 2               | ~18.1 MB        |
| Mono F32 | 4               | ~36.2 MB        |
| RGB U16  | 6               | ~54.3 MB        |

These raw buffers live in `AppContext::image_buffers` for the lifetime of the
session. They are the ground truth and are intentionally never modified.

---

## Session Memory at Rest (50 Mono U16 Frames)

| Component                         | Size        |
| --------------------------------- | ----------- |
| Raw pixel buffers (50 × 18.1 MB)  | ~905 MB     |
| display_cache (50 × ~400 KB JPEG) | ~20 MB      |
| full_res_cache (50 × ~400 KB)     | ~20 MB      |
| blink_cache_12 (50 × ~400 KB)     | ~20 MB      |
| blink_cache_25 (50 × ~400 KB)     | ~20 MB      |
| **Total resident**                | **~985 MB** |

JPEG size per frame is estimated at ~400 KB at display resolution. Actual size
varies with image content (sky background vs. dense star fields). All four
caches are only populated if display, full-res zoom, and blink caching have all
been exercised; in a typical session only `display_cache` and one blink
resolution are likely to be fully populated.

---

## AnalyzeFrames Peak Memory

`execute_all()` in `analyze_frames.rs` clones the pixel data of every loaded
frame into a `Vec<FrameSnapshot>` before handing off to Rayon for parallel
processing. This is required because `AppContext` cannot be shared as `&mut` across threads.

| Component                           | Size         |
| ----------------------------------- | ------------ |
| Raw pixel buffers (resident)        | ~905 MB      |
| FrameSnapshot clones (50 × 18.1 MB) | ~905 MB      |
| Derived caches (resident)           | ~80 MB       |
| **Peak during analysis**            | **~1.89 GB** |

The snapshot allocation is transient — Rayon drops it when `par_iter()` completes and results are collected. Memory returns to the session-at-rest
level after AnalyzeFrames finishes. This matches the observed OS-level spike
during analysis followed by partial recovery.

This is not a leak. It is a structural consequence of Rayon thread safety.
Peak memory during AnalyzeFrames is approximately **2× the raw buffer
footprint** plus resident caches.

---

## The Memory Leak: `SelectDirectory` Did Not Call `clear_session()`

Prior to the fix, `select_directory.rs` manually cleared only `file_list` and `image_buffers` when a new directory was selected:

```rust
// BEFORE (leaking)
ctx.active_directory = Some(normalised.clone());
ctx.file_list.clear();
ctx.image_buffers.clear();
ctx.current_frame = 0;
```

The following were **silently left behind** on every directory switch:

| Leaked structure           | Contents                                   |
| -------------------------- | ------------------------------------------ |
| `display_cache`            | Display-resolution JPEGs, previous session |
| `full_res_cache`           | Full-resolution JPEGs, previous session    |
| `blink_cache_12`           | 12.5% blink JPEGs, previous session        |
| `blink_cache_25`           | 25% blink JPEGs, previous session          |
| `analysis_results`         | Stale per-frame metrics                    |
| `last_session_stats`       | Previous session statistics                |
| `last_analysis_thresholds` | Previous threshold snapshot                |
| `last_stf_params`          | Previous Auto-STF parameters               |
| `last_histogram`           | Previous histogram data                    |

With four JPEG caches × 50 frames × ~400 KB each, each directory switch left
~80 MB of derived cache unreleased. After ten directory switches that
accumulates to ~800 MB of leaked cache on top of the growing raw buffer
footprint.

The raw pixel buffers (`image_buffers`) were cleared, which is why OS-level
memory appeared to partially recover — but the derived caches were not,
producing the steady increase observed across multiple sessions.

---

## The Fix

```rust
// AFTER (correct)
ctx.clear_session();
ctx.active_directory = Some(normalised.clone());
```

`clear_session()` already clears all eight structures listed above and
explicitly preserves `active_directory`, so setting it immediately after is
correct and complete. No other changes are required.

---

## What `clear_session()` Frees

| Structure                  | Freed?                  |
| -------------------------- | ----------------------- |
| `file_list`                | ✓                       |
| `image_buffers`            | ✓                       |
| `display_cache`            | ✓                       |
| `full_res_cache`           | ✓                       |
| `blink_cache_12`           | ✓                       |
| `blink_cache_25`           | ✓                       |
| `analysis_results`         | ✓                       |
| `outlier_frame_paths`      | ✓                       |
| `last_session_stats`       | ✓                       |
| `last_analysis_thresholds` | ✓                       |
| `last_stf_params`          | ✓                       |
| `last_histogram`           | ✓                       |
| `variables`                | ✓                       |
| `active_directory`         | ✗ (preserved by design) |
| `current_session_id`       | ✗ (preserved by design) |
| `is_imported_session`      | ✗ (reset to false)      |

`clear_session()` is complete and correct. The only bug was the failure to call
it from `SelectDirectory`.

---

## Recommendations

1. **Fix applied:** `SelectDirectory` now calls `ctx.clear_session()` before
   setting the new directory. This eliminates the cache leak on directory
   switch.

2. **AnalyzeFrames peak is expected and unavoidable** given Rayon thread-safety
   constraints. Ensure the system has at least 2× the raw session buffer size
   available before running AnalyzeFrames. For 50 mono U16 frames at
   3008×3008, plan for ~1.9 GB peak.

3. **Consider a `Close Session` prompt on directory switch.** Now that `SelectDirectory` calls `clear_session()`, the behavior is correct — but
   users may not expect their loaded images to be discarded when they select a
   new directory. A notification or confirmation may be appropriate depending
   on UX decisions.
