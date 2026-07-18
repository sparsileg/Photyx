// featureFlags.ts — Feature flag store (Issue 130).
// Backed by the feature_flags table; defaults come from the FEATURE_FLAGS
// registry in settings/constants.ts. hydrate() merges persisted rows over
// registry defaults, so a flag that exists in the registry but has never
// been toggled still resolves to its declared default rather than
// undefined.

import { writable } from 'svelte/store';
import { db } from '../db';
import { FEATURE_FLAGS } from '../settings/constants';

export interface FeatureFlagsState {
  flags: Record<string, boolean>;
}

function defaultFlags(): Record<string, boolean> {
  const defaults: Record<string, boolean> = {};
  for (const f of FEATURE_FLAGS) defaults[f.key] = f.default;
  return defaults;
}

function createFeatureFlagsStore() {
  const { subscribe, update } = writable<FeatureFlagsState>({ flags: defaultFlags() });

  return {
    subscribe,

    // Called from +page.svelte onMount, alongside thresholdProfiles.hydrate().
    async hydrate() {
      try {
        const persisted = await db.getFeatureFlags();
        update(() => ({ flags: { ...defaultFlags(), ...persisted } }));
      } catch (e) {
        console.error('Failed to hydrate feature flags:', e);
      }
    },

    // Optimistic local update, same fire-and-forget persist pattern as
    // ui.ts's setTheme/toggleQuickLaunch.
    setFlag(key: string, enabled: boolean) {
      update(s => ({ flags: { ...s.flags, [key]: enabled } }));
      db.setFeatureFlag(key, enabled).catch(e =>
        console.error(`Failed to save feature flag '${key}':`, e)
      );
    },
  };
}

export const featureFlags = createFeatureFlagsStore();
