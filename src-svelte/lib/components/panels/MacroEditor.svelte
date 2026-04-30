<!-- MacroEditor.svelte — Spec §8.6, Phase 9D -->
<script lang="ts">
  import { db } from '../../db';
  import { ui } from '../../stores/ui';
  import { notifications } from '../../stores/notifications';
  import { PCODE_COMMANDS } from '../../pcodeCommands';

  // ── Editor state ─────────────────────────────────────────────────────────
  let macroId      = $state<number | null>(null);
  let macroName    = $state('');
  let displayName  = $state('');
  let macroText    = $state('');
  let fontSize     = $state(16);
  let dirty        = $state(false);
  let confirmingLeave = $state(false);

  // Save As inline state
  let savingAs         = $state(false);
  let saveAsValue      = $state('');

  // ── DOM refs ──────────────────────────────────────────────────────────────
  let textareaEl = $state<HTMLTextAreaElement | undefined>(undefined);
  let backdropEl = $state<HTMLDivElement | undefined>(undefined);

  // ── Load macro when macroEditorFile changes ───────────────────────────────
  let lastLoadedId = -2; // sentinel: -2 = never loaded

  $effect(() => {
    const file = $ui.macroEditorFile;
    const incomingId = file?.id ?? null;
    // Use a stable key: null→-1 for new macros
    const key = incomingId ?? -1;
    if (key === lastLoadedId) return;
    lastLoadedId = key;

    macroId     = incomingId;
    displayName = file?.displayName ?? '';
    macroName   = file?.name ?? '';
    macroText   = file?.script ?? '';
    dirty       = false;
    confirmingLeave = false;
    savingAs    = false;
    saveAsValue = '';
  });

  // ── Font size controls ────────────────────────────────────────────────────
  const FONT_MIN = 12;
  const FONT_MAX = 24;
  function decreaseFontSize() { fontSize = Math.max(FONT_MIN, fontSize - 1); }
  function increaseFontSize() { fontSize = Math.min(FONT_MAX, fontSize + 1); }

  // ── Name derivation ───────────────────────────────────────────────────────
  function deriveName(dn: string): string {
    return dn
      .split('')
      .map(c => c === ' ' ? '-' : c)
      .filter(c => /[a-zA-Z0-9\-_]/.test(c))
      .join('');
  }

  // ── Syntax highlighting ───────────────────────────────────────────────────
  const COMMANDS = PCODE_COMMANDS;

  function escapeHtml(s: string): string {
    return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
  }

  function highlightLine(raw: string): string {
    if (/^\s*#/.test(raw)) {
      return `<span class="hl-comment">${escapeHtml(raw)}</span>`;
    }
    if (!raw.trim()) return escapeHtml(raw);
    const m = raw.match(/^(\s*)(\S+)(.*)/s);
    if (!m) return escapeHtml(raw);
    const [, lead, word, rest] = m;
    const isCmd = COMMANDS.has(word);
    let out = escapeHtml(lead);
    out += isCmd
      ? `<span class="hl-command">${escapeHtml(word)}</span>`
      : `<span class="hl-unknown">${escapeHtml(word)}</span>`;
    let remaining = rest;
    let highlighted = '';
    const tokenRe = /(\$\{?[A-Za-z_][A-Za-z0-9_]*\}?)|([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*(?:"[^"]*"|[^\s]*))|("([^"]*)")|([^\s]+)/g;
    let pos = 0;
    let tm: RegExpExecArray | null;
    while ((tm = tokenRe.exec(remaining)) !== null) {
      if (tm.index > pos) highlighted += escapeHtml(remaining.slice(pos, tm.index));
      if (tm[1]) {
        highlighted += `<span class="hl-variable">${escapeHtml(tm[1])}</span>`;
      } else if (tm[2] !== undefined && tm[3] !== undefined) {
        const eq  = tm[3].indexOf('=');
        const key = tm[2];
        const sep = tm[3].slice(0, eq + 1);
        const val = tm[3].slice(eq + 1);
        highlighted += `<span class="hl-argkey">${escapeHtml(key)}</span>`;
        highlighted += escapeHtml(sep);
        if (val.startsWith('"')) {
          highlighted += `<span class="hl-string">${escapeHtml(val)}</span>`;
        } else {
          highlighted += `<span class="hl-argval">${escapeHtml(val)}</span>`;
        }
      } else if (tm[4] !== undefined) {
        highlighted += `<span class="hl-string">${escapeHtml(tm[4])}</span>`;
      } else {
        highlighted += escapeHtml(tm[0]);
      }
      pos = tm.index + tm[0].length;
    }
    if (pos < remaining.length) highlighted += escapeHtml(remaining.slice(pos));
    out += highlighted;
    return out;
  }

  let highlighted = $derived(
    macroText.split('\n').map(highlightLine).join('\n') + '\n'
  );

  function onTextareaScroll() {
    if (backdropEl && textareaEl) {
      backdropEl.scrollTop  = textareaEl.scrollTop;
      backdropEl.scrollLeft = textareaEl.scrollLeft;
    }
  }

  function onInput() {
    dirty = true;
    onTextareaScroll();
  }

  // ── Save ──────────────────────────────────────────────────────────────────
  async function saveMacro() {
    const dn   = displayName.trim() || 'Untitled';
    const name = deriveName(dn) || 'Untitled';
    try {
      const id = await db.saveMacro(name, dn, macroText);
      macroId     = id;
      macroName   = name;
      displayName = dn;
      dirty       = false;
      // Keep ui.macroEditorFile in sync so the guard key stays stable
      ui.openMacroEditor({ id, name, displayName: dn, script: macroText });
      notifications.success(`Saved: ${dn}`);
    } catch (e) {
      notifications.error(`Save failed: ${e}`);
    }
  }

  // ── Save As ───────────────────────────────────────────────────────────────
  function startSaveAs() {
    savingAs    = true;
    saveAsValue = displayName;
  }

  function cancelSaveAs() {
    savingAs    = false;
    saveAsValue = '';
  }

  async function confirmSaveAs() {
    const dn = saveAsValue.trim();
    if (!dn) { cancelSaveAs(); return; }
    savingAs    = false;
    saveAsValue = '';
    // Treat as a new macro — clear id so save_macro does an insert
    macroId     = null;
    displayName = dn;
    macroName   = deriveName(dn) || 'Untitled';
    await saveMacro();
  }

  // ── Back to Library ───────────────────────────────────────────────────────
  function backToLibrary() {
    if (dirty) {
      confirmingLeave = true;
      return;
    }
    ui.showMacroLibrary();
  }

  function confirmLeave() {
    confirmingLeave = false;
    dirty = false;
    ui.showMacroLibrary();
  }

  function cancelLeave() {
    confirmingLeave = false;
  }

  // ── Title label ───────────────────────────────────────────────────────────
  let titleLabel = $derived((dirty ? '● ' : '') + (displayName || 'Untitled'));
</script>

<div class="macro-editor-panel expanded" style="--me-font: {fontSize}px">

  <div class="me-header">
    <span class="me-title">
      <span class="me-icon">⌨</span>
      Macro Editor —
      <span class="me-filename">{titleLabel}</span>
    </span>
    <button class="me-close-btn" onclick={backToLibrary}>← Library</button>
  </div>

  <div class="me-toolbar">
    <button class="me-btn" onclick={saveMacro}>Save</button>
    <button class="me-btn" onclick={startSaveAs}>Save As…</button>
    <span class="me-font-label">A</span>
    <button class="me-btn me-btn-font" onclick={decreaseFontSize} disabled={fontSize <= FONT_MIN}>−</button>
            <span class="me-font-size">{fontSize}px</span>
            <button class="me-btn me-btn-font" onclick={increaseFontSize} disabled={fontSize >= FONT_MAX}>+</button>
    <span class="me-font-label me-font-label-lg">A</span>
  </div>

  {#if savingAs}
    <div class="me-confirm-bar" onclick={(e) => e.stopPropagation()}>
      <span>Save as:</span>
      <input
        class="ml-rename-input"
        type="text"
        bind:value={saveAsValue}
        onkeydown={(e) => { if (e.key === 'Enter') confirmSaveAs(); if (e.key === 'Escape') cancelSaveAs(); }}
      autofocus
      />
      <span class="ml-new-derived">{deriveName(saveAsValue) || '—'}</span>
      <button class="me-confirm-btn me-confirm-yes" onclick={(e) => { e.stopPropagation(); confirmSaveAs(); }}>Save</button>
      <button class="me-confirm-btn me-confirm-no"  onclick={(e) => { e.stopPropagation(); cancelSaveAs(); }}>Cancel</button>
    </div>
  {/if}

{#if confirmingLeave}
  <div class="me-confirm-bar" onclick={(e) => e.stopPropagation()}>
    <span>⚠ Unsaved changes — discard and return to library?</span>
    <button class="me-confirm-btn me-confirm-yes" onclick={(e) => { e.stopPropagation(); confirmLeave(); }}>Discard</button>
    <button class="me-confirm-btn me-confirm-no"  onclick={(e) => { e.stopPropagation(); cancelLeave(); }}>Cancel</button>
  </div>
{/if}

<div class="me-editor-wrap">
  <div class="me-gutter" aria-hidden="true">
    {#each macroText.split('\n') as _line, i}
      <div class="me-line-num">{i + 1}</div>
    {/each}
    {#if macroText === ''}
      <div class="me-line-num">1</div>
    {/if}
  </div>
  <div class="me-code-area">
    <div class="me-backdrop" bind:this={backdropEl} aria-hidden="true">{@html highlighted}</div>
    <textarea
      class="me-textarea"
      bind:this={textareaEl}
      bind:value={macroText}
      oninput={onInput}
      onscroll={onTextareaScroll}
      spellcheck={false}
      autocomplete="off"
      autocorrect="off"
      autocapitalize="off"
      placeholder="# pcode macro"
      ></textarea>
  </div>
</div>

<div class="me-status">
  <span>{macroText.split('\n').length} lines</span>
  <span class="me-status-sep">·</span>
  <span>{macroText.split('\n').filter(l => l.trim() && !l.trim().startsWith('#')).length} commands</span>
  {#if macroName}
    <span class="me-status-sep">·</span>
    <span class="me-status-name">{macroName}</span>
  {/if}
  {#if dirty}
    <span class="me-status-sep">·</span>
    <span class="me-dirty-indicator">unsaved</span>
  {/if}
</div>
</div>
