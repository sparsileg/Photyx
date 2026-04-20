// ui.ts — UI state store
// Theme, panel visibility, zoom level, stretch mode

import { writable } from 'svelte/store';

export type Theme = 'dark' | 'light' | 'matrix';
export type ZoomLevel = 'fit' | '25' | '50' | '100' | '200';
export type StretchMode = 'auto' | 'linear' | 'histeq';
export type PanelId = 'files' | 'keywords' | 'macro-editor' | 'macro-lib' | 'plugins' | null;

export interface UIState {
    theme: Theme;
    activePanel: PanelId;
    zoomLevel: ZoomLevel;
    stretchMode: StretchMode;
    quickLaunchVisible: boolean;
    activeChannel: 'rgb' | 'r' | 'g' | 'b';
}

function createUIStore() {
    const saved = typeof localStorage !== 'undefined'
        ? localStorage.getItem('photyx-theme') as Theme | null
        : null;

    const initial: UIState = {
        theme: saved ?? 'matrix',
        activePanel: null,
        zoomLevel: 'fit',
        stretchMode: 'auto',
        quickLaunchVisible: true,
        activeChannel: 'rgb',
    };

    const { subscribe, set, update } = writable<UIState>(initial);

    return {
        subscribe,
        set,
        update,
        setTheme: (theme: Theme) => {
            if (typeof localStorage !== 'undefined') {
                localStorage.setItem('photyx-theme', theme);
            }
            update(s => ({ ...s, theme }));
        },
        togglePanel: (panel: PanelId) => update(s => ({
            ...s,
            activePanel: s.activePanel === panel ? null : panel,
        })),
        closePanel: () => update(s => ({ ...s, activePanel: null })),
        setZoom: (zoomLevel: ZoomLevel) => update(s => ({ ...s, zoomLevel })),
        setStretch: (stretchMode: StretchMode) => update(s => ({ ...s, stretchMode })),
        toggleQuickLaunch: () => update(s => ({ ...s, quickLaunchVisible: !s.quickLaunchVisible })),
        setChannel: (ch: 'rgb' | 'r' | 'g' | 'b') => update(s => ({ ...s, activeChannel: ch })),
    };
}

export const ui = createUIStore();
