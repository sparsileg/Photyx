// stores/thresholdProfiles.ts — Threshold profile store.
// Hydrated at startup from get_threshold_profiles and
// get_active_threshold_profile_id. Writes go through
// save_threshold_profile, delete_threshold_profile, and
// set_active_threshold_profile via Tauri commands.
import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export interface ThresholdProfile {
  id:                      number;
  name:                    string;
  description:             string | null;
  bg_median_reject_sigma:  number;
  snr_reject_sigma:        number;
  fwhm_reject_sigma:       number;
  star_count_reject_sigma: number;
  eccentricity_reject_abs: number;
}

export interface ThresholdProfilesState {
  profiles:        ThresholdProfile[];
  activeProfileId: number | null;
}

function createThresholdProfilesStore() {
  const { subscribe, set, update } = writable<ThresholdProfilesState>({
    profiles:        [],
    activeProfileId: null,
  });

  return {
    subscribe,

    // Called from +page.svelte onMount after Tauri is ready.
    async hydrate(): Promise<void> {
      const [profiles, activeProfileId] = await Promise.all([
        invoke<ThresholdProfile[]>('get_threshold_profiles'),
        invoke<number | null>('get_active_threshold_profile_id'),
      ]);
      set({ profiles, activeProfileId });
    },

    // Save a new or existing profile. Returns the saved profile with its
    // DB-assigned id. Caller is responsible for updating the draft.
    async saveProfile(profile: ThresholdProfile): Promise<ThresholdProfile> {
      const saved = await invoke<ThresholdProfile>('save_threshold_profile', { profile });
      update(s => {
        const exists = s.profiles.some(p => p.id === saved.id);
        const profiles = exists
          ? s.profiles.map(p => p.id === saved.id ? saved : p)
          : [...s.profiles, saved];
        return { ...s, profiles };
      });
      return saved;
    },

    // Delete a profile by id. The backend re-seeds Default if the last
    // profile is deleted and returns the updated list implicitly via
    // a subsequent hydrate — we re-hydrate after delete to stay in sync.
    async deleteProfile(id: number): Promise<void> {
      await invoke<void>('delete_threshold_profile', { id });
      // Re-hydrate to pick up any re-seeded Default profile and
      // updated active_threshold_profile_id from the backend.
      await this.hydrate();
    },

    // Persist the active profile selection (called on OK/Apply).
    async setActiveProfile(id: number): Promise<void> {
      await invoke<void>('set_active_threshold_profile', { id });
      update(s => ({ ...s, activeProfileId: id }));
    },
  };
}

export const thresholdProfiles = createThresholdProfilesStore();
