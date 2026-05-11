<!-- AnalysisResults.svelte — Per-frame analysis results table. Spec §8.11 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { ui } from '../stores/ui';
  import { notifications } from '../stores/notifications';
  import { closeSession } from '../commands';

  interface FrameResult {
    index: number;
    filename: string;
    short_name: string;
    fwhm?: number;
    eccentricity?: number;
    star_count?: number;
    signal_weight?: number;
    background_median?: number;
    flag?: string;
    rejection_category?: string;
    triggered?: string[];
    toggled?: boolean;
  }

  interface AnalysisResponse {
    frames:       FrameResult[];
    session_path: string;
    is_imported:  boolean;
  }

  let frames      = $state<FrameResult[]>([]);
  let sessionPath = $state('');
  let isImported  = $state(false);
  let loading     = $state(true);

  // ── Context menu state ────────────────────────────────────────────────────
  interface ContextMenu {
    x:     number;
    y:     number;
    index: number;
  }
  let contextMenu = $state<ContextMenu | null>(null);

  function onRowContextMenu(e: MouseEvent, index: number) {
    e.preventDefault();
    contextMenu = { x: e.clientX, y: e.clientY, index };
  }

  function closeContextMenu() {
    contextMenu = null;
  }

  function toggleFlag(index: number) {
    closeContextMenu();
    frames = frames.map(f => {
      if (f.index !== index) return f;
      const newFlag = f.flag === 'REJECT' ? 'PASS' : 'REJECT';
      return { ...f, flag: newFlag, toggled: true };
    });
  }

  function contextMenuLabel(index: number): string {
    const f = frames.find(f => f.index === index);
    if (!f) return '';
    return f.flag === 'REJECT' ? 'Set to PASS' : 'Set to REJECT';
  }

  // ── Sort ──────────────────────────────────────────────────────────────────
  type SortCol = 'index' | 'short_name' | 'fwhm' | 'eccentricity' | 'star_count'
    | 'signal_weight' | 'background_median' | 'flag' | 'rejection_category';

  let sortCol = $state<SortCol>('index');
  let sortAsc = $state(true);

  function fmt(v: number | undefined, decimals = 3): string {
    if (v === undefined) return '—';
    if (v !== 0 && Math.abs(v) < 0.1) return v.toExponential(3);
    return v.toFixed(decimals);
  }

  function fmtFilename(name: string): string {
    const dot = name.lastIndexOf('.');
    const base = dot >= 0 ? name.slice(0, dot) : name;
    const ext  = dot >= 0 ? name.slice(dot) : '';
    if (base.length <= 21) return name;
    return base.slice(0, 16) + ' … ' + base.slice(-5) + ext;
  }

  function sortBy(col: SortCol) {
    if (sortCol === col) {
      sortAsc = !sortAsc;
    } else {
      sortCol = col;
      sortAsc = true;
    }
  }

  let sorted = $derived((() => {
    const arr = [...frames];
    arr.sort((a, b) => {
      const av = a[sortCol];
      const bv = b[sortCol];
      if (av === undefined && bv === undefined) return 0;
      if (av === undefined) return 1;
      if (bv === undefined) return -1;
      if (typeof av === 'string' && typeof bv === 'string') {
        return sortAsc ? av.localeCompare(bv) : bv.localeCompare(av);
      }
      return sortAsc
        ? (av as number) - (bv as number)
        : (bv as number) - (av as number);
    });
    return arr;
  })());

  function arrow(col: SortCol): string {
    if (sortCol !== col) return '';
    return sortAsc ? ' ▲' : ' ▼';
  }

  function catClass(cat: string | undefined): string {
    if (!cat) return '';
    if (cat === 'O') return 'ar-cat-badge ar-cat-o';
    if (cat === 'T') return 'ar-cat-badge ar-cat-t';
    if (cat === 'B') return 'ar-cat-badge ar-cat-b';
    return 'ar-cat-badge ar-cat-multi';
  }

  // ── Load data ─────────────────────────────────────────────────────────────
  async function loadData() {
    loading = true;
    try {
      const data = await invoke<AnalysisResponse>('get_analysis_results');
      frames      = data.frames.map(f => ({ ...f, toggled: false }));
      sessionPath = data.session_path ?? '';
      isImported  = data.is_imported ?? false;
    } catch (e) {
      notifications.error(`Analysis Results: ${e}`);
    } finally {
      loading = false;
    }
  }

  // ── Commit ────────────────────────────────────────────────────────────────
  async function commitResults() {
    if (isImported) {
      notifications.error('Cannot commit an imported session — no images are loaded.');
      return;
    }

    // Push any local flag toggles back to Rust before committing
    const toggled = frames.filter(f => f.toggled);
    if (toggled.length > 0) {
      try {
        for (const f of toggled) {
          await invoke('set_frame_flag', { path: f.filename, flag: f.flag });
        }
      } catch (e) {
        notifications.error(`Failed to sync flag changes: ${e}`);
        return;
      }
    }

    // Commit is terminal — close view, clear displayed image, close session
    notifications.running('Committing results…');
    try {
      const msg = await invoke<string>('commit_analysis_results');
      notifications.success(msg);
      ui.showView(null);
      ui.clearViewer();
      await closeSession();
    } catch (e) {
      notifications.error(`Commit failed: ${e}`);
    }
  }

  // ── Clipboard copy ────────────────────────────────────────────────────────
  const HEADERS = ['#', 'Filename', 'FWHM', 'Eccentricity', 'Stars', 'Signal Weight', 'Bg Median', 'PXFLAG', 'Category'];

  function buildRows(sep: string): string {
    const q = (v: string) => `"${v.replace(/"/g, '""')}"`;
    const rows = sorted.map(row => [
      String(row.index + 1),
      q(row.filename),
      fmt(row.fwhm),
      fmt(row.eccentricity),
      fmt(row.star_count, 0),
      fmt(row.signal_weight),
      fmt(row.background_median),
      row.flag ?? '—',
      row.rejection_category ?? '—',
    ].join(sep));
    return [HEADERS.map(q).join(sep), ...rows].join('\n');
  }

  async function copyToClipboard() {
    try {
      await navigator.clipboard.writeText(buildRows('\t'));
      notifications.success('Results copied to clipboard.');
    } catch (e) {
      notifications.error(`Copy failed: ${e}`);
    }
  }

  onMount(loadData);
