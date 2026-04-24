// quickLaunch.ts — Quick Launch panel store
// Holds the ordered list of pinned macro buttons.
// Each entry is either a saved .phs path or an inline script snippet.
// Persisted to localStorage (Phase 9 will migrate to tauri-plugin-store).

import { writable } from 'svelte/store';

export interface QuickLaunchEntry {
    id: string;
    name: string;
    script: string;
    icon?: string;
    protected?: boolean;
}

function uid(): string {
    return Date.now().toString(36) + Math.random().toString(36).slice(2, 7);
}

const STORAGE_KEY = 'photyx-quick-launch';

function loadFromStorage(): QuickLaunchEntry[] {
    if (typeof localStorage === 'undefined') return defaultEntries();
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (raw) return JSON.parse(raw);
    } catch { /* ignore */ }
    return defaultEntries();
}

// Built-in default buttons that match the spec's "full Macro Library" default
function defaultEntries(): QuickLaunchEntry[] {
    return [
        { id: uid(), name: 'List KW',    script: 'ListKeywords',  icon: '🏷', protected: true },
        { id: uid(), name: 'AutoStretch',script: 'AutoStretch',   icon: '✨', protected: true },
        { id: uid(), name: 'FWHM',       script: 'ComputeFWHM',   icon: '⭐', protected: true },
        { id: uid(), name: 'Star Count', script: 'CountStars',    icon: '🔢', protected: true },
    ];
}

function createQuickLaunchStore() {
    const { subscribe, set, update } = writable<QuickLaunchEntry[]>(loadFromStorage());

    function persist(entries: QuickLaunchEntry[]) {
        if (typeof localStorage !== 'undefined') {
            localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
        }
    }

    return {
        subscribe,

        pin(entry: { name: string; script: string; icon?: string }) {
            update(entries => {
                // Avoid duplicate names — replace if same name already pinned
                const idx = entries.findIndex(e => e.name === entry.name);
                const newEntry: QuickLaunchEntry = { id: uid(), ...entry };
                let next: QuickLaunchEntry[];
                if (idx >= 0) {
                    next = entries.map((e, i) => i === idx ? { ...e, ...newEntry, id: e.id } : e);
                } else {
                    next = [...entries, newEntry];
                }
                persist(next);
                return next;
            });
        },

        remove(id: string) {
            update(entries => {
                const next = entries.filter(e => e.id !== id);
                persist(next);
                return next;
            });
        },

        rename(id: string, name: string) {
            update(entries => {
                const next = entries.map(e => e.id === id ? { ...e, name } : e);
                persist(next);
                return next;
            });
        },

        move(id: string, direction: 'left' | 'right') {
            update(entries => {
                const i = entries.findIndex(e => e.id === id);
                if (i < 0) return entries;
                const j = direction === 'left' ? i - 1 : i + 1;
                if (j < 0 || j >= entries.length) return entries;
                const next = [...entries];
                [next[i], next[j]] = [next[j], next[i]];
                persist(next);
                return next;
            });
        },

        reset() {
            const entries = defaultEntries();
            persist(entries);
            set(entries);
        },
    };
}

export const quickLaunch = createQuickLaunchStore();
