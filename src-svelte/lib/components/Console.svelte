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
    type: 'input-echo' | 'trace-echo' | 'output' | 'error' | 'warning' | 'success' | 'info';
  }

  let lines = $state<ConsoleLine[]>([
    { id: 0, text: 'Photyx pcode console  v1.0', type: 'info' },
    { id: 1, text: 'Type Help for a command list.', type: 'info' },
  ]);
  let inputValue = $state('');
  let tabHint = $state('');
  let terminalEl = $state<HTMLDivElement>();  // merged buffer (expanded)
  let outputEl   = $state<HTMLDivElement>();  // output only (collapsed)
  let textareaEl = $state<HTMLTextAreaElement>();

  let history: string[] = [];
  let historyIdx = -1;
  let pendingInput = '';
  let nextId = 2;
  let trace = $state(false);

  import { PCODE_COMMANDS } from '../pcodeCommands';
  import { applyAutoStretch, loadFile } from '../commands';
  import { getHelp } from '../pcodeHelp';
  import type { HelpEntry } from '../pcodeHelp';
  import { handleClientCommand, CLIENT_COMMAND_NAMES } from '../clientCommands';

  let { onhelp }: { onhelp: (entry: HelpEntry) => void } = $props();

  // Watch for external console output — drain the queue
  $effect(() => {
    const queue = $consolePipe;
    if (queue.length > 0) {
      queue.forEach(line => append(line.text, line.type));
      consolePipe.set([]);
    }
  });

  const ALL_COMMANDS = [...PCODE_COMMANDS].sort();

  const ARG_HINTS: Record<string, string> = {
    abs:                '(#)',
    addkeyword:         'name=  value=  comment=',
    analyzeframes:      '',
    assert:             'expression=',
    autostretch:        'shadowClip=  targetBackground=',
    binimage:           'factor=',
    blinksequence:      'fps=',
    cacheframes:        '',
    clear:              '',
    clearannotations:    (_raw: string) => { handleClientCommand('clearannotations'); },
    computefwhm:        '',
    contourheatmap:     'palette=[viridis|plasma|coolwarm]  contour_levels=#  threshold=  saturation=',
    copyfile:           'destination=  source=',
    copykeyword:        'from=  to=',
    countfiles:         '',
    cropimage:          'x=  y=  width=  height=',
    debayerimage:       'method=  pattern=',
    definemacro:        '',
    deletekeyword:      'name=  scope=',
    echo:               '',
    else:               '',
    endfor:             '',
    endif:              '',
    filterbykeyword:    'name=  value=',
    floor:              '(#)',
    for:                '',
    gethistogram:       '',
    getimageproperty:   'property=',
    getkeyword:         'name=',
    getsessionproperty: 'property=',
    help:               '',
    if:                 '',
    listfile:           '',
    listkeywords:       '',
    loadfile:           'path=',
    log:                'path=  append=',
    max:                '(#,#)',
    medianvalue:        '',
    min:                '(#,#)',
    modifykeyword:      'name=  value=  comment=  scope=',
    movefile:           'destination=',
    print:              'message (or bare: Print "hello")',
    pwd:                (_raw: string) => { handleClientCommand('pwd'); },
    readall:            '',
    readfit:            '',
    readtiff:           '',
    readxisf:           '',
    round:              '(#)',
    runmacro:           'filename=',
    selectdirectory:    'path=',
    set:                '<varname> = <value>',
    setframe:           'index=',
    setzoom:            'level=',
    showanalysisgraph:   (_raw: string) => { handleClientCommand('showanalysisgraph'); },
    showanalysisresults: (_raw: string) => { handleClientCommand('showanalysisresults'); },
    sqrt:               '(#)',
    test:               '',
    version:             (_raw: string) => { handleClientCommand('version'); },
    writecurrent:       '',
    writefit:           'destination=  overwrite=',
    writeframe:         '',
    writejpeg:          'filename=  destination=  quality=',
    writepng:           'filename=  destination=',
    writetiff:          'destination=  overwrite=',
    writexisf:          'destination=  overwrite=  compress=',
  };

  function scrollToBottom() {
    setTimeout(() => {
      const el = $ui.consoleExpanded ? terminalEl : outputEl;
      if (el) el.scrollTop = el.scrollHeight;
    }, 0);
  }

  function append(text: string, type: ConsoleLine['type']) {
    lines = [...lines, { id: nextId++, text, type }];
    consoleHistory.set(lines);
    scrollToBottom();
  }

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    if ($ui.consoleExpanded && terminalEl) {
      // In expanded terminal mode, grow to fill available space
      const maxHeight = terminalEl.clientHeight - textareaEl.offsetTop - 24;
      textareaEl.style.height = Math.min(textareaEl.scrollHeight, Math.max(maxHeight, 20)) + 'px';
      textareaEl.style.overflowY = textareaEl.scrollHeight > maxHeight ? 'auto' : 'hidden';
    } else {
      // Collapsed: cap at 6 lines
      const maxHeight = 20 * 6;
      textareaEl.style.height = Math.min(textareaEl.scrollHeight, maxHeight) + 'px';
      textareaEl.style.overflowY = textareaEl.scrollHeight > maxHeight ? 'auto' : 'hidden';
    }
  }

  const CLIENT_COMMANDS: Record<string, (raw: string) => void> = {
    pwd: (_raw: string) => {
      const dir = $session.activeDirectory ?? '(no directory selected)';
      append(dir, 'output');
    },
    help: (raw: string) => {
      const parts = raw.trim().split(/\s+/);
      const cmdArg = parts.length > 1 ? parts.slice(1).join(' ').trim() : null;
      if (cmdArg) {
        const entry = getHelp(cmdArg);
        if (entry) {
          onhelp(entry);
        } else {
          append(`No help found for '${cmdArg}'`, 'error');
        }
        return;
      }
      append('Photyx pcode v1.0  —  commands:', 'output');
      append('  File:     SelectDirectory ListFiles FilterByKeyword', 'output');
      append('  I/O:      ReadFIT ReadXISF ReadTIFF ReadAll', 'output');
      append('            WriteFIT WriteXISF WriteTIFF WriteCurrent WritePNG WriteJPEG', 'output');
      append('  Keyword:  AddKeyword DeleteKeyword ModifyKeyword CopyKeyword ListKeywords GetKeyword', 'output');
      append('  Query:    GetImageProperty GetSessionProperty Test', 'output');
      append('  Process:  AutoStretch CropImage BinImage DebayerImage', 'output');
      append('  View:     BlinkSequence CacheFrames SetZoom', 'output');
      append('  Analysis: ComputeFWHM CountStars ComputeEccentricity MedianValue ContourHeatmap', 'output');
      append('  Script:   Set Print Echo CountFiles RunMacro', 'output');
      append('  Files:    MoveFile CopyFile', 'output');
      append('  Console:  pwd Help Clear Version', 'output');
    },
    clear: (_raw: string) => { lines = []; },
    version: (_raw: string) => {
      append('Photyx 1.0.0-dev  |  pcode v1.0  |  Tauri + Svelte + Rust', 'output');
    },
    showanalysisgraph: (_raw: string) => { ui.showView('analysisGraph'); },
    showanalysisresults: (_raw: string) => { ui.showView('analysisResults'); },
    clearannotations: (_raw: string) => { ui.clearAnnotations(); },
  };

  async function dispatch(raw: string) {
    const trimmed = raw.trim();
    if (!trimmed) return;

    const firstLine = trimmed.split('\n')[0].trim();
    const cmdLower = firstLine.split(/\s/)[0].toLowerCase();

    if (CLIENT_COMMANDS[cmdLower]) {
      CLIENT_COMMANDS[cmdLower](firstLine);
      return;
    }

    try {
      const response = await invoke<{
        results: Array<{
          line_number: number;
          command: string;
          success: boolean;
          message: string | null;
          data: Record<string, unknown> | null;
        }>;
        session_changed: boolean;
        display_changed: boolean;
        client_actions:  string[];
      }>('run_script', { script: trimmed });

      for (const result of response.results) {
        if (result.success) {
          const isAssignment = result.command.toLowerCase().startsWith('set ');

          // Trace line — show before output if trace is on
          if (trace && result.trace_line) {
            append(result.trace_line, 'trace-echo');
          }

          // Output — never show assignment results, only trace line
          if (result.message && !isAssignment) {
            result.message.split('\n').forEach(line => {
              if (line) append(line, 'success');
            });
          }

          await syncSessionState(
            result.command.toLowerCase(),
            {},
            result.message,
            result.data,
          );
        } else {
          const msg = result.message ?? 'Unknown error';
          append(msg, 'error');
          notifications.error(msg);
        }
      }
      // Dispatch client actions returned by Rust — no command-name matching needed
      if (!Array.isArray(response.client_actions)) {
        console.warn('Console: client_actions was not an array:', response.client_actions);
      }
      for (const action of response.client_actions ?? []) {
        if (action === 'refresh_autostretch') {
          await applyAutoStretch();
          ui.clearAnnotations();
        }
        if (action === 'refresh_annotations') ui.refreshAnnotations();
        if (action === 'open_keyword_modal')  ui.openKeywordModal();
      }
    } catch (err) {
      const msg = `Invoke error: ${err}`;
      append(msg, 'error');
      notifications.error(msg);
    }
  }

  async function syncSessionState(
    cmd: string,
    args: Record<string, string>,
    output: string | null,
    data: Record<string, unknown> | null = null
  ) {
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
    if (['readallfitfiles','readallxisffiles','readalltifffiles',
         'readallfiles','readfit','readtiff','readxisf','readall','runmacro'].includes(cmd)) {
      if (output) notifications.success(output);
      try {
        const s = await invoke<{ activeDirectory: string; fileList: string[]; currentFrame: number }>('get_session');
        session.setDirectory(s.activeDirectory ?? '');
        session.setFileList(s.fileList);
      } catch (e) {
        notifications.error(`Session sync failed: ${e}`);
      }
    }

    // Client-only commands — intercepted in pcode interpreter, executed here
    if (data?.client_command) {
      const cc = data.client_command as string;
      if (cc === 'showanalysisgraph')  ui.showView('analysisGraph');
      if (cc === 'showanalysisresults') ui.showView('analysisResults');
      if (cc === 'clearannotations')   ui.clearAnnotations();
      if (cc === 'clear')              lines = [];
      if (cc === 'version')            append('Photyx 1.0.0-dev  |  pcode v1.0  |  Tauri + Svelte + Rust', 'output');
      if (cc === 'pwd')                append($session.activeDirectory ?? '(no directory selected)', 'output');
    }

    if (data?.client_command) {
      handleClientCommand(data.client_command as string);
    }
    if (Array.isArray(data?.client_commands)) {
      for (const cc of data.client_commands as string[]) {
        handleClientCommand(cc);
      }
    }

    if (cmd === 'linearstretch' || cmd === 'histogramequalization') ui.requestFrameRefresh();
    if (cmd === 'contourheatmap') {
      const filePath = data?.output as string | null;
      if (filePath) await loadFile(filePath);
    }
    if (cmd === 'loadfile') {
      const filePath = data?.path as string | null;
      if (filePath) await loadFile(filePath);
    }
    if (cmd === 'setframe') ui.clearAnnotations();
  }

  function submit() {
    const raw = inputValue.trim();
    if (!raw) return;

    // Always echo input lines
    raw.split('\n').forEach(line => {
      if (line.trim()) append(line, 'input-echo');
    });

    if (history[0] !== raw) history.unshift(raw);
    if (history.length > 500) history.length = 500;
    historyIdx = -1;
    pendingInput = '';
    inputValue = '';
    tabHint = '';

    if (textareaEl) {
      textareaEl.style.height = 'auto';
      textareaEl.style.overflowY = 'hidden';
    }

    dispatch(raw).catch(err => append(`Error: ${err}`, 'error'));
  }

  function onKeyDown(e: KeyboardEvent) {
    switch (e.key) {
    case 'Enter':
      if (e.shiftKey) {
        setTimeout(autoResize, 0);
        return;
      }
      e.preventDefault();
      submit();
      break;
    case 'ArrowUp':
      if (!inputValue.includes('\n')) {
        e.preventDefault();
        navigateHistory(1);
      }
      break;
    case 'ArrowDown':
      if (!inputValue.includes('\n')) {
        e.preventDefault();
        navigateHistory(-1);
      }
      break;
    case 'Tab':
      e.preventDefault();
      doTabComplete();
      break;
    case 'Escape':
      inputValue = '';
      historyIdx = -1;
      tabHint = '';
      if (textareaEl) {
        textareaEl.style.height = 'auto';
        textareaEl.style.overflowY = 'hidden';
      }
      break;
    }
  }

  function navigateHistory(dir: number) {
    if (history.length === 0) return;
    if (historyIdx === -1 && dir === 1) pendingInput = inputValue;
    historyIdx = Math.max(-1, Math.min(history.length - 1, historyIdx + dir));
    inputValue = historyIdx === -1 ? pendingInput : history[historyIdx];
    setTimeout(autoResize, 0);
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
      const rest = val.slice(spacePos + 1);
      if (cmd === 'help' && rest.length > 0) {
        const matches = ALL_COMMANDS.filter(c => c.toLowerCase().startsWith(rest.toLowerCase()));
        if (matches.length === 1) {
          inputValue = 'help ' + matches[0];
          tabHint = '';
        } else if (matches.length > 1) {
          tabHint = matches.join('  ');
        } else {
          tabHint = '';
        }
      } else {
        tabHint = ARG_HINTS[cmd] ? 'args: ' + ARG_HINTS[cmd] : '';
      }
    }
  }

  export function focus() {
    textareaEl?.focus();
  }
