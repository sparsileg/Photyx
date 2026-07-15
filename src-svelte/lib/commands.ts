// commands.ts — Shared backend command helpers
// Wraps Tauri invoke calls for common operations

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { notifications } from './stores/notifications';
import { session } from './stores/session';
import { ui } from './stores/ui';
import { analysisToggles } from './stores/analysisToggles';
import { pipeToConsole } from './stores/consoleHistory';
import { REJECT_FILE_SUFFIX } from './settings/constants';

/** Runs AnalyzeFrames with an explicit profile (Issue 101) — used by the
 *  Analyze Frames profile-selection popup so a menu-triggered run is
 *  always self-contained and explicit about which thresholds it used,
 *  instead of silently depending on whatever profile happened to be
 *  active. Deliberately does NOT change the saved active profile —
 *  equivalent to typing `AnalyzeFrames profile="..."` by hand. Quick
 *  Launch, saved macros, RunMacro, and the console dispatch AnalyzeFrames
 *  independently of this function and are unaffected. */
export async function runAnalyzeFramesWithProfile(profileName: string) {
  notifications.running('AnalyzeFrames');
  try {
    const response = await invoke<{
      success: boolean;
      output: string | null;
      error: string | null;
    }>('dispatch_command', {
      request: { command: 'AnalyzeFrames', args: { profile: profileName } }
    });
    if (response.success) {
      const msg = response.output ?? 'AnalyzeFrames complete';
      pipeToConsole(msg, 'success');
      notifications.success('AnalyzeFrames complete');
    } else {
      const err = response.error ?? 'AnalyzeFrames failed';
      pipeToConsole(err, 'error');
      notifications.error(err);
    }
  } catch (err) {
    const msg = `AnalyzeFrames error: ${err}`;
    pipeToConsole(msg, 'error');
    notifications.error(msg);
  }
}

/** Sync session store from backend state */
export async function syncSession() {
  const state = await invoke<{
    fileList: string[];
    currentFrame: number;
  }>('get_session');
  session.setFileList(state.fileList);
  session.setCurrentFrame(state.currentFrame);
}

/** Shared commit sequence for Analysis Results and Analysis Graph
 *  (Issue 93) — syncs any pending PXFLAG toggles, commits, syncs the
 *  session store, closes the view, and clears the viewer. Both views
 *  call this so committing produces identical on-disk results and
 *  identical post-commit UI state regardless of which view triggered
 *  it. `isImported` is passed in by the caller (each view already has
 *  it from its own get_analysis_results load) rather than re-fetched
 *  here. */
export async function commitAnalysis(isImported: boolean) {
  if (isImported) {
    notifications.error('Cannot commit an imported session — no images are loaded.');
    return;
  }

  const toggled = analysisToggles.entries();
  if (toggled.length > 0) {
    try {
      for (const [path, flag] of toggled) {
        await invoke('set_frame_flag', { path, flag });
      }
    } catch (e) {
      notifications.error(`Failed to sync flag changes: ${e}`);
      return;
    }
  }

  notifications.running('Committing results…');
  try {
    const msg = await invoke<string>('commit_analysis_results', { append: `.${REJECT_FILE_SUFFIX}` });
    notifications.success(msg);
    await syncSession();
    ui.showView(null);
    ui.clearViewer();
    analysisToggles.clear();
  } catch (e) {
    notifications.error(`Commit failed: ${e}`);
  }
}

const SUPPORTED_ADD_FILES_EXTENSIONS = ['fit', 'fits', 'fts', 'xisf', 'tif', 'tiff'];

/** Core AddFiles pipeline, shared by the file-picker flow and drag-and-drop. */
async function addFilesFromPaths(paths: string[]) {
  const pathsArg = paths.map(p => p.replace(/\\/g, '/')).join(',');

  notifications.running(`AddFiles`);

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

/** Open a multi-file picker and append selected files to the session */
export async function addFiles() {
  let selected;
  try {
    selected = await open({
      directory: false,
      multiple: true,
      filters: [{
        name: 'Supported Images',
        extensions: SUPPORTED_ADD_FILES_EXTENSIONS,
      }],
    });
  } catch (e) {
    notifications.error(`Failed to open file picker: ${e}`);
    return;
  }

  if (!selected || (Array.isArray(selected) && selected.length === 0)) return;

  const paths = Array.isArray(selected) ? selected : [selected];
  await addFilesFromPaths(paths);
}

/** Handle paths dropped onto the app window — filters to supported
 *  extensions and routes through the same AddFiles pipeline as
 *  Session > Add Files. */
export async function handleDroppedPaths(paths: string[]) {
  const filtered = paths.filter(p => {
    const ext = p.split('.').pop()?.toLowerCase() ?? '';
    return SUPPORTED_ADD_FILES_EXTENSIONS.includes(ext);
  });

  if (filtered.length === 0) {
    notifications.error('No supported image files in dropped item(s).');
    return;
  }

  await addFilesFromPaths(filtered);
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
