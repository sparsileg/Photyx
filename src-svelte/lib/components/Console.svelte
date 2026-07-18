<!-- Console.svelte   pcode interactive console. Spec §8.9, §7.9 -->

<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { session } from '../stores/session';
  import { notifications } from '../stores/notifications';
  import { ui } from '../stores/ui';
  import { consoleHistory, consolePipe } from '../stores/consoleHistory';
  import { settings } from '../stores/settings';
  import { jobResult, jobOwner, progress } from '../stores/progress';

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

  // ── Clipboard copy ────────────────────────────────────────────────────────
  // Copies the entire console buffer (all of `lines`, capped by
  // console_history_size), not just what's currently scrolled into view.
  const DEFAULT_COPY_LABEL = 'Copy';
  let copyLabel = $state(DEFAULT_COPY_LABEL);
  let copyResetTimer: ReturnType<typeof setTimeout> | null = null;

  function linePrefix(type: ConsoleLine['type']): string {
    if (type === 'input-echo') return '> ';
    if (type === 'trace-echo') return '+ ';
    return '';
  }

  function buildConsoleText(): string {
    return lines.map(l => `${linePrefix(l.type)}${l.text}`).join('\n');
  }

  async function copyConsoleToClipboard() {
    try {
      await navigator.clipboard.writeText(buildConsoleText());
      copyLabel = 'Copied!';
    } catch (e) {
      console.error('Copy failed:', e);
      copyLabel = 'Copy failed';
    }
    if (copyResetTimer !== null) clearTimeout(copyResetTimer);
    copyResetTimer = setTimeout(() => { copyLabel = DEFAULT_COPY_LABEL; }, 1500);
  }

  import { PCODE_COMMANDS } from '../pcode';
  import { applyAutoStretch, loadFile } from '../commands';
  import { getHelp, ARG_HINT_STRINGS, HELP_DB, extractRunningLabel } from '../pcode';
  import type { HelpEntry } from '../pcode';
  import { handleClientCommand, CLIENT_COMMAND_NAMES } from '../clientCommands';
  import { getVersion } from '@tauri-apps/api/app';

  let { onhelp }: { onhelp: (entry: HelpEntry) => void } = $props();

  // Watch for external console output   drain the queue
  $effect(() => {
    const queue = $consolePipe;
    if (queue.length > 0) {
      queue.forEach(line => append(line.text, line.type));
      consolePipe.set([]);
    }
  });

  // Handle async job results addressed to the console
  $effect(() => {
    const result = $jobResult;
    const owner  = $jobOwner;
    if (!result || owner !== 'console') return;

    // Issue 98: clear job state synchronously, immediately — before any
    // async work starts below. A prior version of this fix delayed this
    // clear to the end of the async IIFE (behind await points added for
    // client-command ordering), which reopened a real race: with jobResult
    // still non-null and this effect still subscribed to it, the 500ms
    // progress poller (or any other redelivery) could write a fresh-but-
    // same-content result back into the store before cleanup ran, causing
    // Svelte to rerun this effect on the same result and reprocess/reprint
    // it — compounding indefinitely (confirmed via Svelte's own
    // effect_update_depth_exceeded guard). Everything below now works off
    // the locally-captured `result`, not the store, so clearing here first
    // is safe.
    jobResult.set(null);
    jobOwner.set(null);

    // Reactive reads ($jobResult, $jobOwner) stay synchronous above, per
    // Svelte's effect dependency-tracking requirements. The rest of the
    // work is wrapped in an async IIFE so the results loop can await
    // syncSessionState() (which can await handleClientCommand()) in
    // order — previously a fire-and-forget async call inside a synchronous
    // loop let a later, fully-synchronous command (e.g. Pwd) print before
    // an earlier async one (e.g. Version, which awaits getVersion()) had
    // resolved, scrambling output order relative to script order.
    (async () => {
      let lastActionData: Record<string, unknown> | null = null;

      for (const r of result.results) {
        if (r.success) {
          const isAssignment = r.command.toLowerCase().startsWith('set ');

          if (trace && r.trace_line) {
            append(r.trace_line, 'trace-echo');
          }

          if (r.message && !isAssignment) {
            r.message.split('\n').forEach(line => {
              if (line) append(line, 'success');
            });
          }

          if (r.data) lastActionData = r.data;

          await syncSessionState(
            r.command.toLowerCase(),
            {},
            r.message,
            r.data,
          );
        } else {
          const msg = r.message ?? 'Unknown error';
          append(msg, 'error');
          if (msg.includes('Load cancelled') || msg.includes('MEMORY_LIMIT_EXCEEDED')) {
            notifications.alert('Too many files to load', msg, 10000);
          } else {
            notifications.error(msg);
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

      // Dispatch client actions
      for (const action of result.client_actions ?? []) {
        if (action === 'refresh_autostretch') {
          const shadowClip       = lastActionData?.shadow_clip      as number | undefined;
          const targetBackground = lastActionData?.target_background as number | undefined;
          applyAutoStretch(shadowClip, targetBackground).then(() => ui.clearAnnotations());
        }
        if (action === 'refresh_annotations') ui.refreshAnnotations();
        if (action === 'open_keyword_modal')  ui.openKeywordModal();
      }

      const anyError = result.results.some(r => !r.success);
      if (!anyError) {
        notifications.success(result.results.at(-1)?.message ?? 'Done.');
      }
    })();
  });

  const ALL_COMMANDS = [...PCODE_COMMANDS].sort();
  const ALL_HELP_TOPICS = Object.keys(HELP_DB).map(k => HELP_DB[k].name.replace(/\(\)$/, '')).sort();

  // ARG_HINTS merges the string hints from pcode.ts with the client command
  // handler functions that must live here (they reference handleClientCommand).
  const ARG_HINTS: Record<string, string | ((_raw: string) => void)> = {
    ...ARG_HINT_STRINGS,
    clearannotations:    (_raw: string) => { handleClientCommand('clearannotations'); },
    pwd:                 (_raw: string) => { handleClientCommand('pwd'); },
    showanalysisgraph:   (_raw: string) => { handleClientCommand('showanalysisgraph'); },
    showanalysisresults: (_raw: string) => { handleClientCommand('showanalysisresults'); },
    version:             (_raw: string) => { handleClientCommand('version'); },
  };

  function scrollToBottom() {
    setTimeout(() => {
      const el = $ui.consoleExpanded ? terminalEl : outputEl;
      if (el) el.scrollTop = el.scrollHeight;
    }, 0);
  }

  function append(text: string, type: ConsoleLine['type']) {
    const limit = $settings.console_history_size ?? 500;
    const next = [...lines, { id: nextId++, text, type }];
    lines = next.length > limit ? next.slice(next.length - limit) : next;
    consoleHistory.set(lines);
    scrollToBottom();
  }

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = 'auto';
    if ($ui.consoleExpanded && terminalEl) {
      const maxHeight = terminalEl.clientHeight - textareaEl.offsetTop - 24;
      textareaEl.style.height = Math.min(textareaEl.scrollHeight, Math.max(maxHeight, 20)) + 'px';
      textareaEl.style.overflowY = textareaEl.scrollHeight > maxHeight ? 'auto' : 'hidden';
    } else {
      const maxHeight = 20 * 6;
      textareaEl.style.height = Math.min(textareaEl.scrollHeight, maxHeight) + 'px';
      textareaEl.style.overflowY = textareaEl.scrollHeight > maxHeight ? 'auto' : 'hidden';
    }
  }

  // Issue 98: help and clear are legitimately console-only (no other entry
  // point has a help modal or a local `lines` array to clear) and stay
  // implemented here directly. The other five — pwd, version,
  // showanalysisgraph, showanalysisresults, clearannotations — now delegate
  // to handleClientCommand(), the single canonical implementation in
  // clientCommands.ts, instead of reimplementing each inline. Output from
  // the delegated commands arrives via the existing consolePipe queue
  // effect above rather than a synchronous append() call.
  const CLIENT_COMMANDS: Record<string, (raw: string) => void> = {
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
      append('Photyx pcode v1.0   commands:', 'output');
      const cols = 4;
      const cmds = ALL_COMMANDS;
      for (let i = 0; i < cmds.length; i += cols) {
        append('  ' + cmds.slice(i, i + cols).join('   '), 'output');
      }
      append('Expression functions: abs  basename  ceil  dirof  floor  max  min  round  sqrt  stripext', 'output');
    },
    clear: (_raw: string) => { lines = []; },
    pwd:                  (_raw: string) => { handleClientCommand('pwd'); },
    version:              (_raw: string) => { handleClientCommand('version'); },
    showanalysisgraph:    (_raw: string) => { handleClientCommand('showanalysisgraph'); },
    showanalysisresults:  (_raw: string) => { handleClientCommand('showanalysisresults'); },
    clearannotations:     (_raw: string) => { handleClientCommand('clearannotations'); },
  };

  async function dispatch(raw: string) {
    let trimmed = raw.trim();
    if (!trimmed) return;

    const firstLine = trimmed.split('\n')[0].trim();
    const cmdLower = firstLine.split(/\s/)[0].toLowerCase();

    if (CLIENT_COMMANDS[cmdLower]) {
      CLIENT_COMMANDS[cmdLower](firstLine);
      return;
    }

    // RejectCurrentFrame with no explicit index= defaults to ctx.current_frame
    // on the backend — but that value only tracks Pixels/pcode navigation,
    // never Blink playback (blinkFrame is separate frontend-only state).
    // While in Blink mode, make "current" mean whatever's actually on
    // screen there, rather than a possibly stale/unrelated backend value.
    if (cmdLower === 'rejectcurrentframe' && $ui.blinkModeActive && !/\bindex\s*=/i.test(firstLine)) {
      trimmed = `${trimmed} index=${$ui.blinkFrameIndex}`;
    }

    try {
      const response = await invoke<{ accepted: boolean }>('run_script', { script: trimmed });
      if (!response.accepted) {
        notifications.error('A script is already running — try again in a moment.');
        return;
      }
      notifications.running(extractRunningLabel(firstLine));
      jobOwner.set('console');
      progress.set({ label: '', current: 0, total: 0 });
      // Result arrives asynchronously via the $effect watching jobResult
    } catch (err) {
      const msg = String(err);
      append(msg, 'error');
      if (msg.includes('Load cancelled') || msg.includes('MEMORY_LIMIT_EXCEEDED')) {
        notifications.alert('Too many files to load', msg, 10000);
      } else {
        notifications.error(msg);
      }
      jobOwner.set(null);
    }
  }

  async function syncSessionState(
    cmd: string,
    args: Record<string, string>,
    output: string | null,
    data: Record<string, unknown> | null = null
  ) {
    if (cmd === 'clearsession') {
      session.setFileList([]);
      session.setCurrentFrame(0);
      session.update(s => ({ ...s, loadedImages: {} }));
      ui.clearViewer();
    }

    // Issue 98: previously this inline if-chain ran, then
    // handleClientCommand(data.client_command) ran again immediately after
    // for the same value — a confirmed double-execution (Version printed
    // twice). handleClientCommand() in clientCommands.ts is the single
    // canonical implementation for all five commands; this file should only
    // ever call it, never reimplement command behavior inline. The
    // client_commands (plural) branch below was previously dead — nothing
    // populated it — until run_macro.rs started emitting it (Issue 98).
    // Awaited (and this whole function made async) so a Version followed
    // by Pwd in the same script prints in script order — see the
    // console-results effect above for the matching fix on the caller side.
    if (data?.client_command) {
      await handleClientCommand(data.client_command as string);
    }
    if (Array.isArray(data?.client_commands)) {
      for (const cc of data.client_commands as string[]) {
        await handleClientCommand(cc);
      }
    }

    if (cmd === 'linearstretch' || cmd === 'histogramequalization' || cmd === 'backgroundextract') ui.requestFrameRefresh();
    if (cmd === 'contourheatmap') {
      const filePath = data?.output as string | null;
      if (filePath) loadFile(filePath);
    }
    if (cmd === 'loadfile') {
      const filePath = data?.path as string | null;
      if (filePath) loadFile(filePath);
    }
    if (cmd === 'setframe') ui.clearAnnotations();
    if (cmd === 'rejectcurrentframe') {
      ui.clearAnnotations();
      ui.requestFrameRefresh();
    }
    if (cmd === 'stackframes' && data?.stack_available) {
      notifications.success('Stack complete — opening result 🔭');
      ui.showView('stackingWorkspace');
    }
    if (cmd === 'clearstack') {
      ui.showView(null);
    }
  }

  function submit() {
    const raw = inputValue.trim();
    if (!raw) return;

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
        const matches = ALL_HELP_TOPICS.filter(c => c.toLowerCase().startsWith(rest.toLowerCase()));
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

<!--    Collapsed layout (separate output + input)                         -->
<div id="console-panel" class:expanded={$ui.consoleExpanded}>
  <div class="console-header" onclick={() => ui.toggleConsole()}>
    <span class="console-title">pcode console {$ui.consoleExpanded ? '▼' : '▲'}</span>
    <div class="console-actions">
      <button class="console-action-btn" onclick={(e) => { e.stopPropagation(); trace = !trace; }}>{trace ? 'Trace' : 'No Trace'}</button>
      <button class="console-action-btn" onclick={(e) => { e.stopPropagation(); lines = []; }}>Clear</button>
      <button class="console-action-btn" onclick={(e) => { e.stopPropagation(); copyConsoleToClipboard(); }}>{copyLabel}</button>
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
