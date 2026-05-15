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
  abs:                 '(#)',
  addfiles:            'paths=',
  addkeyword:          'name=  value=  comment=',
  analyzeframes:       '',
  assert:              'expression=',
  autostretch:         'shadowClip=  targetBackground=',
  binimage:            'factor=',
  blinksequence:       'fps=',
  cacheframes:         '',
  clear:               '',
  clearannotations:    '',
  clearsession:        '',
  clearstack:          '',
  commitstretch:       'shadow_clip=  target_bg=',
  computeeccentricity: '',
  computefwhm:         '',
  contourheatmap:      'palette=[viridis|plasma|coolwarm]  contour_levels=#  threshold=  saturation=',
  copyfile:            'destination=  source=',
  copykeyword:         'from=  to=',
  countfiles:          '',
  debayerimage:        'pattern=[RGGB|BGGR|GRBG|GBRG]  method=[bilinear]',
  deletekeyword:       'name=  scope=',
  else:                '',
  endfor:              '',
  endif:               '',
  filterbykeyword:     'name=  value=',
  floor:               '(#)',
  for:                 '',
  gethistogram:        '',
  getkeyword:          'name=',
  help:                '',
  if:                  '',
  listkeywords:        '',
  loadfile:            'path=',
  log:                 'path=  append=',
  max:                 '(#,#)',
  medianvalue:         '',
  min:                 '(#,#)',
  modifykeyword:       'name=  value=  comment=  scope=',
  movefile:            'destination=',
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
  stackframes:         '[calibration_dir=]',
  version:             '',
  writecurrent:        '',
  writefit:            'destination=  overwrite=',
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
    description: 'Appends a list of explicit file paths to the session. Files already loaded are skipped. Use ClearSession first if you want to start fresh.',
    syntax:      'AddFiles paths=<path>[,<path>...]',
    arguments: [
      { name: 'paths', type: 'string', required: true, description: 'Comma-separated list of full file paths to load' },
    ],
    output:  'Appends the specified files to the session file list.',
    example: 'AddFiles paths="/data/M31/frame001.fit,/data/M31/frame002.fit"',
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
    description: 'Writes all session files to a destination directory in FITS format.',
    syntax:      'WriteFIT destination=<path> [overwrite=<bool>]',
    arguments: [
      { name: 'destination', type: 'path',    required: true,  description: 'Directory to write files to' },
      { name: 'overwrite',   type: 'boolean', required: false, default: 'false', description: 'Whether to overwrite existing files' },
    ],
    output:  'Writes all session files to the destination directory.',
    example: 'WriteFIT destination="/data/Output" overwrite=true',
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
    output:  'Writes files to the destination directory.',
    example: 'WriteXISF destination="/data/Output" overwrite=true compress=false\nWriteXISF destination="/data/Output" stack=true',
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
    description: 'Moves a file to a destination directory. Uses the current frame if no source is specified. Stores the destination path in $NEW_FILE.',
    syntax:      'MoveFile destination=<path> [source=<path>]',
    arguments: [
      { name: 'destination', type: 'path', required: true,  description: 'Destination directory path' },
      { name: 'source',      type: 'path', required: false, description: 'Source file path (default: current frame)' },
    ],
    output:  'Moves the file and removes it from the session file list.',
    example: 'MoveFile destination="/data/Rejects"',
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
    description: 'Stacks all session frames into a single result image using reference frame selection, background normalization, FFT alignment, and sigma-clipped mean combination.',
    syntax:      'StackFrames [calibration_dir=<path>]',
    arguments: [
      { name: 'calibration_dir', type: 'path', required: false, description: 'Optional directory containing calibration frames' },
    ],
    output:  'Produces a transient stacked ImageBuffer displayed in the Stacking Workspace. Reports per-frame progress and a quality summary to the console.',
    example: 'ReadImages path="/data/lights"\nStackFrames\nCommitStretch shadow_clip=-3.5 target_bg=0.10\nWriteXISF destination="/data/output" stack=true',
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
    syntax:      'AnalyzeFrames',
    arguments:   [],
    output:  'Populates analysis results for all frames. Results visible in Analysis Results and Analysis Graph views.',
    example: 'AnalyzeFrames',
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
    description: 'Assigns a value to a script variable. Supports arithmetic expressions, string concatenation, and math functions. String literals on the RHS must use double quotes.',
    syntax:      'Set <varname> = <expression>',
    arguments: [
      { name: 'varname',    type: 'string',     required: true, description: 'Variable name (no $ prefix)' },
      { name: 'expression', type: 'expression', required: true, description: 'Value or expression to assign' },
    ],
    output:  'Stores the result in $<varname> for use in subsequent commands.',
    example: 'Set x = 10\nSet label = "Frame " + $x\nSet sd = sqrt(($x - 5) ^ 2)',
  },

  print: {
    name:        'Print',
    description: 'Outputs a value or expression to the console. Accepts bare expressions — Print $x + 1 and Print "hello" are both valid.',
    syntax:      'Print <expression>',
    arguments: [
      { name: 'message', type: 'expression', required: true, description: 'Value, variable, or expression to print' },
    ],
    output:  'Writes the evaluated result to the console.',
    example: 'Print "Hello world"\nPrint $x + 1\nPrint "FWHM: " + $fwhm',
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

  runmacro: {
    name:        'RunMacro',
    description: 'Executes a saved macro by name from the database.',
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
    example: 'If $fwhm > 3.0\n  Print "Poor focus"\nEndIf',
  },

  for: {
    name:        'For',
    description: 'Iterates over all files in the current session, setting the current frame on each iteration.',
    syntax:      'For\n  ...\nEndFor',
    arguments:   [],
    output:  'Executes the block once per session file.',
    example: 'For\n  ComputeFWHM\n  Print $fwhm\nEndFor',
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
    example: 'Help\nHelp AutoStretch\nHelp Set',
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
};

// ---------------------------------------------------------------------------
// Lookup helpers
// ---------------------------------------------------------------------------

/// Look up a help entry by command name (case-insensitive).
export function getHelp(command: string): HelpEntry | null {
  return HELP_DB[command.toLowerCase()] ?? null;
}