</script>

<!-- ── Collapsed layout (separate output + input) ─────────────────────── -->
<div id="console-panel" class:expanded={$ui.consoleExpanded}>
  <div class="console-header" onclick={() => ui.toggleConsole()}>
    <span class="console-title">pcode console {$ui.consoleExpanded ? '▾' : '▴'}</span>
    <div class="console-actions">
      <button class="console-action-btn" onclick={(e) => { e.stopPropagation(); trace = !trace; }}>{trace ? 'Trace' : 'No Trace'}</button>
      <button class="console-action-btn" onclick={(e) => { e.stopPropagation(); lines = []; }}>Clear</button>
    </div>
  </div>

  {#if !$ui.consoleExpanded}
    <!-- Collapsed: separate output area + input row -->
  <div id="console-output" bind:this={outputEl}>
    {#each lines as line (line.id)}
      <div class="console-line {line.type}">
        {#if line.type === 'input-echo'}
          <span class="line-prompt">&gt;</span>
          <span>{line.text}</span>
        {:else if line.type === 'trace-echo'}
          <span class="line-prompt-trace">+</span>
          <span>{line.text}</span>
        {:else}
          {line.text}
        {/if}
      </div>
    {/each}
  </div>
  <div class="console-input-row">
    <span class="console-prompt">&gt;</span>
    <textarea
      id="console-textarea"
      bind:this={textareaEl}
      bind:value={inputValue}
      onkeydown={onKeyDown}
      oninput={() => {
      if (inputValue.toLowerCase().startsWith('help ')) {
      doTabComplete();
      } else {
      tabHint = '';
      }
      autoResize();
      }}
      rows={1}
      autocomplete="off"
      autocorrect="off"
      autocapitalize="off"
      spellcheck={false}
      placeholder="Type a pcode command…"
      ></textarea>
  </div>
{:else}
  <!-- Expanded: single merged terminal buffer -->
  <div id="console-terminal" bind:this={terminalEl} onclick={() => textareaEl?.focus()}>
    {#each lines as line (line.id)}
      <div class="console-line {line.type}">
        {#if line.type === 'input-echo'}
          <span class="line-prompt">&gt;</span>
          <span>{line.text}</span>
        {:else if line.type === 'trace-echo'}
          <span class="line-prompt-trace">+</span>
          <span>{line.text}</span>
        {:else}
          {line.text}
        {/if}
      </div>
    {/each}

<!-- Input line inline with output -->
      <div class="console-line console-input-inline">
        <span class="line-prompt">&gt;</span>
        <textarea
          id="console-textarea"
          bind:this={textareaEl}
          bind:value={inputValue}
          onkeydown={onKeyDown}
          oninput={() => {
          if (inputValue.toLowerCase().startsWith('help ')) {
          doTabComplete();
          } else {
          tabHint = '';
          }
          autoResize();
          }}
          rows={1}
          autocomplete="off"
          autocorrect="off"
          autocapitalize="off"
          spellcheck={false}
          placeholder="Type a pcode command… (Shift+Enter for newline)"
          ></textarea>
      </div>
  </div>
{/if}

{#if tabHint}
  <div id="tab-hint">{tabHint}</div>
{/if}
</div>
