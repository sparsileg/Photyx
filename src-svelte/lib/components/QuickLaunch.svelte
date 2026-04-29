<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { notifications } from '../stores/notifications';
  import { pipeToConsole } from '../stores/consoleHistory';
  import { ui } from '../stores/ui';
  import { quickLaunch } from '../stores/quickLaunch';
  import { session } from '../stores/session';
  import { applyAutoStretch } from '../commands';

  // ── Context menu state ───────────────────────────────────────────────────
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

  // ── Run entry ────────────────────────────────────────────────────────────
  async function runEntry(script: string) {
    try {
      const response = await invoke<{
        results: Array<{ line_number: number; command: string; success: boolean; message: string | null }>;
        session_changed: boolean;
        display_changed: boolean;
      }>('run_script', { script });

      let anyError = false;
      for (const r of response.results) {
        if (!r.success) {
          notifications.error(`${r.command}: ${r.message ?? 'error'}`);
          anyError = true;
        } else if (r.message) {
          r.message.split('\n').forEach(line => {
            if (line) pipeToConsole(line, 'success');
          });
        }
        if (r.command.toLowerCase() === 'listkeywords' && r.success) {
          ui.openKeywordModal();
        }
      }
      if (!anyError) notifications.success('Done.');
      let autoStretched = false;
      for (const r of response.results) {
        if (r.command.toLowerCase() === 'computefwhm' && r.success) {
          ui.refreshAnnotations();
        }
        if (r.command.toLowerCase() === 'autostretch' && r.success) {
          await applyAutoStretch();
          autoStretched = true;
        }
      }
      if (response.session_changed) {
        const s = await invoke<{ activeDirectory: string; fileList: string[]; currentFrame: number }>('get_session');
        session.setDirectory(s.activeDirectory ?? '');
        session.setFileList(s.fileList);
      }
      if (response.display_changed && !autoStretched) {
        ui.requestFrameRefresh();
      }
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
    style="left: {contextMenu.x}px; top: {contextMenu.y}px;"
    onclick={(e) => e.stopPropagation()}
    >
    <div class="ql-context-item ql-context-remove" onclick={removeEntry}>Remove from Quick Launch</div>
  </div>
{/if}
