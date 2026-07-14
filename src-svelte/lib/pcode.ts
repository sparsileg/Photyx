// pcode.ts — Single source of truth for all pcode command metadata.
//
// Consolidates:
//   - PCODE_COMMANDS  (tab completion + syntax highlighting)
//   - ARG_HINT_STRINGS (console argument completion)
//   - HELP_DB         (help modal content)
//
// Imported by:
//   Console.svelte        all three
//   MacroEditor.svelte    PCODE_COMMANDS
//   HelpModal.svelte      HelpEntry type
//   +page.svelte          HelpEntry type
//
// When adding a new command, update all three sections in this file.

// ---------------------------------------------------------------------------
// 1. Command name registry (tab completion + syntax highlighting)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// 1. Command name registry (tab completion + syntax highlighting)
// ---------------------------------------------------------------------------
//
// The command list itself now lives in pcode_commands.json, not inline here —
// that file is the single source of truth shared with a Rust-side test
// (Issue 99) that verifies every name here actually corresponds to a real
// backend plugin or client command. When adding a new command, add it to
// pcode_commands.json, not this file.

import pcodeCommandList from './pcode_commands.json';

export const PCODE_COMMANDS = new Set(pcodeCommandList);

// ---------------------------------------------------------------------------
// 2. Argument hints (console tab completion   shows after command name)
// ---------------------------------------------------------------------------

// Values are either a hint string or a client-command handler function.
// Handler functions are called directly in Console.svelte without dispatching
// to the backend.
export type ArgHintValue = string | ((_raw: string) => void);

// Note: handler functions reference handleClientCommand from Console.svelte.
// The ARG_HINTS object is defined there using this type; see Console.svelte.
export const ARG_HINT_STRINGS: Record<string, string> = {
  addfiles:            'paths=<path|glob>[,<path|glob>...]',
  addkeyword:          'name=  value=  comment=',
  analyzeframes:       '[profile=]  [scope=all|current]  [threshold=]  [saturation=]',
  assert:              'expression=',
  autostretch:         'shadowClip=  targetBackground=',
  backgroundgradient:  '[sigma=]  [iterations=]  [grid=]',
  backgroundmedian:    '[sigma=]  [iterations=]  [grid=]',
  backgroundstddev:    '[sigma=]  [iterations=]  [grid=]',
  basename:            '($path)',
  cacheframes:         '[resolution=12|25]',
  ceil:                '(#)',
  clear:               '',
  clearannotations:    '',
  clearsession:        '',
  clearstack:          '',
  commitanalysis:      '[append=]',
  commitstretch:       'shadow_clip=  target_bg=',
  computeeccentricity: '[threshold=]  [peak_radius=]  [saturation=]',
  computefwhm:         '[threshold=]  [peak_radius=]  [saturation=]',
  contourheatmap:      'palette=[viridis|plasma|coolwarm]  contour_levels=#  threshold=  saturation=',
  copyfile:            'destination=  [source=]',
  copykeyword:         'from=  to=  [scope=all|current]',
  countfiles:          '',
  countmatches:        'pattern=<glob>',
  countstars:          '[threshold=]  [peak_radius=]  [flood_threshold=]  [saturation=]  [sigma=]  [iterations=]',
  debayerimage:        '',
  deletekeyword:       'name=  scope=',
  dirof:               '($path)',
  else:                '',
  endfor:              '',
  endif:               '',
  exportanalysisreport: '[path=]',
  filterbykeyword:     'name=  value=',
  floor:               '(#)',
  for:                 '<var> = N To M  |  <var> in "<glob>"',
  gethistogram:        '',
  getkeyword:          'name=  [default=]',
  getsystempath:       'name=[downloads|documents|desktop|temp]',
  help:                '',
  if:                  '',
  listkeywords:        '',
  loadfile:            'path=',
  log:                 'path=  append=',
  max:                 '(#,#)',
  min:                 '(#,#)',
  modifykeyword:       'name=  value=  comment=  scope=',
  movefile:            'destination=  [source=]',
  print:               'message (or bare: Print "hello")',
  pwd:                 '',
  readimages:          'path=',
  rejectcurrentframe:  '[index=]  [append=]',
  round:               '(#)',
  runmacro:            'name=',
  set:                 '<varname> = <value>',
  setframe:            'index=',
  showanalysisgraph:   '',
  showanalysisresults: '',
  sqrt:                '(#)',
  stackframes:         '',
  stripext:            '($path)',
  version:             '',
  writecurrent:        '',
  writefit:            'destination=  [overwrite=]  [stack=]',
  writeframe:          '',
  writetiff:           'destination=  overwrite=',
  writexisf:           'destination=  overwrite=  compress=  stack=',
};

// ---------------------------------------------------------------------------
// 3. Help database
// ---------------------------------------------------------------------------

export interface HelpArgument {
  name:        string;
  type:        string;
  required:    boolean;
  default?:    string;
  description: string;
}

export interface HelpEntry {
  name:        string;
  description: string;
  syntax:      string;
  arguments:   HelpArgument[];
  output:      string;
  example:     string;
}

