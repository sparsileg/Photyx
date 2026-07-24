// plugins/pixel_chunking.rs — Shared chunked pixel-snapshot loading + Issue
// 175 background prefetch reader
//
// Issue 174: the pixel source is now the source file on disk, not
// ctx.image_buffers. Since Issue 173, image_buffers is a metadata-only
// registry with raw pixels resident for only the small viewing LRU set;
// analysis/stacking/caching therefore cannot read pixels from it (they
// would see only the handful of recently-viewed frames). Callers process
// ctx.file_list in batches, and this module reads+decodes each batch's
// frames directly from disk into owned snapshots.
//
// Issue 175 adds a background reader thread (PixelReaderHandle) so disk
// read + decode + per-pass pixel conversion (debayer, luminance
// extraction, normalization) for the NEXT batch overlaps with compute on
// the CURRENT batch, instead of blocking in front of it. The conversion
// logic itself is unchanged from Issue 174 — only WHERE it runs moved
// (onto the reader thread). Output must remain identical to 174 on a real
// session; 174 is the regression baseline for 175, not anything older.
//
// FITS sequential-access discipline (cfitsio is not thread-safe): the
// disk read + decode for each path happens in exactly one place at a
// time — either load_pixel_chunk's sequential loop, or (post-175) inside
// the single PixelReaderHandle reader thread. Never inside a caller's
// Rayon closure, and never from two threads at once. This is the entire
// structural safety argument for the reader thread — do not spawn a
// second one anywhere.
//
// Consumers: AnalyzeFrames, CacheFrames, and all three StackFrames passes
// (Pass 0 collect_snapshots, Pass 1 Welford, Pass 2 sigma-clip — Pass 2
// previously had its own private read loop; Issue 175 folds it onto this
// shared reader). Each applies its own failure policy to
// LoadOutcome::Missing / ::Unreadable — see load_request's doc comment.

use crate::analysis::debayer::{bayer_pattern_of, debayer_bilinear, BayerPattern};
use crate::analysis::{extract_luminance, to_f32_normalized, to_luminance};
use crate::context::{AppContext, ColorSpace, ImageBuffer, KeywordEntry, PixelData};
use crate::plugins::image_reader::read_image_file;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

// ── Snapshot types ──────────────────────────────────────────────────────────

/// One frame's pixel data plus the metadata needed to process it, decoded
/// fresh from disk. Carries keywords so callers don't need a second
/// image_buffers lookup for plate scale, debayer decisions, or
/// filter/object metadata.
pub struct FramePixelSnapshot {
    pub path:     String,
    pub width:    usize,
    pub height:   usize,
    // Issue 175: CacheFrames is currently the only Raw/FramePixelSnapshot
    // consumer, and it neither needs channel count (forces mono output
    // regardless of source color) nor keywords (no plate-scale lookup).
    // Kept rather than trimmed, unlike ColorSnapshot — AnalyzeFrames used
    // to read both before its Issue 175 migration to LoadKind::Luma, and
    // any future Raw consumer with AnalyzeFrames-like needs would want
    // them back; trimming now would just mean re-adding them later.
    #[allow(dead_code)]
    pub channels: usize,
    #[allow(dead_code)]
    pub keywords: HashMap<String, KeywordEntry>,
    pub pixels:   PixelData,
}

/// Issue 175: a frame decoded and reduced to luminance — debayered first
/// if the source is Bayer. Used by every caller that only ever needed
/// luma (AnalyzeFrames' star/background metrics, StackFrames Pass 0 star
/// detection, StackFrames group/master/cross-group reference loads, and
/// mono-mode Pass 1/Pass 2 per-frame loads). Replaces a decode-then-
/// convert-on-the-consumer-side step with decode-and-convert on the
/// reader thread.
pub struct LumaSnapshot {
    pub path:        String,
    pub width:       usize,
    pub height:      usize,
    /// Source channel count (1 or 3) — not derivable from `luma` itself
    /// (always single-channel once extracted), but needed by callers that
    /// cache it for later use, e.g. StackFrames' FrameSnapshot.channels,
    /// used in its dimension-consistency check across the whole stack.
    pub channels:    usize,
    /// The source frame's color space, captured at read time. Needed by
    /// callers that cache it for later use — e.g. StackFrames'
    /// FrameSnapshot.color_space, which determines is_color for the whole
    /// stack (master reference's color_space) and is read again in Pass
    /// 1/Pass 2 to decide debayering on subsequent reads of the same file.
    pub color_space: ColorSpace,
    pub keywords:    HashMap<String, KeywordEntry>,
    pub luma:        Vec<f32>,
}

