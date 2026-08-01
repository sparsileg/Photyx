<!-- QuickLaunch.svelte   Quick Launch bar. -->

<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { notifications } from '../stores/notifications';
  import { pipeToConsole } from '../stores/consoleHistory';
  import { ui } from '../stores/ui';
  import { quickLaunch } from '../stores/quickLaunch';
  import { session } from '../stores/session';
  import { applyAutoStretch, runScript } from '../commands';
  import { progress, type JobResult } from '../stores/progress';
  import { extractRunningLabel } from '../pcode';
  import { handleClientCommand } from '../clientCommands';

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

  // ── Process job results from a run entry ────────────────────────────────
  async function processJobResult(result: JobResult) {
    let anyError = false;
    let lastActionData: Record<string, unknown> | null = null;
    let autoStretched = false;

    for (const r of result.results) {
      if (!r.success) {
        const lines = (r.message ?? 'error').split('\n').filter(Boolean);
        const summaryLine = lines.length > 0 ? lines[lines.length - 1] : 'error';
        lines.slice(0, -1).forEach(line => pipeToConsole(line, 'success'));
        notifications.error(`${r.command}: ${summaryLine}`);
        anyError = true;
      } else if (r.message) {
        r.message.split('\n').forEach(line => {
          if (line) pipeToConsole(line, 'success');
        });
      }
      if (r.data) lastActionData = r.data;
      if (r.success && r.data?.client_command) {
        await handleClientCommand(r.data.client_command as string);
      }
      if (r.success && Array.isArray(r.data?.client_commands)) {
        for (const cc of r.data.client_commands as string[]) {
          await handleClientCommand(cc);
        }
      }
    }

    if (result.session_changed) {
      try {
        const s = await invoke<{ fileList: string[]; currentFrame: number }>('get_session');
        session.setFileList(s.fileList);
        session.setCurrentFrame(s.currentFrame);
      } catch (e) {
        notifications.error(`Session sync failed: ${e}`);
      }
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
  }

  // ── Run entry ─────────────────────────────────────────────────────────────
  async function runEntry(script: string) {
    const firstLine = script.trim().split('\n')[0].trim();

    notifications.running(extractRunningLabel(firstLine));
    try {
      const result = await runScript(script);
      await processJobResult(result);
    } catch (err) {
      notifications.error(`Quick Launch error: ${err}`);
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
    style="--ql-menu-x: {contextMenu.x}px; --ql-menu-y: {contextMenu.y}px;"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="ql-context-item ql-context-remove" onclick={removeEntry}>Remove from Quick Launch</div>
  </div>
{/if}
