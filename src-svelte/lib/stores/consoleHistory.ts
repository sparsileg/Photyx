// consoleHistory.ts — shared store for Console line history
// Console.svelte writes to this store; MacroEditor reads from it
// for the "Copy from Console" feature (spec §8.9).

import { writable } from 'svelte/store';

export interface ConsoleLine {
    id: number;
    text: string;
    type: 'input-echo' | 'output' | 'error' | 'warning' | 'success' | 'info';
}

export const consoleHistory = writable<ConsoleLine[]>([]);

// Pipe for external components to send lines to the console
export const consolePipe = writable<ConsoleLine | null>(null);