/// Issue 175: a frame decoded and normalized to [0,1] RGB — debayered
/// first if the source is Bayer, or a straight to_f32_normalized pass-
/// through if the source is already RGB. Used by color-mode StackFrames
/// Pass 1/Pass 2 per-frame loads (what load_frame_pixels's is_color
/// branch used to build on the consumer side). Trimmed to just the pixel
/// buffer: both current call sites already hold the frame's path/width/
/// height/channels/keywords from the FrameSnapshot already in scope at
/// the call site (Pass 0 populated it), so re-carrying them here was
/// dead weight — unlike LumaSnapshot/FramePixelSnapshot, whose callers
/// (Pass 0, CacheFrames) genuinely don't have that metadata elsewhere and
/// do read every field.
pub struct ColorSnapshot {
    pub rgb: Vec<f32>,
}

/// Issue 175: which representation a caller wants for a given file. The
/// reader thread (or load_pixel_chunk's synchronous fallback) performs
/// the corresponding decode + conversion once, on the read side, instead
/// of every caller re-deriving it after the fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadKind {
    /// Decode only — no debayer, no luminance/normalization. CacheFrames
    /// wants this (needs full PixelData for downsample_to_planes).
    Raw,
    /// Decode, debayer if Bayer, extract luminance. AnalyzeFrames,
    /// StackFrames Pass 0, every group/master/cross-group reference load,
    /// and mono-mode Pass 1/Pass 2 per-frame loads want this.
    Luma,
    /// Decode, debayer if Bayer else normalize to [0,1] RGB directly.
    /// Color-mode StackFrames Pass 1/Pass 2 per-frame loads want this.
    ColorNormalized,
}

/// Issue 175: one file + the representation requested for it. The full
/// ordered request list for a plugin run is built up front by the caller
/// (every consumer's request order is knowable before any reading
/// starts — see pixel_chunking's module doc and each caller's own
/// comments for how their list is constructed).
#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub path: String,
    pub kind: LoadKind,
}

/// Issue 175: the LoadedFrame representation actually produced for a
/// request, matching its LoadKind.
pub enum LoadedFrame {
    Raw(FramePixelSnapshot),
    Luma(LumaSnapshot),
    ColorNormalized(ColorSnapshot),
}

/// Per-path outcome of a load, in the same order requests were issued so
/// callers can zip against their own request list and apply policy.
///
/// The Missing/Unreadable split lets each caller distinguish a source file
/// that is gone from one that is present but failed to decode, and word its
/// diagnostics accordingly:
///   - Missing    — the path does not exist on disk.
///   - Unreadable — the file exists but read_image_file failed (carries the
///                  decode error), decoded to a buffer with no pixels, or
///                  (Issue 175) the reader thread panicked while decoding
///                  this specific file — converted to Unreadable rather
///                  than allowed to silently close the channel, so a
///                  caller relying on completeness (AnalyzeFrames) still
///                  sees a definite failure for this path instead of
///                  quietly receiving fewer outcomes than requested.
///
/// Failure policy is the caller's, not this module's: AnalyzeFrames treats
/// either failure as a hard error (partial frame sets corrupt session
/// statistics); StackFrames and CacheFrames record the frame as an
/// exclusion and continue (a degraded stack/cache is still useful, and the
/// exclusion is surfaced loudly in their summaries).
pub enum LoadOutcome {
    Loaded(LoadedFrame),
    Missing { path: String },
    Unreadable { path: String, error: String },
}

// ── Shared per-file loader ───────────────────────────────────────────────────

