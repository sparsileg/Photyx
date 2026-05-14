<!-- StackResult.svelte — Stack result viewer-region component. Stacking doc §3.3 -->
<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { ui } from '../stores/ui';
  import { notifications } from '../stores/notifications';
  import { consolePipe } from '../stores/consoleHistory';

  let imageUrl   = $state<string | null>(null);
  let loading    = $state(true);
  let error      = $state('');
  let stackLabel = $state('');
  let stackStats = $state('');

  interface StackSummary {
    stacked_frames:         number;
    total_frames:           number;
    snr_improvement:        number;
    alignment_success_rate: number;
    background_uniformity:  string;
    target:                 string | null;
    filter:                 string | null;
    integration_seconds:    number;
    completed_at:           string;
  }

  interface StackFrameResult {
    image_url: string;
    summary:   StackSummary | null;
  }

  async function load() {
    loading = true;
    error   = '';
    try {
      const result = await invoke<StackFrameResult>('get_autostretch_stack_frame');
      imageUrl = result.image_url;

      const s = result.summary;
      if (s) {
        const target  = s.target ?? 'unknown';
        const filter  = s.filter ?? '';
        const intMin  = Math.round(s.integration_seconds / 60);
        const dateStr = s.completed_at.slice(0, 16).replace('T', ' ');
        stackLabel = `STACKED RESULT — ${s.stacked_frames} frames — ${dateStr} UTC`;
        stackStats = [
          target,
          filter || null,
          `${intMin}m integration`,
          `SNR ~${s.snr_improvement.toFixed(1)}×`,
          `${(s.alignment_success_rate * 100).toFixed(0)}% aligned`,
          `bg: ${s.background_uniformity}`,
        ].filter(Boolean).join('  ·  ');
      }
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  }

  async function clearStack() {
    try {
      await invoke('run_script', { script: 'ClearStack' });
      consolePipe.update(q => [...q, { text: 'Stack result cleared.', type: 'output' as const }]);
    } catch (e) {
      notifications.error(`ClearStack failed: ${e}`);
    }
    ui.showView(null);
  }

  onMount(() => { load(); });

  let lastActiveView = '';
  $effect(() => {
    const v = $ui.activeView;
    if (v === 'stackResult' && v !== lastActiveView) {
      lastActiveView = v;
      load();
    } else if (v !== 'stackResult') {
      lastActiveView = v ?? '';
    }
  });
</script>

<div id="sr-root">
  <div id="sr-toolbar">
    <span id="sr-title">Stack Result</span>
    {#if stackStats}
      <span class="sr-label-highlight">{stackStats}</span>
    {/if}
    <button class="sr-btn" onclick={load}>↻ Refresh</button>
    <button class="sr-btn sr-close" onclick={clearStack}>✕ Close</button>
  </div>

  <div id="sr-image-wrap">
    {#if loading}
      <div class="sr-status">Loading stack result…</div>
    {:else if error}
      <div class="sr-status sr-error">{error}</div>
    {:else if imageUrl}
      <img id="sr-image" src={imageUrl} alt="Stacked result" />
      {#if stackLabel}
        <div id="sr-stack-label">{stackLabel}</div>
      {/if}
    {:else}
      <div class="sr-status">No stack result available. Run StackFrames first.</div>
    {/if}
  </div>
</div>
