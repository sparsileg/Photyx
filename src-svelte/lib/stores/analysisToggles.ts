// stores/analysisToggles.ts — Pending, uncommitted PXFLAG overrides
// Issue 93: shared between AnalysisResults and AnalysisGraph so a toggle
// made in one view is honored regardless of which view actually commits.
//
// Deliberately NOT synced to the backend on toggle (see Issue 93 design
// discussion) — get_analysis_results reclassifies and overwrites `flag`
// unconditionally on every call, so an immediately-synced toggle would be
// silently wiped by the next Refresh (e.g. the normal "tweak a threshold,
// Refresh to preview" workflow in §6.8). Toggles stay purely local here
// and are only pushed to Rust (via set_frame_flag) immediately before
// commit, matching the documented "local state only until Commit"
// contract in §6.9.

import { writable, get as getStoreValue } from 'svelte/store';

export type PxFlag = 'PASS' | 'REJECT';

function createAnalysisToggles() {
  const store = writable<Record<string, PxFlag>>({});

  return {
    subscribe: store.subscribe,

    /** Flip a frame's pending flag, given its current effective flag
     *  (the toggled value if one already exists, otherwise the frame's
     *  loaded flag — callers pass whichever is currently displayed). */
    toggle(path: string, currentFlag: PxFlag) {
      const next: PxFlag = currentFlag === 'REJECT' ? 'PASS' : 'REJECT';
      store.update(m => ({ ...m, [path]: next }));
    },

    /** The pending override for a path, if any. */
    get(path: string): PxFlag | undefined {
      return getStoreValue(store)[path];
    },

    /** All pending overrides, as [path, flag] pairs — used when syncing
     *  to Rust immediately before commit. */
    entries(): [string, PxFlag][] {
      return Object.entries(getStoreValue(store));
    },

    /** Discards all pending toggles — called on every fresh
     *  get_analysis_results load (Refresh) and after a successful
     *  commit, matching the existing "Refresh discards local toggles"
     *  behavior that was already true of AnalysisResults' own local
     *  state before this store existed. */
    clear() {
      store.set({});
    },
  };
}

export const analysisToggles = createAnalysisToggles();

// ----------------------------------------------------------------------
