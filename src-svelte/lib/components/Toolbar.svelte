<!-- Toolbar.svelte — Zoom, stretch, channel controls. Spec §8.3 -->
<script lang="ts">
  import { ui } from '../stores/ui';
  import type { ZoomLevel } from '../stores/ui';
  import { session, directoryCount } from '../stores/session';
  import { settings } from '../stores/settings';
  import { invoke } from '@tauri-apps/api/core';
  import { MIN_FONT_SIZE, MAX_FONT_SIZE, FONT_SIZE_STEP } from '../settings/constants';

  // ── Stretch sliders ───────────────────────────────────────────────────────

  let localShadowClip = $state($ui.shadowClip);
  let localTargetBg   = $state($ui.targetBg);
  let stretchDebounce: ReturnType<typeof setTimeout> | null = null;
  let stretchInFlight = false;
  let stretchPending: { shadowClip: number; targetBg: number } | null = null;

  async function applyStretch(shadowClip: number, targetBg: number) {
    if (stretchInFlight) {
      stretchPending = { shadowClip, targetBg };
      return;
    }
    stretchInFlight = true;
    try {
      if ($ui.activeView === 'stackingWorkspace') {
        const result = await invoke<{ image_url: string; summary: any }>(
          'get_autostretch_stack_frame',
          { shadowClip, targetBg }
        );
        ui.setStackImage(result.image_url);
      } else if ($session.fileList.length > 0) {
        const url = await invoke<string>(
          'get_autostretch_frame',
          { shadowClip, targetBg }
        );
        ui.setAutostretchFrame(url);
      }
    } catch (e) {
      console.error('Stretch failed:', e);
    } finally {
      stretchInFlight = false;
      if (stretchPending) {
        const p = stretchPending;
        stretchPending = null;
        await applyStretch(p.shadowClip, p.targetBg);
      }
    }
  }

  function onStretchChange(shadowClip: number, targetBg: number) {
    if ($ui.stretchMode !== 'stretched') return;
    if (stretchDebounce) clearTimeout(stretchDebounce);
    stretchDebounce = setTimeout(() => {
      ui.setStretchParams(shadowClip, targetBg);
      applyStretch(shadowClip, targetBg);
    }, 400);
  }

  async function toggleStretchMode() {
    const next = $ui.stretchMode === 'linear' ? 'stretched' : 'linear';
    ui.setStretchMode(next);
    if (next === 'stretched') {
      await applyStretch($ui.shadowClip, $ui.targetBg);
    } else {
      if ($ui.activeView === 'stackingWorkspace') {
        // Workspace will react to stretchMode change and reload linear
      } else {
        ui.setAutostretchFrame(null);
        ui.requestFrameRefresh();
      }
    }
  }

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
  <div class="toolbar-stretch-group">
    <button
      class="toolbar-btn"
      class:active={$ui.stretchMode === 'stretched'}
      onclick={toggleStretchMode}
    >{$ui.stretchMode === 'stretched' ? 'Stretched' : 'Linear'}</button>
    <span class="toolbar-label">Black</span>
    <input
      type="range"
      class="toolbar-stretch-slider"
      min="-4.0"
      max="-1.0"
      step="0.1"
      bind:value={localShadowClip}
      oninput={() => onStretchChange(localShadowClip, localTargetBg)}
    />
    <span class="toolbar-stretch-value">{localShadowClip.toFixed(1)}</span>
    <span class="toolbar-label">Bg</span>
    <input
      type="range"
      class="toolbar-stretch-slider"
      min="0.05"
      max="0.40"
      step="0.01"
      bind:value={localTargetBg}
      oninput={() => onStretchChange(localShadowClip, localTargetBg)}
    />
    <span class="toolbar-stretch-value">{localTargetBg.toFixed(2)}</span>
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
