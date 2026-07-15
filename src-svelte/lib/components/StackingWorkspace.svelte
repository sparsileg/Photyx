<!-- StackingWorkspace.svelte — Stacking workflow viewer-region component -->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { ui } from '../stores/ui';
  import { notifications } from '../stores/notifications';
  import { pipeToConsole } from '../stores/consoleHistory';
  import { jobResult, jobOwner, progress } from '../stores/progress';
  import { runScriptAndWait, lastResultOrThrow } from '../commands';

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
      const response = await invoke<{ accepted: boolean }>('run_script', { script: 'StackFrames' });
      if (!response.accepted) {
        throw new Error('A script is already running — try again in a moment.');
      }
      // Result arrives asynchronously via the $effect below watching jobResult.
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
      const job = await runScriptAndWait(
        `CommitStretch shadow_clip=${shadowClip} target_bg=${targetBg}`,
        'stackingworkspace-commitstretch'
      );
      lastResultOrThrow(job);
      notifications.success('Stretch committed');
      pipeToConsole('Stretch committed.', 'output');
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
          pipeToConsole(r.message!, 'output');
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
      const job = await runScriptAndWait(
        `WriteXISF destination="${destDir}" stack=true`,
        'stackingworkspace-exportxisf'
      );
      const last = lastResultOrThrow(job);
      // WriteXISF reports an existing-file skip as a *successful* result
      // (overwrite=false is the default) rather than an error — surface
      // that distinctly instead of claiming the export happened.
      if (last.message?.startsWith('Skipped')) {
        notifications.warning(last.message);
        pipeToConsole(last.message, 'warning');
      } else {
        notifications.success('Stack exported to XISF');
        pipeToConsole(last.message ?? `Stack exported to ${destDir}`, 'output');
      }
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
    <div class="sw-stats-bar">
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