</script>

<!-- Close context menu on any click outside -->
<svelte:window onclick={closeContextMenu} />

<div id="analysis-results">
  <div class="ar-toolbar">
    <span class="ar-title">Analysis Results</span>
    <button class="ar-btn" onclick={loadData}>↻ Refresh</button>
    <button
      class="ar-btn"
      onclick={commitResults}
      disabled={isImported}
      title={isImported ? 'Cannot commit an imported session' : ''}
    >✓ Commit Results</button>
    <button class="ar-btn" onclick={copyToClipboard}>⎘ Copy</button>
    <button class="ar-close-btn" onclick={() => ui.showView(null)}>✕ Close</button>
  </div>
  <div class="ar-session-path">
    {#if isImported}
      <span class="ar-imported-badge">IMPORTED</span>
    {/if}
    <span class="ar-session-path-label">Session path:</span>
    <span class="ar-session-path-value">{sessionPath || '—'}</span>
  </div>

  {#if loading}
    <div class="ar-loading">Loading…</div>
  {:else}
    <div class="ar-table-wrap">
      <table class="ar-table">
        <thead>
          <tr>
            <th onclick={() => sortBy('index')}>#{ arrow('index')}</th>
            <th onclick={() => sortBy('short_name')}>Filename{arrow('short_name')}</th>
            <th onclick={() => sortBy('fwhm')}>FWHM{arrow('fwhm')}</th>
            <th onclick={() => sortBy('eccentricity')}>Eccentricity{arrow('eccentricity')}</th>
            <th onclick={() => sortBy('star_count')}>Stars{arrow('star_count')}</th>
            <th onclick={() => sortBy('signal_weight')}>Sig. Weight{arrow('signal_weight')}</th>
            <th onclick={() => sortBy('background_median')}>Bg Median{arrow('background_median')}</th>
            <th onclick={() => sortBy('flag')}>PXFLAG{arrow('flag')}</th>
            <th onclick={() => sortBy('rejection_category')}>Category{arrow('rejection_category')}</th>
          </tr>
        </thead>
        <tbody>
          {#each sorted as row (row.index)}
            <tr
              class:ar-row-toggled={row.toggled}
              oncontextmenu={(e) => onRowContextMenu(e, row.index)}
            >
              <td>{row.index + 1}</td>
              <td class="ar-filename">{fmtFilename(row.short_name)}</td>
              <td>{fmt(row.fwhm)}</td>
              <td>{fmt(row.eccentricity)}</td>
              <td>{fmt(row.star_count, 0)}</td>
              <td>{fmt(row.signal_weight)}</td>
              <td>{fmt(row.background_median)}</td>
              <td>{row.flag ?? '—'}</td>
              <td>
                {#if row.rejection_category}
                  <span class={catClass(row.rejection_category)}>{row.rejection_category}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<!-- Context menu -->
{#if contextMenu}
  <div
    class="ar-context-menu"
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
    onclick={(e) => e.stopPropagation()}
  >
    <div
      class="ar-context-item"
      onclick={() => toggleFlag(contextMenu!.index)}
    >
      {contextMenuLabel(contextMenu.index)}
    </div>
  </div>
{/if}
