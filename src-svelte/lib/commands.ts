// commands.ts — Shared backend command helpers
// Wraps Tauri invoke calls for common operations

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { session } from './stores/session';
import { ui } from './stores/ui';
import { notifications } from './stores/notifications';

export type FormatFilter = 'all' | 'fits' | 'xisf' | 'tiff' | 'png' | 'jpeg';

export const FORMAT_FILTERS: { id: FormatFilter; label: string; commands: string[] }[] = [
    { id: 'all',  label: 'All Supported', commands: ['ReadAllFITFiles'] },
    { id: 'fits', label: 'FITS only',     commands: ['ReadAllFITFiles'] },
    { id: 'xisf', label: 'XISF only',     commands: ['ReadAllXISFFiles'] },
    { id: 'tiff', label: 'TIFF only',     commands: ['ReadAllTIFFFiles'] },
    { id: 'png',  label: 'PNG only',      commands: ['ReadAllPNGFiles'] },
    { id: 'jpeg', label: 'JPEG only',     commands: ['ReadAllJPEGFiles'] },
];

/** Sync session store from backend state */
export async function syncSession() {
    const state = await invoke<{
        activeDirectory: string | null;
        fileList: string[];
        currentFrame: number;
    }>('get_session');
    session.setDirectory(state.activeDirectory ?? '');
    session.setFileList(state.fileList);
    session.setCurrentFrame(state.currentFrame);
}

/** Open folder picker and set active directory — does NOT load pixel data */
export async function selectDirectory() {
    let selected;
    try {
        selected = await open({ directory: true, multiple: false });
    } catch (e) {
        notifications.error(`Failed to open folder picker: ${e}`);
        return;
    }

    if (!selected) return;

    const path = typeof selected === 'string' ? selected : selected[0];
    if (!path) return;

    const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
        'dispatch_command',
        { request: { command: 'SelectDirectory', args: { path } } }
    );

    if (!result.success) {
        notifications.error(result.error ?? 'SelectDirectory failed');
        return;
    }

    session.setDirectory(path);
    notifications.info(`Directory: ${path}`);
}

/** Load files from active directory using the specified format filter */
export async function loadFiles(filter: FormatFilter) {
    const entry = FORMAT_FILTERS.find(f => f.id === filter);
    if (!entry) return;

    notifications.info(`Loading ${entry.label} files…`);

    for (const command of entry.commands) {
        const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
            'dispatch_command',
            { request: { command, args: {} } }
        );

        if (!result.success) {
            notifications.error(result.error ?? `${command} failed`);
            return;
        }

        if (result.output) notifications.info(result.output);
    }

    await syncSession();
}

/** Clear all loaded images and reset session. Active directory is preserved. */
export async function closeSession() {
    const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
        'dispatch_command',
        { request: { command: 'ClearSession', args: {} } }
    );

    if (!result.success) {
        notifications.error(result.error ?? 'ClearSession failed');
        return;
    }

    session.setFileList([]);
    session.setCurrentFrame(0);
    ui.clearViewer();
    notifications.info('Session cleared.');
}

/** Set current frame, run AutoStretch, refresh viewer */
export async function displayFrame(index: number) {
    try {
        await invoke('dispatch_command', {
            request: { command: 'SetFrame', args: { index: String(index) } }
        });

        session.setCurrentFrame(index);

        const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
            'dispatch_command',
            { request: { command: 'AutoStretch', args: {} } }
        );

        if (!result.success) {
            notifications.error(result.error ?? 'AutoStretch failed');
            return;
        }

        ui.requestFrameRefresh();

    } catch (e) {
        notifications.error(`Failed to display frame: ${e}`);
    }
}
