// pcode.ts   Single source of truth for all pcode command metadata.
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

export const PCODE_COMMANDS = new Set([
  //    Session
  'ClearSession',
  //    Read commands
  'AddFiles',
  'ReadImages',
  //    Write commands
  'WriteCurrent',
  'WriteFIT',
  'WriteFrame',
  'WriteTIFF',
  'WriteXISF',
  //    Keyword commands
  'AddKeyword',
  'CopyKeyword',
  'DeleteKeyword',
  'GetKeyword',
  'ListKeywords',
  'ModifyKeyword',
  //    Stacking
  'ClearStack',
  'CommitStretch',
  'StackFrames',
  //    Image analysis
  'AnalyzeFrames',
  'CommitAnalysis',
  'ExportAnalysisReport',
  'ComputeEccentricity',
  'ComputeFWHM',
  'ContourHeatmap',
  'CountStars',
  'GetHistogram',
  'MedianValue',
  //    Image processing
  'AutoStretch',
  'BinImage',
  'DebayerImage',
  //    Display & navigation
  'BlinkSequence',
  'CacheFrames',
  'SetFrame',
  'SetZoom',
  //    Scripting
  'Assert',
  'CountFiles',
  'CountMatches',
  'Else',
  'EndFor',
  'EndIf',
  'For',
  'GetSystemPath',
  'If',
  'LoadFile',
  'Log',
  'Print',
  'RunMacro',
  'Set',
  //    File management
  'CopyFile',
  'FilterByKeyword',
  'MoveFile',
  //    Console built-ins
  'Clear',
  'Help',
  //    Client command handlers
  'ClearAnnotations',
  'pwd',
  'ShowAnalysisGraph',
  'ShowAnalysisResults',
  'Version',
]);

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
  analyzeframes:       '[profile=]',
  assert:              'expression=',
  autostretch:         'shadowClip=  targetBackground=',
  basename:            '($path)',
  binimage:            'factor=',
  blinksequence:       'fps=',
  cacheframes:         '',
  ceil:                '(#)',
  clear:               '',
  clearannotations:    '',
  clearsession:        '',
  clearstack:          '',
  commitanalysis:      '[append=]',
  commitstretch:       'shadow_clip=  target_bg=',
  computeeccentricity: '',
  computefwhm:         '',
  contourheatmap:      'palette=[viridis|plasma|coolwarm]  contour_levels=#  threshold=  saturation=',
  copyfile:            'destination=  source=',
  copykeyword:         'from=  to=',
  countfiles:          '',
  countmatches:        'pattern=<glob>',
  debayerimage:        'pattern=[RGGB|BGGR|GRBG|GBRG]  method=[bilinear]',
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
  getkeyword:          'name=',
  getsystempath:       'name=[downloads|documents|desktop|temp]',
  help:                '',
  if:                  '',
  listkeywords:        '',
  loadfile:            'path=',
  log:                 'path=  append=',
  max:                 '(#,#)',
  medianvalue:         '',
  min:                 '(#,#)',
  modifykeyword:       'name=  value=  comment=  scope=',
  movefile:            'destination=  [source=]',
  print:               'message (or bare: Print "hello")',
  pwd:                 '',
  readimages:          'path=',
  round:               '(#)',
  runmacro:            'name=',
  set:                 '<varname> = <value>',
  setframe:            'index=',
  setzoom:             'level=[fit|25|50|100|200]',
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
    description: 'Writes all buffered images back to their source paths in their original format using an atomic temp-rename.',
    syntax:      'WriteCurrent',
    arguments:   [],
    output:  'Overwrites each source file with the current in-memory buffer.',
    example: 'WriteCurrent',
  },

  writeframe: {
    name:        'WriteFrame',
    description: 'Writes the currently active frame only back to its source format using an atomic temp-rename.',
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
    description: 'Moves a file to a destination. Uses the current frame if no source is specified. If the destination is an existing directory, the file is moved into it preserving its filename. If the destination is a full file path (mv semantics), the file is moved and renamed in one step. The destination parent directory is created automatically if needed.',
    syntax:      'MoveFile destination=<path> [source=<path>]',
    arguments: [
      { name: 'destination', type: 'path', required: true,  description: 'Destination directory path, or full destination file path for rename-during-move.' },
      { name: 'source',      type: 'path', required: false, description: 'Source file path (default: current frame). May be a file outside the session.' },
    ],
    output:  'Moves (and optionally renames) the file. Removes it from the session file list if it was a session file.',
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
    description: 'Copies a keyword value from one keyword name to another in the current frame.',
    syntax:      'CopyKeyword from=<string> to=<string>',
    arguments: [
      { name: 'from', type: 'string', required: true, description: 'Source keyword name' },
      { name: 'to',   type: 'string', required: true, description: 'Destination keyword name' },
    ],
    output:  'Creates or updates the destination keyword with the value from the source keyword.',
    example: 'CopyKeyword from=EXPTIME to=EXPOSURE',
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
    description: 'Retrieves a FITS keyword value from the current frame and stores it as a script variable. The variable name is the keyword name uppercased.',
    syntax:      'GetKeyword name=<string>',
    arguments: [
      { name: 'name', type: 'string', required: true, description: 'Keyword name to retrieve' },
    ],
    output:  'Stores the keyword value in $<NAME> (uppercase). Example: GetKeyword name=FILTER stores result in $FILTER.',
    example: 'GetKeyword name=FILTER\nPrint $FILTER',
  },

  //    Analysis

  analyzeframes: {
    name:        'AnalyzeFrames',
    description: 'Computes five quality metrics for all loaded frames (FWHM, eccentricity, star count, signal weight, background median) and classifies each frame as PASS or REJECT using iterative sigma clipping.',
    syntax:      'AnalyzeFrames [profile=<name>]',
    arguments: [
      { name: 'profile', type: 'string', required: false, description: 'Threshold profile name to use for this run (e.g. profile=Session). If omitted, uses the active profile set in Edit > Analysis Parameters.' },
    ],
    output:  'Populates analysis results for all frames. Results visible in Analysis Results and Analysis Graph views.',
    example: 'AnalyzeFrames\nAnalyzeFrames profile=Session\nAnalyzeFrames profile=Project',
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
    description: 'Computes the Full Width at Half Maximum (FWHM) for detected stars in the current frame. Displays per-star circle annotations on the viewer overlay.',
    syntax:      'ComputeFWHM',
    arguments:   [],
    output:  'Displays star overlay annotations. Stores result in $fwhm.',
    example: 'ComputeFWHM\nPrint $fwhm',
  },

  computeeccentricity: {
    name:        'ComputeEccentricity',
    description: 'Computes the mean star eccentricity for the current frame. Values close to 0 indicate round stars; values close to 1 indicate elongated stars.',
    syntax:      'ComputeEccentricity',
    arguments:   [],
    output:  'Stores result in $eccentricity.',
    example: 'ComputeEccentricity\nPrint $eccentricity',
  },

  countstars: {
    name:        'CountStars',
    description: 'Counts the number of detected stars in the current frame.',
    syntax:      'CountStars',
    arguments:   [],
    output:  'Stores result in $starcount.',
    example: 'CountStars\nPrint $starcount',
  },

  medianvalue: {
    name:        'MedianValue',
    description: 'Returns the median pixel value per channel for the current frame.',
    syntax:      'MedianValue',
    arguments:   [],
    output:  'Outputs median value(s) to the console.',
    example: 'MedianValue',
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
    description: 'Generates a false-color spatial FWHM heatmap for the current frame. Writes the result as an XISF file to the source file\'s directory and stores the output path in $NEW_FILE.',
    syntax:      'ContourHeatmap [palette=viridis|plasma|coolwarm] [contour_levels=<int>] [threshold=<float>] [saturation=<float>]',
    arguments: [
      { name: 'palette',        type: 'string',  required: false, default: 'viridis', description: 'Color palette: viridis, plasma, or coolwarm' },
      { name: 'contour_levels', type: 'integer', required: false, default: '10',      description: 'Number of contour levels' },
      { name: 'threshold',      type: 'float',   required: false,                     description: 'Rejection threshold for outlier pixels' },
      { name: 'saturation',     type: 'float',   required: false, default: '1.0',     description: 'Color saturation multiplier' },
    ],
    output:  'Generates a heatmap XISF and loads it in the viewer. Stores path in $NEW_FILE.',
    example: 'ContourHeatmap palette=plasma contour_levels=12',
  },

  debayerimage: {
    name:        'DebayerImage',
    description: 'Debayers a Bayer CFA image on demand using bilinear interpolation. The Bayer pattern is read from the BAYERPAT keyword if present; the pattern= argument overrides it.',
    syntax:      'DebayerImage [pattern=RGGB|BGGR|GRBG|GBRG] [method=bilinear]',
    arguments: [
      { name: 'pattern', type: 'string', required: false, default: 'RGGB',     description: 'Bayer CFA pattern (overrides BAYERPAT keyword if present)' },
      { name: 'method',  type: 'string', required: false, default: 'bilinear', description: 'Interpolation method (currently only bilinear is supported)' },
    ],
    output:  'Converts the current frame from mono Bayer to interleaved RGB in place.',
    example: 'DebayerImage pattern=RGGB\nDebayerImage pattern=BGGR method=bilinear',
  },

  binimage: {
    name:        'BinImage',
    description: 'Bins the current image by an integer factor, reducing resolution by averaging pixel blocks.',
    syntax:      'BinImage factor=<integer>',
    arguments: [
      { name: 'factor', type: 'integer', required: true, description: 'Binning factor (e.g. 2 = 2x2 bin, halves each dimension)' },
    ],
    output:  'Replaces the current frame with the binned result.',
    example: 'BinImage factor=2',
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

  setzoom: {
    name:        'SetZoom',
    description: 'Sets the viewer zoom level.',
    syntax:      'SetZoom level=<fit|25|50|100|200>',
    arguments: [
      { name: 'level', type: 'string', required: true, description: 'Zoom level: fit, 25, 50, 100, or 200' },
    ],
    output:  'Updates the viewer zoom.',
    example: 'SetZoom level=fit\nSetZoom level=100',
  },

  cacheframes: {
    name:        'CacheFrames',
    description: 'Pre-builds the blink cache for all session frames at both resolutions in the background.',
    syntax:      'CacheFrames',
    arguments:   [],
    output:  'Triggers background cache build. Required before using BlinkSequence.',
    example: 'CacheFrames',
  },

  blinksequence: {
    name:        'BlinkSequence',
    description: 'Starts blinking through all session frames in sequence for visual quality inspection. Requires CacheFrames to have been run first.',
    syntax:      'BlinkSequence [fps=<float>]',
    arguments: [
      { name: 'fps', type: 'float', required: false, default: '2.0', description: 'Frames per second for blink playback' },
    ],
    output:  'Activates the blink viewer.',
    example: 'CacheFrames\nBlinkSequence fps=3',
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
    description: 'Two loop forms, both closed with EndFor:\n\n(1) Numeric range — iterates from N to M inclusive. Both bounds may be variables or expressions.\n\n(2) Glob iterator — expands a glob pattern at runtime and iterates over each matched path as a string, binding it to the loop variable. Unmatched patterns produce a warning and the body does not execute.\n\nLoops may be nested. Numeric and glob loops can be mixed.',
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
