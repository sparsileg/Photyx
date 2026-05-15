<!-- StackingWorkspace.svelte — Stacking workflow viewer-region component -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { ui } from '../stores/ui';
  import { notifications } from '../stores/notifications';
  import { consolePipe } from '../stores/consoleHistory';

  // ── State ─────────────────────────────────────────────────────────────────

  let calibrationDir  = $state<string | null>(null);

  type StackPhase = 'idle' | 'stacking' | 'stacked';
  let phase           = $state<StackPhase>('idle');
  let imageUrl        = $state<string | null>(null);
  let imageMode       = $state<'linear' | 'stretched'>('linear');
  let stackLabel      = $state('');
  let stackStats      = $state('');
  let error           = $state('');

  const SHADOW_CLIP_PRESETS = [
    { label: '-1.0', value: -1.0 },
    { label: '-1.5', value: -1.5 },
    { label: '-2.0', value: -2.0 },
    { label: '-2.5', value: -2.5 },
    { label: '-2.8', value: -2.8 },
    { label: '-3.5', value: -3.5 },
    { label: '-4.0', value: -4.0 },
  ];
  const TARGET_BG_PRESETS = [
    { label: '0.40', value: 0.40 },
    { label: '0.30', value: 0.30 },
    { label: '0.25', value: 0.25 },
    { label: '0.20', value: 0.20 },
    { label: '0.15', value: 0.15 },
    { label: '0.10', value: 0.10 },
    { label: '0.05', value: 0.05 },
  ];
  let shadowClip      = $state(-2.8);
  let targetBg        = $state(0.15);
  let stretchPending  = $state(false);
  let exporting       = $state(false);

  const hasStack   = $derived(phase === 'stacked');
  const isStacking = $derived(phase === 'stacking');

  // ── Calibration directory picker ──────────────────────────────────────────

  async function pickCalibrationDir() {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === 'string') {
        calibrationDir = selected.replace(/\\/g, '/');
      }
    } catch (e) {
      notifications.error(`Failed to open directory picker: ${e}`);
    }
  }

  function clearCalibrationDir() {
    calibrationDir = null;
  }

  // ── Stacking ──────────────────────────────────────────────────────────────

  async function runStack() {
    phase      = 'stacking';
    error      = '';
    imageUrl   = null;
    stackLabel = '';
    stackStats = '';

    notifications.running('StackFrames running…');

    try {
      let script = 'StackFrames';
      if (calibrationDir) {
        script += ` calibration_dir="${calibrationDir}"`;
      }

      const response = await invoke<{
        results: Array<{ success: boolean; message: string | null; command: string }>;
      }>('run_script', { script });

      const last = response.results[response.results.length - 1];
      if (!last?.success) {
        throw new Error(last?.message ?? 'StackFrames failed');
      }

      for (const r of response.results) {
        if (r.message) {
          consolePipe.update(q => [...q, { text: r.message!, type: 'output' as const }]);
        }
      }

      notifications.success('Stacking complete');
      phase = 'stacked';
      await loadLinear();
    } catch (e) {
      error = `${e}`;
      phase = 'idle';
      notifications.error(`StackFrames failed: ${e}`);
    }
  }

  // ── Display ───────────────────────────────────────────────────────────────

  async function loadLinear() {
    try {
      const dataUrl = await invoke<string>('get_stack_frame');
      imageUrl  = dataUrl;
      imageMode = 'linear';
      await loadSummary();
    } catch (e) {
      error = `Failed to load stack image: ${e}`;
    }
  }

  async function loadSummary() {
    try {
      const result = await invoke<{ image_url: string; summary: any }>(
        'get_autostretch_stack_frame',
        { shadowClip: null, targetBg: null }
      );
      buildLabel(result.summary);
    } catch {
      // Summary unavailable — not fatal
    }
  }

  async function applyStretch() {
    if (!hasStack) return;
    stretchPending = true;
    try {
      const result = await invoke<{ image_url: string; summary: any }>(
        'get_autostretch_stack_frame',
        { shadowClip, targetBg }
      );
      imageUrl  = result.image_url;
      imageMode = 'stretched';
      buildLabel(result.summary);
    } catch (e) {
      notifications.error(`Stretch failed: ${e}`);
    } finally {
      stretchPending = false;
    }
  }

  async function commitStretch() {
    if (!hasStack) return;
    try {
      await invoke('run_script', {
        script: `CommitStretch shadow_clip=${shadowClip} target_bg=${targetBg}`
      });
      notifications.success('Stretch committed');
      consolePipe.update(q => [...q, { text: 'Stretch committed.', type: 'output' as const }]);
    } catch (e) {
      notifications.error(`CommitStretch failed: ${e}`);
    }
  }

  function buildLabel(summary: any) {
    if (!summary) return;
    const target  = summary.target ?? 'unknown';
    const filter  = summary.filter ?? '';
    const intMin  = Math.round((summary.integration_seconds ?? 0) / 60);
    const dateStr = (summary.completed_at ?? '').slice(0, 16).replace('T', ' ');
    stackLabel = `STACKED RESULT — ${summary.stacked_frames} / ${summary.total_frames} frames — ${dateStr} UTC`;
    stackStats = [
      target,
      filter || null,
      intMin ? `${intMin}m integration` : null,
      summary.snr_improvement != null ? `SNR ~${summary.snr_improvement.toFixed(1)}×` : null,
      summary.alignment_success_rate != null
        ? `${(summary.alignment_success_rate * 100).toFixed(0)}% aligned` : null,
      summary.background_uniformity ? `bg: ${summary.background_uniformity}` : null,
    ].filter(Boolean).join('  ·  ');
  }

  // ── Stretch dropdown handlers ─────────────────────────────────────────────

  function onShadowClipChange(e: Event) {
    shadowClip = parseFloat((e.target as HTMLSelectElement).value);
    if (hasStack) applyStretch();
  }

  function onTargetBgChange(e: Event) {
    targetBg = parseFloat((e.target as HTMLSelectElement).value);
    if (hasStack) applyStretch();
  }

  // ── Export ────────────────────────────────────────────────────────────────

  async function exportXisf() {
    if (!hasStack) return;
    let destDir: string | null = null;
    try {
      const selected = await open({ directory: true, multiple: false });
      if (!selected || typeof selected !== 'string') return;
      destDir = selected.replace(/\\/g, '/');
    } catch (e) {
      notifications.error(`Failed to open directory picker: ${e}`);
      return;
    }

    exporting = true;
    notifications.running('Exporting stack…');
    try {
      await invoke('run_script', {
        script: `WriteXISF destination="${destDir}" stack=true`
      });
      notifications.success('Stack exported to XISF');
      consolePipe.update(q => [...q, {
        text: `Stack exported to ${destDir}`,
        type: 'output' as const
      }]);
    } catch (e) {
      notifications.error(`Export failed: ${e}`);
    } finally {
      exporting = false;
    }
  }

  // ── Close ─────────────────────────────────────────────────────────────────

  function close() {
    ui.showView(null);
  }

  // ── Mount — check for existing stack result ───────────────────────────────

  onMount(async () => {
    try {
      const dataUrl = await invoke<string>('get_stack_frame');
      if (dataUrl) {
        phase     = 'stacked';
        imageUrl  = dataUrl;
        imageMode = 'linear';
        await loadSummary();
      }
    } catch {
      // No existing stack — stay idle
    }
  });
