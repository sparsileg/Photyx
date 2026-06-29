<!-- QuickLaunch.svelte   Quick Launch bar. -->

<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { notifications } from '../stores/notifications';
  import { pipeToConsole } from '../stores/consoleHistory';
  import { ui } from '../stores/ui';
  import { quickLaunch } from '../stores/quickLaunch';
  import { session } from '../stores/session';
  import { applyAutoStretch } from '../commands';
  import { jobResult, jobOwner, progress } from '../stores/progress';

  // ── Context menu state ────────────────────────────────────────────────────
  let contextMenu = $state<{ x: number; y: number; id: string } | null>(null);

  function onContextMenu(e: MouseEvent, entry: { id: string; protected?: boolean }) {
    e.preventDefault();
    if (entry.protected) return;
    contextMenu = { x: e.clientX, y: e.clientY, id: entry.id };
  }

  function removeEntry() {
    if (contextMenu) {
      quickLaunch.remove(contextMenu.id);
      contextMenu = null;
    }
  }

  function dismissContext() {
    contextMenu = null;
  }

  // ── Handle async job results addressed to quick launch ────────────────────
  $effect(() => {
    const result = $jobResult;
    const owner  = $jobOwner;
    if (!result || owner !== 'quicklaunch') return;

    let anyError = false;
    let lastActionData: Record<string, unknown> | null = null;
    let autoStretched = false;

    for (const r of result.results) {
      if (!r.success) {
        notifications.error(`${r.command}: ${r.message ?? 'error'}`);
        anyError = true;
      } else if (r.message) {
        r.message.split('\n').forEach(line => {
          if (line) pipeToConsole(line, 'success');
        });
      }
      if (r.data) lastActionData = r.data;
    }

    if (result.session_changed) {
      invoke<{ fileList: string[]; currentFrame: number }>('get_session').then(s => {
        session.setFileList(s.fileList);
      }).catch(e => {
        notifications.error(`Session sync failed: ${e}`);
      });
    }

    for (const action of result.client_actions ?? []) {
      if (action === 'refresh_autostretch') {
        const shadowClip       = lastActionData?.shadow_clip      as number | undefined;
        const targetBackground = lastActionData?.target_background as number | undefined;
        applyAutoStretch(shadowClip, targetBackground).then(() => {
          autoStretched = true;
          if (result.display_changed && !autoStretched) ui.requestFrameRefresh();
        });
      }
      if (action === 'refresh_annotations') ui.refreshAnnotations();
      if (action === 'open_keyword_modal')  ui.openKeywordModal();
    }

    if (result.display_changed && !autoStretched) {
      ui.requestFrameRefresh();
    }

    if (!anyError) notifications.success('Done.');

    // Clear job state
    jobResult.set(null);
    jobOwner.set(null);
  });

  // ── Run entry ─────────────────────────────────────────────────────────────
  async function runEntry(script: string) {
    const firstLine = script.trim().split('\n')[0].trim();
    notifications.running(firstLine);
    jobOwner.set('quicklaunch');
    progress.set({ label: '', current: 0, total: 0 });

    try {
      await invoke('run_script', { script });
      // Result arrives asynchronously via the $effect watching jobResult
    } catch (err) {
      notifications.error(`Quick Launch error: ${err}`);
      jobOwner.set(null);
    }
  }
</script>

<svelte:window onclick={dismissContext} />

<div id="quick-launch">
  <div id="ql-buttons">
    {#each $quickLaunch as entry (entry.id)}
      <button
        class="ql-btn"
        onclick={() => runEntry(entry.script)}
        oncontextmenu={(e) => onContextMenu(e, entry)}
      >
        {#if entry.icon}<span class="ql-icon">{entry.icon}</span>{/if}
        {entry.name}
      </button>
    {/each}
  </div>
</div>

{#if contextMenu}
  <div
    class="ql-context-menu"
    style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="ql-context-item ql-context-remove" onclick={removeEntry}>Remove from Quick Launch</div>
  </div>
{/if}
