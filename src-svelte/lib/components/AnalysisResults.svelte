<!-- AnalysisResults.svelte — Per-frame analysis results table. Spec §8.11 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { save } from '@tauri-apps/plugin-dialog';
  import { writeTextFile } from '@tauri-apps/plugin-fs';
  import { ui } from '../stores/ui';
  import { notifications } from '../stores/notifications';

  interface FrameResult {
    index: number;
    filename: string;
    short_name: string;
    fwhm?: number;
    eccentricity?: number;
    star_count?: number;
    snr_estimate?: number;
    background_median?: number;
    flag?: string;
    rejection_category?: string;
  }

  let frames = $state<FrameResult[]>([]);
  let loading = $state(true);

  type SortCol = 'index' | 'short_name' | 'fwhm' | 'eccentricity' | 'star_count'
    | 'snr_estimate' | 'background_median' | 'flag' | 'rejection_category';

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

  // Returns the CSS class for a rejection category badge.
  // Single-letter categories get their own color; multi-category gets neutral purple.
  function catClass(cat: string | undefined): string {
    if (!cat) return '';
    if (cat === 'O') return 'ar-cat-badge ar-cat-o';
    if (cat === 'T') return 'ar-cat-badge ar-cat-t';
    if (cat === 'B') return 'ar-cat-badge ar-cat-b';
    return 'ar-cat-badge ar-cat-multi';
  }

  async function loadData() {
    loading = true;
    try {
      const data = await invoke<{ frames: FrameResult[] }>('get_analysis_results');
      frames = data.frames;
    } catch (e) {
      notifications.error(`Analysis Results: ${e}`);
    } finally {
      loading = false;
    }
  }

  async function commitResults() {
    notifications.running('Writing PXFLAG to files…');
    try {
      const msg = await invoke<string>('commit_analysis_results');
      notifications.success(msg);
    } catch (e) {
      notifications.error(`Commit failed: ${e}`);
    }
  }

  const HEADERS = ['#', 'Filename', 'FWHM', 'Eccentricity', 'Stars', 'SNR', 'Bg Median', 'PXFLAG', 'Category'];

  function buildRows(sep: string): string {
    const q = (v: string) => `"${v.replace(/"/g, '""')}"`;
    const rows = sorted.map(row => [
      String(row.index + 1),
      q(row.filename),
      fmt(row.fwhm),
      fmt(row.eccentricity),
      fmt(row.star_count, 0),
      fmt(row.snr_estimate),
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

  async function exportCsv() {
    try {
      const path = await save({
        title: 'Export Analysis Results',
        defaultPath: 'analysis_results.csv',
        filters: [{ name: 'CSV', extensions: ['csv'] }],
      });
      if (!path) return;
      await writeTextFile(path, buildRows(','));
      notifications.success('CSV exported.');
    } catch (e) {
      notifications.error(`Export failed: ${e}`);
    }
  }

  onMount(loadData);
</script>

<div id="analysis-results">
  <div class="ar-toolbar">
    <span class="ar-title">Analysis Results</span>
    <button class="ar-btn" onclick={loadData}>↻ Refresh</button>
    <button class="ar-btn" onclick={commitResults}>✓ Commit Results</button>
    <button class="ar-btn" onclick={copyToClipboard}>⎘ Copy</button>
    <button class="ar-btn" onclick={exportCsv}>⬇ Export CSV</button>
    <button class="ar-close-btn" onclick={() => ui.showView(null)}>✕ Close</button>
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
            <th onclick={() => sortBy('snr_estimate')}>SNR{arrow('snr_estimate')}</th>
            <th onclick={() => sortBy('background_median')}>Bg Median{arrow('background_median')}</th>
            <th onclick={() => sortBy('flag')}>PXFLAG{arrow('flag')}</th>
            <th onclick={() => sortBy('rejection_category')}>Category{arrow('rejection_category')}</th>
          </tr>
        </thead>
        <tbody>
          {#each sorted as row (row.index)}
            <tr>
              <td>{row.index + 1}</td>
              <td class="ar-filename">{fmtFilename(row.short_name)}</td>
              <td>{fmt(row.fwhm)}</td>
              <td>{fmt(row.eccentricity)}</td>
              <td>{fmt(row.star_count, 0)}</td>
              <td>{fmt(row.snr_estimate)}</td>
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