/// Read + decode + convert exactly one file, producing the representation
/// `kind` asks for. Shared by load_pixel_chunk's synchronous loop, the
/// PixelReaderHandle background reader's production loader, and any
/// other caller with a genuinely one-off (not looped) load — e.g.
/// StackFrames' load_debayered_luma, which calls this directly rather
/// than duplicating the debayer-or-luminance branching logic — the
/// actual decode/debayer/normalize logic lives in exactly one place.
///
/// pub(crate): callers outside this module use this for isolated,
/// non-looped loads where a background reader thread would be pure
/// overhead (nothing else running concurrently to overlap the read
/// against). Looped/batched loads should go through
/// PixelReaderHandle::spawn_disk_reader instead, not call this directly
/// in a loop — that would silently forfeit the Issue 175 prefetch
/// overlap this module exists to provide.
///
/// Sequential by contract: whichever caller invokes this (load_pixel_chunk,
/// the reader thread, or a direct one-off caller) must do so from a single
/// thread — this is the one place FITS files are opened, and cfitsio is
/// not thread-safe.
pub(crate) fn load_request(path: &str, kind: LoadKind) -> LoadOutcome {
    // Distinguish "gone" from "present but undecodable" before attempting
    // the decode, so the two produce different diagnostics.
    if !Path::new(path).exists() {
        return LoadOutcome::Missing { path: path.to_string() };
    }

    let buf = match read_image_file(path) {
        Ok(b)  => b,
        Err(e) => return LoadOutcome::Unreadable { path: path.to_string(), error: e },
    };

    if buf.pixels.is_none() {
        return LoadOutcome::Unreadable {
            path:  path.to_string(),
            error: "decoded buffer contains no pixel data".to_string(),
        };
    }

    let width    = buf.width as usize;
    let height   = buf.height as usize;
    let channels = buf.channels as usize;

    // Captured before buf is moved into the match arms below. ColorSpace
    // is Clone (already relied on elsewhere, e.g. stack_frames.rs's
    // ref_color_space = snapshots[master_ref_idx].color_space.clone()),
    // so this is a cheap value capture, not a second read — and is_bayer
    // still borrows rather than moves.
    let color_space = buf.color_space.clone();
    let is_bayer     = color_space == ColorSpace::Bayer;

    let frame = match kind {
        LoadKind::Raw => {
            let ImageBuffer { keywords, pixels, .. } = buf;
            LoadedFrame::Raw(FramePixelSnapshot {
                path: path.to_string(),
                width, height, channels,
                keywords,
                pixels: pixels.unwrap(), // checked non-None above
            })
        }

        LoadKind::Luma => {
            let ImageBuffer { keywords, pixels, .. } = buf;
            let pixels = pixels.unwrap();
            let luma = if is_bayer {
                let pattern = bayer_pattern_of(&keywords).unwrap_or(BayerPattern::RGGB);
                let mono    = to_f32_normalized(&pixels);
                let rgb     = debayer_bilinear(&mono, width, height, pattern);
                extract_luminance(&rgb, width, height, 3)
            } else {
                to_luminance(&pixels, channels)
            };
            LoadedFrame::Luma(LumaSnapshot {
                path: path.to_string(), width, height, channels, color_space, keywords, luma,
            })
        }

        LoadKind::ColorNormalized => {
            let ImageBuffer { keywords, pixels, .. } = buf;
            let pixels = pixels.unwrap();
            let rgb = if is_bayer {
                let pattern = bayer_pattern_of(&keywords).unwrap_or(BayerPattern::RGGB);
                let mono    = to_f32_normalized(&pixels);
                debayer_bilinear(&mono, width, height, pattern)
            } else {
                to_f32_normalized(&pixels)
            };
            LoadedFrame::ColorNormalized(ColorSnapshot { rgb })
        }
    };

    LoadOutcome::Loaded(frame)
}

// ── Synchronous batch loader (Issue 174 entry point, kept as a fallback /
//    tested-in-isolation primitive per the Issue 175 design doc) ───────────

/// Read + decode a batch of frames from disk, sequentially, as Raw
/// snapshots. One LoadOutcome per input path, in order.
///
/// Sequential by contract: this is a single-threaded loop and the one
/// place FITS files are opened when called — do not parallelize it, and
/// do not call it from more than one thread at a time. Callers
/// parallelize over the returned Loaded snapshots, not over this loop.
///
/// Unused in production as of Issue 175 — all three former callers
/// (CacheFrames, AnalyzeFrames) migrated to PixelReaderHandle's
/// background reader. Kept intentionally as a synchronous fallback and
/// as the simplest primitive to unit-test load_request's Raw path in
/// isolation, per the Issue 175 design doc.
#[allow(dead_code)]
pub fn load_pixel_chunk(paths: &[String]) -> Vec<LoadOutcome> {
    paths.iter().map(|path| load_request(path, LoadKind::Raw)).collect()
}

/// Resolve the effective chunk size from ctx.rayon_thread_count.
/// Used by AnalyzeFrames, StackFrames, and CacheFrames.
pub fn chunk_size(ctx: &AppContext) -> usize {
    (ctx.rayon_thread_count as usize).max(1)
}

// ── Issue 175: prefetch capacity ─────────────────────────────────────────────

