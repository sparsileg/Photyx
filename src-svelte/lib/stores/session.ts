// session.ts — Photyx session state store
// Tracks file list, loaded images, current frame

import { writable, derived } from 'svelte/store';

export interface KeywordEntry {
  name: string;
  value: string;
  comment: string | null;
}

export interface ImageMeta {
  filename: string;
  width: number;
  height: number;
  displayWidth: number;
  bitDepth: string;
  colorSpace: string;
  channels: number;
  keywords: Record<string, KeywordEntry>;
}

export interface SessionState {
  fileList: string[];
  loadedImages: Record<string, ImageMeta>;
  currentFrame: number;
  variables: Record<string, string>;
}

function createSessionStore() {
  const initial: SessionState = {
    fileList: [],
    loadedImages: {},
    currentFrame: 0,
    variables: {},
  };

  const { subscribe, set, update } = writable<SessionState>(initial);

  return {
    subscribe,
    set,
    update,
    setFileList: (files: string[]) => update(s => ({ ...s, fileList: files })),
    setCurrentFrame: (idx: number) => update(s => ({ ...s, currentFrame: idx })),
    setVariable: (name: string, value: string) =>
      update(s => ({ ...s, variables: { ...s.variables, [name]: value } })),
    reset: () => set(initial),
  };
}

export const session = createSessionStore();

// Derived: current image metadata
export const currentImage = derived(session, $s => {
  const path = $s.fileList[$s.currentFrame];
  if (!path) return null;
  return $s.loadedImages[path] ?? null;
});

// Derived: file count
export const fileCount = derived(session, $s => $s.fileList.length);

// Derived: unique directory count from file list
export const directoryCount = derived(session, $s => {
  const dirs = new Set(
    $s.fileList.map(f => {
      const parts = f.replace(/\\/g, '/').split('/');
      parts.pop();
      return parts.join('/');
    })
  );
  return dirs.size;
});
