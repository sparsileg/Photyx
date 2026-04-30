<!-- MenuBar.svelte — Application menu bar. Spec §8.2 -->

<script lang="ts">
  import { consolePipe } from '../stores/consoleHistory';
  import { db } from '../db';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { invoke } from '@tauri-apps/api/core';
  import { notifications } from '../stores/notifications';
  import { open } from '@tauri-apps/plugin-dialog';
  import { quickLaunch } from '../stores/quickLaunch';
  import { selectDirectory, closeSession, applyAutoStretch, loadFile } from '../commands';
  import { ui } from '../stores/ui';

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
    case 'about':             ui.openAbout(); break;
    case 'analysis-graph':    ui.showView('analysisGraph'); break;
    case 'analysis-results':  ui.showView('analysisResults'); break;
    case 'analyze-frames':    runAnalyzeFrames(); break;
    case 'backup-database':   backupDatabase(); break;
    case 'close-session':     closeSession(); break;
    case 'contour-plot':      runContourHeatmap(); break;
    case 'exit':              db.closeSession().catch(() => {}).finally(() => getCurrentWindow().close()); break;
    case 'keywords':          ui.togglePanel('keywords'); break;
    case 'load-single-image': loadSingleImage(); break;
    case 'log-viewer':        ui.openLogViewer(); break;
    case 'macro-library':     ui.togglePanel('macro-lib'); break;
    case 'plugin-manager':    ui.togglePanel('plugins'); break;
    case 'restore-database':  restoreDatabase(); break;
    case 'run-macro':         ui.togglePanel('macro-editor'); break;
    case 'select-directory':  selectDirectory(); break;
    case 'theme-dark':        ui.setTheme('dark'); break;
    case 'theme-light':       ui.setTheme('light'); break;
    case 'theme-matrix':      ui.setTheme('matrix'); break;
    default: notifications.info(`${a} — not yet implemented`);
    }
  }

  async function runAnalyzeFrames() {
    notifications.running('AnalyzeFrames running…');
    try {
      const response = await invoke<{
        success: boolean;
        output: string | null;
        error: string | null;
      }>('dispatch_command', {
        request: { command: 'AnalyzeFrames', args: {} }
      });
      if (response.success) {
        const msg = response.output ?? 'AnalyzeFrames complete';
        consolePipe.set({ id: Date.now(), text: msg, type: 'success' });
        notifications.success('AnalyzeFrames complete');
      } else {
        const err = response.error ?? 'AnalyzeFrames failed';
        consolePipe.set({ id: Date.now(), text: err, type: 'error' });
        notifications.error(err);
      }
    } catch (err) {
      const msg = `AnalyzeFrames error: ${err}`;
      consolePipe.set({ id: Date.now(), text: msg, type: 'error' });
      notifications.error(msg);
    }
  }

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
        consolePipe.set({ id: Date.now(), text: msg, type: 'success' });
        notifications.success(msg);
        await applyAutoStretch();
      } else {
        const err = response.error ?? 'AutoStretch failed';
        consolePipe.set({ id: Date.now(), text: err, type: 'error' });
        notifications.error(err);
      }
    } catch (err) {
      const msg = `AutoStretch error: ${err}`;
      consolePipe.set({ id: Date.now(), text: msg, type: 'error' });
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
      // Brief pause to let the reopened connection settle
      await new Promise(resolve => setTimeout(resolve, 200));
      // Re-hydrate frontend stores from restored DB
      const prefs = await db.getAllPreferences();
      ui.hydrateFromDb(prefs);
      const buttons = await db.getQuickLaunchButtons();
      quickLaunch.hydrate(buttons);
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
        consolePipe.set({ id: Date.now(), text: msg, type: 'success' });
        notifications.success(msg);
        const filePath = response.data?.output as string | null;
        if (filePath) await loadFile(filePath);
      } else {
        const err = response.error ?? 'ContourHeatmap failed';
        consolePipe.set({ id: Date.now(), text: err, type: 'error' });
        notifications.error(err);
      }
    } catch (err) {
      const msg = `ContourHeatmap error: ${err}`;
      consolePipe.set({ id: Date.now(), text: msg, type: 'error' });
      notifications.error(msg);
    }
  }
</script>

<svelte:window onclick={close} />

<div id="menu-bar">
  {#each [
    { name: 'File', items: [
  { label: 'Select Directory…',    action: 'select-directory',     shortcut: 'Ctrl+O' },
  { label: 'Load Single Image…',   action: 'load-single-image' },
  { label: 'Close Session',        action: 'close-session' },
  { sep: true },
  { label: 'Exit',         action: 'exit' },
  ]},
  { name: 'Edit', items: [
  { label: 'Preferences',        action: 'preferences' },
  { label: 'Analysis Parameters', action: 'analysis-parameters' },
  ]},
  { name: 'View', items: [
  { label: 'Theme: Dark',  action: 'theme-dark' },
  { label: 'Theme: Light', action: 'theme-light' },
  { label: 'Theme: Matrix',action: 'theme-matrix' },
  ]},
  { name: 'Analyze', items: [
  { label: 'Analyze Frames',   action: 'analyze-frames' },
  { label: 'Analysis Results', action: 'analysis-results' },
  { label: 'Analysis Graph',   action: 'analysis-graph' },
  { label: 'Contour Plot',     action: 'contour-plot' },
  ]},
  { name: 'Tools', items: [
  { label: 'Backup Database', action: 'backup-database' },
  { label: 'Restore Database', action: 'restore-database' },
  { sep: true },
  { label: 'Log Viewer',      action: 'log-viewer' },
  ]},
  { name: 'Help', items: [
  { label: 'About Photyx', action: 'about' },
  { label: 'Documentation',action: 'documentation' },
  ]},
  ] as menu}
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