/// Ceiling on how many decoded frames may sit buffered in the reader
/// channel at once, independent of thread count. Each buffered item is a
/// fully decoded (and possibly debayered-to-RGB, i.e. 3x) frame buffer —
/// e.g. a 3008x3008 OSC frame debayered to RGB f32 is ~108MB. Capping this
/// separately from rayon_thread_count keeps a high-core machine from
/// ballooning the prefetch buffer (n_threads=32 would otherwise mean 32
/// buffered frames ~= several GB).
///
/// TODO: move to settings::defaults::PREFETCH_MAX alongside the other
/// non-persisted runtime constants, per the constants-live-in-defaults
/// convention — left local here since defaults.rs wasn't part of this
/// change's file set.
pub const PREFETCH_MAX: usize = 4;

/// Prefetch depth for the chunk-shaped consumers (CacheFrames,
/// AnalyzeFrames, StackFrames Pass 2): enough to keep one batch buffered
/// while the previous batch is consumed (the 2x transient-memory budget),
/// capped by PREFETCH_MAX regardless of chunk_size.
pub fn prefetch_capacity_chunked(ctx: &AppContext) -> usize {
    chunk_size(ctx).min(PREFETCH_MAX)
}

/// Prefetch depth for the single-frame-sequential consumers (StackFrames
/// Pass 0 collect_snapshots, Pass 1 Welford): consumption here is one
/// frame at a time, so a shallow lookahead fully hides read latency
/// without holding more decoded frames than necessary.
pub const PREFETCH_SEQUENTIAL_DEPTH: usize = 3;

// ── Issue 175: background reader thread ──────────────────────────────────────

/// Owns a background reader thread that works through an ordered list of
/// LoadRequests, sending each LoadOutcome to the caller as it's ready.
/// RAII: dropping the handle stops the reader and joins it, on every exit
/// path (Ok, Err, or panic unwind) — meant to be held the same way
/// StackFrames::execute holds ProgressClearGuard.
///
/// Exactly one PixelReaderHandle may be reading from disk at a time within
/// a given plugin execute() call — this is the single-sequential-reader
/// property cfitsio's thread-unsafety requires. Never spawn a second one
/// concurrently with an existing one still in scope.
pub struct PixelReaderHandle {
    receiver: Option<Receiver<LoadOutcome>>,
    stop:     Arc<AtomicBool>,
    handle:   Option<JoinHandle<()>>,
}

impl PixelReaderHandle {
    /// Spawn the reader thread. `requests` is the full, precomputed,
    /// ordered list of files to read for this plugin run — every caller
    /// can determine its complete request list before any reading starts
    /// (e.g. StackFrames Pass 1's group-reference-then-per-frame ordering
    /// is known once Pass 0 has assigned every snapshot's group).
    ///
    /// `load_one` is injectable so tests can exercise the reader/handle's
    /// shutdown behavior with a synthetic, synchronizable loader instead
    /// of real disk I/O. Production callers should use
    /// `spawn_disk_reader`, which wires up the real loader.
    pub fn spawn<F>(requests: Vec<LoadRequest>, capacity: usize, load_one: F) -> Self
    where
        F: Fn(&LoadRequest) -> LoadOutcome + Send + 'static,
    {
        let (tx, rx): (SyncSender<LoadOutcome>, Receiver<LoadOutcome>) =
            sync_channel(capacity.max(1));
        let stop = Arc::new(AtomicBool::new(false));
        let stop_reader = Arc::clone(&stop);

        let handle = thread::spawn(move || {
            for req in requests {
                // Acquire pairs with the Release store in Drop, so this
                // early-out is a guaranteed observation of a shutdown
                // requested before this file started, not incidental.
                // Checked before each file — catches cancellation between
                // files, not mid-read on the current file (cancellation
                // is per-file granularity; see the module-level note in
                // the Issue 175 design doc for why finer granularity
                // wasn't worth threading into the format decoders).
                if stop_reader.load(Ordering::Acquire) {
                    break;
                }

                // A decode that PANICS (as opposed to returning Err) must
                // not silently vanish and let the consumer misread the
                // resulting channel-close as normal completion —
                // AnalyzeFrames' hard-error contract specifically depends
                // on not confusing "reader died mid-run" with "reader
                // finished". catch_unwind converts a panic into
                // Unreadable so it flows through each consumer's normal
                // failure policy instead. load_one should never actually
                // panic on ordinary decode failures (read_image_file
                // returns Result) — this is the backstop for an
                // unexpected panic inside decode/conversion.
                let outcome = match std::panic::catch_unwind(
                    std::panic::AssertUnwindSafe(|| load_one(&req))
                ) {
                    Ok(o)  => o,
                    Err(_) => LoadOutcome::Unreadable {
                        path:  req.path.clone(),
                        error: "reader thread panicked while decoding this frame".to_string(),
                    },
                };

                if tx.send(outcome).is_err() {
                    // Receiver dropped — consumer is gone. Expected
                    // shutdown path (see PixelReaderHandle::drop), not an
                    // error to log.
                    break;
                }
            }
            // Normal completion: `tx` drops here, closing the channel.
            // The consumer's next recv() returns None — no separate
            // "done" sentinel needed.
        });

        Self { receiver: Some(rx), stop, handle: Some(handle) }
    }

