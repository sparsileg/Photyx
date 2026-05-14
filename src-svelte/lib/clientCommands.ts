// clientCommands.ts — Handles pcode client-only commands that affect frontend state.
// Used by Console.svelte (interactive path) and any component that calls run_script
// (MacroLibrary, QuickLaunch, etc.) to execute frontend actions from pcode results.

import { get } from 'svelte/store';
import { ui } from './stores/ui';
import { session } from './stores/session';
import { pipeToConsole } from './stores/consoleHistory';

/// The set of command names intercepted by the pcode interpreter as client commands.
/// Must stay in sync with CLIENT_COMMANDS in src-tauri/src/pcode/mod.rs.
export const CLIENT_COMMAND_NAMES = new Set([
  'ShowAnalysisGraph',
  'ShowAnalysisResults',
  'ClearAnnotations',
  'Version',
  'Pwd',
]);

function pwdCommand(): void {
  const fileList = get(session).fileList;
  if (fileList.length === 0) {
    pipeToConsole('(no files loaded)', 'output');
  } else {
    const dirs = [...new Set(fileList.map(f => {
      const parts = f.replace(/\\/g, '/').split('/');
      parts.pop();
      return parts.join('/');
    }))].sort();
    dirs.forEach(d => pipeToConsole(d, 'output'));
  }
}

/// Executes a client-only command by name (case-insensitive).
/// Called from Console.svelte (interactive) and any run_script caller (macro, Quick Launch).
export function handleClientCommand(cc: string): void {
  switch (cc.toLowerCase()) {
    case 'showanalysisgraph':
      ui.showView('analysisGraph');
      break;
    case 'showanalysisresults':
      ui.showView('analysisResults');
      break;
    case 'clearannotations':
      ui.clearAnnotations();
      break;
    case 'version':
      pipeToConsole('Photyx 1.0.0-dev  |  pcode v1.0  |  Tauri + Svelte + Rust', 'output');
      break;
    case 'pwd':
      pwdCommand();
      break;
  }
}

// ----------------------------------------------------------------------
