// pcodeCommands.ts — Single source of truth for all pcode command names.
// Imported by Console.svelte (tab completion) and MacroEditor.svelte (syntax highlighting).
// Update this file only when commands are added, removed, or renamed.
export const PCODE_COMMANDS = new Set([
  // ── Directory & session
  'AddFiles',
  'ClearSession',
  'ReadImages',

  // ── Write commands ───────────────────────────────────────────────────────
  'WriteCurrent',
  'WriteFIT',
  'WriteFrame',
  'WriteTIFF',
  'WriteXISF',

  // ── Keyword commands ─────────────────────────────────────────────────────
  'AddKeyword',
  'CopyKeyword',
  'DeleteKeyword',
  'GetKeyword',
  'ListKeywords',
  'ModifyKeyword',

// ── Stacking ─────────────────────────────────────────────────────────────
  'ClearStack',
  'StackFrames',

  // ── Image analysis ───────────────────────────────────────────────────────
  'AnalyzeFrames',
  'ClearAnnotations',
  'ComputeEccentricity',
  'ComputeFWHM',
  'ContourHeatmap',
  'CountStars',
  'GetHistogram',
  'GetImageProperty',
  'GetSessionProperty',
  'MedianValue',
  'ShowAnalysisGraph',
  'ShowAnalysisResults',

  // ── Image processing ─────────────────────────────────────────────────────
  'AutoStretch',
  'BinImage',
  'CropImage',
  'DebayerImage',

  // ── Display & navigation ─────────────────────────────────────────────────
  'BlinkSequence',
  'CacheFrames',
  'SetFrame',
  'SetZoom',

  // ── Scripting ────────────────────────────────────────────────────────────
  'Assert',
  'CountFiles',
  'DefineMacro',
  'Echo',
  'Else',
  'EndFor',
  'EndIf',
  'For',
  'If',
  'LoadFile',
  'Log',
  'Print',
  'RunMacro',
  'Set',
  'Test',

  // ── File management ──────────────────────────────────────────────────────
  'CopyFile',
  'FilterByKeyword',
  'ListFiles',
  'MoveFile',

  // ── Console built-ins ────────────────────────────────────────────────────
  'Clear',
  'Help',
  'Version',
  'pwd',
]);


// ----------------------------------------------------------------------
