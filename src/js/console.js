// console.js — Photyx interactive pcode console UI

'use strict';

const ConsoleUI = (() => {

    const MAX_HISTORY = 500;

    let history      = [];   // persisted command history
    let historyIdx   = -1;   // navigation position (-1 = typing new)
    let pendingInput = '';   // saved draft while navigating history

    let outputEl, inputEl, tabHintEl;

    function init() {
        outputEl  = document.getElementById('console-output');
        inputEl   = document.getElementById('console-input');
        tabHintEl = document.getElementById('tab-hint');

        // Wire pcode output back to this console
        PcodeInterpreter.setOutputCallback(handleOutput);

        // Input events
        inputEl.addEventListener('keydown', onKeyDown);
        inputEl.addEventListener('input', onInput);

        // Buttons
        document.getElementById('console-clear-btn')?.addEventListener('click', () => {
            PcodeInterpreter.executeLine('Clear');
        });
        document.getElementById('console-copy-btn')?.addEventListener('click', copyToMacroEditor);

        // Welcome
        appendLine('Photyx pcode console  v1.0-dev', 'info');
        appendLine('Type Help for a command list.', 'info');
        appendLine('', 'output');
    }

    // ── Output handler (called by pcode interpreter) ─────────────────────────

    function handleOutput(text, type) {
        if (type === 'clear') {
            outputEl.innerHTML = '';
            return;
        }
        appendLine(text, type);
    }

    function appendLine(text, type = 'output') {
        const div = document.createElement('div');
        div.className = `console-line ${type}`;

        if (type === 'input-echo') {
            const prompt = document.createElement('span');
            prompt.className = 'line-prompt';
            prompt.textContent = '>';
            const content = document.createElement('span');
            content.textContent = ' ' + text;
            div.appendChild(prompt);
            div.appendChild(content);
        } else {
            div.textContent = text;
        }

        outputEl.appendChild(div);
        outputEl.scrollTop = outputEl.scrollHeight;

        // Also push to status bar
        if (type === 'error')   StatusBar.set(text, 'error');
        if (type === 'warning') StatusBar.set(text, 'warning');
        if (type === 'success') StatusBar.set(text, 'success');
    }

    // ── Input handling ───────────────────────────────────────────────────────

    function onKeyDown(e) {
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
                inputEl.value = '';
                historyIdx = -1;
                tabHintEl.textContent = '';
                break;
        }
    }

    function onInput() {
        // Clear tab hint on any normal typing
        if (tabHintEl) tabHintEl.textContent = '';
    }

    function submit() {
        const raw = inputEl.value.trim();
        if (!raw) return;

        // Echo
        appendLine(raw, 'input-echo');

        // History
        if (history[0] !== raw) history.unshift(raw);
        if (history.length > MAX_HISTORY) history.length = MAX_HISTORY;
        historyIdx = -1;
        pendingInput = '';

        // Clear input
        inputEl.value = '';
        if (tabHintEl) tabHintEl.textContent = '';

        // Execute
        PcodeInterpreter.executeLine(raw);
    }

    function navigateHistory(dir) {
        if (history.length === 0) return;

        if (historyIdx === -1 && dir === 1) {
            pendingInput = inputEl.value;
        }

        historyIdx = Math.max(-1, Math.min(history.length - 1, historyIdx + dir));

        if (historyIdx === -1) {
            inputEl.value = pendingInput;
        } else {
            inputEl.value = history[historyIdx];
        }

        // Move cursor to end
        setTimeout(() => {
            inputEl.selectionStart = inputEl.selectionEnd = inputEl.value.length;
        }, 0);
    }

    function doTabComplete() {
        const val       = inputEl.value;
        const spacePos  = val.indexOf(' ');
        const isCommand = spacePos === -1;   // still typing the command name

        if (isCommand) {
            const matches = PcodeInterpreter.complete(val);
            if (matches.length === 1) {
                inputEl.value = matches[0] + ' ';
                if (tabHintEl) tabHintEl.textContent = '';
            } else if (matches.length > 1) {
                if (tabHintEl) tabHintEl.textContent = matches.join('  ');
            }
        } else {
            // Argument completion — hint common arg names for the command
            const cmd     = val.slice(0, spacePos).toLowerCase();
            const argHint = COMMAND_ARG_HINTS[cmd];
            if (argHint && tabHintEl) {
                tabHintEl.textContent = 'args: ' + argHint.join('  ');
            }
        }
    }

    // Argument hints for tab completion on command names
    const COMMAND_ARG_HINTS = {
        selectdirectory:      ['path='],
        addkeyword:           ['name=  value=  comment='],
        deletekeyword:        ['name='],
        modifykeyword:        ['name=  value='],
        copykeyword:          ['from=  to='],
        filterbykeyword:      ['name=  value='],
        autostretch:          ['method=  shadowClip=  targetBackground='],
        linearstretch:        ['black=  white='],
        cropimage:            ['x=  y=  width=  height='],
        binimage:             ['factor='],
        debayerimage:         ['method=  pattern='],
        blinksequence:        ['fps='],
        setzoom:              ['level='],
        writeallfitfiles:     ['destination=  overwrite='],
        writeallxisffiles:    ['destination=  overwrite='],
        writealltifffiles:    ['destination=  overwrite='],
        writepng:             ['filename=  destination='],
        writejpeg:            ['filename=  destination=  quality='],
        set:                  ['<varname> = <value>'],
        getimageproperty:     ['property='],
        getsessionproperty:   ['property='],
        runmacro:             ['filename='],
    };

    // ── Copy to Macro Editor ─────────────────────────────────────────────────

    function copyToMacroEditor() {
        const lines = Array.from(outputEl.querySelectorAll('.console-line.input-echo'))
            .map(el => el.querySelector('span:last-child')?.textContent?.trim() || '')
            .filter(Boolean);

        if (lines.length === 0) {
            StatusBar.set('No commands in console history to copy.', 'warning');
            return;
        }

        const macroText = lines.join('\n');
        const textarea  = document.getElementById('macro-textarea');
        if (textarea) {
            textarea.value = macroText;
            // Open the macro editor panel
            PanelManager.open('macro-editor');
            StatusBar.set(`Copied ${lines.length} command(s) to Macro Editor.`, 'success');
        }
    }

    // ── Public ───────────────────────────────────────────────────────────────

    function focus() {
        inputEl?.focus();
    }

    return { init, focus, appendLine };

})();
