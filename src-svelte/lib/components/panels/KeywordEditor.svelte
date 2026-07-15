<!-- KeywordEditor.svelte — Spec §8.6 -->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { ui } from '../../stores/ui';
  import { session, currentImage } from '../../stores/session';
  import { notifications } from '../../stores/notifications';

  // ── State ────────────────────────────────────────────────────────────────
  let selectedKw       = $state<string | null>(null);
  let editingCell      = $state<{ kw: string; field: 'name' | 'value' | 'comment' } | null>(null);
  let editingValue     = $state('');
  let saving           = $state(false);
  let confirmingDelete = $state<string | null>(null);
  let addingRow        = $state(false);
  let newName          = $state('');
  let newValue         = $state('');
  let newComment       = $state('');
  let newNameError     = $state('');

  // ── Sorted keyword list ──────────────────────────────────────────────────
  let keywords = $derived(
    $currentImage
      ? Object.values($currentImage.keywords).sort((a, b) => a.name.localeCompare(b.name))
      : []
  );

  // ── Reload keywords from Rust ────────────────────────────────────────────
  async function reload(silent = false) {
    try {
      const path = $session.fileList[$session.currentFrame];
      if (!path) return;
      const kws = await invoke<Record<string, { name: string; value: string; comment: string | null }>>('get_keywords');
      session.update(s => ({
        ...s,
        loadedImages: {
          ...s.loadedImages,
          [path]: { ...s.loadedImages[path], keywords: kws }
        }
      }));
      if (!silent) notifications.info('Keywords reloaded.');
    } catch (e) {
      notifications.error(`Reload failed: ${e}`);
    }
  }

  // ── Start editing a cell ─────────────────────────────────────────────────
  function startEdit(kwName: string, field: 'name' | 'value' | 'comment', current: string) {
    editingCell  = { kw: kwName, field };
    editingValue = current;
  }

  // ── Commit edit ──────────────────────────────────────────────────────────
  // ── FITS length validation ────────────────────────────────────────────────
  const MAX_COMBINED = 68;

  function validateAndTruncate(value: string, comment: string): { value: string; comment: string; warned: boolean } {
    // Strip existing quotes for length calculation
    const cleanVal = value.replace(/^'|'$/g, '').trimEnd();
    const combined = cleanVal.length + (comment.length > 0 ? 2 + comment.length : 0); // 2 for '/ '
    if (combined <= MAX_COMBINED) return { value, comment, warned: false };
    // Truncate comment to fit
    const maxComment = MAX_COMBINED - cleanVal.length - 2;
    const truncated = maxComment > 0 ? comment.slice(0, maxComment) : '';
    notifications.warning(
      `FITS limit: value + comment must not exceed ${MAX_COMBINED} characters. ` +
        `Comment truncated to ${truncated.length} characters.`
    );
    return { value, comment: truncated, warned: true };
  }

  // ── Commit edit ──────────────────────────────────────────────────────────
  async function commitEdit() {
    if (!editingCell) return;
    const { kw, field } = editingCell;
    const kwEntry = $currentImage?.keywords[kw];
    if (!kwEntry) { editingCell = null; return; }

    try {
      if (field === 'value') {
        const { value, comment } = validateAndTruncate(editingValue, kwEntry.comment ?? '');
        await invoke('dispatch_command', {
          request: { command: 'ModifyKeyword', args: {
            name:    kw,
            value:   value,
            comment: comment,
            scope:   'current',
          }}
        });
      } else {
        // comment — ModifyKeyword with existing value and new comment
        const { value, comment } = validateAndTruncate(kwEntry.value, editingValue);
        await invoke('dispatch_command', {
          request: { command: 'ModifyKeyword', args: {
            name:    kw,
            value:   value,
            comment: comment,
            scope:   'current',
          }}
        });
      }
      editingCell = null;
      await reload(true);
    } catch (e) {
      notifications.error(`Edit failed: ${e}`);
      editingCell = null;
    }
  }

  function cancelEdit() {
    editingCell = null;
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter')  { e.preventDefault(); commitEdit(); }
    if (e.key === 'Escape') { e.preventDefault(); cancelEdit(); }
  }

  // ── Add keyword ──────────────────────────────────────────────────────────
  function startAddRow() {
    addingRow    = true;
    newName      = '';
    newValue     = '';
    newComment   = '';
    newNameError = '';
  }

  function cancelAddRow() {
    addingRow    = false;
    newName      = '';
    newValue     = '';
    newComment   = '';
    newNameError = '';
  }

  async function commitAddRow() {
    const name = newName.trim().toUpperCase();
    if (!name) {
      newNameError = 'Name is required.';
      return;
    }
    if (name.length > 8) {
      newNameError = `Must be 8 characters or less (has ${name.length}).`;
      return;
    }
    if (!/^[A-Z0-9_-]+$/.test(name)) {
      newNameError = 'Letters, digits, hyphens, and underscores only.';
      return;
    }
    const { value, comment } = validateAndTruncate(newValue.trim(), newComment.trim());
    try {
      await invoke('dispatch_command', {
        request: { command: 'AddKeyword', args: { name, value, comment, scope: 'current' } }
      });
      await reload();
      notifications.success(`Added: ${name}`);
      cancelAddRow();
    } catch (e) {
      notifications.error(`Add failed: ${e}`);
    }
  }

  function onAddKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter')  { e.preventDefault(); commitAddRow(); }
    if (e.key === 'Escape') { e.preventDefault(); cancelAddRow(); }
  }

  // ── Delete keyword ───────────────────────────────────────────────────────
  function startDelete() {
    if (!selectedKw) { notifications.warning('Select a keyword to delete.'); return; }
    confirmingDelete = selectedKw;
  }

  function cancelDelete() {
    confirmingDelete = null;
  }

  async function confirmDeleteKeyword() {
    if (!confirmingDelete) return;
    const name = confirmingDelete;
    try {
      await invoke('dispatch_command', {
        request: { command: 'DeleteKeyword', args: { name, scope: 'current' } }
      });
      selectedKw = null;
      confirmingDelete = null;
      await reload();
      notifications.success(`Deleted: ${name}`);
    } catch (e) {
      confirmingDelete = null;
      notifications.error(`Delete failed: ${e}`);
    }
  }

  // ── Write changes to disk ────────────────────────────────────────────────
  async function writeChanges() {
    saving = true;
    notifications.running('Writing changes…');
    try {
      const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
        'dispatch_command',
        { request: { command: 'WriteFrame', args: {} } }
      );
      if (result.success) {
        notifications.success('Changes written to disk.');
      } else {
        notifications.error(result.error ?? 'WriteCurrent failed');
      }
    } catch (e) {
      notifications.error(`Write failed: ${e}`);
    } finally {
      saving = false;
    }
  }
