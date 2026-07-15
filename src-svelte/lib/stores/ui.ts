// ui.ts — UI state store
// Theme, panel visibility, zoom level, stretch mode

import { writable } from 'svelte/store';
import { db } from '../db';
import { SHADOW_CLIP_DEFAULT, TARGET_BG_DEFAULT } from '../settings/constants';

export type Theme = 'dark' | 'light' | 'matrix';
export type ZoomLevel = 'fit' | '25' | '50' | '100' | '200';
export type PanelId = 'files' | 'keywords' | 'macro-editor' | 'macro-lib' | 'plugins' | null;
export type ActiveChannel = 'rgb';

// ── Viewer-region view registry ───────────────────────────────────────────────
// To add a new view: add one entry here. showView() handles the rest.
export const VIEWS = [
  'analysisGraph',
  'analysisResults',
  'stackingWorkspace',
] as const;

export type ViewName = typeof VIEWS[number];

export interface UIState {
  aboutOpen:           boolean;
  activeChannel:       ActiveChannel;
  activePanel:         PanelId;
  activeView:          ViewName | null;
  analysisParametersOpen: boolean;
  analyzeFramesProfilePickerOpen: boolean;
  annotationToken:     number;
  autostretchImageUrl: string | null;
  blinkCached:         boolean;
  blinkCaching:        boolean;
  blinkImageUrl:       string | null;
  blinkFrameIndex:     number;
  blinkModeActive:     boolean;
  blinkPlaying:        boolean;
  blinkResolution:     '12' | '25';
  blinkTabActive:      boolean;
  consoleExpanded:     boolean;
  displayImageUrl:     string | null;
  dragActive:          boolean;
  frameRefreshToken:   number;
  keywordModalOpen:    boolean;
  logViewerOpen:       boolean;
  macroEditorFile:     { id: number | null; name: string; displayName: string; script: string } | null;
  preferencesOpen:     boolean;
  quickLaunchVisible:  boolean;
  shadowClip:          number;
  stackImageUrl:       string | null;
  stretchMode:         'linear' | 'stretched';
  targetBg:            number;
  theme:               Theme;
  viewerClearToken:    number;
  zoomLevel:           ZoomLevel;
}

const initial: UIState = {
  aboutOpen:           false,
  activeChannel:       'rgb',
  activePanel:         null,
  activeView:          null,
  analysisParametersOpen: false,
  analyzeFramesProfilePickerOpen: false,
  annotationToken:     0,
  autostretchImageUrl: null,
  blinkCached:         false,
  blinkCaching:        false,
  blinkImageUrl:       null,
  blinkFrameIndex:     0,
  blinkModeActive:     false,
  blinkPlaying:        false,
  blinkResolution:     '12',
  blinkTabActive:      false,
  consoleExpanded:     false,
  displayImageUrl:     null,
  dragActive:          false,
  frameRefreshToken:   0,
  keywordModalOpen:    false,
  logViewerOpen:       false,
  macroEditorFile:     null,
  preferencesOpen:     false,
  quickLaunchVisible:  true,
  shadowClip:          SHADOW_CLIP_DEFAULT,
  stackImageUrl:       null,
  stretchMode:         'linear',
  targetBg:            TARGET_BG_DEFAULT,
  theme:               'matrix',   // overwritten by hydrateFromDb()
  viewerClearToken:    0,
  zoomLevel:           'fit',
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
    setBlinkFrameIndex: (index: number) => update(s => ({ ...s, blinkFrameIndex: index })),
    setAutostretchFrame: (url: string | null) => update(s => ({ ...s, autostretchImageUrl: url })),
    setDisplayImage:  (url: string | null) => update(s => ({ ...s, displayImageUrl: url })),
    setStackImage:    (url: string | null) => update(s => ({ ...s, stackImageUrl: url })),
    setBlinkCached:   (v: boolean) => update(s => ({ ...s, blinkCached: v })),
    setBlinkCaching:  (v: boolean) => update(s => ({ ...s, blinkCaching: v })),
    setBlinkTabActive:(v: boolean) => update(s => ({ ...s, blinkTabActive: v })),
    setDragActive:    (v: boolean) => update(s => ({ ...s, dragActive: v })),
    setBlinkResolution:(v: '12' | '25') => update(s => ({ ...s, blinkResolution: v })),
    setBlinkModeActive:(v: boolean) => update(s => ({ ...s, blinkModeActive: v })),
    openKeywordModal:  () => update(s => ({ ...s, keywordModalOpen: true })),
    closeKeywordModal: () => update(s => ({ ...s, keywordModalOpen: false })),
    openLogViewer:     () => update(s => ({ ...s, logViewerOpen: true })),
    closeLogViewer:    () => update(s => ({ ...s, logViewerOpen: false })),
    openAbout: () => update(s => ({ ...s, aboutOpen: true })),
    closeAbout: () => update(s => ({ ...s, aboutOpen: false })),
    openAnalysisParameters: () => update(s => ({ ...s, analysisParametersOpen: true })),
    closeAnalysisParameters: () => update(s => ({ ...s, analysisParametersOpen: false })),
    openAnalyzeFramesProfilePicker: () => update(s => ({ ...s, analyzeFramesProfilePickerOpen: true })),
    closeAnalyzeFramesProfilePicker: () => update(s => ({ ...s, analyzeFramesProfilePickerOpen: false })),
    openPreferences: () => update(s => ({ ...s, preferencesOpen: true })),
    closePreferences: () => update(s => ({ ...s, preferencesOpen: false })),
    openMacroEditor:   (file: { id: number | null; name: string; displayName: string; script: string } | null) => update(s => ({
      ...s,
      macroEditorFile: file,
      activePanel:     'macro-editor',
    })),
    showMacroLibrary:  () => update(s => ({ ...s, activePanel: 'macro-lib' })),
    setBlinkPlaying:   (v: boolean) => update(s => ({ ...s, blinkPlaying: v })),
    refreshAnnotations:() => update(s => ({ ...s, annotationToken:  Math.abs(s.annotationToken) + 1 })),
    clearAnnotations:  () => update(s => ({ ...s, annotationToken: -(Math.abs(s.annotationToken) + 1) })),

    // ── View management ───────────────────────────────────────────────────
    setStretchMode: (stretchMode: 'linear' | 'stretched') => update(s => ({ ...s, stretchMode })),
    setStretchParams: (shadowClip: number, targetBg: number) => update(s => ({ ...s, shadowClip, targetBg })),
    showView: (view: ViewName | null) => update(s => ({ ...s, activeView: view })),
  };
}

export const ui = createUIStore();
