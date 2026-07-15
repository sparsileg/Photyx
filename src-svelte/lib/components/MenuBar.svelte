<!-- MenuBar.svelte — Application menu bar. Spec §8.2 -->

<script lang="ts">
  import { pipeToConsole } from '../stores/consoleHistory';
  import { db } from '../db';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { invoke } from '@tauri-apps/api/core';
  import { notifications } from '../stores/notifications';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { readTextFile } from '@tauri-apps/plugin-fs';
  import { quickLaunch } from '../stores/quickLaunch';
  import { addFiles, closeSession, applyAutoStretch, loadFile } from '../commands';
  import { settings } from '../stores/settings';
  import { thresholdProfiles } from '../stores/thresholdProfiles';
  import { ui } from '../stores/ui';
  import { DEFAULT_FONT_SIZE } from '../settings/constants';

  interface MenuItem {
    sep?:      boolean;
    label?:    string;
    action?:   string;
    shortcut?: string;
  }

  interface MenuDef {
    name:  string;
    items: MenuItem[];
  }

  let openMenu = $state<string | null>(null);

  function toggle(name: string) {
    openMenu = openMenu === name ? null : name;
  }

  function close() {
    openMenu = null;
  }

  function action(a: string) {
    close();
    switch (a) {
    case 'about':               ui.openAbout(); break;
    case 'add-files':           addFiles(); break;
    case 'analysis-graph':      ui.showView('analysisGraph'); break;
    case 'analysis-parameters': ui.openAnalysisParameters(); break;
    case 'analysis-results':    ui.showView('analysisResults'); break;
    case 'analyze-frames':      ui.openAnalyzeFramesProfilePicker(); break;
    case 'backup-database':     backupDatabase(); break;
    case 'close-session':       closeSession(); break;
    case 'contour-plot':        runContourHeatmap(); break;
    case 'exit':                getCurrentWindow().close(); break;
    case 'export-analysis-results': exportSessionJson(); break;
    case 'import-analysis-results': importSessionJson(); break;
    case 'keywords':            ui.togglePanel('keywords'); break;
    case 'load-single-image':   loadSingleImage(); break;
    case 'log-viewer':          ui.openLogViewer(); break;
    case 'macro-library':       ui.togglePanel('macro-lib'); break;
    case 'plugin-manager':      ui.togglePanel('plugins'); break;
    case 'preferences':         ui.openPreferences(); break;
    case 'restore-database':    restoreDatabase(); break;
    case 'run-macro':           ui.togglePanel('macro-editor'); break;
    case 'save-as-fits':        saveAsFits(); break;
    case 'stacking-workspace':  ui.showView('stackingWorkspace'); break;
    case 'theme-dark':          ui.setTheme('dark'); break;
    case 'theme-light':         ui.setTheme('light'); break;
    case 'theme-matrix':        ui.setTheme('matrix'); break;
    default: notifications.info(`${a} — not yet implemented`);
    }
  }

  // AnalyzeFrames dispatch (Issue 101) now lives in commands.ts as
  // runAnalyzeFramesWithProfile(), called from AnalyzeFramesProfileDialog
  // after the user picks a profile — see openAnalyzeFramesProfilePicker
  // above. Kept out of this file since the dialog itself triggers it.

  async function runAutoStretch() {
    try {
      const response = await invoke<{
        success: boolean;
        output: string | null;
        error: string | null;
      }>('dispatch_command', {
        request: { command: 'AutoStretch', args: {} }
      });
      if (response.success) {
        const msg = response.output ?? 'AutoStretch applied';
        pipeToConsole(msg, 'success');
        notifications.success(msg);
        await applyAutoStretch();
      } else {
        const err = response.error ?? 'AutoStretch failed';
        pipeToConsole(err, 'error');
        notifications.error(err);
      }
    } catch (err) {
      const msg = `AutoStretch error: ${err}`;
      pipeToConsole(msg, 'error');
      notifications.error(msg);
    }
  }

  async function loadSingleImage() {
    let selected;
    try {
      selected = await open({
        multiple: false,
        filters: [{ name: 'Image Files', extensions: ['xisf', 'fit', 'fits', 'tiff', 'tif'] }]
      });
    } catch (e) {
      notifications.error(`Failed to open file picker: ${e}`);
      return;
    }
    if (!selected) return;
    const path = typeof selected === 'string' ? selected : selected[0];
    if (!path) return;
    await loadFile(path);
    // load_file always returns raw/linear pixel data, so the Linear/Stretched
    // toggle must be reset here rather than carrying over stale state from
    // whatever was previously displayed.
    ui.setStretchMode('linear');
    ui.setAutostretchFrame(null);
  }

  async function saveAsFits() {
    let destPath: string | null;
    try {
      destPath = await save({
        title:   'Save as FITS',
        filters: [{ name: 'FITS Image', extensions: ['fit'] }],
      });
    } catch (e) {
      notifications.error(`Save cancelled: ${e}`);
      return;
    }
    if (!destPath) return;

    // Determine whether there is a stack result or a current session frame
    const isStackingWorkspace = document.getElementById('sw-root') !== null;
    const stackArg = isStackingWorkspace ? ' stack=true' : '';

    notifications.running('Saving FITS…');
    try {
      const response = await invoke<{
        results: Array<{ success: boolean; message: string | null }>;
      }>('run_script', {
        script: `WriteFIT destination="${destPath.replace(/\\/g, '/')}" overwrite=true${stackArg}`
      });
      const last = response.results[response.results.length - 1];
      if (last?.success) {
        notifications.success(`Saved: ${destPath}`);
        pipeToConsole(`Saved FITS: ${destPath}`, 'success');
      } else {
        throw new Error(last?.message ?? 'WriteFIT failed');
      }
    } catch (e) {
      notifications.error(`Save failed: ${e}`);
    }
  }

  async function backupDatabase() {
    notifications.running('Backing up database…');
    try {
      const path = await db.backupDatabase();
      notifications.success(`Database backed up to ${path}`);
    } catch (e) {
      notifications.error(`Backup failed: ${e}`);
    }
  }

  async function restoreDatabase() {
    let selected;
    try {
      selected = await open({
        multiple: false,
        filters: [{ name: 'Photyx Database Backup', extensions: ['zip'] }]
      });
    } catch (e) {
      notifications.error(`Failed to open file picker: ${e}`);
      return;
    }
    if (!selected) return;
    const path = typeof selected === 'string' ? selected : selected[0];
    if (!path) return;
    try {
      await db.restoreDatabase(path);
      await new Promise(resolve => setTimeout(resolve, 200));
      const prefs = await db.getAllPreferences();
      ui.hydrateFromDb(prefs);
      settings.hydrate(prefs);
      const buttons = await db.getQuickLaunchButtons();
      quickLaunch.hydrate(buttons);
      await thresholdProfiles.hydrate();
      // ui_font_size is applied as a direct DOM write, not through the
      // reactive store system (same as +page.svelte's onMount) — without
      // re-running this here, the restored value shows correctly in the
      // Preferences dialog but the actual rendered text size doesn't
      // change until something else happens to trigger it.
      const fontSize = parseFloat(prefs['ui_font_size'] ?? String(DEFAULT_FONT_SIZE)) || DEFAULT_FONT_SIZE;
      document.documentElement.style.fontSize = `${fontSize}px`;
      notifications.success('Database restored successfully.');
    } catch (e) {
      notifications.error(`Restore failed: ${e}`);
    }
  }

  async function runContourHeatmap() {
    notifications.running('ContourHeatmap running…');
    try {
      const response = await invoke<{
        success: boolean;
        output: string | null;
        error: string | null;
        data: Record<string, unknown> | null;
      }>('dispatch_command', {
        request: { command: 'ContourHeatmap', args: {} }
      });
      if (response.success) {
        const msg = response.output ?? 'ContourHeatmap complete';
        pipeToConsole(msg, 'success');
        notifications.success(msg);
        const filePath = response.data?.output as string | null;
        if (filePath) await loadFile(filePath);
      } else {
        const err = response.error ?? 'ContourHeatmap failed';
        pipeToConsole(err, 'error');
        notifications.error(err);
      }
    } catch (err) {
      const msg = `ContourHeatmap error: ${err}`;
      pipeToConsole(msg, 'error');
      notifications.error(msg);
    }
  }

  // ── Session JSON export ───────────────────────────────────────────────────

  async function exportSessionJson() {
    // Derive a default filename suggestion from analysis results for the save dialog
    let defName = 'session.json';
    try {
      const data = await invoke<any>('get_analysis_results');
      if (!data.frames || data.frames.length === 0) {
        notifications.error('No analysis results to export. Run AnalyzeFrames first.');
        return;
      }
      const first = (data.frames[0].short_name as string) ?? '';
      const targetMatch = first.match(/^Light_([^_]+)_/);
      const dateMatch   = first.match(/(\d{8})-\d{6}/);
      if (targetMatch && dateMatch) {
        defName = `${targetMatch[1]}_${dateMatch[1]}_analysis.json`;
      } else if (targetMatch) {
        defName = `${targetMatch[1]}_analysis.json`;
      }
    } catch (e) {
      notifications.error(`Export failed: could not load analysis results: ${e}`);
      return;
    }

    let savePath: string | null;
    try {
      savePath = await save({
        title:       'Export Session JSON',
        defaultPath: defName,
        filters:     [{ name: 'Photyx Session JSON', extensions: ['json'] }],
      });
    } catch (e) {
      notifications.error(`Export cancelled: ${e}`);
      return;
    }
    if (!savePath) return;

    notifications.running('Exporting analysis report…');
    try {
      const response = await invoke<{
        results: Array<{ success: boolean; message: string | null }>;
      }>('run_script', {
        script: `ExportAnalysisReport path="${savePath.replace(/\\/g, '/')}"`
      });
      const last = response.results[response.results.length - 1];
      if (last?.success) {
        notifications.success('Session exported.');
        pipeToConsole(last.message ?? 'Analysis report exported.', 'success');
      } else {
        throw new Error(last?.message ?? 'ExportAnalysisReport failed');
      }
    } catch (e) {
      notifications.error(`Export failed: ${e}`);
    }
  }

  // ── Session JSON import ───────────────────────────────────────────────────

  async function importSessionJson() {
    let selected: string | string[] | null;
    try {
      selected = await open({
        multiple: false,
        filters:  [{ name: 'Photyx Session JSON', extensions: ['json'] }],
      });
    } catch (e) {
      notifications.error(`Import cancelled: ${e}`);
      return;
    }
    if (!selected) return;
    const filePath = typeof selected === 'string' ? selected : selected[0];
    if (!filePath) return;

    let raw: string;
    try {
      raw = await readTextFile(filePath);
    } catch (e) {
      notifications.error(`Could not read file: ${e}`);
      return;
    }

    let payload: any;
    try {
      payload = JSON.parse(raw);
    } catch (e) {
      notifications.error(`Invalid JSON file: ${e}`);
      return;
    }

    // Basic validation
    if (!payload.frames || !Array.isArray(payload.frames)) {
      notifications.error('Invalid session JSON: missing frames array.');
      return;
    }

    notifications.running('Importing session ');
    try {
      await invoke('load_analysis_json', { payload });
      notifications.success(`Session imported — ${payload.frames.length} frames`);
      // Open the results view so the user sees the imported data immediately
      ui.showView('analysisResults');
    } catch (e) {
      notifications.error(`Import failed: ${e}`);
    }
  }

  // ── Menu definitions ──────────────────────────────────────────────────────
  const MENUS: MenuDef[] = [
    { name: 'File', items: [
      { label: 'Load Single Image ', action: 'load-single-image' },
      { sep: true },
      { label: 'Save as FITS',       action: 'save-as-fits' },
      { sep: true },
      { label: 'Exit',               action: 'exit' },
    ]},
    { name: 'Session', items: [
      { label: 'Add Files', action: 'add-files', shortcut: 'Ctrl+O' },
      { label: 'Clear Session', action: 'close-session' },
    ]},
    { name: 'Edit', items: [
      { label: 'Preferences',         action: 'preferences' },
      { label: 'Analysis Parameters', action: 'analysis-parameters' },
    ]},
    { name: 'View', items: [
      { label: 'Theme: Dark',   action: 'theme-dark' },
      { label: 'Theme: Light',  action: 'theme-light' },
      { label: 'Theme: Matrix', action: 'theme-matrix' },
    ]},
    { name: 'Analyze', items: [
      { label: 'Analyze Frames',          action: 'analyze-frames' },
      { label: 'Analysis Results',        action: 'analysis-results' },
      { label: 'Analysis Graph',          action: 'analysis-graph' },
      { sep: true },
      { label: 'Export Analysis Results', action: 'export-analysis-results' },
      { label: 'Import Analysis Results', action: 'import-analysis-results' },
      { sep: true },
      { label: 'Stacking Workspace',      action: 'stacking-workspace' },
      // { sep: true },
      // { label: 'Contour Plot',            action: 'contour-plot' },
    ]},
    { name: 'Tools', items: [
      { label: 'Backup Database',  action: 'backup-database' },
      { label: 'Restore Database', action: 'restore-database' },
      { sep: true },
      { label: 'Log Viewer', action: 'log-viewer' },
    ]},
    { name: 'Help', items: [
      { label: 'About Photyx',  action: 'about' },
      { label: 'Documentation', action: 'documentation' },
    ]},
  ];
</script>

<svelte:window onclick={close} />

<div id="menu-bar">
  {#each MENUS as menu}
    <div
      class="menu-item"
      class:open={openMenu === menu.name}
      onclick={(e) => { e.stopPropagation(); toggle(menu.name); }}
    >
      {menu.name}
      {#if openMenu === menu.name}
        <div class="menu-dropdown">
          {#each menu.items as item}
            {#if item.sep}
              <div class="menu-separator"></div>
            {:else}
              <div
                class="menu-dropdown-item"
                onclick={(e) => { e.stopPropagation(); action(item.action ?? ''); }}
              >
                {item.label}
                {#if item.shortcut}
                  <span class="shortcut">{item.shortcut}</span>
                {/if}
              </div>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  {/each}
</div>
