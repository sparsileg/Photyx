// stores/settings.ts — Frontend settings store.
// Hydrated at startup from get_all_preferences (via db.ts).
// The Preferences dialog reads from this store, edits a draft copy,
// and calls savePreferences() on OK/Apply to write back.

import { writable } from 'svelte/store';
import { db } from '../db';
import {
  DEFAULT_FONT_SIZE,
  JPEG_QUALITY_DEFAULT,
  CONSOLE_HISTORY_DEFAULT,
  MACRO_FONT_DEFAULT,
  BUFFER_POOL_DEFAULT_GB,
  SHADOW_CLIP_DEFAULT,
  TARGET_BG_DEFAULT,
  GB,
} from '../settings/constants';

export interface AppPreferences {
  jpeg_quality:              number;
  backup_directory:          string;
  console_history_size:      number;
  macro_editor_font_size:    number;
  buffer_pool_memory_limit:  number;  // stored as bytes, converted to/from GB in UI
  autostretch_shadow_clip:   number;
  autostretch_target_bg:     number;
  ui_font_size:              number;  // root font size in px
  rayon_thread_count:        number;  // -1 = num_cpus - 1 at runtime
}

const defaults: AppPreferences = {
  jpeg_quality:             JPEG_QUALITY_DEFAULT,
  backup_directory:         '',
  console_history_size:     CONSOLE_HISTORY_DEFAULT,
  macro_editor_font_size:   MACRO_FONT_DEFAULT,
  buffer_pool_memory_limit: BUFFER_POOL_DEFAULT_GB * GB,
  autostretch_shadow_clip:  SHADOW_CLIP_DEFAULT,
  autostretch_target_bg:    TARGET_BG_DEFAULT,
  ui_font_size:             DEFAULT_FONT_SIZE,
  rayon_thread_count:       -1,
};

function createSettingsStore() {
  const { subscribe, set, update } = writable<AppPreferences>({ ...defaults });

  return {
    subscribe,

    // Called from +page.svelte onMount after get_all_preferences returns.
    // Applies DB values over defaults; missing keys stay at defaults.
    hydrate(prefs: Record<string, string>) {
      update(s => {
        const n = { ...s };
        if (prefs['jpeg_quality'])
          n.jpeg_quality = parseInt(prefs['jpeg_quality'], 10) || s.jpeg_quality;
        if (prefs['backup_directory'] !== undefined)
          n.backup_directory = prefs['backup_directory'];
        if (prefs['console_history_size'])
          n.console_history_size = parseInt(prefs['console_history_size'], 10) || s.console_history_size;
        if (prefs['macro_editor_font_size'])
          n.macro_editor_font_size = parseInt(prefs['macro_editor_font_size'], 10) || s.macro_editor_font_size;
        if (prefs['buffer_pool_memory_limit'])
          n.buffer_pool_memory_limit = parseInt(prefs['buffer_pool_memory_limit'], 10) || s.buffer_pool_memory_limit;
        if (prefs['autostretch_shadow_clip'])
          n.autostretch_shadow_clip = parseFloat(prefs['autostretch_shadow_clip']) || s.autostretch_shadow_clip;
        if (prefs['autostretch_target_bg'])
          n.autostretch_target_bg = parseFloat(prefs['autostretch_target_bg']) || s.autostretch_target_bg;
        if (prefs['ui_font_size'])
          n.ui_font_size = parseFloat(prefs['ui_font_size']) || s.ui_font_size;
        if (prefs['rayon_thread_count']) {
          const parsed = parseInt(prefs['rayon_thread_count'], 10);
          // parseInt returns NaN (not null/undefined) on a corrupt/non-numeric
          // pref row, so `?? fallback` never triggered — NaN could enter the
          // store silently. Issue 121.
          n.rayon_thread_count = Number.isNaN(parsed) ? s.rayon_thread_count : parsed;
        }
        return n;
      });
    },

    // Write a batch of changed preferences to the DB and update the store.
    // Called by the Preferences dialog on OK or Apply.
    async savePreferences(changed: Partial<AppPreferences>): Promise<void> {
      const calls: Promise<void>[] = [];
      for (const [key, value] of Object.entries(changed)) {
        calls.push(db.setPreference(key, String(value)));
      }
      await Promise.all(calls);
      update(s => ({ ...s, ...changed }));
    },
  };
}

export const settings = createSettingsStore();
