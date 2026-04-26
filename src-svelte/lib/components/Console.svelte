<!-- Console.svelte — pcode interactive console. Spec §8.9, §7.9 -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { session } from '../stores/session';
    import { notifications } from '../stores/notifications';
    import { ui } from '../stores/ui';
    import { consoleHistory, consolePipe } from '../stores/consoleHistory';

    interface ConsoleLine {
        id: number;
        text: string;
        type: 'input-echo' | 'output' | 'error' | 'warning' | 'success' | 'info';
    }

    let lines = $state<ConsoleLine[]>([
        { id: 0, text: 'Photyx pcode console  v1.0', type: 'info' },
        { id: 1, text: 'Type Help for a command list.', type: 'info' },
    ]);
    let inputValue = $state('');
    let tabHint = $state('');
    let outputEl = $state<HTMLDivElement>();
    let inputEl = $state<HTMLInputElement>();

    let history: string[] = [];
    let historyIdx = -1;
    let pendingInput = '';
    let nextId = 2;

    import { PCODE_COMMANDS } from '../pcodeCommands';

    // Watch for external console output
    $effect(() => {
        const line = $consolePipe;
        if (line) {
            append(line.text, line.type);
            consolePipe.set(null);
        }
    });
    const ALL_COMMANDS = [...PCODE_COMMANDS].sort();

    const ARG_HINTS: Record<string, string> = {
        addkeyword:         'name=  value=  comment=',
        autostretch:        'shadowClip=  targetBackground=',
        binimage:           'factor=',
        blinksequence:      'fps=',
        copykeyword:        'from=  to=',
        cropimage:          'x=  y=  width=  height=',
        debayerimage:       'method=  pattern=',
        deletekeyword:      'name=',
        filterbykeyword:    'name=  value=',
        gethistogram:       '',
        getimageproperty:   'property=',
        getsessionproperty: 'property=',
        modifykeyword:      'name=  value= comment=',
        runmacro:           'filename=',
        selectdirectory:    'path=',
        set:                '<varname> = <value>',
        setzoom:            'level=',
        writeallfitfiles:   'destination=  overwrite=',
        readallfiles:       '',
        writecurrentfiles:  '',
        writealltifffiles:  'destination=  overwrite=',
        writeallxisffiles:  'destination=  overwrite=',
        writefit:           'destination=  overwrite=',
        writetiff:          'destination=  overwrite=',
        writexisf:          'destination=  overwrite=  compress=',
        writecurrent:       '',
        readfit:            '',
        readtiff:           '',
        readxisf:           '',
        readall:            '',
        addkeyword:         'name=  value=  comment=  scope=',
        deletekeyword:      'name=  scope=',
        modifykeyword:      'name=  value=  comment=  scope=',
        movefile:           'destination=',
        setframe:           'index=',
        log:                'path=  append=',
        countfiles:         '',
        print:              'message=',
        assert:             'expression=',
        writejpeg:          'filename=  destination=  quality=',
        writepng:           'filename=  destination=',
    };

    function append(text: string, type: ConsoleLine['type']) {
        lines = [...lines, { id: nextId++, text, type }];
        consoleHistory.set(lines);
        setTimeout(() => {
            if (outputEl) outputEl.scrollTop = outputEl.scrollHeight;
        }, 0);
    }

    function tokenize(line: string): { command: string; args: Record<string, string> } | null {
        line = line.trim();
        if (!line || line.startsWith('#')) return null;

        const firstSpace = line.search(/\s/);
        const command = firstSpace === -1 ? line : line.slice(0, firstSpace);
        const rest = firstSpace === -1 ? '' : line.slice(firstSpace + 1).trim();

        const args: Record<string, string> = {};
        const argRe = /([A-Za-z_][A-Za-z0-9_]*)=(?:"([^"]*)"|(\S+))/g;
        let match;
        while ((match = argRe.exec(rest)) !== null) {
            args[match[1].toLowerCase()] = match[2] !== undefined ? match[2] : match[3];
        }

        return { command, args };
    }

    const CLIENT_COMMANDS: Record<string, (args: Record<string, string>) => boolean> = {
        pwd: () => {
            const dir = $session.activeDirectory ?? '(no directory selected)';
            append(dir, 'output');
            return true;
        },
        help: () => {
            append('Photyx pcode v1.0  —  commands:', 'output');
            append('  File:     SelectDirectory ListFiles FilterByKeyword', 'output');
            append('  I/O:      ReadFIT ReadXISF ReadTIFF ReadAll', 'output');
            append('            WriteFIT WriteXISF WriteTIFF WriteCurrent WritePNG WriteJPEG', 'output');
            append('  Keyword:  AddKeyword DeleteKeyword ModifyKeyword CopyKeyword ListKeywords GetKeyword', 'output');
            append('  Query:    GetImageProperty GetSessionProperty Test', 'output');
            append('  Process:  AutoStretch CropImage BinImage DebayerImage', 'output');
            append('  View:     BlinkSequence CacheFrames SetZoom', 'output');
            append('  Analysis: ComputeFWHM CountStars ComputeEccentricity MedianValue ContourPlot', 'output');
            append('  Script:   Set Print Echo CountFiles RunMacro', 'output');
            append('  Console:  pwd Help Clear Version', 'output');
            return true;
        },
        clear: () => {
            lines = [];
            return true;
        },
        version: () => {
            append('Photyx 1.0.0-dev  |  pcode v1.0  |  Tauri + Svelte + Rust', 'output');
            return true;
        },
        showanalysisgraph: () => {
            ui.showView('analysisGraph');
            return true;
        },
        showanalysisresults: () => {
            ui.showView('analysisResults');
            return true;
        },
        clearannotations: () => {
            ui.clearAnnotations();
            return true;
        },
    };

    async function dispatch(raw: string) {
        const parsed = tokenize(raw);
        if (!parsed) return;

        const cmdLower = parsed.command.toLowerCase();

        if (CLIENT_COMMANDS[cmdLower]) {
            CLIENT_COMMANDS[cmdLower](parsed.args);
            return;
        }

        try {
            const response = await invoke<{
                success: boolean;
                output: string | null;
                error: string | null;
            }>('dispatch_command', {
                request: {
                    command: parsed.command,
                    args: parsed.args,
                }
            });

            if (response.success) {
                if (response.output) {
                    response.output.split('\n').forEach(line => {
                        if (line) append(line, 'success');
                    });
                }
                await syncSessionState(cmdLower, parsed.args, response.output);
            } else {
                append(response.error ?? 'Unknown error', 'error');
                notifications.error(response.error ?? 'Unknown error');
            }
        } catch (err) {
            const msg = `Invoke error: ${err}`;
            append(msg, 'error');
            notifications.error(msg);
        }
    }

    async function syncSessionState(cmd: string, args: Record<string, string>, output: string | null) {
        if (cmd === 'selectdirectory' && args.path) {
            session.setDirectory(args.path);
            session.setFileList([]);
            session.update(s => ({ ...s, loadedImages: {} }));
            ui.requestViewerClear();
            notifications.info(`Directory: ${args.path}`);
        }
        if (cmd === 'clearsession') {
            session.setDirectory('');
            session.setFileList([]);
            session.setCurrentFrame(0);
            session.update(s => ({ ...s, loadedImages: {} }));
            ui.clearViewer();
        }
        if (cmd === 'readallfitfiles' || cmd === 'readallxisffiles' || cmd === 'readalltifffiles'
            || cmd === 'readallfiles' || cmd === 'readfit' || cmd === 'readtiff'
            || cmd === 'readxisf' || cmd === 'readall' || cmd === 'runmacro') {
            if (output) notifications.success(output);
            try {
                const s = await invoke<{ activeDirectory: string; fileList: string[]; currentFrame: number }>('get_session');
                session.setDirectory(s.activeDirectory ?? '');
                session.setFileList(s.fileList);
            } catch (e) {
                notifications.error(`Session sync failed: ${e}`);
            }
        }
        if (cmd === 'autostretch' || cmd === 'linearstretch' || cmd === 'histogramequalization') {
            ui.requestFrameRefresh();
        }
        if (cmd === 'computefwhm') {
            ui.refreshAnnotations();
        }
        if (cmd === 'setframe' || cmd === 'autostretch') {
            ui.clearAnnotations();
        }
    }

    function submit() {
        const raw = inputValue.trim();
        if (!raw) return;

        append(raw, 'input-echo');

        if (history[0] !== raw) history.unshift(raw);
        if (history.length > 500) history.length = 500;
        historyIdx = -1;
        pendingInput = '';
        inputValue = '';
        tabHint = '';

        // Fire and forget — don't await so UI stays responsive during execution
        dispatch(raw).catch(err => append(`Error: ${err}`, 'error'));
    }

    function onKeyDown(e: KeyboardEvent) {
        switch (e.key) {
            case 'Enter':
                e.preventDefault();
                submit();
                break;
            case 'ArrowUp':
                e.preventDefault();
                navigateHistory(1);
                break;
            case 'ArrowDown':
                e.preventDefault();
                navigateHistory(-1);
                break;
            case 'Tab':
                e.preventDefault();
                doTabComplete();
                break;
            case 'Escape':
                inputValue = '';
                historyIdx = -1;
                tabHint = '';
                break;
        }
    }

    function navigateHistory(dir: number) {
        if (history.length === 0) return;
        if (historyIdx === -1 && dir === 1) pendingInput = inputValue;
        historyIdx = Math.max(-1, Math.min(history.length - 1, historyIdx + dir));
        inputValue = historyIdx === -1 ? pendingInput : history[historyIdx];
    }

    function doTabComplete() {
        const val = inputValue;
        const spacePos = val.indexOf(' ');
        if (spacePos === -1) {
            const matches = ALL_COMMANDS.filter(c => c.toLowerCase().startsWith(val.toLowerCase()));
            if (matches.length === 1) {
                inputValue = matches[0] + ' ';
                tabHint = '';
            } else if (matches.length > 1) {
                tabHint = matches.join('  ');
            }
        } else {
            const cmd = val.slice(0, spacePos).toLowerCase();
            tabHint = ARG_HINTS[cmd] ? 'args: ' + ARG_HINTS[cmd] : '';
        }
    }

    export function focus() {
        inputEl?.focus();
    }
