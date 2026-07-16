<!-- +page.svelte   Photyx main application shell. Spec §8.1 -->

<script lang="ts">
  import AboutModal from '../lib/components/AboutModal.svelte';
  import AnalysisGraph from '../lib/components/AnalysisGraph.svelte';
  import AnalysisResults from '../lib/components/AnalysisResults.svelte';
  import AnalyzeFramesProfileDialog from '../lib/components/AnalyzeFramesProfileDialog.svelte';
  import Console from '../lib/components/Console.svelte';
  import HelpModal from '../lib/components/HelpModal.svelte';
  import IconSidebar from '../lib/components/IconSidebar.svelte';
  import InfoPanel from '../lib/components/InfoPanel.svelte';
  import KeywordModal from '../lib/components/KeywordModal.svelte';
  import LogViewer from '../lib/components/LogViewer.svelte';
  import MacroEditor from '../lib/components/panels/MacroEditor.svelte';
  import MenuBar from '../lib/components/MenuBar.svelte';
  import PreferencesDialog from '../lib/components/PreferencesDialog.svelte';
  import QuickLaunch from '../lib/components/QuickLaunch.svelte';
  import StackingWorkspace from '../lib/components/StackingWorkspace.svelte';
  import StatusBar from '../lib/components/StatusBar.svelte';
  import ThresholdProfilesDialog from '../lib/components/ThresholdProfilesDialog.svelte';
  import Toolbar from '../lib/components/Toolbar.svelte';
  import Viewer from '../lib/components/Viewer.svelte';
  import type { HelpEntry } from '../lib/pcode';
  import { VIEWS } from '../lib/stores/ui';
  import { db } from '../lib/db';
  import '../lib/types/svelte-elements';
  import { invoke } from '@tauri-apps/api/core';
  import { DEFAULT_FONT_SIZE } from '../lib/settings/constants';
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { handleDroppedPaths } from '../lib/commands';
  import { quickLaunch } from '../lib/stores/quickLaunch';
  import { session } from '../lib/stores/session';
  import { settings } from '../lib/stores/settings';
  import { thresholdProfiles } from '../lib/stores/thresholdProfiles';
  import { ui } from '../lib/stores/ui';

  // Load theme stylesheet dynamically
  let themeLink: HTMLLinkElement | null = null;
  let lastTheme: string | null = null;

  $effect(() => {
    const theme = $ui.theme;
    if (theme === lastTheme) return;
    lastTheme = theme;
    if (themeLink) themeLink.remove();
    themeLink = document.createElement('link');
    themeLink.rel = 'stylesheet';
    themeLink.href = `/themes/${theme}.css`;
    document.head.appendChild(themeLink);
  });

  // Help modal
  let helpEntry = $state<HelpEntry | null>(null);

  // Blink filename overlay
  let blinkFilename = $state('');
  function onBlinkFrame(filename: string) {
    blinkFilename = filename;
  }

  // Mouse pixel tracking   prop callback, never touches the store
  let mousePixel = $state<{ x: number; y: number } | null>(null);
  function onMousePixel(px: { x: number; y: number } | null) {
    mousePixel = px;
  }

  // DB hydration   runs once on startup
  onMount(async () => {
    let prefs: Record<string, string> = {};
    try {
      await db.migrateLocalStorage();
      prefs = await db.getAllPreferences();
      ui.hydrateFromDb(prefs);
      settings.hydrate(prefs);
      const fontSize = parseFloat(prefs['ui_font_size'] ?? String(DEFAULT_FONT_SIZE)) || DEFAULT_FONT_SIZE;
      document.documentElement.style.fontSize = `${fontSize}px`;
      const buttons = await db.getQuickLaunchButtons();
      quickLaunch.hydrate(buttons);
      await thresholdProfiles.hydrate();
    } catch (e) {
      console.error('DB hydration failed:', e);
    }

    // (close_session is called via File > Exit   see MenuBar.svelte)
  });

  // Native OS file drag-and-drop   routes through AddFiles regardless of
  // count or which view is currently active, same as Session > Add Files.
  onMount(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type === 'over') {
        ui.setDragActive(true);
      } else if (event.payload.type === 'drop') {
        ui.setDragActive(false);
        handleDroppedPaths(event.payload.paths);
      } else {
        ui.setDragActive(false);
      }
    }).then(fn => { unlisten = fn; });

    return () => { unlisten?.(); };
  });

  async function syncSessionState() {
    const state = await invoke<{
      fileList: string[];
      currentFrame: number;
    }>('get_session');
    session.setFileList(state.fileList);
    session.setCurrentFrame(state.currentFrame);
  }

  // Keyboard shortcuts per spec §8.13
  function onKeyDown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
    switch (e.key) {
    case '0': ui.setZoom('fit');  break;
    case '1': ui.setZoom('25');   break;
    case '2': ui.setZoom('50');   break;
    case '3': ui.setZoom('100');  break;
    case '4': ui.setZoom('200');  break;
    }
  }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if $ui.analysisParametersOpen}
  <ThresholdProfilesDialog onclose={() => ui.closeAnalysisParameters()} />
{/if}
{#if $ui.analyzeFramesProfilePickerOpen}
  <AnalyzeFramesProfileDialog onclose={() => ui.closeAnalyzeFramesProfilePicker()} />
{/if}
{#if $ui.preferencesOpen}
  <PreferencesDialog onclose={() => ui.closePreferences()} />
{/if}
{#if $ui.keywordModalOpen}
  <KeywordModal onclose={() => ui.closeKeywordModal()} />
{/if}
{#if $ui.logViewerOpen}
  <LogViewer onclose={() => ui.closeLogViewer()} />
{/if}
{#if $ui.aboutOpen}
  <AboutModal onclose={() => ui.closeAbout()} />
{/if}

{#if helpEntry}
  <HelpModal entry={helpEntry} onclose={() => helpEntry = null} />
{/if}

<div id="app">
  {#if $ui.dragActive}
    <div id="drag-drop-overlay">Drop files to add to session</div>
  {/if}
  <MenuBar />
  <Toolbar />
  <div id="content-area">
    <IconSidebar />
    {#if $ui.activePanel === 'macro-editor'}
      <MacroEditor />
    {/if}

<div id="right-column">
  <QuickLaunch />
  <div id="viewer-region">
    {#if $ui.activeView === 'analysisGraph'}
      <AnalysisGraph />
    {:else if $ui.activeView === 'analysisResults'}
      <AnalysisResults />
    {:else if $ui.activeView === 'stackingWorkspace'}
      <StackingWorkspace />
    {:else}
      <Viewer onMousePixel={onMousePixel} />
    {/if}
    {#if !$ui.consoleExpanded}
      {#if $ui.blinkTabActive && blinkFilename}
        <div id="blink-filename-overlay">{blinkFilename}</div>
      {:else if !$ui.blinkTabActive && $ui.activeView === null && $session.fileList.length > 0 && $session.fileList[$session.currentFrame]}
        <div id="blink-filename-overlay">{$session.fileList[$session.currentFrame]?.split(/[\\/]/).pop() ?? ''}</div>
      {/if}
    {/if}
    <div id="bottom-panel" class:console-expanded={$ui.consoleExpanded} class:hidden={$ui.activeView === 'stackingWorkspace'}>
      <Console onhelp={(entry) => helpEntry = entry} />
        <InfoPanel onBlinkFrame={onBlinkFrame} mousePixel={mousePixel} />
    </div>
  </div>
</div>
  </div>

  <StatusBar />
</div>
