// stores/progress.ts — polling store for backend progress atomics
import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export const progress = writable<{ label: string; current: number; total: number }>({
    label:   '',
    current: 0,
    total:   0,
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

// Issue 201: jobResult/jobOwner and the get_job_result poll retired —
// run_script now returns JobResult directly from its own await, so there's
// no longer a shared slot for callers to disambiguate via an owner string.
setInterval(async () => {
    try {
        const [label, current, total] = await invoke<[string, number, number]>('get_progress');
        progress.set({ label, current, total });
    } catch {
        // backend not ready — ignore
    }
}, 500);

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
