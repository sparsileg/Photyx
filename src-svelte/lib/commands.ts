// commands.ts — Shared backend command helpers
// Wraps Tauri invoke calls for common operations

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { session } from './stores/session';
import { ui } from './stores/ui';
import { notifications } from './stores/notifications';

export type FormatFilter = 'all' | 'fits' | 'xisf' | 'tiff' | 'png' | 'jpeg';

export const FORMAT_FILTERS: { id: FormatFilter; label: string; commands: string[] }[] = [
    { id: 'all',  label: 'All Supported', commands: ['ReadAll'] },
    { id: 'fits', label: 'FITS only',     commands: ['ReadFIT'] },
    { id: 'xisf', label: 'XISF only',     commands: ['ReadXISF'] },
    { id: 'tiff', label: 'TIFF only',     commands: ['ReadTIFF'] },
    { id: 'png',  label: 'PNG only',      commands: ['ReadPNG'] },
    { id: 'jpeg', label: 'JPEG only',     commands: ['ReadJPEG'] },
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
    session.setFileList([]);
    session.update(s => ({ ...s, loadedImages: {} }));
    ui.clearViewer();
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

    // Start background blink cache build
    invoke('start_background_cache').catch(e => {
        console.warn('Background cache failed to start:', e);
    });
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
    console.trace('displayFrame called with index:', index);
    try {
        await invoke('dispatch_command', {
            request: { command: 'SetFrame', args: { index: String(index) } }
        });

        session.setCurrentFrame(index);

        // Check if display cache already populated for this frame
        const cacheCheck = await invoke<{
            current_frame: number;
            file_count: number;
            buffer: { display_width: number } | null;
        }>('debug_buffer_info');

        const needsStretch = !cacheCheck.buffer || cacheCheck.buffer.display_width === 0;

        if (needsStretch) {
            const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
                'dispatch_command',
                { request: { command: 'AutoStretch', args: {} } }
            );

            if (!result.success) {
                notifications.error(result.error ?? 'AutoStretch failed');
                return;
            }
        }

        // Fetch buffer metadata for the current frame
        const info = await invoke<{
            current_frame: number;
            file_count: number;
            buffer: {
                filename: string;
                width: number;
                height: number;
                display_width: number;
                bit_depth: string;
                channels: number;
                has_pixels: boolean;
            } | null;
        }>('debug_buffer_info');

        if (info.buffer) {
            const path = await invoke<{
                activeDirectory: string | null;
                fileList: string[];
                currentFrame: number;
            }>('get_session');
            const filePath = path.fileList[index];
            if (filePath) {
                const keywords = await invoke<Record<string, { name: string; value: string; comment: string | null }>>('get_keywords');
                session.update(s => ({
                    ...s,
                    loadedImages: {
                        ...s.loadedImages,
                        [filePath]: {
                            filename: info.buffer!.filename,
                            width: info.buffer!.width,
                            height: info.buffer!.height,
                            displayWidth: info.buffer!.display_width,
                            bitDepth: info.buffer!.bit_depth,
                            colorSpace: info.buffer!.channels === 3 ? 'RGB' : 'Mono',
                            channels: info.buffer!.channels,
                            keywords,
                        }
                    }
                }));
            }
        }

        console.log('requestFrameRefresh called from displayFrame');
        ui.requestFrameRefresh();

    } catch (e) {
        notifications.error(`Failed to display frame: ${e}`);
    }
}
