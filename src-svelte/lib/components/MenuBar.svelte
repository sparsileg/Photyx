<!-- MenuBar.svelte — Application menu bar. Spec §8.2 -->

<script lang="ts">
  import { pipeToConsole } from '../stores/consoleHistory';
  import { db } from '../db';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { invoke } from '@tauri-apps/api/core';
  import { notifications } from '../stores/notifications';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
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
    case 'about':               ui.openAbout(); break;
    case 'analysis-parameters': ui.openAnalysisParameters(); break;
    case 'preferences':         ui.openPreferences(); break;
    case 'analysis-graph':      ui.showView('analysisGraph'); break;
    case 'analysis-results':    ui.showView('analysisResults'); break;
    case 'analyze-frames':      runAnalyzeFrames(); break;
    case 'backup-database':     backupDatabase(); break;
    case 'close-session':       closeSession(); break;
    case 'contour-plot':        runContourHeatmap(); break;
    case 'exit':                db.closeSession().catch(() => {}).finally(() => getCurrentWindow().close()); break;
    case 'export-session-json': exportSessionJson(); break;
    case 'import-session-json': importSessionJson(); break;
    case 'keywords':            ui.togglePanel('keywords'); break;
    case 'load-single-image':   loadSingleImage(); break;
    case 'log-viewer':          ui.openLogViewer(); break;
    case 'macro-library':       ui.togglePanel('macro-lib'); break;
    case 'plugin-manager':      ui.togglePanel('plugins'); break;
    case 'restore-database':    restoreDatabase(); break;
    case 'run-macro':           ui.togglePanel('macro-editor'); break;
    case 'select-directory':    selectDirectory(); break;
    case 'theme-dark':          ui.setTheme('dark'); break;
    case 'theme-light':         ui.setTheme('light'); break;
    case 'theme-matrix':        ui.setTheme('matrix'); break;
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
        pipeToConsole(msg, 'success');
        notifications.success('AnalyzeFrames complete');
      } else {
        const err = response.error ?? 'AnalyzeFrames failed';
        pipeToConsole(err, 'error');
        notifications.error(err);
      }
    } catch (err) {
      const msg = `AnalyzeFrames error: ${err}`;
      pipeToConsole(msg, 'error');
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
    // Fetch current analysis results from Rust
    let data: any;
    try {
      data = await invoke('get_analysis_results');
    } catch (e) {
      notifications.error(`Export failed: could not load analysis results: ${e}`);
      return;
    }

    if (!data.frames || data.frames.length === 0) {
      notifications.error('No analysis results to export. Run AnalyzeFrames first.');
      return;
    }

    // Fetch current threshold profile for metadata
    let activeProfileId: number | null = null;
    let profileName = 'Default';
    let thresholds: any = {};
    try {
      activeProfileId = await invoke('get_active_threshold_profile_id');
      const profiles: any[] = await invoke('get_threshold_profiles');
      const active = profiles.find((p: any) => p.id === activeProfileId) ?? profiles[0];
      if (active) {
        profileName = active.name;
        thresholds = {
          bg_median_reject_sigma:      active.bg_median_reject_sigma,
          signal_weight_reject_sigma:  active.signal_weight_reject_sigma,
          fwhm_reject_sigma:           active.fwhm_reject_sigma,
          star_count_reject_sigma:     active.star_count_reject_sigma,
          eccentricity_reject_abs:     active.eccentricity_reject_abs,
        };
      }
    } catch (e) {
      // Non-fatal — continue with empty thresholds
    }

    // Build per-frame array using basenames only for portability
    const frames = data.frames.map((f: any) => ({
      filename:           f.short_name,
      fwhm:               f.fwhm,
      eccentricity:       f.eccentricity,
      star_count:         f.star_count,
      signal_weight:      f.signal_weight,
      background_median:  f.background_median,
      flag:               f.flag || 'PASS',
      triggered_by:       f.triggered ?? [],
      rejection_category: f.rejection_category ?? null,
    }));

    // Outlier paths — strip to basenames
    const outlierPaths = (data.outlier_paths ?? []).map((p: string) => {
      return p.split('/').pop() ?? p.split('\\').pop() ?? p;
    });

    const json = {
      photyx_version:         '1.0.0',
      exported_at:            new Date().toISOString(),
      active_directory:       data.session_path ?? '',
      threshold_profile_name: profileName,
      thresholds,
      session_stats:          data.session_stats ?? {},
      outlier_paths:          outlierPaths,
      frames,
    };

    // Derive default filename from the first frame: Light_<target>_..._<YYYYMMDD>-######_...
    // e.g. Light_M82_180.0s_Bin1_gain101_20240206-190228_-20.0C_0001.fit → M82_20240206.json
    let defName = 'session.json';
    if (data.frames.length > 0) {
      const first = (data.frames[0].short_name as string) ?? '';
      const targetMatch = first.match(/^Light_([^_]+)_/);
      const dateMatch   = first.match(/(\d{8})-\d{6}/);
      if (targetMatch && dateMatch) {
        defName = `${targetMatch[1]}_${dateMatch[1]}.json`;
      } else if (targetMatch) {
        defName = `${targetMatch[1]}.json`;
      }
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

    try {
      await writeTextFile(savePath, JSON.stringify(json, null, 2));
      notifications.success('Session exported.');
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
    if (!payload.active_directory) {
      notifications.error('Invalid session JSON: missing active_directory.');
      return;
    }

    notifications.running('Importing session…');
    try {
      await invoke('load_analysis_json', { payload });
      notifications.success(`Session imported — ${payload.frames.length} frames from ${payload.active_directory}`);
      // Open the results view so the user sees the imported data immediately
      ui.showView('analysisResults');
    } catch (e) {
      notifications.error(`Import failed: ${e}`);
    }
  }
</script>

<svelte:window onclick={close} />

<div id="menu-bar">
  {#each [
    { name: 'File', items: [
      { label: 'Load Single Image…', action: 'load-single-image' },
      { sep: true },
      { label: 'Exit',               action: 'exit' },
    ]},
    { name: 'Session', items: [
      { label: 'Select Directory…',    action: 'select-directory', shortcut: 'Ctrl+O' },
      { label: 'Close Session',        action: 'close-session' },
      { sep: true },
      { label: 'Export Session JSON…', action: 'export-session-json' },
      { label: 'Import Session JSON…', action: 'import-session-json' },
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
      { label: 'Analyze Frames',   action: 'analyze-frames' },
      { label: 'Analysis Results', action: 'analysis-results' },
      { label: 'Analysis Graph',   action: 'analysis-graph' },
      { sep: true },
      { label: 'Contour Plot',     action: 'contour-plot' },
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