</script>

<link rel="stylesheet" href="/css/stackingworkspace.css" />

<div id="sw-root">
  <div id="sw-toolbar">
    <span id="sw-title">Stacking Workspace</span>

    <div class="sw-separator"></div>

    <!-- Calibration -->
    <span class="sw-label">Cal:</span>
    {#if calibrationDir}
      <span class="sw-label-highlight" title={calibrationDir}>
        {calibrationDir.split('/').pop()}
      </span>
      <button class="sw-btn" onclick={clearCalibrationDir}>✕</button>
    {:else}
      <button class="sw-btn" onclick={pickCalibrationDir}>Browse…</button>
    {/if}

    <div class="sw-separator"></div>

    <!-- Stack -->
    <button
      class="sw-btn sw-btn-primary"
      onclick={runStack}
      disabled={isStacking}
    >
      {isStacking ? 'Stacking…' : '▶ Stack'}
    </button>

    <div class="sw-separator"></div>

    <!-- Stretch -->
    <span class="sw-label">Black:</span>
    <select
      class="sw-select"
      disabled={!hasStack || stretchPending}
      onchange={onShadowClipChange}
    >
      {#each SHADOW_CLIP_PRESETS as p}
        <option value={p.value} selected={p.value === shadowClip}>{p.label}</option>
      {/each}
    </select>

    <span class="sw-label">Background:</span>
    <select
      class="sw-select"
      disabled={!hasStack || stretchPending}
      onchange={onTargetBgChange}
    >
      {#each TARGET_BG_PRESETS as p}
        <option value={p.value} selected={p.value === targetBg}>{p.label}</option>
      {/each}
    </select>

    <button
      class="sw-btn sw-btn-commit"
      disabled={!hasStack || stretchPending}
      onclick={commitStretch}
    >Commit Stretch</button>

    <div class="sw-separator"></div>

    <!-- Export -->
    <button
      class="sw-btn sw-btn-primary"
      disabled={!hasStack || exporting}
      onclick={exportXisf}
    >{exporting ? 'Exporting…' : '↓ Export XISF'}</button>

    <!-- Linear/Stretched toggle -->
    {#if hasStack}
      <button
        class="sw-btn"
        onclick={() => imageMode === 'linear' ? applyStretch() : loadLinear()}
      >
        {imageMode === 'linear' ? 'View Stretched' : 'View Linear'}
      </button>
    {/if}

    <button class="sw-btn sw-close" onclick={close}>✕ Close</button>
  </div>

  <!-- Stats bar -->
  {#if stackStats}
    <div style="padding: 3px 12px; background: var(--card-bg); border-bottom: 1px solid var(--border-color); font-size: 11px; color: var(--primary-color); font-family: monospace; flex-shrink: 0;">
      {stackStats}
    </div>
  {/if}

  <!-- Image area -->
  <div id="sw-image-wrap">
    {#if isStacking}
      <div class="sw-status">
        <span>Stacking in progress…</span>
        <span class="sw-status-hint">This may take a moment for large frame sets</span>
      </div>
    {:else if error}
      <div class="sw-status sw-error">{error}</div>
    {:else if imageUrl}
      <img id="sw-image" src={imageUrl} alt="Stack result" />
      {#if stackLabel}
        <div id="sw-stack-label">{stackLabel}</div>
      {/if}
    {:else}
      <div class="sw-status">
        <span>No stack result yet</span>
        <span class="sw-status-hint">Optionally specify a calibration directory, then click ▶ Stack</span>
      </div>
    {/if}
  </div>
</div>