export const HELP_DB: Record<string, HelpEntry> = {

  //    File / Session

  addfiles: {
    name:        'AddFiles',
    description: 'Appends one or more files to the session. Accepts explicit file paths, glob patterns, or a mix of both in a comma-separated list. Files already loaded are skipped. Use ClearSession first if you want to start fresh.',
    syntax:      'AddFiles paths=<path|glob>[,<path|glob>...]',
    arguments: [
      { name: 'paths', type: 'string', required: true, description: 'Comma-separated list of file paths and/or glob patterns. Wildcards: * matches any characters, ? matches one character, [...] matches a character class. Unmatched glob patterns produce a warning rather than an error.' },
    ],
    output:  'Appends matched files to the session file list. Reports load count, estimated memory usage, and any glob warnings.',
    example: 'AddFiles paths="/data/M31/frame001.fit,/data/M31/frame002.fit"\nAddFiles paths="/data/M31/lights/*.fit"\nAddFiles paths="J:/projects/M82/M82-*-sess-*/lights/*.fit"\nAddFiles paths="/data/M31/lights/*.fit,/data/M31/extra/frame099.fit"',
  },

  readimages: {
    name:        'ReadImages',
    description: 'Loads a single image file or all supported images in a directory (FITS, XISF, TIFF) into the session. Files already loaded are skipped.',
    syntax:      'ReadImages path=<path>',
    arguments: [
      { name: 'path', type: 'path', required: true, description: 'Full path to a single image file or a directory' },
    ],
    output:  'Appends the loaded file(s) to the session file list. Skips duplicates.',
    example: 'ReadImages path="/home/stan/astro-data/M31/lights"\nReadImages path="/home/stan/astro-data/M31/lights/frame001.xisf"',
  },

  clearsession: {
    name:        'ClearSession',
    description: 'Clears all files and state from the current session.',
    syntax:      'ClearSession',
    arguments:   [],
    output:  'Empties the file list, clears the viewer, and resets session state.',
    example: 'ClearSession',
  },

  loadfile: {
    name:        'LoadFile',
    description: 'Loads a single file for display without adding it to the session file list. Stores the path in $LOAD_FILE_PATH.',
    syntax:      'LoadFile path=<path>',
    arguments: [
      { name: 'path', type: 'path', required: true, description: 'Full path to the file to load' },
    ],
    output:  'Displays the file in the viewer. Does not affect the session file list.',
    example: 'LoadFile path="/data/Heatmaps/fwhm_heatmap.xisf"',
  },

  //    Write

  writefit: {
    name:        'WriteFIT',
    description: 'Writes session files to a destination directory in FITS format. Use stack=true to write the transient stack result as a single file instead. The .fit extension is appended automatically if not specified.',
    syntax:      'WriteFIT destination=<path> [overwrite=<bool>] [stack=<bool>]',
    arguments: [
      { name: 'destination', type: 'path',    required: true,  description: 'Output directory (session frames) or file path (stack=true). Extension .fit appended automatically if omitted.' },
      { name: 'overwrite',   type: 'boolean', required: false, default: 'false', description: 'Overwrite existing files' },
      { name: 'stack',       type: 'boolean', required: false, default: 'false', description: 'Write the transient stack result as a single FITS file instead of all session frames' },
    ],
    output:  'Writes FITS file(s) to the destination path. When stack=true, stores the output path in $STACKED.',
    example: 'WriteFIT destination="/data/Output" overwrite=true\nWriteFIT destination="/data/masters/flat_master" stack=true\nPrint $STACKED',
  },

  writetiff: {
    name:        'WriteTIFF',
    description: 'Writes all session files to a destination directory in TIFF format with AstroTIFF keyword embedding.',
    syntax:      'WriteTIFF destination=<path> [overwrite=<bool>]',
    arguments: [
      { name: 'destination', type: 'path',    required: true,  description: 'Directory to write files to' },
      { name: 'overwrite',   type: 'boolean', required: false, default: 'false', description: 'Whether to overwrite existing files' },
    ],
    output:  'Writes all session files to the destination directory.',
    example: 'WriteTIFF destination="/data/Output" overwrite=true',
  },

  writexisf: {
    name:        'WriteXISF',
    description: 'Writes all session files to a destination directory in XISF format. Use stack=true to export the transient stack result instead.',
    syntax:      'WriteXISF destination=<path> [overwrite=<bool>] [compress=<bool>] [stack=<bool>]',
    arguments: [
      { name: 'destination', type: 'path',    required: true,  description: 'Directory to write files to' },
      { name: 'overwrite',   type: 'boolean', required: false, default: 'false', description: 'Whether to overwrite existing files' },
      { name: 'compress',    type: 'boolean', required: false, default: 'false', description: 'Whether to apply LZ4HC XISF compression' },
      { name: 'stack',       type: 'boolean', required: false, default: 'false', description: 'Write the transient stack result instead of session files' },
    ],
    output:  'Writes files to the destination directory. When stack=true, stores the output path in $STACKED.',
    example: 'WriteXISF destination="/data/Output" overwrite=true compress=false\nWriteXISF destination="/data/Output" stack=true\nPrint $STACKED',
  },

  writecurrent: {
    name:        'WriteCurrent',
    description: 'Writes all buffered images back to their source paths in their original format. For FITS files this updates keywords only, leaving pixel data untouched; XISF and TIFF get a full rewrite via an atomic temp-rename.',
    syntax:      'WriteCurrent',
    arguments:   [],
    output:  'Overwrites each source file with the current in-memory keyword/pixel state (see description for the FITS keyword-only caveat).',
    example: 'WriteCurrent',
  },

  writeframe: {
    name:        'WriteFrame',
    description: 'Writes the currently active frame only back to its source format using an atomic temp-rename. Unlike WriteCurrent, this always performs a full pixel + keyword rewrite, including for FITS files.',
    syntax:      'WriteFrame',
    arguments:   [],
    output:  'Overwrites the current frame source file with the in-memory buffer.',
    example: 'WriteFrame',
  },

  copyfile: {
    name:        'CopyFile',
    description: 'Copies a file to a destination directory. Uses the current frame if no source is specified. Stores the destination path in $NEW_FILE.',
    syntax:      'CopyFile destination=<path> [source=<path>]',
    arguments: [
      { name: 'destination', type: 'path', required: true,  description: 'Destination directory path (created automatically if needed)' },
      { name: 'source',      type: 'path', required: false, description: 'Source file path (default: current frame)' },
    ],
    output:  'Copies the file to the destination directory. Source file and session are unchanged. Stores destination path in $NEW_FILE.',
    example: 'CopyFile destination="/data/Backups"\nCopyFile source="$NEW_FILE" destination="/data/Heatmaps"',
  },

  movefile: {
    name:        'MoveFile',
    description: 'Moves a file to a destination. Uses the current frame if no source is specified. If the destination is an existing directory, the file is moved into it preserving its filename. If the destination is a full file path (mv semantics), the file is moved and renamed in one step. The destination parent directory is created automatically if needed. Stores the destination path in $NEW_FILE.',
    syntax:      'MoveFile destination=<path> [source=<path>]',
    arguments: [
      { name: 'destination', type: 'path', required: true,  description: 'Destination directory path, or full destination file path for rename-during-move.' },
      { name: 'source',      type: 'path', required: false, description: 'Source file path (default: current frame). May be a file outside the session.' },
    ],
    output:  'Moves (and optionally renames) the file. Stores the destination path in $NEW_FILE. Removes it from the session file list if it was a session file.',
    example: 'MoveFile destination="/data/Rejects"\nMoveFile source="$f" destination="/data/Rejects"\n# Rename during move (mv semantics):\nSet cleaned = stripext($f)\nMoveFile source="$f" destination="$cleaned"',
  },

  filterbykeyword: {
    name:        'FilterByKeyword',
    description: 'Filters the active session file list to only those frames where the specified keyword matches the given value. Frames that do not match are removed from the session.',
    syntax:      'FilterByKeyword name=<string> value=<string>',
    arguments: [
      { name: 'name',  type: 'string', required: true, description: 'Keyword name to filter on' },
      { name: 'value', type: 'string', required: true, description: 'Value to match (case-insensitive)' },
    ],
    output:  'Reduces the session file list to matching frames only.',
    example: 'FilterByKeyword name=FILTER value=Ha\nFilterByKeyword name=OBJECT value="M31"',
  },

  //    Stacking

  stackframes: {
    name:        'StackFrames',
    description: 'Stacks loaded frames using FFT alignment, triangle rigid refinement, meridian-flip-aware group reference selection, and two-pass sigma-clipped mean combination.',
    syntax:      'StackFrames',
    arguments:   [],
    output:  'Produces a transient stacked ImageBuffer in the Stacking Workspace. Reports progress and a quality summary to the console.',
    example: 'StackFrames\nReadImages path="/data/lights"\nStackFrames\nCommitStretch shadow_clip=-3.5 target_bg=0.10\nWriteXISF destination="/data/output" stack=true',
  },

  clearstack: {
    name:        'ClearStack',
    description: 'Discards the transient stack result and per-frame contribution data.',
    syntax:      'ClearStack',
    arguments:   [],
    output:  'Clears the stack buffer. The Stacking Workspace viewer closes.',
    example: 'ClearStack',
  },

  commitstretch: {
    name:        'CommitStretch',
    description: 'Permanently applies the Auto-STF stretch to the stack result pixel buffer. After committing, the stack buffer holds non-linear (stretched) pixel data. Use WriteXISF stack=true to export.',
    syntax:      'CommitStretch [shadow_clip=<float>] [target_bg=<float>]',
    arguments: [
      { name: 'shadow_clip', type: 'float', required: false, description: 'Shadow clipping factor (default: context value)' },
      { name: 'target_bg',   type: 'float', required: false, description: 'Target background value 0.0-1.0 (default: context value)' },
    ],
    output:  'Modifies the stack result pixel buffer in place.',
    example: 'CommitStretch shadow_clip=-3.5 target_bg=0.10',
  },

  //    Keywords

  addkeyword: {
    name:        'AddKeyword',
    description: 'Adds or replaces a FITS keyword on loaded images.',
    syntax:      'AddKeyword name=<string> value=<string> [comment=<string>] [scope=all|current]',
    arguments: [
      { name: 'name',    type: 'string', required: true,  description: 'Keyword name (max 8 characters)' },
      { name: 'value',   type: 'string', required: true,  description: 'Keyword value' },
      { name: 'comment', type: 'string', required: false, description: 'Optional FITS comment' },
      { name: 'scope',   type: 'string', required: false, default: 'all', description: 'Apply to all frames or current frame only' },
    ],
    output:  'Adds or updates the keyword in the specified frame(s).',
    example: 'AddKeyword name=TELESCOP value="Celestron EdgeHD 8" comment="Telescope used"',
  },

  deletekeyword: {
    name:        'DeleteKeyword',
    description: 'Removes a FITS keyword from loaded images.',
    syntax:      'DeleteKeyword name=<string> [scope=all|current]',
    arguments: [
      { name: 'name',  type: 'string', required: true,  description: 'Keyword name to delete' },
      { name: 'scope', type: 'string', required: false, default: 'all', description: 'Apply to all frames or current frame only' },
    ],
    output:  'Removes the keyword from the specified frame(s).',
    example: 'DeleteKeyword name=EXPTIME scope=all',
  },

  modifykeyword: {
    name:        'ModifyKeyword',
    description: 'Changes the value of an existing FITS keyword.',
    syntax:      'ModifyKeyword name=<string> value=<string> [comment=<string>] [scope=all|current]',
    arguments: [
      { name: 'name',    type: 'string', required: true,  description: 'Keyword name to modify' },
      { name: 'value',   type: 'string', required: true,  description: 'New keyword value' },
      { name: 'comment', type: 'string', required: false, description: 'New comment (optional)' },
      { name: 'scope',   type: 'string', required: false, default: 'all', description: 'Apply to all frames or current frame only' },
    ],
    output:  'Updates the keyword value in the specified frame(s).',
    example: 'ModifyKeyword name=OBJECT value="M31 Andromeda" scope=all',
  },

  copykeyword: {
    name:        'CopyKeyword',
    description: 'Copies a keyword value from one keyword name to another.',
    syntax:      'CopyKeyword from=<string> to=<string> [scope=all|current]',
    arguments: [
      { name: 'from',  type: 'string', required: true,  description: 'Source keyword name' },
      { name: 'to',    type: 'string', required: true,  description: 'Destination keyword name' },
      { name: 'scope', type: 'string', required: false, default: 'all', description: 'Apply to all frames or current frame only' },
    ],
    output:  'Creates or updates the destination keyword with the value from the source keyword, on the frame(s) in scope.',
    example: 'CopyKeyword from=EXPTIME to=EXPOSURE\nCopyKeyword from=EXPTIME to=EXPOSURE scope=current',
  },

  listkeywords: {
    name:        'ListKeywords',
    description: 'Lists all FITS header keywords for the current frame.',
    syntax:      'ListKeywords',
    arguments:   [],
    output:  'Outputs all keywords as formatted name = value / comment lines, sorted alphabetically.',
    example: 'ListKeywords',
  },

  getkeyword: {
    name:        'GetKeyword',
    description: 'Retrieves a FITS keyword value from the current frame and stores it as a script variable. The variable name is the keyword name uppercased. If the keyword is not found and default= is given, the default value is stored instead of halting the script.',
    syntax:      'GetKeyword name=<string> [default=<string>]',
    arguments: [
      { name: 'name',    type: 'string', required: true,  description: 'Keyword name to retrieve' },
      { name: 'default', type: 'string', required: false, description: 'Fallback value to use if the keyword is not found on the current frame, instead of halting the script (e.g. default="" or default="NULL"). Does not apply to no-frame-loaded or no-buffer errors.' },
    ],
    output:  'Stores the keyword value in $<NAME> (uppercase). Example: GetKeyword name=FILTER stores result in $FILTER.',
    example: 'GetKeyword name=FILTER\nPrint $FILTER\n\nGetKeyword name=OBJECT default=""\nIf $OBJECT == ""\n  Print "OBJECT keyword not set"\nEndIf',
  },

  //    Analysis

  analyzeframes: {
    name:        'AnalyzeFrames',
    description: 'Computes five quality metrics for loaded frames (FWHM, eccentricity, star count, signal weight, background median). With scope=all (default), classifies each frame as PASS or REJECT via iterative sigma clipping across the session. With scope=current, runs the same metrics on only the current frame and reports raw values — no session stats or classification.',
    syntax:      'AnalyzeFrames [profile=<name>] [scope=all|current] [threshold=<float>] [saturation=<float>]',
    arguments: [
      { name: 'profile',    type: 'string', required: false, description: 'Threshold profile name to use for this run (e.g. profile=Session). If omitted, uses the active profile set in Edit > Analysis Parameters.' },
      { name: 'scope',      type: 'string', required: false, default: 'all', description: 'all runs the full session analysis and classification; current inspects only the current frame.' },
      { name: 'threshold',  type: 'float',  required: false, default: '5.0', description: 'Star detection threshold in units of background std dev' },
      { name: 'saturation', type: 'float',  required: false, default: '0.98', description: 'Saturation threshold — stars at or above this value are rejected from detection' },
    ],
    output:  'Populates analysis results for all frames (or the current frame, with scope=current). Results visible in Analysis Results and Analysis Graph views.',
    example: 'AnalyzeFrames\nAnalyzeFrames profile=Session\nAnalyzeFrames profile=Project\nAnalyzeFrames scope=current',
  },

  autostretch: {
    name:        'AutoStretch',
    description: 'Applies an automatic stretch to the current frame for display using the PixInsight-compatible Auto-STF algorithm. The raw pixel buffer is not modified.',
    syntax:      'AutoStretch [shadowClip=<float>] [targetBackground=<float>]',
    arguments: [
      { name: 'shadowClip',       type: 'float', required: false, default: '-2.8',  description: 'Shadow clipping point in sigma units' },
      { name: 'targetBackground', type: 'float', required: false, default: '0.15',  description: 'Target background level (0.0-1.0)' },
    ],
    output:  'Updates the viewer with the stretched image. Raw buffer unchanged.',
    example: 'AutoStretch shadowClip=-2.8 targetBackground=0.25',
  },

  computefwhm: {
    name:        'ComputeFWHM',
    description: 'Computes the median Full Width at Half Maximum (FWHM) for detected stars in the current frame, in pixels (and arcseconds when FOCALLEN, INSTRUME, and XBINNING keywords are present). Displays per-star circle annotations on the viewer overlay.',
    syntax:      'ComputeFWHM [threshold=<float>] [peak_radius=<int>] [saturation=<float>]',
    arguments: [
      { name: 'threshold',   type: 'float',   required: false, default: '5.0', description: 'Star detection threshold in units of background std dev' },
      { name: 'peak_radius', type: 'integer', required: false, default: '3',   description: 'Radius in pixels for the local-maximum test' },
      { name: 'saturation',  type: 'float',   required: false, default: '0.98', description: 'Stars at or above this peak value are rejected as saturated' },
    ],
    output:  'Displays star overlay annotations. Stores result in $fwhm.',
    example: 'ComputeFWHM\nPrint $fwhm',
  },

  computeeccentricity: {
    name:        'ComputeEccentricity',
    description: 'Computes the mean star eccentricity for the current frame. Values close to 0 indicate round stars; values close to 1 indicate elongated stars.',
    syntax:      'ComputeEccentricity [threshold=<float>] [peak_radius=<int>] [saturation=<float>]',
    arguments: [
      { name: 'threshold',   type: 'float',   required: false, default: '5.0', description: 'Star detection threshold in units of background std dev' },
      { name: 'peak_radius', type: 'integer', required: false, default: '3',   description: 'Radius in pixels for the local-maximum test' },
      { name: 'saturation',  type: 'float',   required: false, default: '0.98', description: 'Stars at or above this peak value are rejected as saturated' },
    ],
    output:  'Stores result in $eccentricity.',
    example: 'ComputeEccentricity\nPrint $eccentricity',
  },

  countstars: {
    name:        'CountStars',
    description: 'Counts the number of detected stars in the current frame using peak-finding on a sigma-clipped, background-subtracted image.',
    syntax:      'CountStars [threshold=<float>] [peak_radius=<int>] [flood_threshold=<float>] [saturation=<float>] [sigma=<float>] [iterations=<int>]',
    arguments: [
      { name: 'threshold',       type: 'float',   required: false, default: '5.0', description: 'Detection threshold in units of background std dev' },
      { name: 'peak_radius',     type: 'integer', required: false, default: '3',   description: 'Radius in pixels for the local-maximum test' },
      { name: 'flood_threshold', type: 'float',   required: false, default: '2.0', description: 'Flood-fill lower bound in units of background std dev' },
      { name: 'saturation',      type: 'float',   required: false, default: '0.98', description: 'Peak value at or above which a star is considered saturated and rejected' },
      { name: 'sigma',           type: 'float',   required: false, default: '3.0', description: 'Sigma-clipping threshold for background estimation' },
      { name: 'iterations',      type: 'integer', required: false, default: '5',   description: 'Maximum sigma-clipping iterations for background estimation' },
    ],
    output:  'Stores result in $starcount.',
    example: 'CountStars\nPrint $starcount',
  },

  gethistogram: {
    name:        'GetHistogram',
    description: 'Computes the histogram and basic statistics for the current frame.',
    syntax:      'GetHistogram',
    arguments:   [],
    output:  'Returns statistics including median, std dev, and clipping percentage.',
    example: 'GetHistogram',
  },

  contourheatmap: {
    name:        'ContourHeatmap',
    description: 'Generates a false-color spatial FWHM heatmap for the current frame. Writes the result as an XISF file named <source_stem>_heatmap.xisf to the source file\'s directory and stores the output path in $NEW_FILE.',
    syntax:      'ContourHeatmap [palette=viridis|plasma|coolwarm] [contour_levels=<int>] [threshold=<float>] [saturation=<float>]',
    arguments: [
      { name: 'palette',        type: 'string',  required: false, default: 'viridis', description: 'Color palette: viridis, plasma, or coolwarm' },
      { name: 'contour_levels', type: 'integer', required: false, default: '10',      description: 'Number of contour levels (minimum 2)' },
      { name: 'threshold',      type: 'float',   required: false, default: '5.0',     description: 'Star detection threshold in units of background std dev' },
      { name: 'saturation',     type: 'float',   required: false, default: '0.98',    description: 'Stars at or above this peak value are rejected as saturated' },
    ],
    output:  'Generates a heatmap XISF and loads it in the viewer. Stores path in $NEW_FILE.',
    example: 'ContourHeatmap palette=plasma contour_levels=12',
  },

  backgroundmedian: {
    name:        'BackgroundMedian',
    description: 'Computes the sigma-clipped background median for the current frame. This is one of the five metrics AnalyzeFrames computes internally for every frame; running it standalone is useful for inspecting or tuning background estimation on a single frame.',
    syntax:      'BackgroundMedian [sigma=<float>] [iterations=<int>] [grid=<int>]',
    arguments: [
      { name: 'sigma',      type: 'float',   required: false, default: '3.0', description: 'Sigma-clipping threshold in std dev units' },
      { name: 'iterations', type: 'integer', required: false, default: '5',   description: 'Maximum sigma-clipping iterations' },
      { name: 'grid',       type: 'integer', required: false, default: '4',   description: 'Grid divisions per axis used internally for gradient estimation' },
    ],
    output:  'Reports the background median for the current frame.',
    example: 'BackgroundMedian\nBackgroundMedian sigma=2.5 iterations=8',
  },

  backgroundstddev: {
    name:        'BackgroundStdDev',
    description: 'Deprecated but fully operational. Computes the sigma-clipped background standard deviation for the current frame. No longer used by AnalyzeFrames — it correlated 0.92–0.999 with BackgroundMedian and added no discriminating signal — but still runs the full computation and returns real results when called directly.',
    syntax:      'BackgroundStdDev [sigma=<float>] [iterations=<int>] [grid=<int>]',
    arguments: [
      { name: 'sigma',      type: 'float',   required: false, default: '3.0', description: 'Sigma-clipping threshold in std dev units' },
      { name: 'iterations', type: 'integer', required: false, default: '5',   description: 'Maximum sigma-clipping iterations' },
      { name: 'grid',       type: 'integer', required: false, default: '4',   description: 'Grid divisions per axis used internally for gradient estimation' },
    ],
    output:  'Reports the background standard deviation for the current frame.',
    example: 'BackgroundStdDev',
  },

  backgroundgradient: {
    name:        'BackgroundGradient',
    description: 'Deprecated but fully operational. Computes a background gradient estimate for the current frame. No longer used by AnalyzeFrames due to session-dependent sign reversal that made it unreliable as a rejection criterion, but still runs when called directly.',
    syntax:      'BackgroundGradient [sigma=<float>] [iterations=<int>] [grid=<int>]',
    arguments: [
      { name: 'sigma',      type: 'float',   required: false, default: '3.0', description: 'Sigma-clipping threshold in std dev units' },
      { name: 'iterations', type: 'integer', required: false, default: '5',   description: 'Maximum sigma-clipping iterations' },
      { name: 'grid',       type: 'integer', required: false, default: '4',   description: 'Grid divisions per axis used internally for gradient estimation' },
    ],
    output:  'Reports the background gradient estimate for the current frame.',
    example: 'BackgroundGradient',
  },

  debayerimage: {
    name:        'DebayerImage',
    description: 'Debayers a Bayer CFA image to interleaved RGB using bilinear interpolation. Operates on the transient stack result if one exists, otherwise the current session frame. The Bayer pattern is always read from the BAYERPAT (or BAYER_PATTERN) keyword, defaulting to RGGB if neither is present. Takes no arguments — there is currently no way to override the pattern or interpolation method from pcode.',
    syntax:      'DebayerImage',
    arguments:   [],
    output:  'Converts the target buffer (stack result or current frame) from mono/Bayer to interleaved RGB in place. Frames already RGB are left unchanged.',
    example: 'DebayerImage',
  },

  commitanalysis: {
    name:        'CommitAnalysis',
    description: 'Moves all REJECT frames to a rejected/ subfolder within each frame\'s source directory and removes them from the session. Pass frames remain loaded. Optionally appends a suffix to each moved filename.',
    syntax:      'CommitAnalysis [append=<ext>]',
    arguments: [
      { name: 'append', type: 'string', required: false, default: '', description: 'Suffix appended after the original filename extension (e.g. append=.session produces frame.fit.session). Leading dot is optional. Defaults to no suffix.' },
    ],
    output:  'Reports pass count, reject count, and number of files moved.',
    example: 'CommitAnalysis\nCommitAnalysis append=.session\nCommitAnalysis append=.project',
  },

  exportanalysisreport: {
    name:        'ExportAnalysisReport',
    description: 'Exports the current analysis results as a Photyx session JSON file. If path is omitted, a filename is derived from the first frame and written to the system Downloads folder.',
    syntax:      'ExportAnalysisReport [path=<path>]',
    arguments: [
      { name: 'path', type: 'path', required: false, description: 'Full destination path for the JSON file. If omitted, written to the Downloads folder with an auto-derived filename.' },
    ],
    output:  'Writes the JSON file and reports the output path.',
    example: 'ExportAnalysisReport\nExportAnalysisReport path="$downloads/M82-Project-Duo-Analysis.json"',
  },

  //    Display & navigation

  setframe: {
    name:        'SetFrame',
    description: 'Sets the current active frame by zero-based index.',
    syntax:      'SetFrame index=<integer>',
    arguments: [
      { name: 'index', type: 'integer', required: true, description: 'Zero-based frame index' },
    ],
    output:  'Updates the viewer to display the specified frame.',
    example: 'SetFrame index=0',
  },

  cacheframes: {
    name:        'CacheFrames',
    description: 'Pre-builds the blink cache for all session frames in the background. Required before using BlinkSequence.',
    syntax:      'CacheFrames [resolution=<12|25>]',
    arguments: [
      { name: 'resolution', type: 'string', required: false, description: '12 (12.5%) or 25 (25%). If omitted, both resolutions are cached.' },
    ],
    output:  'Triggers background cache build. Required before using BlinkSequence.',
    example: 'CacheFrames\nCacheFrames resolution=25',
  },

  rejectcurrentframe: {
    name:        'RejectCurrentFrame',
    description: 'Moves a single frame to a rejected/ subfolder within its own source directory and removes it from the session and all display/blink caches. Defaults to the current frame if index is not specified. Unlike CommitAnalysis, this acts on one frame ad hoc and does not require AnalyzeFrames to have been run, and does not touch session-wide analysis results or stats.',
    syntax:      'RejectCurrentFrame [index=<integer>] [append=<string>]',
    arguments: [
      { name: 'index',  type: 'integer', required: false, description: 'Zero-based frame index to reject. Defaults to the current frame if omitted.' },
      { name: 'append', type: 'string',  required: false, default: 'reject', description: 'Suffix appended after the original filename extension (e.g. append=cloudy produces frame.fit.cloudy). Leading dot is optional.' },
    ],
    output:  'Moves the file to rejected/<filename>.<suffix> and removes it from the session. Reports the new path.',
    example: 'RejectCurrentFrame\nRejectCurrentFrame index=42\nRejectCurrentFrame append=cloudy',
  },

  clearannotations: {
    name:        'ClearAnnotations',
    description: 'Removes all star and analysis overlay annotations from the viewer.',
    syntax:      'ClearAnnotations',
    arguments:   [],
    output:  'Clears the annotation overlay.',
    example: 'ClearAnnotations',
  },

  showanalysisgraph: {
    name:        'ShowAnalysisGraph',
    description: 'Opens the Analysis Graph view in the viewer region.',
    syntax:      'ShowAnalysisGraph',
    arguments:   [],
    output:  'Switches the viewer to the Analysis Graph view.',
    example: 'AnalyzeFrames\nShowAnalysisGraph',
  },

  showanalysisresults: {
    name:        'ShowAnalysisResults',
    description: 'Opens the Analysis Results table view in the viewer region.',
    syntax:      'ShowAnalysisResults',
    arguments:   [],
    output:  'Switches the viewer to the Analysis Results table.',
    example: 'AnalyzeFrames\nShowAnalysisResults',
  },

  //    Scripting

  set: {
    name:        'Set',
    description: 'Assigns a value to a script variable. Supports arithmetic expressions, string concatenation, and path functions. String literals on the RHS must use double quotes.',
    syntax:      'Set <varname> = <expression>',
    arguments: [
      { name: 'varname',    type: 'string',     required: true, description: 'Variable name (no $ prefix)' },
      { name: 'expression', type: 'expression', required: true, description: 'Value or expression to assign' },
    ],
    output:  'Stores the result in $<varname> for use in subsequent commands.',
    example: 'Set x = 10\nSet label = "Frame " + $x\nSet sd = sqrt(($x - 5) ^ 2)\nSet dir = dirof($f)\nSet name = basename($f)\nSet clean = stripext($f)',
  },

  print: {
    name:        'Print',
    description: 'Outputs a value or expression to the console. Accepts bare expressions — Print $x + 1 and Print "hello" are both valid.',
    syntax:      'Print <expression>',
    arguments: [
      { name: 'message', type: 'expression', required: true, description: 'Value, variable, or expression to print' },
    ],
    output:  'Writes the evaluated result to the console.',
    example: 'Print "Hello world"\nPrint $x + 1\nPrint "FWHM: " + $fwhm\nPrint dirof($f) + "/" + basename($f)',
  },

  assert: {
    name:        'Assert',
    description: 'Halts macro execution with an error if the expression evaluates to false. Silent on pass in both Trace and No Trace modes.',
    syntax:      'Assert expression=<condition>',
    arguments: [
      { name: 'expression', type: 'condition', required: true, description: 'Boolean condition to test (e.g. $x > 0, $name == $expected)' },
    ],
    output:  'Silent on pass. Halts execution with ASSERT_FAILED error on failure.',
    example: 'Assert expression="$filecount > 0"',
  },

  countfiles: {
    name:        'CountFiles',
    description: 'Stores the number of files currently loaded in the session in $filecount.',
    syntax:      'CountFiles',
    arguments:   [],
    output:  'Stores result in $filecount.',
    example: 'CountFiles\nPrint $filecount',
  },

  countmatches: {
    name:        'CountMatches',
    description: 'Counts filesystem entries (files or directories) matching a glob pattern and stores the result in $matchcount. Useful for conditionally executing blocks only when matching entries exist.',
    syntax:      'CountMatches pattern=<glob>',
    arguments: [
      { name: 'pattern', type: 'string', required: true, description: 'Glob pattern to match. Supports *, ?, and [...] wildcards anywhere in the path.' },
    ],
    output:  'Stores match count in $matchcount.',
    example: 'CountMatches pattern="$project/*-duo-*"\nIf $matchcount > 0\n  Print "Found " + $matchcount + " duo sessions"\nEndIf',
  },

  getsystempath: {
    name:        'GetSystemPath',
    description: 'Retrieves a well-known system directory path and stores it in a variable named after the requested path. Supported names: downloads, documents, desktop, temp.',
    syntax:      'GetSystemPath name=<downloads|documents|desktop|temp>',
    arguments: [
      { name: 'name', type: 'string', required: true, description: 'System path to retrieve: downloads, documents, desktop, or temp. The result is stored in $<name> (e.g. name=downloads stores in $downloads).' },
    ],
    output:  'Stores the resolved path in $<name>, normalized to forward slashes.',
    example: 'GetSystemPath name=downloads\nPrint $downloads\nExportAnalysisReport path="$downloads/M82-Project-Analysis.json"\n\nGetSystemPath name=temp\nPrint $temp',
  },

  runmacro: {
    name:        'RunMacro',
    description: 'Executes a saved macro by name from the database. Inner command output and Print statements appear in the console line by line.',
    syntax:      'RunMacro name=<string>',
    arguments: [
      { name: 'name', type: 'string', required: true, description: 'Name of the macro to execute' },
    ],
    output:  'Executes all commands in the macro. Output appears in the console.',
    example: 'RunMacro name="my-workflow"',
  },

  log: {
    name:        'Log',
    description: 'Writes collected macro output since the last Log call to a file.',
    syntax:      'Log path=<path> [append=<bool>]',
    arguments: [
      { name: 'path',   type: 'path',    required: true,  description: 'Output log file path' },
      { name: 'append', type: 'boolean', required: false, default: 'false', description: 'Append to existing file instead of overwriting' },
    ],
    output:  'Writes output to the specified log file.',
    example: 'Log path="/logs/session.log" append=true',
  },

  //    Flow control

  if: {
    name:        'If',
    description: 'Begins a conditional block. The block executes if the expression evaluates to true.',
    syntax:      'If <expression>\n  ...\nEndIf\n\nIf <expression>\n  ...\nElse\n  ...\nEndIf',
    arguments: [
      { name: 'expression', type: 'condition', required: true, description: 'Boolean condition' },
    ],
    output:  'Executes the block conditionally.',
    example: 'If $fwhm > 3.0\n  Print "Poor focus"\nEndIf\n\nCountMatches pattern="$project/*-duo-*"\nIf $matchcount > 0\n  Print "Duo sessions found"\nEndIf',
  },

  for: {
    name:        'For',
    description: 'Two loop forms, both closed with EndFor:\n\n(1) Numeric range — iterates from N to M inclusive. Both bounds may be variables or expressions.\n\n(2) Glob iterator — expands a glob pattern at runtime and iterates over each matched path as a string, binding it to the loop variable. Unmatched patterns produce a warning and the body does not execute — the script continues rather than halting.\n\nLoops may be nested. Numeric and glob loops can be mixed.',
    syntax:      'For <var> = N To M\n  ...\nEndFor\n\nfor <var> in "<glob_pattern>"\n  ...\nEndFor',
    arguments:   [],
    output:  'Numeric: executes the block M-N+1 times. Glob: executes once per matched path, binding the full path string to $<var>.',
    example: 'For i = 1 To 5\n  Print "Frame " + $i\nEndFor\n\nfor d in "J:/projects/M82/M82-*-sess-*"\n  ClearSession\n  AddFiles paths="$d/lights/*.fit"\n  AnalyzeFrames profile="Session"\n  CommitAnalysis append=.session\nEndFor\n\n# Restore rejected files:\nfor f in "$project/*/lights/rejected/*.fit.session"\n  Set cleaned = stripext($f)\n  Set dest = dirof(dirof($f)) + "/" + basename($cleaned)\n  MoveFile source="$f" destination="$dest"\nEndFor',
  },

  //    Console only

  help: {
    name:        'Help',
    description: 'Displays help for a specific command, or lists all available commands.',
    syntax:      'Help [command]',
    arguments: [
      { name: 'command', type: 'string', required: false, description: 'Command name to get help for' },
    ],
    output:  'Opens the help modal for the specified command, or prints the command list.',
    example: 'Help\nHelp AutoStretch\nHelp Set\nHelp For',
  },

  clear: {
    name:        'Clear',
    description: 'Clears all output from the console.',
    syntax:      'Clear',
    arguments:   [],
    output:  'Empties the console output buffer.',
    example: 'Clear',
  },

  version: {
    name:        'Version',
    description: 'Displays the current Photyx and pcode version.',
    syntax:      'Version',
    arguments:   [],
    output:  'Prints version information to the console.',
    example: 'Version',
  },

  pwd: {
    name:        'pwd',
    description: 'Prints the unique source directories of all files currently loaded in the session.',
    syntax:      'pwd',
    arguments:   [],
    output:  'Outputs one directory path per line to the console.',
    example: 'pwd',
  },

//    Expression functions (searchable via help)

  abs: {
    name:        'abs()',
    description: 'Expression function. Returns the absolute value of a number.',
    syntax:      'abs(x)',
    arguments:   [],
    output:      'Returns a number.',
    example:     'Set a = abs(-5)\n# $a = 5\nSet a = abs($x - $mean)',
  },

  ceil: {
    name:        'ceil()',
    description: 'Expression function. Rounds a number up to the nearest integer.',
    syntax:      'ceil(x)',
    arguments:   [],
    output:      'Returns a number.',
    example:     'Set c = ceil(3.2)\n# $c = 4',
  },

  floor: {
    name:        'floor()',
    description: 'Expression function. Rounds a number down to the nearest integer.',
    syntax:      'floor(x)',
    arguments:   [],
    output:      'Returns a number.',
    example:     'Set f = floor(3.9)\n# $f = 3',
  },

  max: {
    name:        'max()',
    description: 'Expression function. Returns the larger of two numeric values.',
    syntax:      'max(x, y)',
    arguments:   [],
    output:      'Returns a number.',
    example:     'Set m = max($a, $b)\nSet clipped = max($value, 0)',
  },

  min: {
    name:        'min()',
    description: 'Expression function. Returns the smaller of two numeric values.',
    syntax:      'min(x, y)',
    arguments:   [],
    output:      'Returns a number.',
    example:     'Set m = min($a, $b)\nSet clipped = min($value, 65535)',
  },

  round: {
    name:        'round()',
    description: 'Expression function. Rounds a number to the nearest integer.',
    syntax:      'round(x)',
    arguments:   [],
    output:      'Returns a number.',
    example:     'Set r = round(3.5)\n# $r = 4\nSet r = round($fwhm)',
  },

  sqrt: {
    name:        'sqrt()',
    description: 'Expression function. Returns the square root of a number. Errors if the argument is negative.',
    syntax:      'sqrt(x)',
    arguments:   [],
    output:      'Returns a number.',
    example:     'Set s = sqrt(9)\n# $s = 3\nSet sigma = sqrt(($x - $mean) ^ 2)',
  },

  basename: {
    name:        'basename()',
    description: 'Expression function. Returns the filename portion of a path, stripping all leading directory components. Path separators are normalized before processing.',
    syntax:      'basename($path)',
    arguments:   [],
    output:  'Returns a string containing only the filename.',
    example: 'Set name = basename($f)\n# If $f = "/data/lights/frame001.fit.session"\n# $name = "frame001.fit.session"\nPrint basename($f)',
  },

  dirof: {
    name:        'dirof()',
    description: 'Expression function. Returns the directory portion of a path, stripping the filename. Path separators are normalized to forward slashes. Returns "." if no directory component exists.',
    syntax:      'dirof($path)',
    arguments:   [],
    output:  'Returns a string containing the directory path.',
    example: 'Set dir = dirof($f)\n# If $f = "/data/lights/rejected/frame001.fit"\n# $dir = "/data/lights/rejected"\n\n# Walk up two levels:\nSet parent = dirof(dirof($f))',
  },

  stripext: {
    name:        'stripext()',
    description: 'Expression function. Strips any suffix appended after the last known image extension (.fit, .fits, .fts, .xisf). Used to remove .session or .project suffixes added by CommitAnalysis. Returns the path unchanged if no known image extension is found.',
    syntax:      'stripext($path)',
    arguments:   [],
    output:  'Returns the path with the trailing suffix removed.',
    example: 'Set cleaned = stripext($f)\n# If $f = "/data/lights/rejected/frame001.fit.session"\n# $cleaned = "/data/lights/rejected/frame001.fit"\n\n# Full restore pattern:\nSet cleaned = stripext($f)\nSet dest = dirof(dirof($f)) + "/" + basename($cleaned)\nMoveFile source="$f" destination="$dest"',
  },

};

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/// Look up a help entry by command name (case-insensitive).
export function getHelp(command: string): HelpEntry | null {
  return HELP_DB[command.toLowerCase()] ?? null;
}

/// Extracts a clean, human-readable label for the running-status bar.
/// For a `RunMacro name="..."` line (quoted or bare), returns just the
/// macro name with hyphens converted back to spaces (macro `name` is
/// auto-derived from `display_name` by replacing spaces with hyphens,
/// so this reverses that transformation without a DB lookup).
/// Any other command is returned unchanged.
export function extractRunningLabel(command: string): string {
  const match = command.match(/^RunMacro\s+name=(?:"([^"]+)"|(\S+))/i);
  if (!match) return command;
  return (match[1] ?? match[2]).replace(/-/g, ' ');
}

// --------------------------------------------------------------------------
