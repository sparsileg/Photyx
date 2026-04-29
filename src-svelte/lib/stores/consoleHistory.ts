// consoleHistory.ts — shared store for Console line history
// Console.svelte writes to this store; MacroEditor reads from it
// for the "Copy from Console" feature (spec §8.9).
import { writable } from 'svelte/store';

export interface ConsoleLine {
  id: number;
  text: string;
  type: 'input-echo' | 'trace-echo' | 'output' | 'error' | 'warning' | 'success' | 'info';
}

export const consoleHistory = writable<ConsoleLine[]>([]);

// Pipe for external components to send lines to the console.
// Uses a queue (array) so rapid successive writes don't overwrite each other.
export const consolePipe = writable<ConsoleLine[]>([]);

export function pipeToConsole(text: string, type: ConsoleLine['type'] = 'success') {
  consolePipe.update(q => [...q, { id: Date.now() + Math.random(), text, type }]);
}
