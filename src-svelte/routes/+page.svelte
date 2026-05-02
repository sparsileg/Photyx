<!-- +page.svelte — Photyx main application shell. Spec §8.1 -->

<script lang="ts">
  import AboutModal from '../lib/components/AboutModal.svelte';
  import AnalysisGraph from '../lib/components/AnalysisGraph.svelte';
  import AnalysisResults from '../lib/components/AnalysisResults.svelte';
  import Console from '../lib/components/Console.svelte';
  import HelpModal from '../lib/components/HelpModal.svelte';
  import IconSidebar from '../lib/components/IconSidebar.svelte';
  import InfoPanel from '../lib/components/InfoPanel.svelte';
  import KeywordModal from '../lib/components/KeywordModal.svelte';
  import LogViewer from '../lib/components/LogViewer.svelte';
  import MacroEditor from '../lib/components/panels/MacroEditor.svelte';
  import MenuBar from '../lib/components/MenuBar.svelte';
  import PreferencesDialog from '../lib/components/PreferencesDialog.svelte';
  import ThresholdProfilesDialog from '../lib/components/ThresholdProfilesDialog.svelte';
  import QuickLaunch from '../lib/components/QuickLaunch.svelte';
  import StatusBar from '../lib/components/StatusBar.svelte';
  import Toolbar from '../lib/components/Toolbar.svelte';
  import Viewer from '../lib/components/Viewer.svelte';
  import type { HelpEntry } from '../lib/pcodeHelp';
  import { VIEWS } from '../lib/stores/ui.ts';
  import { db } from '../lib/db';
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { quickLaunch } from '../lib/stores/quickLaunch';
  import { session } from '../lib/stores/session';
  import { settings } from '../lib/stores/settings';
  import { thresholdProfiles } from '../lib/stores/thresholdProfiles';
  import { ui } from '../lib/stores/ui';

  // Load theme stylesheet dynamically
  let themeLink: HTMLLinkElement | null = null;

  $effect(() => {
    const theme = $ui.theme;
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

  // Mouse pixel tracking — prop callback, never touches the store
  let mousePixel = $state<{ x: number; y: number } | null>(null);
  function onMousePixel(px: { x: number; y: number } | null) {
    mousePixel = px;
  }

  // DB hydration and crash recovery — runs once on startup
  onMount(async () => {
    let prefs: Record<string, string> = {};
    try {
      await db.migrateLocalStorage();
      prefs = await db.getAllPreferences();
      ui.hydrateFromDb(prefs);
      settings.hydrate(prefs);
      const buttons = await db.getQuickLaunchButtons();
      quickLaunch.hydrate(buttons);
      await thresholdProfiles.hydrate();
    } catch (e) {
      console.error('DB hydration failed:', e);
    }

    // Restore last directory — set both frontend store and Rust ctx.active_directory
    try {
      const lastDir = prefs['last_directory'];
      if (lastDir) {
        session.setDirectory(lastDir);
        await invoke('dispatch_command', {
          request: { command: 'SelectDirectory', args: { path: lastDir } }
        });
      }
    } catch (e) {
      console.error('Failed to restore last directory:', e);
    }

    // Check for crash recovery candidate
    try {
      const recovery = await db.checkCrashRecovery();
      if (recovery?.active_directory) {
        showRecoveryOffer = true;
        pendingRecovery = recovery;
      }
    } catch (e) {
      console.error('Crash recovery check failed:', e);
    }

    // (close_session is called via File > Exit — see MenuBar.svelte)
  });

  // Crash recovery state
  let showRecoveryOffer = $state(false);
  let pendingRecovery = $state<{
    active_directory: string | null;
    file_list: string | null;
    current_frame_index: number | null;
    written_at: number;
  } | null>(null);

  async function acceptRecovery() {
    if (!pendingRecovery?.active_directory) return;
    showRecoveryOffer = false;
    const dir = pendingRecovery.active_directory;
    try {
      await invoke('run_script', {
        script: `SelectDirectory path="${dir}"\nReadAll`
      });
      session.setDirectory(dir);
      await db.setPreference('last_directory', dir);
      await db.openSession(dir, 0);
      await syncSessionState();
    } catch (e) {
      console.error('Session recovery failed:', e);
    }
    pendingRecovery = null;
  }

  function dismissRecovery() {
    showRecoveryOffer = false;
    pendingRecovery = null;
    // Mark the open session as closed so it doesn't trigger again
    db.closeSession().catch(() => {});
  }

  async function syncSessionState() {
    const state = await invoke<{
      activeDirectory: string | null;
      fileList: string[];
      currentFrame: number;
    }>('get_session');
    if (state.activeDirectory) session.setDirectory(state.activeDirectory);
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

{#if showRecoveryOffer && pendingRecovery}
  <div class="recovery-bar">
    <span>A previous session was not closed cleanly. Restore <strong>{pendingRecovery.active_directory}</strong>?</span>
    <button onclick={acceptRecovery}>Restore</button>
    <button onclick={dismissRecovery}>Dismiss</button>
  </div>
{/if}

<div id="app">
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
    <div id="bottom-panel" class:console-expanded={$ui.consoleExpanded}>
      <Console onhelp={(entry) => helpEntry = entry} />
        <InfoPanel onBlinkFrame={onBlinkFrame} mousePixel={mousePixel} />
    </div>
  </div>
</div>
  </div>

  <StatusBar />
</div>
