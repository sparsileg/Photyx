// commands.ts — Shared backend command helpers
// Wraps Tauri invoke calls for common operations

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { notifications } from './stores/notifications';
import { session } from './stores/session';
import { ui } from './stores/ui';

/** Sync session store from backend state */
export async function syncSession() {
  const state = await invoke<{
    fileList: string[];
    currentFrame: number;
  }>('get_session');
  session.setFileList(state.fileList);
  session.setCurrentFrame(state.currentFrame);
}

/** Open a multi-file picker and append selected files to the session */
export async function addFiles() {
  console.log('addFiles: called');
  let selected;
  try {
    selected = await open({
      directory: false,
      multiple: true,
      filters: [{
        name: 'Supported Images',
        extensions: ['fit', 'fits', 'fts', 'xisf', 'tif', 'tiff'],
      }],
    });
  } catch (e) {
    notifications.error(`Failed to open file picker: ${e}`);
    return;
  }

  if (!selected || (Array.isArray(selected) && selected.length === 0)) return;

  const paths = Array.isArray(selected) ? selected : [selected];
  const pathsArg = paths.map(p => p.replace(/\\/g, '/')).join(',');

  notifications.running(`Loading ${paths.length} file(s)…`);

  const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
    'dispatch_command',
    { request: { command: 'AddFiles', args: { paths: pathsArg } } }
  );

  if (!result.success) {
    const msg = result.error ?? 'AddFiles failed';
    if (msg.includes('Load cancelled') || msg.includes('MEMORY_LIMIT_EXCEEDED')) {
      notifications.alert('Too many files to load', msg, 10000);
    } else {
      notifications.error(msg);
    }
    return;
  }

  await syncSession();

  // Start background blink cache build
  invoke('start_background_cache').catch(e => {
    console.warn('Background cache failed to start:', e);
  });

  // Ensure current frame metadata is populated for correct zoom scaling in blink
  await displayFrame(0);

  if (result.output) notifications.success(result.output);
}

/** Clear all loaded images and reset session. */
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

/** Load a file from disk and display it in the viewer without adding to the session */
export async function loadFile(path: string) {
  try {
    const dataUrl = await invoke<string>('load_file', { path });
    ui.setDisplayImage(dataUrl);

    const s = await invoke<{ fileList: string[]; currentFrame: number }>('get_session');
    session.setFileList(s.fileList);
    session.setCurrentFrame(s.currentFrame);

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
      const keywords = await invoke<Record<string, { name: string; value: string; comment: string | null }>>('get_keywords');
      session.update(st => ({
        ...st,
        loadedImages: {
          ...st.loadedImages,
          [path]: {
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
  } catch (e) {
    notifications.error(`Failed to load file: ${e}`);
  }
}

/** Apply AutoStretch to the current frame and display the result */
export async function applyAutoStretch(shadowClip?: number, targetBackground?: number) {
  try {
    const dataUrl = await invoke<string>('get_autostretch_frame', {
      shadowClip: shadowClip ?? null,
      targetBackground: targetBackground ?? null,
    });
    ui.setAutostretchFrame(dataUrl);
  } catch (e) {
    notifications.error(`AutoStretch failed: ${e}`);
  }
}

/** Set current frame and refresh viewer with raw (unstretched) pixels */
export async function displayFrame(index: number) {
  console.trace('displayFrame called with index:', index);
  try {
    ui.clearAnnotations();

    try {
      await invoke<{ success: boolean; error: string | null }>('dispatch_command', {
        request: { command: 'SetFrame', args: { index: String(index) } }
      });
    } catch (e) {
      console.error('SetFrame invoke error:', e);
    }

    session.setCurrentFrame(index);

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
      try {
        const s = await invoke<{ fileList: string[]; currentFrame: number }>('get_session');
        const filePath = s.fileList[index];
        if (filePath) {
          const keywords = await invoke<Record<string, { name: string; value: string; comment: string | null }>>('get_keywords');
          session.update(st => ({
            ...st,
            loadedImages: {
              ...st.loadedImages,
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
      } catch (e) {
        console.error('Error fetching session/keywords:', e);
      }
    }

    ui.requestFrameRefresh();

  } catch (e) {
    notifications.error(`Failed to display frame: ${e}`);
  }
}
