<!-- StackingWorkspace.svelte — Stacking workflow viewer-region component -->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { ui } from '../stores/ui';
  import { notifications } from '../stores/notifications';
  import { consolePipe } from '../stores/consoleHistory';
  import { jobResult, jobOwner, progress } from '../stores/progress';

  // ── State ─────────────────────────────────────────────────────────────────

  const shadowClip = $derived($ui.shadowClip);
  const targetBg   = $derived($ui.targetBg);

  type StackPhase = 'idle' | 'stacking' | 'stacked';
  let phase           = $state<StackPhase>('idle');
  let imageUrl        = $state<string | null>(null);
  let stackLabel      = $state('');
  let stackStats      = $state('');
  let error           = $state('');

  let stretchPending  = $state(false);
  let exporting       = $state(false);

  const hasStack   = $derived(phase === 'stacked');
  const isStacking = $derived(phase === 'stacking');

  // ── Stacking ──────────────────────────────────────────────────────────────

  async function runStack() {
    phase      = 'stacking';
    error      = '';
    imageUrl   = null;
    stackLabel = '';
    stackStats = '';
    notifications.running('StackFrames');
    progress.set({ label: '', current: 0, total: 0 });
    jobOwner.set('stackingworkspace');
    try {
      await invoke('run_script', { script: 'StackFrames' });
    } catch (e) {
      error = `${e}`;
      phase = 'idle';
      notifications.error(`StackFrames failed: ${e}`);
      jobOwner.set(null);
    }
  }

  // ── Display ───────────────────────────────────────────────────────────────

  async function loadLinear() {
    try {
      const dataUrl = await invoke<string>('get_stack_frame');
      imageUrl  = dataUrl;
      await loadSummary();
      if ($ui.stretchMode === 'stretched') {
        await applyStretch();
      }
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

  async function applyStretch(sc = $ui.shadowClip, tb = $ui.targetBg) {
    if (!hasStack) return;
    stretchPending = true;
    try {
      const result = await invoke<{ image_url: string; summary: any }>(
        'get_autostretch_stack_frame',
        { shadowClip: sc, targetBg: tb }
      );
      imageUrl  = result.image_url;
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

  // ── React to stretch mode changes from toolbar ────────────────────────────

  //    Handle async job result for StackFrames
  $effect(() => {
    const result = $jobResult;
    const owner  = $jobOwner;
    if (!result || owner !== 'stackingworkspace') return;

    const last = result.results.at(-1);
    if (!last?.success) {
      error = last?.message ?? 'StackFrames failed';
      phase = 'idle';
      notifications.error(`StackFrames failed: ${error}`);
    } else {
      for (const r of result.results) {
        if (r.message) {
          consolePipe.update(q => [...q, { text: r.message!, type: 'output' as const }]);
        }
      }
      notifications.success('Stacking complete');
      phase = 'stacked';
      loadLinear();
    }

    jobResult.set(null);
    jobOwner.set(null);
  });

  //    React to stretch mode changes from toolbar
  $effect(() => {
    const mode = $ui.stretchMode;
    if (!hasStack) return;
    if (mode === 'stretched') {
      untrack(() => applyStretch($ui.shadowClip, $ui.targetBg));
    } else {
      untrack(() => loadLinear());
    }
  });

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

    <!-- Stack -->
    <button
      class="sw-btn sw-btn-primary"
      onclick={runStack}
      disabled={isStacking}
    >
      {isStacking ? 'Stacking…' : '▶ Stack'}
    </button>

    <div class="sw-separator"></div>

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
        <span class="sw-status-hint">Click ▶ Stack to begin</span>
      </div>
    {/if}
  </div>
</div>
