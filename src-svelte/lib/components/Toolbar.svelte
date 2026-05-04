<!-- Toolbar.svelte — Zoom, stretch, channel controls. Spec §8.3 -->
<script lang="ts">
  import { ui } from '../stores/ui';
  import type { ZoomLevel } from '../stores/ui';
  import { session } from '../stores/session';
  const zoomLevels: { id: ZoomLevel; label: string }[] = [
    { id: 'fit',  label: 'Fit' },
    { id: '25',   label: '25%' },
    { id: '50',   label: '50%' },
    { id: '100',  label: '100%' },
    { id: '200',  label: '200%' },
  ];
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
      {$session.activeDirectory ?? 'No directory selected'}
    </span>
  </div>
</div>
