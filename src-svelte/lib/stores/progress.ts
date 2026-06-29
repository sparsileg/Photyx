// stores/progress.ts — polling store for backend progress atomics and job results
import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export const progress = writable<{ current: number; total: number }>({
    current: 0,
    total: 0,
});

export interface ScriptResult {
    line_number:    number;
    command:        string;
    success:        boolean;
    message:        string | null;
    data:           Record<string, unknown> | null;
    trace_line:     string | null;
    client_actions: string[];
}

export interface JobResult {
    results:         ScriptResult[];
    session_changed: boolean;
    display_changed: boolean;
    client_actions:  string[];
}

export const jobResult  = writable<JobResult | null>(null);
export const jobOwner   = writable<string | null>(null);

setInterval(async () => {
  try {
        const [current, total] = await invoke<[number, number]>('get_progress');
        progress.set({ current, total });
    } catch {
        // backend not ready — ignore
    }

    try {
        const result = await invoke<JobResult | null>('get_job_result');
        if (result !== null) {
            jobResult.set(result);
        }
    } catch {
        // backend not ready — ignore
    }
}, 500);
