// ui.ts — UI state store
// Theme, panel visibility, zoom level, stretch mode

import { writable } from 'svelte/store';

export type Theme = 'dark' | 'light' | 'matrix';
export type ZoomLevel = 'fit' | '25' | '50' | '100' | '200';
export type PanelId = 'files' | 'keywords' | 'macro-editor' | 'macro-lib' | 'plugins' | null;

export interface UIState {
    theme: Theme;
    activePanel: PanelId;
    zoomLevel: ZoomLevel;
    quickLaunchVisible: boolean;
    activeChannel: 'rgb' | 'r' | 'g' | 'b';
    frameRefreshToken: number;
    viewerClearToken: number;
    consoleExpanded: boolean;
    blinkImageUrl: string | null;
    blinkCached: boolean;
    blinkCaching: boolean;
    blinkTabActive: boolean;
    blinkResolution: '12' | '25';
    blinkModeActive: boolean;
    keywordModalOpen: boolean;
    blinkPlaying: boolean;
    showQualityFlags:  boolean;
    currentBlinkFlag:  string;
    showAnalysisGraph: boolean;
}

function createUIStore() {
    const saved = typeof localStorage !== 'undefined'
        ? localStorage.getItem('photyx-theme') as Theme | null
        : null;

    const initial: UIState = {
        theme: saved ?? 'matrix',
        activePanel: null,
        zoomLevel: 'fit',
        quickLaunchVisible: true,
        activeChannel: 'rgb',
        frameRefreshToken: 0,
        viewerClearToken: 0,
        consoleExpanded: false,
        blinkImageUrl: null,
        blinkCached: false,
        blinkCaching: false,
        blinkTabActive: false,
        blinkResolution: '12',
        blinkModeActive: false,
        keywordModalOpen: false,
        blinkPlaying: false,
        showQualityFlags:  true,
        currentBlinkFlag:  '',
        showAnalysisGraph: false,
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
        toggleQuickLaunch: () => update(s => ({ ...s, quickLaunchVisible: !s.quickLaunchVisible })),
        setChannel: (ch: 'rgb' | 'r' | 'g' | 'b') => update(s => ({ ...s, activeChannel: ch })),
        requestFrameRefresh: () => update(s => ({ ...s, frameRefreshToken: s.frameRefreshToken + 1 })),
        clearViewer: () => update(s => ({ ...s, viewerClearToken: s.viewerClearToken + 1 })),
        toggleConsole: () => update(s => ({ ...s, consoleExpanded: !s.consoleExpanded })),
        setBlinkFrame: (url: string | null) => update(s => ({ ...s, blinkImageUrl: url })),
        setBlinkCached: (v: boolean) => update(s => ({ ...s, blinkCached: v })),
        setBlinkCaching: (v: boolean) => update(s => ({ ...s, blinkCaching: v })),
        setBlinkTabActive: (v: boolean) => update(s => ({ ...s, blinkTabActive: v })),
        setBlinkResolution: (v: '12' | '25') => update(s => ({ ...s, blinkResolution: v })),
        setBlinkModeActive: (v: boolean) => update(s => ({ ...s, blinkModeActive: v })),
        openKeywordModal: () => update(s => ({ ...s, keywordModalOpen: true })),
        closeKeywordModal: () => update(s => ({ ...s, keywordModalOpen: false })),
        setBlinkPlaying: (v: boolean) => update(s => ({ ...s, blinkPlaying: v })),
        setShowQualityFlags: (v: boolean) => update(s => ({ ...s, showQualityFlags: v })),
        setCurrentBlinkFlag:  (v: string)  => update(s => ({ ...s, currentBlinkFlag: v })),
        setShowAnalysisGraph: (v: boolean) => update(s => ({ ...s, showAnalysisGraph: v })),
    };

}

export const ui = createUIStore();