    /// Production entry point: spawns with the real disk-reading loader
    /// (load_request), so callers only need to supply the request list
    /// and a capacity — see prefetch_capacity_chunked /
    /// PREFETCH_SEQUENTIAL_DEPTH for how to pick the latter.
    pub fn spawn_disk_reader(requests: Vec<LoadRequest>, capacity: usize) -> Self {
        Self::spawn(requests, capacity, |req| load_request(&req.path, req.kind))
    }

    /// Blocking receive of the next loaded frame, or `None` once the
    /// channel is closed.
    ///
    /// IMPORTANT: `None` means "the channel is closed", NOT "every
    /// requested frame was fulfilled". On normal completion these
    /// coincide (the reader sends an outcome for every request, then
    /// closes). But a caller that needs to guarantee completeness
    /// (AnalyzeFrames, whose Issue 174 contract is a hard abort on any
    /// missing/unreadable frame — a partial set corrupts session
    /// statistics for every other frame) must NOT infer completeness from
    /// `None` alone. It must count received outcomes against its own
    /// request count and treat a shortfall as failure, independent of
    /// whatever caused it.
    pub fn recv(&mut self) -> Option<LoadOutcome> {
        self.receiver.as_ref().and_then(|r| r.recv().ok())
    }
}

impl Drop for PixelReaderHandle {
    fn drop(&mut self) {
        // 1. Signal stop first. Release pairs with the reader's Acquire
        //    load. Catches the reader before its NEXT read if it is not
        //    currently blocked inside send().
        self.stop.store(true, Ordering::Release);

        // 2. Drop the receiver. This is the step that actually unblocks a
        //    reader thread already parked inside a blocking send() on a
        //    full channel — step 1 alone cannot do that, since the stop
        //    flag is only checked between reads, not while blocked inside
        //    send(). Order matters: this must happen here, explicitly,
        //    before the join() below — not left to implicit field-drop
        //    order.
        self.receiver.take();

        // 3. Join. After steps 1+2, the reader thread is guaranteed to
        //    observe either the stop flag (if it was between reads) or a
        //    send() Err (if it was blocked on a full channel) on its very
        //    next check — so this join cannot hang. A reader panic is
        //    logged, never propagated or acted on further here — nothing
        //    panic-prone runs after join() so a Drop invoked during an
        //    unwind can't double-panic into an abort.
        if let Some(h) = self.handle.take() {
            if h.join().is_err() {
                tracing::error!("PixelReaderHandle: reader thread panicked during shutdown");
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod reader_tests {
    use super::*;
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    fn synthetic_ok(path: &str) -> LoadOutcome {
        LoadOutcome::Loaded(LoadedFrame::Luma(LumaSnapshot {
            path:        path.to_string(),
            width:       1,
            height:      1,
            // Values don't matter for these tests — none of the five
            // reader tests inspect channels/color_space, they only check
            // shutdown/panic behavior around the channel itself. Picked
            // arbitrary-but-valid values (1 channel, Mono) rather than
            // leaving them meaningless-but-present.
            channels:    1,
            color_space: ColorSpace::Mono,
            keywords:    HashMap::new(),
            luma:        vec![0.0],
        }))
    }

    fn make_requests(n: usize) -> Vec<LoadRequest> {
        (0..n)
            .map(|i| LoadRequest { path: format!("synthetic_frame_{}", i), kind: LoadKind::Luma })
            .collect()
    }

    /// Runs `f` on its own thread and fails the test if it doesn't
    /// complete within `timeout` — turns a shutdown hang into a fast,
    /// legible test failure instead of hanging the whole suite.
    fn with_timeout<F: FnOnce() + Send + 'static>(f: F, timeout: Duration) {
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            f();
            let _ = tx.send(());
        });
        if rx.recv_timeout(timeout).is_err() {
            panic!("operation did not complete within {:?} — likely deadlock in PixelReaderHandle shutdown", timeout);
        }
        worker.join().expect("test worker thread panicked");
    }

    #[test]
    fn test_immediate_drop_no_recv() {
        with_timeout(|| {
            let requests = make_requests(5);
            let reader = PixelReaderHandle::spawn(requests, 2, |req| synthetic_ok(&req.path));
            drop(reader);
        }, Duration::from_secs(5));
    }

    #[test]
    fn test_drop_mid_stream() {
        with_timeout(|| {
            let requests = make_requests(5);
            let mut reader = PixelReaderHandle::spawn(requests, 2, |req| synthetic_ok(&req.path));
            let _ = reader.recv();
            drop(reader);
        }, Duration::from_secs(5));
    }

    #[test]
    fn test_normal_completion() {
        with_timeout(|| {
            let requests = make_requests(3);
            let mut reader = PixelReaderHandle::spawn(requests, 2, |req| synthetic_ok(&req.path));

            let mut count = 0;
            while reader.recv().is_some() {
                count += 1;
            }
            assert_eq!(count, 3, "expected an outcome for every request before None");

            // Thread already exited via normal completion — join here
            // should be immediate, not just "not hanging".
            drop(reader);
        }, Duration::from_secs(5));
    }

    /// The specific deadlock case Drop must handle: capacity=1 with 3+
    /// requests, dropped before any recv(). A synchronization barrier
    /// (rather than timing) guarantees the reader has reached the
    /// blocked-inside-send() state before drop() runs — without it, this
    /// test would be racy and might never actually exercise the failure
    /// mode it claims to cover.
    #[test]
    fn test_deadlock_case_reader_blocked_in_send() {
        with_timeout(|| {
            let pair: Arc<(Mutex<usize>, Condvar)> = Arc::new((Mutex::new(0), Condvar::new()));
            let pair_reader = Arc::clone(&pair);

            let requests = make_requests(3);
            // capacity=1: the 1st send succeeds and fills the channel;
            // the reader then calls load_one for the 2nd request, at
            // which point we signal the barrier — the 2nd send() that
            // follows will block, since nothing has drained the 1st item.
            let reader = PixelReaderHandle::spawn(requests, 1, move |req| {
                let (lock, cvar) = &*pair_reader;
                let mut produced = lock.lock().expect("lock poisoned");
                *produced += 1;
                if *produced == 2 {
                    cvar.notify_all();
                }
                drop(produced);
                synthetic_ok(&req.path)
            });

            let (lock, cvar) = &*pair;
            let guard = lock.lock().expect("lock poisoned");
            let (_guard, timed_out) = cvar
                .wait_timeout_while(guard, Duration::from_secs(5), |produced| *produced < 2)
                .expect("condvar wait failed");
            assert!(!timed_out.timed_out(), "reader never reached its 2nd item — test setup is broken, not the Drop logic under test");

            // Reader has produced its 2nd item and is now blocked inside
            // send() (capacity=1, 1st item still unread). Drop without
            // ever calling recv() — this only completes if Drop's
            // ordering (stop, then drop receiver, then join) is correct.
            drop(reader);
        }, Duration::from_secs(10));
    }

    #[test]
    fn test_panic_in_loader_becomes_unreadable() {
        with_timeout(|| {
            let requests = make_requests(2);
            let mut reader = PixelReaderHandle::spawn(requests, 2, |req| {
                if req.path == "synthetic_frame_0" {
                    panic!("synthetic decode panic for test");
                }
                synthetic_ok(&req.path)
            });

            match reader.recv() {
                Some(LoadOutcome::Unreadable { path, error }) => {
                    assert_eq!(path, "synthetic_frame_0");
                    assert!(error.contains("panicked"), "error message should say the reader panicked: {}", error);
                }
                other => panic!(
                    "expected LoadOutcome::Unreadable for the panicking request, got: {}",
                    match other {
                        Some(LoadOutcome::Loaded(_))     => "Loaded",
                        Some(LoadOutcome::Missing { .. }) => "Missing",
                        Some(LoadOutcome::Unreadable { .. }) => unreachable!(),
                        None => "None (channel closed early)",
                    }
                ),
            }

            // The reader must continue past the panic to the next
            // request — one bad file shouldn't take down the whole batch.
            match reader.recv() {
                Some(LoadOutcome::Loaded(LoadedFrame::Luma(_))) => {}
                _ => panic!("expected the 2nd request to load normally after the 1st panicked, got a different result"),
            }
        }, Duration::from_secs(5));
    }
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
