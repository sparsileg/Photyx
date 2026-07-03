// stores/thresholdProfiles.ts — Threshold profile store.
// Hydrated at startup from db.getThresholdProfiles() and
// db.getActiveThresholdProfileId(). Writes go through
// db.saveThresholdProfile(), db.deleteThresholdProfile(), and
// db.setActiveThresholdProfile().
import { writable } from 'svelte/store';
import { db, type ThresholdProfile } from '../db';

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
        db.getThresholdProfiles(),
        db.getActiveThresholdProfileId(),
      ]);
      set({ profiles, activeProfileId });
    },

    // Save a new or existing profile. Returns the saved profile with its
    // DB-assigned id. Caller is responsible for updating the draft.
    async saveProfile(profile: ThresholdProfile): Promise<ThresholdProfile> {
      const saved = await db.saveThresholdProfile(profile);
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
      await db.deleteThresholdProfile(id);
      // Re-hydrate to pick up any re-seeded Default profile and
      // updated active_threshold_profile_id from the backend.
      await this.hydrate();
    },

    // Persist the active profile selection (called on OK/Apply).
    async setActiveProfile(id: number): Promise<void> {
      await db.setActiveThresholdProfile(id);
      update(s => ({ ...s, activeProfileId: id }));
    },
  };
}

export const thresholdProfiles = createThresholdProfilesStore();
