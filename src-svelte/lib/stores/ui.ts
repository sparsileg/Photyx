// ui.ts — UI state store
// Theme, panel visibility, zoom level, stretch mode

import { writable } from 'svelte/store';
import { db } from '../db';

export type Theme = 'dark' | 'light' | 'matrix';
export type ZoomLevel = 'fit' | '25' | '50' | '100' | '200';
export type PanelId = 'files' | 'keywords' | 'macro-editor' | 'macro-lib' | 'plugins' | null;

// ── Viewer-region view registry ───────────────────────────────────────────────
// To add a new view: add one entry here. showView() handles the rest.
export const VIEWS = [
  'analysisGraph',
  'analysisResults',
] as const;

export type ViewName = typeof VIEWS[number];

export interface UIState {
  theme:              Theme;
  activePanel:        PanelId;
  activeView:         ViewName | null;
  zoomLevel:          ZoomLevel;
  quickLaunchVisible: boolean;
  activeChannel:      'rgb' | 'r' | 'g' | 'b';
  frameRefreshToken:  number;
  viewerClearToken:   number;
  consoleExpanded:    boolean;
  blinkImageUrl:      string | null;
  autostretchImageUrl: string | null;
  displayImageUrl:    string | null;
  blinkCached:        boolean;
  blinkCaching:       boolean;
  blinkTabActive:     boolean;
  blinkResolution:    '12' | '25';
  blinkModeActive:    boolean;
  keywordModalOpen:   boolean;
  logViewerOpen:      boolean;
  aboutOpen:          boolean;
  macroEditorFile:    { id: number | null; name: string; displayName: string; script: string } | null;
  blinkPlaying:       boolean;
  showQualityFlags:   boolean;
  currentBlinkFlag:   string;
  annotationToken:    number;
}

const initial: UIState = {
  theme:              'matrix',   // overwritten by hydrateFromDb()
  activePanel:        null,
  activeView:         null,
  zoomLevel:          'fit',
  quickLaunchVisible: true,
  activeChannel:      'rgb',
  frameRefreshToken:  0,
  viewerClearToken:   0,
  consoleExpanded:    false,
  blinkImageUrl:      null,
  autostretchImageUrl: null,
  displayImageUrl:    null,
  blinkCached:        false,
  blinkCaching:       false,
  blinkTabActive:     false,
  blinkResolution:    '12',
  blinkModeActive:    false,
  keywordModalOpen:   false,
  logViewerOpen:      false,
  aboutOpen:          false,
  macroEditorFile:    null,
  blinkPlaying:       false,
  showQualityFlags:   true,
  currentBlinkFlag:   '',
  annotationToken:    0,
};

function createUIStore() {
  const { subscribe, set, update } = writable<UIState>(initial);

  return {
    subscribe,
    set,
    update,

    // Called from +page.svelte onMount after DB hydration.
    // Applies any preference-backed state from the preferences table.
    hydrateFromDb(prefs: Record<string, string>) {
      const validThemes: Theme[] = ['dark', 'light', 'matrix'];
      const savedTheme = prefs['theme'] as Theme;
      const theme = validThemes.includes(savedTheme) ? savedTheme : 'matrix';
      update(s => ({
        ...s,
        theme,
        quickLaunchVisible: prefs['quick_launch_visible'] !== 'false',
      }));
    },

    setTheme(theme: Theme) {
      db.setPreference('theme', theme).catch(e =>
        console.error('Failed to save theme:', e)
                                            );
      update(s => ({ ...s, theme }));
    },

    togglePanel: (panel: PanelId) => update(s => ({
      ...s,
      activePanel: s.activePanel === panel ? null : panel,
    })),
    closePanel: () => update(s => ({ ...s, activePanel: null })),
    setZoom: (zoomLevel: ZoomLevel) => update(s => ({ ...s, zoomLevel })),
    toggleQuickLaunch: () => update(s => {
      const next = !s.quickLaunchVisible;
      db.setPreference('quick_launch_visible', String(next)).catch(e =>
        console.error('Failed to save quick_launch_visible:', e)
                                                                  );
      return { ...s, quickLaunchVisible: next };
    }),
    setChannel: (ch: 'rgb' | 'r' | 'g' | 'b') => update(s => ({ ...s, activeChannel: ch })),
    requestFrameRefresh: () => update(s => ({
      ...s,
      frameRefreshToken:   s.frameRefreshToken + 1,
      autostretchImageUrl: null,
      displayImageUrl:     null,
    })),
    requestViewerClear: () => update(s => ({ ...s, viewerClearToken: s.viewerClearToken + 1 })),
    clearViewer: () => update(s => ({
      ...s,
      viewerClearToken:    s.viewerClearToken + 1,
      autostretchImageUrl: null,
    })),
    toggleConsole:    () => update(s => ({ ...s, consoleExpanded: !s.consoleExpanded })),
    setBlinkFrame:    (url: string | null) => update(s => ({ ...s, blinkImageUrl: url })),
    setAutostretchFrame: (url: string | null) => update(s => ({ ...s, autostretchImageUrl: url })),
    setDisplayImage:  (url: string | null) => update(s => ({ ...s, displayImageUrl: url })),
    setBlinkCached:   (v: boolean) => update(s => ({ ...s, blinkCached: v })),
    setBlinkCaching:  (v: boolean) => update(s => ({ ...s, blinkCaching: v })),
    setBlinkTabActive:(v: boolean) => update(s => ({ ...s, blinkTabActive: v })),
    setBlinkResolution:(v: '12' | '25') => update(s => ({ ...s, blinkResolution: v })),
    setBlinkModeActive:(v: boolean) => update(s => ({ ...s, blinkModeActive: v })),
    openKeywordModal:  () => update(s => ({ ...s, keywordModalOpen: true })),
    closeKeywordModal: () => update(s => ({ ...s, keywordModalOpen: false })),
    openLogViewer:     () => update(s => ({ ...s, logViewerOpen: true })),
    closeLogViewer:    () => update(s => ({ ...s, logViewerOpen: false })),
    openAbout:         () => update(s => ({ ...s, aboutOpen: true })),
    closeAbout:        () => update(s => ({ ...s, aboutOpen: false })),
    openMacroEditor:   (file: { id: number | null; name: string; displayName: string; script: string } | null) => update(s => ({
      ...s,
      macroEditorFile: file,
      activePanel:     'macro-editor',
    })),
    showMacroLibrary:  () => update(s => ({ ...s, activePanel: 'macro-lib' })),
    setBlinkPlaying:   (v: boolean) => update(s => ({ ...s, blinkPlaying: v })),
    setShowQualityFlags:(v: boolean) => update(s => ({ ...s, showQualityFlags: v })),
    setCurrentBlinkFlag:(v: string) => update(s => ({ ...s, currentBlinkFlag: v })),
    refreshAnnotations:() => update(s => ({ ...s, annotationToken:  Math.abs(s.annotationToken) + 1 })),
    clearAnnotations:  () => update(s => ({ ...s, annotationToken: -(Math.abs(s.annotationToken) + 1) })),

    // ── View management ───────────────────────────────────────────────────
    showView: (view: ViewName | null) => update(s => ({ ...s, activeView: view })),
  };
}

export const ui = createUIStore();