</script>

<div id="console-panel" class:expanded={$ui.consoleExpanded}>
    <div class="console-header" onclick={() => ui.toggleConsole()}>
        <span class="console-title">pcode console {$ui.consoleExpanded ? '▾ expanded' : '▴'}</span>
        <div class="console-actions">
            <button class="console-action-btn" onclick={(e) => { e.stopPropagation(); lines = []; }}>Clear</button>
        </div>
    </div>
    <div id="console-output" bind:this={outputEl}>
        {#each lines as line (line.id)}
            <div class="console-line {line.type}">
                {#if line.type === 'input-echo'}
                    <span class="line-prompt">&gt;</span>
                    <span> {line.text}</span>
                {:else}
                    {line.text}
                {/if}
            </div>
        {/each}
    </div>
    <div class="console-input-row">
        <span class="console-prompt">&gt;</span>
        <input
            type="text"
            id="console-input"
            bind:this={inputEl}
            bind:value={inputValue}
            onkeydown={onKeyDown}
            oninput={() => tabHint = ''}
            autocomplete="off"
            autocorrect="off"
            autocapitalize="off"
            spellcheck={false}
            placeholder="Type a pcode command… (Tab to complete)"
        />
    </div>
    {#if tabHint}
        <div id="tab-hint">{tabHint}</div>
    {/if}
</div>
