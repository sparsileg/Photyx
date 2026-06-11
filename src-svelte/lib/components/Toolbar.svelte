<!-- Toolbar.svelte — Zoom, stretch, channel controls. Spec §8.3 -->
<script lang="ts">
  import { ui } from '../stores/ui';
  import type { ZoomLevel } from '../stores/ui';
  import { session, directoryCount } from '../stores/session';
  import { settings } from '../stores/settings';
  import { invoke } from '@tauri-apps/api/core';
  import { MIN_FONT_SIZE, MAX_FONT_SIZE, FONT_SIZE_STEP } from '../settings/constants';

  const zoomLevels: { id: ZoomLevel; label: string }[] = [
    { id: 'fit',  label: 'Fit' },
    { id: '25',   label: '25%' },
    { id: '50',   label: '50%' },
    { id: '100',  label: '100%' },
    { id: '200',  label: '200%' },
  ];

  function adjustFontSize(delta: number) {
    const current = $settings.ui_font_size;
    const next = Math.round((current + delta) * 10) / 10;
    if (next < MIN_FONT_SIZE || next > MAX_FONT_SIZE) return;
    document.documentElement.style.fontSize = `${next}px`;
    settings.savePreferences({ ui_font_size: next });
  }
</script>

<div id="toolbar">
  <div class="toolbar-group">
    <span class="toolbar-label">Zoom</span>
    {#each zoomLevels as z}
      <button
        class="toolbar-btn"
        class:active={$ui.zoomLevel === z.id}
        disabled={$ui.blinkTabActive}
        onclick={() => ui.setZoom(z.id)}
        >{z.label}</button>
    {/each}
  </div>
  <div class="toolbar-sep"></div>
<div class="toolbar-group">
    <span class="toolbar-dir">
      {$session.fileList.length === 0
        ? 'No files loaded'
        : `${$session.fileList.length} file(s) · ${$directoryCount} director${$directoryCount === 1 ? 'y' : 'ies'}`}
    </span>
  </div>
  <div class="toolbar-sep"></div>
  <div class="toolbar-group toolbar-fontsize-group">
    <span class="toolbar-label">Text</span>
    <button
      class="toolbar-btn toolbar-fontsize-btn"
      onclick={() => adjustFontSize(-FONT_SIZE_STEP)}
      disabled={$settings.ui_font_size <= MIN_FONT_SIZE}
    >◀</button>
    <span class="toolbar-fontsize-display">{$settings.ui_font_size}px</span>
    <button
      class="toolbar-fontsize-btn toolbar-btn"
      onclick={() => adjustFontSize(FONT_SIZE_STEP)}
      disabled={$settings.ui_font_size >= MAX_FONT_SIZE}
    >▶</button>
  </div>
</div>
