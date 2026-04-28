// quickLaunch.ts — Quick Launch panel store
// Holds the ordered list of pinned macro buttons.
// Persisted to SQLite via db.ts (migrated from localStorage in Phase 9).

import { writable } from 'svelte/store';
import { db } from '../db';

export interface QuickLaunchEntry {
    id:         string;
    name:       string;
    script:     string;
    icon?:      string;
    protected?: boolean;
}

function uid(): string {
    return Date.now().toString(36) + Math.random().toString(36).slice(2, 7);
}

// Built-in default buttons — used only when DB returns empty (new install)
function defaultEntries(): QuickLaunchEntry[] {
    return [
        { id: uid(), name: 'List KW',    script: 'ListKeywords', icon: '🏷', protected: true },
        { id: uid(), name: 'FWHM',       script: 'ComputeFWHM',  icon: '⭐', protected: true },
        { id: uid(), name: 'Star Count', script: 'CountStars',   icon: '🔢', protected: true },
    ];
}

function toDbFormat(entries: QuickLaunchEntry[]) {
    return entries.map(e => ({ label: e.name, script: e.script }));
}

function createQuickLaunchStore() {
    const { subscribe, set, update } = writable<QuickLaunchEntry[]>(defaultEntries());

    async function persist(entries: QuickLaunchEntry[]) {
        try {
            await db.saveQuickLaunchButtons(toDbFormat(entries));
        } catch (e) {
            console.error('Failed to save Quick Launch buttons:', e);
        }
    }

    return {
        subscribe,

        // Called from +page.svelte onMount after DB hydration
        hydrate(buttons: { id: number; label: string; script: string }[]) {
            if (buttons.length === 0) return; // keep defaults for new install
            const entries: QuickLaunchEntry[] = buttons.map(b => ({
                id:     b.id.toString(),
                name:   b.label,
                script: b.script,
            }));
            set(entries);
        },

        pin(entry: { name: string; script: string; icon?: string }) {
            update(entries => {
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
