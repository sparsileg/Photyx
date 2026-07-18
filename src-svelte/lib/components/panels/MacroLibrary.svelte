<!-- MacroLibrary.svelte — Spec §8.6 -->

<script lang="ts">
  import { onMount } from 'svelte';
  import { db, type MacroRow } from '../../db';
  import { ui } from '../../stores/ui';
  import { quickLaunch } from '../../stores/quickLaunch';
  import { session } from '../../stores/session';
  import { notifications } from '../../stores/notifications';
  import { pipeToConsole } from '../../stores/consoleHistory';
  import { jobResult, jobOwner, progress } from '../../stores/progress';
  import { invoke } from '@tauri-apps/api/core';
  import { handleClientCommand } from '../../clientCommands';
  import { applyAutoStretch } from '../../commands';

  // ── State ────────────────────────────────────────────────────────────────
  let macros        = $state<MacroRow[]>([]);
  let loading       = $state(true);
  let pinned        = $state<Set<number>>(new Set());
  let confirmDelete = $state<number | null>(null);
  let pinnedWarning = $state<number | null>(null);
  let running       = $state<number | null>(null);

  // Inline rename state
  let renamingId       = $state<number | null>(null);
  let renameValue      = $state('');

  // Inline new macro state
  let creatingNew      = $state(false);
  let newDisplayName   = $state('');

  // ── Load ─────────────────────────────────────────────────────────────────
  async function loadMacros() {
    loading = true;
    try {
      macros = await db.getMacros();
    } catch (e) {
      notifications.error(`Macro Library: ${e}`);
    } finally {
      loading = false;
    }
  }

  // ── New macro ─────────────────────────────────────────────────────────────
  function startNew() {
    creatingNew    = true;
    newDisplayName = '';
  }

  function cancelNew() {
    creatingNew    = false;
    newDisplayName = '';
  }

  async function confirmNew() {
    const displayName = newDisplayName.trim();
    if (!displayName) return;
    const name = deriveName(displayName);
    if (!name) {
      notifications.error('Name produces no valid identifier.');
      return;
    }
    creatingNew = false;
    newDisplayName = '';
    ui.openMacroEditor({ id: null, name, displayName, script: '' });
  }

  // ── Edit ──────────────────────────────────────────────────────────────────
  function editMacro(macro: MacroRow) {
    ui.openMacroEditor({
      id:          macro.id,
      name:        macro.name,
      displayName: macro.display_name,
      script:      macro.script,
    });
  }

  // ── Rename ────────────────────────────────────────────────────────────────
  function startRename(macro: MacroRow) {
    renamingId  = macro.id;
    renameValue = macro.display_name;
  }

  function cancelRename() {
    renamingId  = null;
    renameValue = '';
  }

  async function confirmRename(id: number) {
    const newDisplayName = renameValue.trim();
    if (!newDisplayName) { cancelRename(); return; }
    try {
      await db.renameMacro(id, newDisplayName);
      notifications.success(`Renamed to: ${newDisplayName}`);
      renamingId = null;
      await loadMacros();
    } catch (e) {
      notifications.error(`Rename failed: ${e}`);
    }
  }

  // ── Pin ───────────────────────────────────────────────────────────────────
  function pinMacro(macro: MacroRow) {
    quickLaunch.pin({
      name:   macro.display_name,
      script: `RunMacro name="${macro.name}"`,
      icon:   '📜',
    });
    notifications.success(`Pinned: ${macro.display_name}`);
  }

  // ── Delete ────────────────────────────────────────────────────────────────
  function requestDelete(macro: MacroRow) {
    if (pinned.has(macro.id)) {
      pinnedWarning = macro.id;
      confirmDelete = null;
    } else {
      confirmDelete = macro.id;
      pinnedWarning = null;
    }
  }

  async function confirmDeleteMacro(id: number) {
    try {
      await db.deleteMacro(id);
      confirmDelete = null;
      notifications.success('Macro deleted.');
      await loadMacros();
    } catch (e) {
      notifications.error(`Delete failed: ${e}`);
    }
  }

  function cancelDelete() {
    confirmDelete = null;
    pinnedWarning = null;
  }

  // ── Run ───────────────────────────────────────────────────────────────────
  let runningMacroRef = $state<MacroRow | null>(null);

  async function runMacro(macro: MacroRow) {
    if (running === macro.id) return;
    running = macro.id;
    runningMacroRef = macro;
    notifications.running(macro.display_name);
    jobOwner.set('macro-library');
    progress.set({ label: '', current: 0, total: 0 });
    try {
      await invoke('run_script', { script: `RunMacro name="${macro.name}"` });
      // Result arrives asynchronously via the $effect watching jobResult
    } catch (e) {
      notifications.error(`Run failed: ${e}`);
      jobOwner.set(null);
      running = null;
      runningMacroRef = null;
    }
  }

  // Handle async job results addressed to the Macro Library
  $effect(() => {
    const result = $jobResult;
    const owner  = $jobOwner;
    if (!result || owner !== 'macro-library') return;

    const macro = runningMacroRef;

    // Issue 98: clear job state synchronously, immediately — before any
    // async work starts below. See the matching fix (and its full
    // explanation) in Console.svelte's equivalent effect: delaying this
    // clear behind await points reopens a race with the progress poller
    // that can cause Svelte to rerun this effect on the same still-cached
    // result and reprocess it indefinitely.
    running = null;
    runningMacroRef = null;
    jobResult.set(null);
    jobOwner.set(null);

    (async () => {
      let anyError = false;
      let lastActionData: Record<string, unknown> | null = null;

      for (const r of result.results) {
        if (!r.success) {
          // A failed result's message can be a single line (a typical plugin
          // error) or, for RunMacro, the entire accumulated output of a long
          // successful run followed by the failure summary. Pipe everything
          // but the last line to the console where it belongs; reserve the
          // notification banner for just the final summary/error line.
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
        // Issue 98: awaited, and this whole block moved into an async IIFE,
        // so multiple client commands from the same result (the singular
        // client_command below, or the client_commands array from a
        // RunMacro-wrapped macro) print in the order they actually ran
        // rather than racing based on which one's internal async work
        // (e.g. Version's getVersion() call) happens to resolve first.
        if (r.success && r.data?.client_command) {
          await handleClientCommand(r.data.client_command as string);
        }
        if (r.success && Array.isArray(r.data?.client_commands)) {
          for (const cc of r.data.client_commands as string[]) {
            await handleClientCommand(cc);
          }
        }
      }

      if (!anyError && macro) {
        notifications.success(`${macro.display_name} complete.`);
        db.incrementMacroRunCount(macro.id);
      }

      if (result.session_changed) {
        try {
          const s = await invoke<{ fileList: string[]; currentFrame: number }>('get_session');
          session.setFileList(s.fileList);
        } catch (e) {
          console.warn('MacroLibrary: session sync failed:', e);
        }
      }

      // Dispatch client actions returned by Rust — no command-name matching needed
      let autoStretched      = false;
      let annotationsRefreshed = false;
      if (!Array.isArray(result.client_actions)) {
        console.warn('MacroLibrary: client_actions was not an array:', result.client_actions, 'macro:', macro?.name);
      }
      for (const action of result.client_actions ?? []) {
        if (action === 'refresh_autostretch') {
          const shadowClip       = lastActionData?.shadow_clip      as number | undefined;
          const targetBackground = lastActionData?.target_background as number | undefined;
          applyAutoStretch(shadowClip, targetBackground);
          autoStretched = true;
        }
        if (action === 'refresh_annotations') {
          ui.refreshAnnotations();
          annotationsRefreshed = true;
        }
        if (action === 'open_keyword_modal') ui.openKeywordModal();
      }
      if (result.display_changed && !autoStretched && !annotationsRefreshed) {
        ui.requestFrameRefresh();
      }
    })();
  });

  // ── Helpers ───────────────────────────────────────────────────────────────
  function deriveName(displayName: string): string {
    return displayName
      .split('')
      .map(c => c === ' ' ? '-' : c)
      .filter(c => /[a-zA-Z0-9\-_]/.test(c))
      .join('');
  }

  function formatRunCount(macro: MacroRow): string {
    if (macro.run_count === 0) return 'Never run';
    return `Run ${macro.run_count}×`;
  }

  // ── Pinned state — sync with Quick Launch store ───────────────────────────
  $effect(() => {
    const ql = $quickLaunch;
    const pinnedNames = new Set<string>();
    for (const entry of ql) {
      const match = entry.script.match(/RunMacro name="([^"]+)"/);
      if (match) pinnedNames.add(match[1]);
    }
    pinned = new Set(
      macros
        .filter(m => pinnedNames.has(m.name))
        .map(m => m.id)
    );
  });

  onMount(loadMacros);
