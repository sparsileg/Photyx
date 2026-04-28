// db.ts — Central database access layer.
// All Tauri commands that touch SQLite go through this module.
// No component or store should call invoke() directly for DB operations.

import { invoke } from '@tauri-apps/api/core';

export interface QuickLaunchButton {
    id:       number;
    position: number;
    label:    string;
    script:   string;
}

export const db = {

    // ── Preferences ───────────────────────────────────────────────────────────

    getAllPreferences(): Promise<Record<string, string>> {
        return invoke('get_all_preferences');
    },

    setPreference(key: string, value: string): Promise<void> {
        return invoke('set_preference', { key, value });
    },

    // ── Quick Launch ──────────────────────────────────────────────────────────

    getQuickLaunchButtons(): Promise<QuickLaunchButton[]> {
        return invoke('get_quick_launch_buttons');
    },

    saveQuickLaunchButtons(buttons: { label: string; script: string }[]): Promise<void> {
        return invoke('save_quick_launch_buttons', { buttons });
    },

    // ── Recent Directories ────────────────────────────────────────────────────

    getRecentDirectories(): Promise<string[]> {
        return invoke('get_recent_directories');
    },

    recordDirectoryVisit(path: string): Promise<void> {
        return invoke('record_directory_visit', { path });
    },

    // ── Migration ─────────────────────────────────────────────────────────────

    async migrateLocalStorage(): Promise<void> {
        const prefs = await db.getAllPreferences();
        if (prefs['localStorage_migrated'] === 'true') return;

        const theme = localStorage.getItem('photyx-theme');
        if (theme) await db.setPreference('theme', theme);

        const ql = localStorage.getItem('photyx-quick-launch');
        if (ql) {
            try {
                const entries = JSON.parse(ql);
                const buttons = entries.map((e: { name: string; script: string }) => ({
                    label:  e.name,
                    script: e.script,
                }));
                await db.saveQuickLaunchButtons(buttons);
            } catch { /* ignore malformed data */ }
        }

        await db.setPreference('localStorage_migrated', 'true');
        localStorage.removeItem('photyx-theme');
        localStorage.removeItem('photyx-quick-launch');
    },
};
