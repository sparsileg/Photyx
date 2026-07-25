// commands.ts — Shared backend command helpers
// Wraps Tauri invoke calls for common operations

import { invoke } from '@tauri-apps/api/core';
import { get } from 'svelte/store';
import { open } from '@tauri-apps/plugin-dialog';
import { notifications } from './stores/notifications';
import { session } from './stores/session';
import { ui } from './stores/ui';
import { analysisToggles } from './stores/analysisToggles';
import { pipeToConsole } from './stores/consoleHistory';
import { jobResult, jobOwner, progress, type JobResult, type ScriptResult } from './stores/progress';
import { REJECT_FILE_SUFFIX } from './settings/constants';

/** Dispatches a pcode script via run_script and resolves with the JobResult
 *  once the backend posts it — bridging run_script's fire-and-forget contract
 *  (accepted immediately; the real result arrives later via jobResult/jobOwner,
 *  see stores/progress.ts) into a single awaitable call for one-shot call
 *  sites that don't need their own progress-driven $effect (Issue 114).
 *
 *  Rejects immediately if another script is already running (the backend's
 *  JOB_RUNNING guard) rather than queuing. Clears any stale/orphaned
 *  jobResult before claiming ownership, so a leftover result from an earlier
 *  uncollected run can never be mistaken for this one — safe to do
 *  unconditionally since a second script can only be accepted once the
 *  first one's owner has already consumed (or abandoned) its result.
 *
 *  `owner` must be distinct from 'console' and from any other concurrent
 *  caller's owner string — Console.svelte's own $effect only reacts to
 *  jobOwner === 'console', so any other value passes through untouched. */
export function runScriptAndWait(script: string, owner: string): Promise<JobResult> {
  return new Promise<JobResult>((resolve, reject) => {
    (async () => {
      let response: { accepted: boolean };
      try {
        response = await invoke<{ accepted: boolean }>('run_script', { script });
      } catch (e) {
        reject(e);
        return;
      }
      if (!response.accepted) {
        reject(new Error('A script is already running — try again in a moment.'));
        return;
      }

      jobResult.set(null);
      jobOwner.set(owner);
      progress.set({ label: '', current: 0, total: 0 });

      const unsubscribe = jobResult.subscribe((result) => {
        if (result === null) return;
        if (get(jobOwner) !== owner) return;
        unsubscribe();
        jobResult.set(null);
        jobOwner.set(null);
        resolve(result);
      });
    })();
  });
}

/** Returns the script's last line result, or throws with its message if that
 *  line did not succeed — the "did the backend command actually succeed"
 *  check every runScriptAndWait caller needs before reporting success. */
export function lastResultOrThrow(job: JobResult): ScriptResult {
  const last = job.results.at(-1);
  if (!last?.success) {
    throw new Error(last?.message ?? 'Script failed');
  }
  return last;
}


/** Runs AnalyzeFrames with an explicit profile (Issue 101) — used by the
 *  Analyze Frames profile-selection popup so a menu-triggered run is
 *  always self-contained and explicit about which thresholds it used,
 *  instead of silently depending on whatever profile happened to be
 *  active. Deliberately does NOT change the saved active profile —
 *  equivalent to typing `AnalyzeFrames profile="..."` by hand. Quick
 *  Launch, saved macros, RunMacro, and the console dispatch AnalyzeFrames
 *  independently of this function and are unaffected.
 *
 *  Issue 177: previously invoked `dispatch_command` directly, which is an
 *  async Tauri command that holds the AppContext lock for the entire
 *  plugin execute() and does not resolve until the run completes — no
 *  progress-polling contract on that path at all, unlike console/macro
 *  dispatch. That mismatched architecture is what caused the progress bar
 *  and the whole UI to freeze for the run's duration. Now routed through
 *  the same runScriptAndWait/run_script path console already uses: the
 *  script is accepted near-instantly on its own thread, and progress
 *  ticks via the existing poll in stores/progress.ts, exactly like
 *  AddFiles and every other console-dispatched command. */
export async function runAnalyzeFramesWithProfile(profileName: string) {
  notifications.running('AnalyzeFrames');
  try {
    const job = await runScriptAndWait(
      `AnalyzeFrames profile="${profileName}"`,
      'analyzeFramesMenu'
    );
    const last = lastResultOrThrow(job);
    const msg = last.message ?? 'AnalyzeFrames complete';
    pipeToConsole(msg, 'success');
    notifications.success('AnalyzeFrames complete');
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
    if (msg.includes('FAILED TO MOVE')) {
      notifications.error(msg);
    } else {
      notifications.success(msg);
    }
    await syncSession();
    ui.showView(null);
    ui.clearViewer();
    analysisToggles.clear();
  } catch (e) {
    notifications.error(`Commit failed: ${e}`);
  }
}

const SUPPORTED_ADD_FILES_EXTENSIONS = ['fit', 'fits', 'fts', 'xisf', 'tif', 'tiff'];

/** Core AddFiles pipeline, shared by the file-picker flow and drag-and-drop.
 *
 *  Issue 177 (related): previously invoked dispatch_command directly,
 *  the same await-the-whole-execution mechanism implicated in the
 *  AnalyzeFrames menu freeze. AddFiles' second and later invocations in
 *  a session were observed to freeze the whole UI until the load
 *  finished, first invocation always clean — now routed through
 *  runScriptAndWait/run_script instead, matching the pattern already
 *  used by ExportAnalysisReport and WriteFIT. Paths are comma-joined as
 *  before, now wrapped in one quoted pcode string argument rather than
 *  passed as a raw arg map entry. */
async function addFilesFromPaths(paths: string[]) {
  const pathsArg = paths.map(p => p.replace(/\\/g, '/')).join(',');

  notifications.running(`AddFiles`);

  let job;
  try {
    job = await runScriptAndWait(`AddFiles paths="${pathsArg}"`, 'addFiles');
  } catch (e) {
    notifications.error(`AddFiles failed: ${e}`);
    return;
  }

  let last;
  try {
    last = lastResultOrThrow(job);
  } catch (e) {
    // MEMORY_LIMIT_EXCEEDED special case removed (Issue 173): the load-time
    // memory gate is retired, so the backend can no longer emit it.
    notifications.error(`AddFiles failed: ${e}`);
    return;
  }

  await syncSession();

  // Blink caches are now built during the load itself (Issue 173) — no
  // post-load background build to start.

  // Ensure current frame metadata is populated for correct zoom scaling in blink
  await displayFrame(0);

  if (last.message) notifications.success(last.message);
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

/** Load a file from disk and display it in the viewer. Adds the file to
 *  the session if not already present (Issue 157) — reloading an
 *  already-open path refreshes it in place instead. */
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