</script>

<div class="sliding-panel active">
  <div class="panel-header">
    <span>Macro Library</span>
    <div class="panel-header-actions">
      <button class="panel-action-btn" onclick={startNew} title="Create a new macro">New</button>
      <button class="panel-action-btn" onclick={loadMacros} title="Refresh">↻</button>
      <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
    </div>
  </div>

  {#if creatingNew}
    <div class="ml-new-bar">
      <input
        class="ml-new-input"
        type="text"
        placeholder="Display name…"
        bind:value={newDisplayName}
        onkeydown={(e) => { if (e.key === 'Enter') confirmNew(); if (e.key === 'Escape') cancelNew(); }}
      autofocus
      />
      <span class="ml-new-derived">{deriveName(newDisplayName) || '—'}</span>
      <button class="ml-confirm-yes" onclick={confirmNew}>Create</button>
      <button class="ml-confirm-no"  onclick={cancelNew}>Cancel</button>
    </div>
  {/if}

<div class="panel-body">
  {#if loading}
    <div class="ml-empty">Loading…</div>
  {:else if macros.length === 0}
    <div class="ml-empty">
      No macros found.<br/>
      Click New to create a macro.
    </div>
  {:else}
    {#each macros as macro (macro.id)}
      <div class="ml-item" title={macro.script.split('\n').filter(l => l.trim().startsWith('#')).slice(0,3).map(l => l.replace(/^#\s*/, '')).join('\n') || undefined}>
        <div class="ml-item-top">
          {#if renamingId === macro.id}
            <input
              class="ml-rename-input"
              type="text"
              bind:value={renameValue}
              onkeydown={(e) => { if (e.key === 'Enter') confirmRename(macro.id); if (e.key === 'Escape') cancelRename(); }}
            autofocus
            />
            <button class="ml-action-btn" onclick={() => confirmRename(macro.id)}>OK</button>
            <button class="ml-action-btn" onclick={cancelRename}> </button>
          {:else}
            <span class="ml-name">{macro.display_name}</span>
          {/if}
        </div>
        {#if renamingId !== macro.id}
        <div class="ml-item-middle">
          <div class="ml-item-actions">
            <button class="ml-action-btn" onclick={() => editMacro(macro)} title="Edit macro">Edit</button>
            <button class="ml-action-btn" onclick={(e) => { e.stopPropagation(); startRename(macro); }} title="Rename macro">Rename</button>
            <button class="ml-action-btn ml-delete-btn" onclick={() => requestDelete(macro)} title="Delete macro">Delete</button>
          </div>
        </div>
        {/if}
        <div class="ml-item-bottom">
          <span class="ml-size">{formatRunCount(macro)}</span>
          <div class="ml-item-actions">
            <button
              class="ml-action-btn"
              class:ml-pin-active={pinned.has(macro.id)}
              onclick={() => pinMacro(macro)}
              title="Pin to Quick Launch"
              >📌 {pinned.has(macro.id) ? 'Pinned' : 'Pin'}</button>
            <button
              class="ml-action-btn ml-run-btn"
              onclick={() => runMacro(macro)}
              disabled={running === macro.id}
              title="Run macro"
              >{running === macro.id ? '◌ Running…' : '▶ Run'}</button>
          </div>
        </div>
        {#if confirmDelete === macro.id}
          <div class="ml-confirm-bar" onclick={(e) => e.stopPropagation()}>
            <span>Delete {macro.display_name}? This cannot be undone.</span>
            <button class="ml-confirm-yes" onclick={(e) => { e.stopPropagation(); confirmDeleteMacro(macro.id); }}>Delete</button>
            <button class="ml-confirm-no"  onclick={(e) => { e.stopPropagation(); cancelDelete(); }}>Cancel</button>
          </div>
        {/if}
        {#if pinnedWarning === macro.id}
          <div class="ml-confirm-bar ml-pinned-warning" onclick={(e) => e.stopPropagation()}>
            <span>Remove from Quick Launch first.</span>
            <button class="ml-confirm-no" onclick={(e) => { e.stopPropagation(); cancelDelete(); }}>OK</button>
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>
<div class="ml-footer">
  {macros.length} macro{macros.length !== 1 ? 's' : ''}
</div>
</div>