</script>

<div class="sliding-panel active">
  <div class="panel-header">
    <span>Keyword Editor</span>
    <span class="panel-close" onclick={() => ui.closePanel()}>✕</span>
  </div>

  <div class="kw-actions">
    <button class="kw-btn" onclick={startAddRow} disabled={addingRow}>+ Add</button>
    <button class="kw-btn" onclick={startDelete} disabled={!selectedKw}>− Delete</button>
    <button class="kw-btn" onclick={() => reload()}>⟳ Reload</button>
    <button class="kw-btn kw-btn-write" onclick={writeChanges} disabled={saving}>
      {saving ? '◌ Writing…' : '💾 Write Changes'}
    </button>
  </div>

  {#if confirmingDelete}
    <div class="kw-confirm-bar" onclick={(e) => e.stopPropagation()}>
      <span>Delete {confirmingDelete}? This cannot be undone.</span>
      <button class="kw-confirm-yes" onclick={(e) => { e.stopPropagation(); confirmDeleteKeyword(); }}>Delete</button>
      <button class="kw-confirm-no"  onclick={(e) => { e.stopPropagation(); cancelDelete(); }}>Cancel</button>
    </div>
  {/if}

  <div class="panel-body kw-panel-body">
    <table class="keyword-table">
      <thead>
        <tr>
          <th>Keyword</th>
          <th>Value</th>
          <th>Comment</th>
        </tr>
      </thead>
      <tbody>
        {#if addingRow}
          <tr class="kw-add-row">
            <td class="kw-name">
              <input
                class="kw-input"
                placeholder="NAME"
                maxlength="8"
                bind:value={newName}
                onkeydown={onAddKeyDown}
                autofocus
              />
              {#if newNameError}
                <div class="kw-add-error">{newNameError}</div>
              {/if}
            </td>
            <td class="kw-value">
              <input
                class="kw-input"
                placeholder="value"
                bind:value={newValue}
                onkeydown={onAddKeyDown}
              />
            </td>
            <td class="kw-comment">
              <div class="kw-add-comment-wrap">
                <input
                  class="kw-input"
                  placeholder="comment (optional)"
                  bind:value={newComment}
                  onkeydown={onAddKeyDown}
                />
                <button class="kw-add-confirm" title="Add keyword" onclick={commitAddRow}>✓</button>
                <button class="kw-add-cancel"  title="Cancel" onclick={cancelAddRow}>✕</button>
              </div>
            </td>
          </tr>
        {/if}
        {#if !$currentImage}
          <tr><td colspan="3" class="kw-empty">No image loaded</td></tr>
        {:else if keywords.length === 0 && !addingRow}
          <tr><td colspan="3" class="kw-empty">No keywords</td></tr>
        {:else}
          {#each keywords as kw}
            <tr
              class:kw-selected={selectedKw === kw.name}
              onclick={() => selectedKw = kw.name}
              >
              <!-- Name — read only; delete and re-add to rename -->
              <td class="kw-name">
                <span>{kw.name}</span>
              </td>
              <!-- Value -->
              <td class="kw-value" ondblclick={() => startEdit(kw.name, 'value', kw.value)}>
                {#if editingCell?.kw === kw.name && editingCell?.field === 'value'}
                  <input
                    class="kw-input"
                    bind:value={editingValue}
                    onblur={commitEdit}
                    onkeydown={onKeyDown}
                    autofocus
                    />
                  {:else}
                    <span>{kw.value}</span>
                  {/if}
                </td>
              <!-- Comment -->
              <td class="kw-comment" ondblclick={() => startEdit(kw.name, 'comment', kw.comment ?? '')}>
                {#if editingCell?.kw === kw.name && editingCell?.field === 'comment'}
                  <input
                    class="kw-input"
                    bind:value={editingValue}
                    onblur={commitEdit}
                    onkeydown={onKeyDown}
                    autofocus
                    />
                  {:else}
                    <span>{kw.comment ?? ''}</span>
                  {/if}
                </td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>
  </div>
</div>
