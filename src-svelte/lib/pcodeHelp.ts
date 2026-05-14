// pcodeHelp.ts — pcode command help database
// Used by the Help modal triggered from the console via: help <command>

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

  // ── File / Session ──────────────────────────────────────────────────────

  addfiles: {
    name:        'AddFiles',
    description: 'Appends a list of explicit file paths to the session. Files already loaded are skipped. Use ClearSession first if you want to start fresh.',
    syntax:      'AddFiles paths=<path>[,<path>...]',
    arguments: [
      { name: 'paths', type: 'string', required: true, description: 'Comma-separated list of full file paths to load' },
    ],
    output:  'Appends the specified files to the session file list.',
    example: 'AddFiles paths="D:/M31/frame001.fit,D:/M31/frame002.fit"',
  },

  writefit: {
    name:        'WriteFIT',
    description: 'Writes all session files to a destination directory in FITS format.',
    syntax:      'WriteFIT destination=<path> [overwrite=<bool>]',
    arguments: [
      { name: 'destination', type: 'path',    required: true,  description: 'Directory to write files to' },
      { name: 'overwrite',   type: 'boolean', required: false, default: 'false', description: 'Whether to overwrite existing files' },
    ],
    output:  'Writes all session files to the destination directory.',
    example: 'WriteFIT destination="D:/Output" overwrite=true',
  },

  writetiff: {
    name:        'WriteTIFF',
    description: 'Writes all session files to a destination directory in TIFF format.',
    syntax:      'WriteTIFF destination=<path> [overwrite=<bool>]',
    arguments: [
      { name: 'destination', type: 'path',    required: true,  description: 'Directory to write files to' },
      { name: 'overwrite',   type: 'boolean', required: false, default: 'false', description: 'Whether to overwrite existing files' },
    ],
    output:  'Writes all session files to the destination directory.',
    example: 'WriteTIFF destination="D:/Output" overwrite=true',
  },

  writexisf: {
    name:        'WriteXISF',
    description: 'Writes all session files to a destination directory in XISF format.',
    syntax:      'WriteXISF destination=<path> [overwrite=<bool>] [compress=<bool>]',
    arguments: [
      { name: 'destination', type: 'path',    required: true,  description: 'Directory to write files to' },
      { name: 'overwrite',   type: 'boolean', required: false, default: 'false', description: 'Whether to overwrite existing files' },
      { name: 'compress',    type: 'boolean', required: false, default: 'false', description: 'Whether to apply XISF compression' },
    ],
    output:  'Writes all session files to the destination directory.',
    example: 'WriteXISF destination="D:/Output" overwrite=true compress=false',
  },

  writecurrent: {
    name:        'WriteCurrent',
    description: 'Writes the current frame to disk, updating the file in place.',
    syntax:      'WriteCurrent',
    arguments:   [],
    output:  'Overwrites the current frame file with the current in-memory buffer.',
    example: 'WriteCurrent',
  },

  writeframe: {
    name:        'WriteFrame',
    description: 'Writes the current frame to a specified path.',
    syntax:      'WriteFrame destination=<path> [overwrite=<bool>]',
    arguments: [
      { name: 'destination', type: 'path',    required: true,  description: 'Full output file path' },
      { name: 'overwrite',   type: 'boolean', required: false, default: 'false', description: 'Whether to overwrite if file exists' },
    ],
    output:  'Writes the current frame to the specified path.',
    example: 'WriteFrame destination="D:/Output/frame001.fit"',
  },

  copyfile: {
    name:        'CopyFile',
    description: 'Copies a file to a destination directory. Uses the current frame if no source is specified. Stores the destination path in $NEW_FILE.',
    syntax:      'CopyFile destination=<path> [source=<path>]',
    arguments: [
      { name: 'destination', type: 'path', required: true,  description: 'Destination directory path' },
      { name: 'source',      type: 'path', required: false, description: 'Source file path (default: current frame)' },
    ],
    output:  'Copies the file to the destination directory. Source file and session are unchanged.',
    example: 'CopyFile destination="D:/Backups"\nCopyFile source="$NEW_FILE" destination="D:/Heatmaps"',
  },

  movefile: {
    name:        'MoveFile',
    description: 'Moves a file to a destination directory. Uses the current frame if no source is specified.',
    syntax:      'MoveFile destination=<path> [source=<path>]',
    arguments: [
      { name: 'destination', type: 'path', required: true,  description: 'Destination directory path' },
      { name: 'source',      type: 'path', required: false, description: 'Source file path (default: current frame)' },
    ],
    output:  'Moves the file and removes it from the session file list.',
    example: 'MoveFile destination="D:/Rejects"',
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
    description: 'Loads a single file for display without adding it to the session file list.',
    syntax:      'LoadFile path=<path>',
    arguments: [
      { name: 'path', type: 'path', required: true, description: 'Full path to the file to load' },
    ],
    output:  'Displays the file in the viewer. Does not affect the session file list.',
    example: 'LoadFile path="D:/Heatmaps/fwhm_heatmap.xisf"',
  },

  // ── Keywords ────────────────────────────────────────────────────────────

  addkeyword: {
    name:        'AddKeyword',
    description: 'Adds a FITS keyword to the current frame or all frames in the session.',
    syntax:      'AddKeyword name=<string> value=<string> [comment=<string>] [scope=current|all]',
    arguments: [
      { name: 'name',    type: 'string', required: true,  description: 'Keyword name (max 8 characters)' },
      { name: 'value',   type: 'string', required: true,  description: 'Keyword value' },
      { name: 'comment', type: 'string', required: false, description: 'Optional FITS comment' },
      { name: 'scope',   type: 'string', required: false, default: 'current', description: 'Apply to current frame or all frames' },
    ],
    output:  'Adds or updates the keyword in the specified frame(s).',
    example: 'AddKeyword name=TELESCOP value="Celestron EdgeHD 8" comment="Telescope used"',
  },

  deletekeyword: {
    name:        'DeleteKeyword',
    description: 'Removes a FITS keyword from the current frame or all frames.',
    syntax:      'DeleteKeyword name=<string> [scope=current|all]',
    arguments: [
      { name: 'name',  type: 'string', required: true,  description: 'Keyword name to delete' },
      { name: 'scope', type: 'string', required: false, default: 'current', description: 'Apply to current frame or all frames' },
    ],
    output:  'Removes the keyword from the specified frame(s).',
    example: 'DeleteKeyword name=EXPTIME scope=all',
  },

  modifykeyword: {
    name:        'ModifyKeyword',
    description: 'Modifies an existing FITS keyword value in the current frame or all frames.',
    syntax:      'ModifyKeyword name=<string> value=<string> [comment=<string>] [scope=current|all]',
    arguments: [
      { name: 'name',    type: 'string', required: true,  description: 'Keyword name to modify' },
      { name: 'value',   type: 'string', required: true,  description: 'New keyword value' },
      { name: 'comment', type: 'string', required: false, description: 'New comment (optional)' },
      { name: 'scope',   type: 'string', required: false, default: 'current', description: 'Apply to current frame or all frames' },
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
    description: 'Retrieves a FITS keyword value from the current frame and stores it as a script variable.',
    syntax:      'GetKeyword name=<string>',
    arguments: [
      { name: 'name', type: 'string', required: true, description: 'Keyword name to retrieve' },
    ],
    output:  'Stores the keyword value in $<NAME> (uppercase). Returns the value.',
    example: 'GetKeyword name=EXPTIME\nPrint $EXPTIME',
  },

  // ── Analysis ────────────────────────────────────────────────────────────

  autostretch: {
    name:        'AutoStretch',
    description: 'Applies an automatic stretch to the current frame using the PixInsight-compatible Auto-STF algorithm.',
    syntax:      'AutoStretch [shadowClip=<float>] [targetBackground=<float>]',
    arguments: [
      { name: 'shadowClip',       type: 'float', required: false, default: '-2.8',  description: 'Shadow clipping point in sigma units' },
      { name: 'targetBackground', type: 'float', required: false, default: '0.15',  description: 'Target background level (0.0–1.0)' },
    ],
    output:  'Updates the viewer with the stretched image.',
    example: 'AutoStretch shadowClip=-2.8 targetBackground=0.25',
  },

  analyzeframes: {
    name:        'AnalyzeFrames',
    description: 'Runs full quality analysis on all session frames, computing FWHM, eccentricity, star count, SNR, and background metrics. Flags outlier frames as REJECT.',
    syntax:      'AnalyzeFrames',
    arguments:   [],
    output:  'Populates analysis results for all frames. Results visible in Analysis Results and Analysis Graph views.',
    example: 'SelectFiles paths="D:/M31/Lights"\nAnalyzeFrames',
  },

  computefwhm: {
    name:        'ComputeFWHM',
    description: 'Computes the Full Width at Half Maximum (FWHM) for stars in the current frame as a measure of focus quality.',
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

  gethistogram: {
    name:        'GetHistogram',
    description: 'Computes the histogram and basic statistics for the current frame.',
    syntax:      'GetHistogram',
    arguments:   [],
    output:  'Displays histogram in the info panel. Returns statistics including median, mean, and std dev.',
    example: 'GetHistogram',
  },

  contourheatmap: {
    name:        'ContourHeatmap',
    description: 'Generates a false-color heatmap of FWHM across the current frame, showing spatial focus variation across the sensor.',
    syntax:      'ContourHeatmap [palette=viridis|plasma|coolwarm] [contour_levels=<int>] [threshold=<float>] [saturation=<float>]',
    arguments: [
      { name: 'palette',        type: 'string',  required: false, default: 'viridis', description: 'Color palette: viridis, plasma, or coolwarm' },
      { name: 'contour_levels', type: 'integer', required: false, default: '10',      description: 'Number of contour levels' },
      { name: 'threshold',      type: 'float',   required: false,                     description: 'Rejection threshold for outlier pixels' },
      { name: 'saturation',     type: 'float',   required: false, default: '1.0',     description: 'Color saturation multiplier' },
    ],
    output:  'Generates a heatmap image and loads it in the viewer.',
    example: 'ContourHeatmap palette=plasma contour_levels=12',
  },

  // ── Scripting ───────────────────────────────────────────────────────────

  set: {
    name:        'Set',
    description: 'Assigns a value to a script variable. Supports arithmetic expressions, string concatenation, and math functions.',
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
    description: 'Outputs a value or expression to the console. Supports variable references and arithmetic expressions.',
    syntax:      'Print <expression>',
    arguments: [
      { name: 'message', type: 'expression', required: true, description: 'Value, variable, or expression to print' },
    ],
    output:  'Writes the evaluated result to the console.',
    example: 'Print "Hello world"\nPrint $x + 1\nPrint "FWHM: " + $fwhm',
  },

  assert: {
    name:        'Assert',
    description: 'Halts macro execution with an error if the expression evaluates to false. Silent on success.',
    syntax:      'Assert expression=<condition>',
    arguments: [
      { name: 'expression', type: 'condition', required: true, description: 'Boolean condition to test (e.g. $x > 0, $name == $expected)' },
    ],
    output:  'Silent on pass. Halts execution with ASSERT_FAILED error on failure.',
    example: 'Assert expression="$filecount > 0"\nAssert expression="$fwhm == $expected_fwhm"',
  },

  countfiles: {
    name:        'CountFiles',
    description: 'Returns the number of files currently loaded in the session.',
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
    output:  'Executes all commands in the macro. Print output appears in the console.',
    example: 'RunMacro name="my-workflow"',
  },

  log: {
    name:        'Log',
    description: 'Writes the current script execution results to a log file.',
    syntax:      'Log path=<path> [append=<bool>]',
    arguments: [
      { name: 'path',   type: 'path',    required: true,  description: 'Output log file path' },
      { name: 'append', type: 'boolean', required: false, default: 'false', description: 'Append to existing file instead of overwriting' },
    ],
    output:  'Writes all preceding command results to the specified log file.',
    example: 'Log path="D:/logs/session.log" append=true',
  },

  // ── View / Display ──────────────────────────────────────────────────────

  setframe: {
    name:        'SetFrame',
    description: 'Sets the current frame to the specified zero-based index.',
    syntax:      'SetFrame index=<integer>',
    arguments: [
      { name: 'index', type: 'integer', required: true, description: 'Zero-based frame index' },
    ],
    output:  'Updates the viewer to display the specified frame.',
    example: 'SetFrame index=0',
  },

  cacheframes: {
    name:        'CacheFrames',
    description: 'Pre-builds the blink cache for all session frames in the background.',
    syntax:      'CacheFrames',
    arguments:   [],
    output:  'Triggers background cache build. Required before using BlinkSequence.',
    example: 'CacheFrames',
  },

  blinksequence: {
    name:        'BlinkSequence',
    description: 'Starts blinking through all session frames in sequence for visual quality inspection.',
    syntax:      'BlinkSequence [fps=<float>]',
    arguments: [
      { name: 'fps', type: 'float', required: false, default: '2.0', description: 'Frames per second for blink playback' },
    ],
    output:  'Activates the blink viewer. Requires CacheFrames to have been run first.',
    example: 'CacheFrames\nBlinkSequence fps=3',
  },

  // ── Console only ────────────────────────────────────────────────────────

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

/// Look up a help entry by command name (case-insensitive).
export function getHelp(command: string): HelpEntry | null {
  return HELP_DB[command.toLowerCase()] ?? null;
}
