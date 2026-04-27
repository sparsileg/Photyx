<!-- MacroEditor.svelte — Spec §8.6, Phase 5 -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';
    import { ui } from '../../stores/ui';
    import { session } from '../../stores/session';
    import { notifications } from '../../stores/notifications';
    import { consoleHistory } from '../../stores/consoleHistory';
    import { PCODE_COMMANDS } from '../../pcodeCommands';

    // ── Props ────────────────────────────────────────────────────────────────────
    // Passed in via ui.macroEditorFile — null means new/blank macro
    let filePath = $derived($ui.macroEditorFile?.path ?? null);
    let fileName = $derived($ui.macroEditorFile?.name ?? 'Untitled');

    // ── Editor state ────────────────────────────────────────────────────────────
    let macroText  = $state('');
    let fontSize   = $state(16);
    let running    = $state(false);
    let dirty      = $state(false);
    let confirmingLeave = $state(false);
    let savedPath  = $state<string | null>(null);
    let macroName  = $state('Untitled');

    // ── DOM refs ────────────────────────────────────────────────────────────────
    let textareaEl = $state<HTMLTextAreaElement | undefined>(undefined);
    let backdropEl = $state<HTMLDivElement | undefined>(undefined);

    // ── Load file when macroEditorFile changes ───────────────────────────────────
    let lastLoadedPath = '';

    $effect(() => {
        const path = $ui.macroEditorFile?.path ?? null;
        const name = $ui.macroEditorFile?.name ?? 'Untitled';
        if (path === lastLoadedPath) return;
        lastLoadedPath = path ?? '';
        macroName = name;
        savedPath = path;
        dirty = false;
        macroText = '';
        if (path) {
            readTextFile(path).then(text => {
                macroText = text;
            }).catch(() => {
                macroText = '';
            });
        }
    });

    // ── Font size controls ───────────────────────────────────────────────────────
    const FONT_MIN = 12;
    const FONT_MAX = 24;
    function decreaseFontSize() { fontSize = Math.max(FONT_MIN, fontSize - 1); }
    function increaseFontSize() { fontSize = Math.min(FONT_MAX, fontSize + 1); }

    // ── Syntax highlighting ──────────────────────────────────────────────────────
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
                const eq = tm[3].indexOf('=');
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

    // ── Run macro ────────────────────────────────────────────────────────────────
    async function runMacro() {
        if (running) return;
        const script = macroText.trim();
        if (!script) { notifications.warning('Macro editor is empty.'); return; }
        running = true;
        notifications.running('Running macro…');
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
                }
            }
            if (!anyError) notifications.success('Macro complete.');
            if (response.session_changed) {
                try {
                    const s = await invoke<{ activeDirectory: string; fileList: string[]; currentFrame: number }>('get_session');
                    session.setDirectory(s.activeDirectory ?? '');
                    session.setFileList(s.fileList);
                } catch (e) {
                    notifications.error(`Session sync failed: ${e}`);
                }
            }
            if (response.display_changed) ui.requestFrameRefresh();
        } catch (err) {
            notifications.error(`Macro failed: ${err}`);
        } finally {
            running = false;
        }
    }

    // ── Save ─────────────────────────────────────────────────────────────────────
    async function saveMacro() {
        try {
            let path = savedPath;
            if (!path) {
                // New file — use macroName to build path in Macros directory
                const dir = await invoke<string>('get_macros_dir');
                const safeName = macroName.replace(/[^a-zA-Z0-9_\- ]/g, '').trim() || 'Untitled';
                path = `${dir}/${safeName}.phs`;
            }
            await writeTextFile(path, macroText);
            savedPath = path;
            dirty = false;
            notifications.success(`Saved: ${macroName}.phs`);
        } catch (err) {
            notifications.error(`Save failed: ${err}`);
        }
    }

    async function saveAs() {
        // Prompt for a new name only — folder is always Macros directory
        const newName = window.prompt('Save macro as (name only):', macroName);
        if (!newName?.trim()) return;
        macroName = newName.trim();
        savedPath = null; // force new path
        await saveMacro();
    }

    // ── Copy from Console ────────────────────────────────────────────────────────
    function copyFromConsole() {
        const lines = $consoleHistory;
        if (!lines.length) { notifications.warning('Console history is empty.'); return; }
        const commands = lines
            .filter(l => l.type === 'input-echo')
            .map(l => l.text)
            .join('\n');
        if (!commands) { notifications.warning('No commands in console history.'); return; }
        macroText = (macroText ? macroText + '\n' : '') + commands;
        dirty = true;
        notifications.info('Console history copied to editor.');
    }

    // ── Back to Library ───────────────────────────────────────────────────────────
    function backToLibrary() {
        if (dirty) {
            confirmingLeave = true;
            return;
        }
        ui.update(s => ({ ...s, activePanel: 'macro-lib', macroEditorFile: null }));
    }

    function confirmLeave() {
        confirmingLeave = false;
        dirty = false;
        ui.update(s => ({ ...s, activePanel: 'macro-lib', macroEditorFile: null }));
    }

    function cancelLeave() {
        confirmingLeave = false;
    }

    // ── Title label ──────────────────────────────────────────────────────────────
    let titleLabel = $derived((dirty ? '● ' : '') + macroName);
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
        <button class="me-btn me-btn-run" onclick={runMacro} disabled={running}>
            {running ? '◌ Running…' : '▶ Run'}
        </button>
        <div class="me-sep"></div>
        <button class="me-btn" onclick={saveMacro}>Save</button>
        <button class="me-btn" onclick={saveAs}>Save As…</button>
        <div class="me-sep"></div>
        <button class="me-btn" onclick={copyFromConsole}>Copy from Console</button>
        <span class="me-font-label">A</span>
        <button class="me-btn me-btn-font" onclick={decreaseFontSize} disabled={fontSize <= FONT_MIN}>−</button>
        <span class="me-font-size">{fontSize}px</span>
        <button class="me-btn me-btn-font" onclick={increaseFontSize} disabled={fontSize >= FONT_MAX}>+</button>
        <span class="me-font-label me-font-label-lg">A</span>
    </div>

    {#if confirmingLeave}
        <div class="me-confirm-bar" onclick={(e) => e.stopPropagation()}>
            <span>⚠ Unsaved changes — discard and return to library?</span>
            <button class="me-confirm-btn me-confirm-yes" onclick={(e) => { e.stopPropagation(); confirmLeave(); }}>Discard</button>
            <button class="me-confirm-btn me-confirm-no" onclick={(e) => { e.stopPropagation(); cancelLeave(); }}>Cancel</button>
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
        {#if running}
            <span class="me-status-sep">·</span>
            <span class="me-running-indicator">● running</span>
        {/if}
        {#if dirty}
            <span class="me-status-sep">·</span>
            <span class="me-dirty-indicator">unsaved</span>
        {/if}
    </div>
</div>
