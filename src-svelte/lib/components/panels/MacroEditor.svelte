<!-- MacroEditor.svelte — Spec §8.6, Phase 5 -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { save, open } from '@tauri-apps/plugin-dialog';
    import { writeTextFile, readTextFile } from '@tauri-apps/plugin-fs';
    import { ui } from '../../stores/ui';
    import { session } from '../../stores/session';
    import { notifications } from '../../stores/notifications';
    import { consoleHistory } from '../../stores/consoleHistory';

    // ── Editor state ────────────────────────────────────────────────────────────
    let macroText  = $state('');
    let fontSize   = $state(16);          // px; spec §9.4 default (user set to 16)
    let expanded   = $state(true);
    let running    = $state(false);
    let savedPath  = $state<string | null>(null);   // path of currently loaded file
    let dirty      = $state(false);                 // unsaved changes

    // ── DOM refs ────────────────────────────────────────────────────────────────
    let textareaEl = $state<HTMLTextAreaElement | undefined>(undefined);
    let backdropEl = $state<HTMLDivElement | undefined>(undefined);
    let scrollEl   = $state<HTMLDivElement | undefined>(undefined);

    // ── Font size controls ───────────────────────────────────────────────────────
    const FONT_MIN = 12;
    const FONT_MAX = 24;
    function decreaseFontSize() { fontSize = Math.max(FONT_MIN, fontSize - 1); }
    function increaseFontSize() { fontSize = Math.min(FONT_MAX, fontSize + 1); }

    // ── Syntax highlighting ──────────────────────────────────────────────────────
    // Rendered in a backdrop <div> behind a transparent <textarea>.

    import { PCODE_COMMANDS } from '../../pcodeCommands';
    const COMMANDS = PCODE_COMMANDS;

    function escapeHtml(s: string): string {
        return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
    }

    function highlightLine(raw: string): string {
        // Comment line
        if (/^\s*#/.test(raw)) {
            return `<span class="hl-comment">${escapeHtml(raw)}</span>`;
        }

        // Empty / whitespace-only
        if (!raw.trim()) return escapeHtml(raw);

        // Split at first whitespace to isolate command token
        const m = raw.match(/^(\s*)(\S+)(.*)/s);
        if (!m) return escapeHtml(raw);
        const [, lead, word, rest] = m;

        const isCmd = COMMANDS.has(word);
        let out = escapeHtml(lead);
        out += isCmd
            ? `<span class="hl-command">${escapeHtml(word)}</span>`
            : `<span class="hl-unknown">${escapeHtml(word)}</span>`;

        // In the rest: highlight key= pairs, $variables, quoted strings
        let remaining = rest;
        let highlighted = '';
        // Process token by token using a combined regex
        const tokenRe = /(\$\{?[A-Za-z_][A-Za-z0-9_]*\}?)|([A-Za-z_][A-Za-z0-9_]*)(\s*=\s*(?:"[^"]*"|[^\s]*))|("([^"]*)")|([^\s]+)/g;
        let pos = 0;
        let tm: RegExpExecArray | null;
        while ((tm = tokenRe.exec(remaining)) !== null) {
            // Gap text between tokens
            if (tm.index > pos) {
                highlighted += escapeHtml(remaining.slice(pos, tm.index));
            }
            if (tm[1]) {
                // $variable
                highlighted += `<span class="hl-variable">${escapeHtml(tm[1])}</span>`;
            } else if (tm[2] !== undefined && tm[3] !== undefined) {
                // key=value — split into key, =, value
                const eq = tm[3].indexOf('=');
                const eqAndVal = tm[3];
                const key = tm[2];
                const sep = eqAndVal.slice(0, eq + 1);
                const val = eqAndVal.slice(eq + 1);
                highlighted += `<span class="hl-argkey">${escapeHtml(key)}</span>`;
                highlighted += escapeHtml(sep);
                // value: quoted string vs bare
                if (val.startsWith('"')) {
                    highlighted += `<span class="hl-string">${escapeHtml(val)}</span>`;
                } else {
                    highlighted += `<span class="hl-argval">${escapeHtml(val)}</span>`;
                }
            } else if (tm[4] !== undefined) {
                // Quoted string not part of key=
                highlighted += `<span class="hl-string">${escapeHtml(tm[4])}</span>`;
            } else {
                highlighted += escapeHtml(tm[0]);
            }
            pos = tm.index + tm[0].length;
        }
        if (pos < remaining.length) {
            highlighted += escapeHtml(remaining.slice(pos));
        }
        out += highlighted;
        return out;
    }

    let highlighted = $derived(
        macroText
            .split('\n')
            .map(highlightLine)
            .join('\n') + '\n'   // trailing newline keeps backdrop height in sync
    );

    // ── Keep backdrop scroll in sync with textarea ───────────────────────────────
    function onTextareaScroll() {
        if (backdropEl && textareaEl) {
            backdropEl.scrollTop  = textareaEl.scrollTop;
            backdropEl.scrollLeft = textareaEl.scrollLeft;
        }
    }

    function onInput() {
        dirty = true;
        onTextareaScroll(); // recheck sync on every keystroke
    }

    // ── Run macro ────────────────────────────────────────────────────────────────
    async function runMacro() {
        if (running) return;
        const script = macroText.trim();
        if (!script) { notifications.warning('Macro editor is empty.'); return; }

        running = true;
        notifications.info('Running macro…');
        try {
            // run_script returns Vec<PcodeResult> as JSON array
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
            if (response.display_changed) {
                ui.requestFrameRefresh();
            }
        } catch (err) {
            notifications.error(`Macro failed: ${err}`);
        } finally {
            running = false;
        }
    }

    // ── Save ─────────────────────────────────────────────────────────────────────
    async function saveMacro() {
        try {
            const path = savedPath ?? await save({
                title: 'Save Macro',
                defaultPath: 'macro.phs',
                filters: [{ name: 'Photyx Macro', extensions: ['phs'] }],
            });
            if (!path) return;
            await writeTextFile(path, macroText);
            savedPath = path;
            dirty = false;
            notifications.success(`Saved: ${path.split(/[\\/]/).pop()}`);
        } catch (err) {
            notifications.error(`Save failed: ${err}`);
        }
    }

    async function saveAsMacro() {
        savedPath = null;   // force dialog
        await saveMacro();
    }

    // ── Load ─────────────────────────────────────────────────────────────────────
    async function loadMacro() {
        try {
            const result = await open({
                title: 'Open Macro',
                filters: [{ name: 'Photyx Macro', extensions: ['phs'] }],
                multiple: false,
            });
            const path = typeof result === 'string' ? result : null;
            if (!path) return;
            const normalizedPath = path.replace(/\\/g, '/');
            const text = await readTextFile(normalizedPath);
            macroText = text;
            savedPath = path;
            dirty = false;
            notifications.info(`Loaded: ${path.split(/[\\/]/).pop()}`);
        } catch (err) {
            notifications.error(`Load failed: ${err}`);
        }
    }

    // ── Copy from Console ────────────────────────────────────────────────────────
    // consoleHistory store holds the lines array from Console.svelte
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

    // ── Pin to Quick Launch ───────────────────────────────────────────────────────
    // Phase 5 prep: stores name + script into a quickLaunch store entry.
    // Actual QuickLaunch panel consumes this store.
    import { quickLaunch } from '../../stores/quickLaunch';

    async function pinToQuickLaunch() {
        const name = savedPath
            ? savedPath.split(/[\\/]/).pop()!.replace(/\.phs$/i, '')
            : 'Untitled';
        quickLaunch.pin({ name, script: macroText });
        notifications.success(`"${name}" pinned to Quick Launch.`);
    }

    // ── Expansion toggle ─────────────────────────────────────────────────────────
    function toggleExpanded() { expanded = !expanded; }

    // ── Title bar label ──────────────────────────────────────────────────────────
    let titleLabel = $derived(() => {
        const base = savedPath ? savedPath.split(/[\\/]/).pop()! : 'Untitled';
        return (dirty ? '● ' : '') + base;
    });
</script>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Panel root: .expanded class switches to full-width overlay mode            -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
<div class="macro-editor-panel" class:expanded style="--me-font: {fontSize}px">

    <!-- Header ────────────────────────────────────────────────────────────────── -->
    <div class="me-header" role="button" tabindex="-1">
        <span class="me-title">
            <span class="me-icon">⌨</span>
            Macro Editor
            <span class="me-filename">{titleLabel()}</span>
        </span>
        <button class="me-close-btn" onclick={() => ui.closePanel()}>✕</button>
    </div>

    <!-- Toolbar ───────────────────────────────────────────────────────────────── -->
    <div class="me-toolbar">
        <!-- Run group -->
        <button class="me-btn me-btn-run" onclick={runMacro} disabled={running} title="Run macro (Ctrl+Enter)">
            {running ? '◌ Running…' : '▶ Run'}
        </button>

        <div class="me-sep"></div>

        <!-- File group -->
        <button class="me-btn" onclick={saveMacro}   title="Save (Ctrl+S)">Save</button>
        <button class="me-btn" onclick={saveAsMacro} title="Save as new file">Save As…</button>
        <button class="me-btn" onclick={loadMacro}   title="Open .phs file">Load…</button>

        <div class="me-sep"></div>

        <!-- Utility group -->
        <button class="me-btn" onclick={copyFromConsole} title="Paste console history into editor">Copy from Console</button>
        <button class="me-btn me-btn-pin" onclick={pinToQuickLaunch} title="Add this macro to the Quick Launch panel">📌 Pin to Quick Launch</button>

        <span class="me-font-label">A</span>
        <button class="me-btn me-btn-font" onclick={decreaseFontSize} disabled={fontSize <= FONT_MIN} title="Decrease font size">−</button>
        <span class="me-font-size">{fontSize}px</span>
        <button class="me-btn me-btn-font" onclick={increaseFontSize} disabled={fontSize >= FONT_MAX} title="Increase font size">+</button>
        <span class="me-font-label me-font-label-lg">A</span>

        <div class="me-sep"></div>
        <button class="me-btn" onclick={() => ui.closePanel()} title="Collapse editor">◀ Collapse</button>
    </div>

    <!-- Editor body: syntax-highlighted backdrop + transparent textarea ───────── -->
    <div class="me-editor-wrap" bind:this={scrollEl}>
        <!-- Line numbers -->
        <div class="me-gutter" aria-hidden="true">
            {#each macroText.split('\n') as _line, i}
                <div class="me-line-num">{i + 1}</div>
            {/each}
            <!-- Pad so gutter is never shorter than 1 line -->
            {#if macroText === ''}
                <div class="me-line-num">1</div>
            {/if}
        </div>

        <!-- Code area -->
        <div class="me-code-area">
            <!-- Highlighted backdrop (aria-hidden, purely visual) -->
            <div
                class="me-backdrop"
                bind:this={backdropEl}
                aria-hidden="true"
            >{@html highlighted}</div>

            <!-- Actual editable textarea (transparent text so backdrop shows through) -->
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
                placeholder="# pcode macro&#10;SelectDirectory path=&quot;/path/to/images&quot;&#10;ReadAllFITFiles&#10;AutoStretch method=asinh"
            ></textarea>
        </div>
    </div>

    <!-- Status bar ────────────────────────────────────────────────────────────── -->
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
