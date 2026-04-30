<!-- FileBrowser.svelte — Spec §8.6 -->

<script lang="ts">
  import { ui } from '../../stores/ui';
  import { session } from '../../stores/session';
  import { selectDirectory, loadFiles, displayFrame } from '../../commands';
  import type { FormatFilter } from '../../commands';
  import { FORMAT_FILTERS } from '../../commands';
  import Dropdown from '../Dropdown.svelte';

  const MIN_WIDTH = 280;
  const MAX_WIDTH = 540;
  const CHAR_WIDTH = 7.5;
  const PADDING = 48;

  let formatFilter = $state<FormatFilter>('all');

  // Recompute panel width whenever file list changes
  $effect(() => {
    const files = $session.fileList;
    const panel = document.getElementById('panel-container');
    if (!panel) return;

    if (files.length === 0) {
      panel.style.setProperty('--panel-width', `${MIN_WIDTH}px`);
      return;
    }

    const longest = files.reduce((max, f) => {
      const name = f.split('/').pop() ?? f;
      return name.length > max ? name.length : max;
    }, 0);

    const needed = Math.ceil(longest * CHAR_WIDTH + PADDING);
    const width = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, needed));
    panel.style.setProperty('--panel-width', `${width}px`);
  });

  // Snap back to minimum when panel closes
  $effect(() => {
    const panel = document.getElementById('panel-container');
    if (!panel) return;
    if ($ui.activePanel !== 'files') {
      panel.style.setProperty('--panel-width', `${MIN_WIDTH}px`);
    }
  });
</script>

<div class="sliding-panel active">
  <div class="panel-header">
    <span>File Browser</span>
    <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
  </div>
  <div class="dir-bar">
    <span class="dir-path" title={$session.activeDirectory ?? ''}>
      {$session.activeDirectory ?? '(no directory selected)'}
    </span>
    <button class="dir-browse-btn" onclick={selectDirectory}>…</button>
  </div>
  <div class="file-browser-controls">
    <Dropdown
      className="format-filter-select"
      bind:value={formatFilter}
      openUp={false}
      width={130}
      options={FORMAT_FILTERS.map(f => ({ value: f.id, label: f.label }))}
      />
      <button
        class="load-btn"
        disabled={!$session.activeDirectory}
        onclick={() => loadFiles(formatFilter)}
        >Load</button>
  </div>
  <div class="panel-body">
    <ul class="file-list">
      {#if $session.fileList.length === 0}
        <li class="file-item empty-hint">
          Select a directory and click Load
        </li>
      {:else}
        {#each $session.fileList as file, i}
          <li
            class="file-item"
            class:selected={$session.currentFrame === i}
            onclick={() => { displayFrame(i); ui.closePanel(); }}
            >
            <span class="file-name">{file.split('/').pop()}</span>
          </li>
        {/each}
      {/if}
    </ul>
  </div>
</div>
