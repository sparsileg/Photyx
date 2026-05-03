<!-- AnalysisResults.svelte — Per-frame analysis results table. Spec §8.11 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
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
  }

  let frames = $state<FrameResult[]>([]);
  let loading = $state(true);

  type SortCol = 'index' | 'short_name' | 'fwhm' | 'eccentricity' | 'star_count'
    | 'snr_estimate' | 'background_median' | 'flag';

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

  onMount(loadData);
</script>

<div id="analysis-results">
  <div class="ar-toolbar">
    <span class="ar-title">Analysis Results</span>
    <button class="ar-btn" onclick={loadData}>↻ Refresh</button>
    <button class="ar-btn" onclick={commitResults}>✓ Commit Results</button>
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
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
