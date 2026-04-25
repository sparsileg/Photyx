// pcodeCommands.ts — Single source of truth for all pcode command names.
// Imported by Console.svelte (tab completion) and MacroEditor.svelte (syntax highlighting).
// Update this file only when commands are added, removed, or renamed.

export const PCODE_COMMANDS = new Set([
    // ── Directory & session ──────────────────────────────────────────────────
    'SelectDirectory',
    'ClearSession',

    // ── Read commands ────────────────────────────────────────────────────────
    'ReadFIT',
    'ReadXISF',
    'ReadTIFF',
    'ReadAll',

    // ── Write commands ───────────────────────────────────────────────────────
    'WriteFIT',
    'WriteXISF',
    'WriteTIFF',
    'WriteCurrent',
    'WritePNG',
    'WriteJPEG',

    // ── Keyword commands ─────────────────────────────────────────────────────
    'AddKeyword',
    'DeleteKeyword',
    'ModifyKeyword',
    'CopyKeyword',
    'ListKeywords',
    'GetKeyword',

    // ── Image analysis ───────────────────────────────────────────────────────
    "AnalyzeFrames",
    "BackgroundGradient",
    "BackgroundMedian",
    "BackgroundStdDev",
    "ComputeEccentricity",
    "ComputeFWHM",
    "CountStars",
    'ComputeEccentricity',
    'ComputeFWHM',
    'ContourPlot',
    'CountStars',
    'GetHistogram',
    'GetImageProperty',
    'GetSessionProperty',
    'MedianValue',
    'ShowAnalysisGraph',
    'SnrEstimate',

    // ── Image processing ─────────────────────────────────────────────────────
    'AutoStretch',
    'CropImage',
    'BinImage',
    'DebayerImage',
    'AnalyzeFrames',

    // ── Display & navigation ─────────────────────────────────────────────────
    'SetFrame',
    'SetZoom',
    'BlinkSequence',
    'CacheFrames',

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
    'Log',
    'MoveFile',
    'Print',
    'RunMacro',
    'Set',

    // ── Console built-ins ────────────────────────────────────────────────────
    'Help',
    'Clear',
    'Version',
    'pwd',
    'Test',
    'ListFiles',
    'FilterByKeyword',
]);


// ----------------------------------------------------------------------
