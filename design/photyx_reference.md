# Photyx — Reference Document

**Version:** 1
**Last updated:** 28 April 2026
**Status:** Active — updated as commands, keywords, and settings are added

This document is the authoritative lookup reference for pcode commands, interrogation properties, keyword mappings, analysis metrics, and settings. It is a companion to `photyx_spec.md` (requirements) and `development_notes.md` (implementation). Upload this document when doing work involving pcode scripting, keyword management, analysis, or settings configuration.

---

## 1. pcode Command Dictionary

All pcode commands in the initial release. Arguments in brackets are optional.

| Command             | Category        | Description                                                                                                                                                                                         | Key Arguments                                          |
| ------------------- | --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| AddKeyword          | Keyword         | Adds or replaces a keyword on loaded images                                                                                                                                                         | name, value, [comment], [scope=all\|current]           |
| AnalyzeFrames       | Frame Analysis  | Computes seven quality metrics for all loaded frames, classifies each as PASS or REJECT, writes PXFLAG keyword                                                                                      | —                                                      |
| Assert              | Scripting       | Halts execution with an error if expression is false; silent on pass in both Trace and No Trace modes                                                                                               | expression                                             |
| AutoStretch         | Processing      | Applies Auto-STF stretch to current frame (display only — raw buffer unchanged)                                                                                                                     | [shadowClip], [targetBackground]                       |
| BinImage            | Processing      | Bins the image by an integer factor                                                                                                                                                                 | factor                                                 |
| BlinkSequence       | Blink & View    | Starts blinking the loaded image set                                                                                                                                                                | [fps]                                                  |
| CacheFrames         | Blink & View    | Pre-decodes and caches all frames for blinking at both resolutions                                                                                                                                  | —                                                      |
| ClearSession        | Session         | Clears all loaded images and resets session state                                                                                                                                                   | —                                                      |
| ComputeEccentricity | Analysis        | Calculates eccentricity for detected stars on current frame                                                                                                                                         | —                                                      |
| ComputeFWHM         | Analysis        | Calculates FWHM for detected stars; displays per-star circle annotations on viewer overlay                                                                                                          | —                                                      |
| ContourHeatmap      | Analysis        | Generates spatial FWHM heatmap for current frame; writes XISF to active directory; stores output path in `$NEW_FILE`                                                                                | [palette], [contour_levels], [threshold], [saturation] |
| CopyKeyword         | Keyword         | Copies a keyword value to a new keyword name                                                                                                                                                        | from, to                                               |
| CountFiles          | Scripting       | Stores number of files in current list in `$filecount`                                                                                                                                              | —                                                      |
| CountStars          | Analysis        | Counts detected stars in current frame                                                                                                                                                              | —                                                      |
| CropImage           | Processing      | Crops the image to a specified region                                                                                                                                                               | x, y, width, height                                    |
| DebayerImage        | Processing      | Debayers a Bayer CFA image on demand                                                                                                                                                                | [pattern], [method=nearest\|bilinear\|vng\|ahd]        |
| DeleteKeyword       | Keyword         | Removes a keyword from loaded images                                                                                                                                                                | name, [scope=all\|current]                             |
| FilterByKeyword     | File Management | Filters the active file list by keyword value                                                                                                                                                       | name, value                                            |
| GetHistogram        | Processing      | Computes histogram statistics for current frame (median, std dev, clipping %)                                                                                                                       | —                                                      |
| GetImageProperty    | Interrogation   | Retrieves an image property into a variable; see §2 for full property list                                                                                                                          | property                                               |
| GetKeyword          | Interrogation   | Retrieves a keyword value; auto-stores in `$<NAME>` (uppercase) — e.g. `GetKeyword name=FILTER` stores result in `$FILTER`                                                                          | name                                                   |
| GetSessionProperty  | Interrogation   | Retrieves a session state value into a variable; see §2 for full property list                                                                                                                      | property                                               |
| ListFiles           | File Management | Lists files in the active directory                                                                                                                                                                 | [filter]                                               |
| ListKeywords        | Keyword         | Lists all keywords for the current image                                                                                                                                                            | —                                                      |
| LoadFile            | File Management | Loads a single image file into the session without clearing existing session; stores path in `$LOAD_FILE_PATH`                                                                                      | path                                                   |
| Log                 | Scripting       | Writes collected macro output since last Log call to a file                                                                                                                                         | path, [append]                                         |
| MedianValue         | Analysis        | Returns the median pixel value per channel                                                                                                                                                          | —                                                      |
| ModifyKeyword       | Keyword         | Changes the value of an existing keyword                                                                                                                                                            | name, value, [comment], [scope=all\|current]           |
| MoveFile            | File Management | Moves a file to a destination directory; defaults to current frame if source= not specified; stores path in `$NEW_FILE`                                                                             | [source], destination                                  |
| Print               | Scripting       | Outputs a message to the pcode console; accepts bare expressions — `Print $x + 1` and `Print "hello"` are both valid                                                                                | message (positional or bare expression)                |
| ReadAll             | I/O             | Reads all supported image files (FITS + XISF + TIFF) in the active directory                                                                                                                        | —                                                      |
| ReadFIT             | I/O             | Reads all FITS files in the active directory                                                                                                                                                        | —                                                      |
| ReadTIFF            | I/O             | Reads all TIFF files in the active directory                                                                                                                                                        | —                                                      |
| ReadXISF            | I/O             | Reads all XISF files in the active directory                                                                                                                                                        | —                                                      |
| RunMacro            | Scripting       | Executes a saved macro by name; bare names resolve automatically; if the macro has @param declarations, parameters are passed as named arguments (e.g. `RunMacro ProcessLights INPUT_DIR="D:/M31"`) | name, [param=value …] \|                               |
| SelectDirectory     | File Management | Sets the active working directory                                                                                                                                                                   | path                                                   |
| Set                 | Scripting       | Assigns a value to a variable; string literals on the RHS must use double quotes                                                                                                                    | varname = value                                        |
| SetFrame            | Navigation      | Sets the current active frame by index (0-based)                                                                                                                                                    | index                                                  |
| SetZoom             | Blink & View    | Sets the viewer zoom level                                                                                                                                                                          | level (fit, 25, 50, 100, 200)                          |
| Test                | Interrogation   | Performs a boolean test and stores result in `$Result`; see §2 for full test list                                                                                                                   | expression                                             |
| WriteCurrent        | I/O             | Writes all buffered images back to their source paths in their original format (atomic temp-rename)                                                                                                 | —                                                      |
| WriteFIT            | I/O             | Writes all buffered images as FITS files (atomic temp-rename)                                                                                                                                       | destination, [overwrite]                               |
| WriteFrame          | I/O             | Writes the currently active frame only back to its source format (atomic temp-rename)                                                                                                               | —                                                      |
| WriteTIFF           | I/O             | Writes all buffered images as TIFF files with AstroTIFF keyword embedding (atomic temp-rename)                                                                                                      | destination, [overwrite]                               |
| WriteXISF           | I/O             | Writes all buffered images as XISF files (atomic temp-rename)                                                                                                                                       | destination, [overwrite], [compress=true\|false]       |
| pwd                 | Console         | Prints the current active directory (client-side only)                                                                                                                                              | —                                                      |

### 1.1 Command Aliases

Old-style names are retained as backward-compatible aliases but should not be used in new scripts.

| Alias             | Canonical Name |
| ----------------- | -------------- |
| ReadAllFITFiles   | ReadFIT        |
| ReadAllXISFFiles  | ReadXISF       |
| ReadAllTIFFFiles  | ReadTIFF       |
| ReadAllFiles      | ReadAll        |
| WriteAllFITFiles  | WriteFIT       |
| WriteAllXISFFiles | WriteXISF      |
| WriteAllTIFFFiles | WriteTIFF      |
| WriteCurrentFiles | WriteCurrent   |

### 1.2 Keyword Scope Parameter

`AddKeyword`, `DeleteKeyword`, and `ModifyKeyword` accept an optional `scope` parameter:

- `scope=all` (default) — applies to all loaded frames
- `scope=current` — applies only to the current frame as set by `SetFrame`

### 1.3 $NEW_FILE Convention

Plugins that create a new file store its path in `ctx.variables["NEW_FILE"]`. Scripts can use `$NEW_FILE` immediately after the generating command:

```
ContourHeatmap
MoveFile source="$NEW_FILE" destination="D:/heatmaps/"
```

### 1.4 Trace Mode

The console header Trace / No Trace toggle controls execution verbosity:

| Mode     | Command echo | Set assignment output | Plugin output |
| -------- | ------------ | --------------------- | ------------- |
| Trace    | ✓ shown      | ✓ shown               | ✓ shown       |
| No Trace | suppressed   | suppressed            | ✓ shown       |

`Assert` is always silent on pass regardless of trace mode. On failure it prints an error and halts.

### 1.5 String Literal Rules

- String literals in `Set` expressions must use **double quotes**: `Set name = "M31"`
- Single quotes are not supported and will cause a tokenizer error
- Smart/curly quotes are normalized to straight quotes automatically before evaluation

### 1.6 @param Token System

Macros declare runtime parameters using `@param` comment lines at the top of the script. Syntax:

```
@param NAME "Description" required|optional [default="value"]
```

| Field | Description |
| ----- | ----------- |
| `NAME` | Token name; used as `$NAME` inside the script |
| `"Description"` | Label shown in the parameter prompt dialog |
| `required\|optional` | Whether the user must supply a value |
| `default="value"` | Default value for optional params; used if user leaves blank |


**At run time:**

- Console: `RunMacro ProcessLights INPUT_DIR="D:/M31" OUTPUT_DIR="D:/Output"`

- Macro Library / Quick Launch: parameter prompt dialog appears before execution

- Parameters become pcode variables (`$INPUT_DIR`, etc.) for the duration of the macro

**Quick Launch rule:** buttons store only `RunMacro name=X` — never embedded parameter values. Parameters are always resolved at run time.

---

## 2. Interrogation Properties

### 2.1 Image Properties (GetImageProperty)

| Property     | Type    | Description                               | Example Value                    |
| ------------ | ------- | ----------------------------------------- | -------------------------------- |
| Width        | Integer | Image width in pixels                     | 4656                             |
| Height       | Integer | Image height in pixels                    | 3520                             |
| Channels     | Integer | Number of color channels                  | 1 (mono), 3 (RGB)                |
| BitDepth     | Integer | Bits per pixel per channel                | 8, 16, 32                        |
| DataType     | String  | Pixel data type                           | UInt16, Float32                  |
| ColorSpace   | String  | Color space of the image                  | Mono, RGB, Bayer                 |
| BayerPattern | String  | Bayer filter pattern if present           | RGGB, BGGR, GRBG, GBRG           |
| FileFormat   | String  | Source file format                        | FITS, XISF, TIFF, PNG, JPEG      |
| Filename     | String  | Full path of the source file              | //192.168.1.100/M31/frame001.fit |
| FileSize     | Integer | File size in bytes                        | 32440320                         |
| ImageIndex   | Integer | Index of image in current file list       | 0-based                          |
| IsDebayered  | Boolean | Whether debayering has been applied       | true, false                      |
| HasKeywords  | Boolean | Whether the file contains header keywords | true, false                      |
| Compression  | String  | Compression algorithm if applicable       | LZ4, zstd, zlib, None            |
| ByteOrder    | String  | Byte order of pixel data                  | BigEndian, LittleEndian          |

### 2.2 Keyword Properties (GetKeyword)

`GetKeyword name=X` retrieves a keyword value and auto-stores it in `$<NAME>` (uppercase). Any keyword in the file header can be retrieved; the table below lists common astrophotography keywords.

| Keyword  | Type    | Description                          | Example Value           |
| -------- | ------- | ------------------------------------ | ----------------------- |
| OBJECT   | String  | Target object name                   | M31                     |
| TELESCOP | String  | Telescope name                       | Celestron EdgeHD 8      |
| INSTRUME | String  | Camera/instrument name               | ZWO ASI2600MC           |
| EXPTIME  | Float   | Exposure time in seconds             | 300.0                   |
| GAIN     | Integer | Camera gain setting                  | 100                     |
| OFFSET   | Integer | Camera offset setting                | 30                      |
| TEMP     | Float   | Sensor temperature in Celsius        | -10.0                   |
| FILTER   | String  | Filter name                          | Ha, OIII, Lum, duo      |
| BAYERPAT | String  | Bayer pattern from capture software  | RGGB                    |
| XBINNING | Integer | Horizontal binning factor            | 1                       |
| YBINNING | Integer | Vertical binning factor              | 1                       |
| FOCALLEN | Float   | Focal length in mm                   | 2032.0                  |
| APERTURE | Float   | Aperture in mm                       | 203.2                   |
| RA       | Float   | Right ascension of target in degrees | 10.6848                 |
| DEC      | Float   | Declination of target in degrees     | 41.2692                 |
| DATE-OBS | String  | Date and time of observation (UTC)   | 2024-11-15T22:30:00     |
| SITELONG | Float   | Observatory longitude                | -105.1786               |
| SITELAT  | Float   | Observatory latitude                 | 40.5853                 |
| SITEELEV | Float   | Observatory elevation in meters      | 1524.0                  |
| IMAGETYP | String  | Frame type                           | Light, Dark, Flat, Bias |
| SWCREATE | String  | Software that created the file       | Photyx 1.0              |
| PXFLAG   | String  | Photyx frame analysis recommendation | PASS, REJECT            |

### 2.3 Session Properties (GetSessionProperty)

| Property        | Type    | Description                             | Example Value         |
| --------------- | ------- | --------------------------------------- | --------------------- |
| FileCount       | Integer | Number of files in the active file list | 47                    |
| ActiveDirectory | String  | Current active working directory        | D:/Astrophotos/M31    |
| CurrentFrame    | Integer | Index of the currently displayed frame  | 0-based               |
| LoadedFileCount | Integer | Number of files loaded into buffer pool | 12                    |
| TotalMemoryUsed | Integer | Buffer pool memory usage in bytes       | 1073741824            |
| Platform        | String  | Current OS platform                     | Windows, macOS, Linux |
| PhotoyxVersion  | String  | Running version of Photyx               | 1.0.0                 |

### 2.4 Boolean Tests (Test)

| Test Expression          | Description                                  |
| ------------------------ | -------------------------------------------- |
| ImageHasBayerPattern     | True if a Bayer pattern is detected          |
| ImageIsColor             | True if image has 3 channels                 |
| ImageIsMono              | True if image has 1 channel                  |
| ImageIsDebayered         | True if debayering has been applied          |
| KeywordExists name=X     | True if the named keyword is present         |
| FileCountExceeds count=X | True if file list exceeds X files            |
| DirectoryExists path=X   | True if the specified path exists            |
| FileExists path=X        | True if the specified file exists            |
| VariableIsSet name=X     | True if the named variable has been assigned |

---

## 3. FITS-to-XISF Keyword Mapping

When converting FITS to XISF, all FITS keywords are written verbatim into the FITSKeyword block. Keywords with a known XISF Property equivalent are additionally written into the Properties block.

| FITS Keyword | XISF Property                        |
| ------------ | ------------------------------------ |
| OBJECT       | Observation:Object:Name              |
| TELESCOP     | Instrument:Telescope:Name            |
| INSTRUME     | Instrument:Camera:Name               |
| EXPTIME      | Observation:Time:ExposureTime        |
| FILTER       | Instrument:Filter:Name               |
| GAIN         | Instrument:Camera:Gain               |
| TEMP         | Instrument:Camera:Temperature        |
| DATE-OBS     | Observation:Time:Start               |
| RA           | Observation:Object:RA                |
| DEC          | Observation:Object:Dec               |
| CRVAL1       | Observation:Center:RA                |
| CRVAL2       | Observation:Center:Dec               |
| RADESYS      | Observation:CelestialReferenceSystem |
| EQUINOX      | Observation:Equinox                  |
| SITELAT      | Observation:Location:Latitude        |
| SITELONG     | Observation:Location:Longitude       |
| SITEELEV     | Observation:Location:Elevation       |
| XBINNING     | Instrument:Camera:XBinning           |
| YBINNING     | Instrument:Camera:YBinning           |
| FOCALLEN     | Instrument:Telescope:FocalLength     |
| IMAGETYP     | Observation:Image:Type               |

WCS transformation keywords (`CRPIX1/2`, `CD1_1`, `CD1_2`, `CD2_1`, `CD2_2`, `CDELT1/2`, `CROTA1/2`, `LONPOLE`, `LATPOLE`, `PV1_*`, all PC matrix keywords) have no XISF Property equivalents and are preserved verbatim in the FITSKeyword block only.

---

## 4. AnalyzeFrames Metrics & Classification

### 4.1 Metrics

| Metric              | Description                                             | Threshold Type           | Default Reject |
| ------------------- | ------------------------------------------------------- | ------------------------ | -------------- |
| Background median   | Sigma-clipped sky background level                      | Sigma (session-relative) | > +2.5σ        |
| Background std dev  | Noise floor estimate                                    | Sigma (session-relative) | > +2.5σ        |
| Background gradient | Spatial variation across 8×8 grid                       | Sigma (session-relative) | > +2.5σ        |
| SNR estimate        | Star signal / background noise                          | Sigma (session-relative) | < -2.5σ        |
| FWHM                | Median FWHM via intensity-weighted second-order moments | Sigma (session-relative) | > +2.5σ        |
| Eccentricity        | Median eccentricity via second-order moments            | Absolute                 | > 0.85         |
| Star count          | Stars detected (minimum 5 connected pixels)             | Sigma (session-relative) | < -1.5σ        |

### 4.2 Classification

- **PASS / REJECT only** — no SUSPECT classification
- A frame is REJECT if any single metric exceeds its threshold
- `triggered_by` records which metrics caused the REJECT (visible in Analysis Graph tooltip)
- PXFLAG keyword is written to each file immediately when AnalyzeFrames completes

### 4.3 PXFLAG Keyword

```
PXFLAG = 'PASS'    / Photyx frame analysis recommendation
PXFLAG = 'REJECT'  / Photyx frame analysis recommendation
```

User can override any frame's flag during blink review with P (pass) or R (reject) keyboard shortcuts. Each keypress writes PXFLAG to the file immediately.

---

## 5. Settings Reference

### 5.1 UI / Viewer Settings

| Setting              | Default        | Persisted |
| -------------------- | -------------- | --------- |
| Color theme          | Matrix         | ✓         |
| Default zoom level   | Fit            | ✗         |
| Default blink rate   | 0.1s per frame | ✗         |
| Default channel view | RGB            | ✗         |

### 5.2 File & Path Settings

| Setting                   | Default             | Persisted |
| ------------------------- | ------------------- | --------- |
| Default working directory | Last used directory | ✓         |
| Default JPEG quality      | 75%                 | ✓         |
| Overwrite behavior        | Prompt              | ✓         |
| Recent directories list   | Last 10             | ✓         |
| Format filter selection   | All Supported       | ✓         |

### 5.3 pcode / Macro Settings

| Setting                 | Default             | Persisted |
| ----------------------- | ------------------- | --------- |
| Macro library directory | OS app data Macros/ | ✓         |
| Console history size    | 500 commands        | ✓         |
| Error behavior          | Halt on error       | ✓         |
| Macro editor font size  | 13px                | ✓         |

### 5.4 Performance Settings

| Setting                     | Default           | Persisted |
| --------------------------- | ----------------- | --------- |
| Buffer pool memory limit    | 4 GB              | ✓         |
| Blink pre-cache frame count | All loaded frames | ✓         |
| Rayon thread count          | num_cpus - 1      | ✓         |

### 5.5 Quick Launch Settings

| Setting            | Default | Persisted |
| ------------------ | ------- | --------- |
| Button assignments | —       | ✓         |
| Grid column count  | 4       | ✓         |
| Panel visible      | true    | ✓         |

### 5.6 Rig Profile Defaults (AnalyzeFrames)

| Threshold                  | Type     | Default |
| -------------------------- | -------- | ------- |
| Background median Reject   | Sigma    | +2.5σ   |
| Background std dev Reject  | Sigma    | +2.5σ   |
| Background gradient Reject | Sigma    | +2.5σ   |
| SNR estimate Reject        | Sigma    | -2.5σ   |
| FWHM Reject                | Sigma    | +2.5σ   |
| Eccentricity Reject        | Absolute | 0.85    |
| Star count Reject          | Sigma    | -1.5σ   |

---

## 6. File Format Coverage

### 6.1 Read Support

| Format                 | Notes                                               |
| ---------------------- | --------------------------------------------------- |
| FITS (.fit/.fits/.fts) | Via fitsio / cfitsio; sequential loading only       |
| XISF (.xisf)           | Via photyx-xisf crate; LZ4, LZ4HC, zstd, zlib       |
| TIFF (.tif/.tiff)      | U8, U16, U32→U16, F32; AstroTIFF keyword round-trip |
| PNG (.png)             | Viewing and format conversion only; no keywords     |
| JPEG (.jpg/.jpeg)      | Viewing and format conversion only; no keywords     |

### 6.2 Write Support

| Format            | Notes                                                  |
| ----------------- | ------------------------------------------------------ |
| FITS (.fit/.fits) | Full keyword support; BZERO/BSCALE for unsigned 16-bit |
| XISF (.xisf)      | Dual-write to FITSKeyword block and Properties block   |
| TIFF (.tif/.tiff) | AstroTIFF keyword embedding in ImageDescription tag    |
| PNG (.png)        | 16-bit support                                         |
| JPEG (.jpg)       | 8-bit; quality configurable (default 75%)              |

### 6.3 Keyword Support by Format

| Format | Read Keywords | Write Keywords | Notes                                   |
| ------ | ------------- | -------------- | --------------------------------------- |
| FITS   | ✓             | ✓              | Full FITS header                        |
| XISF   | ✓             | ✓              | Both FITSKeyword and Properties blocks  |
| TIFF   | ✓             | ✓              | AstroTIFF convention (ImageDescription) |
| PNG    | ✗             | ✗              | —                                       |
| JPEG   | ✗             | ✗              | —                                       |

---

## 7. Tauri Commands (Implemented)

| Command                  | Description                                                                                           |
| ------------------------ | ----------------------------------------------------------------------------------------------------- |
| `dispatch_command`       | Dispatches a single pcode command to the plugin registry (legacy interactive path)                    |
| `run_script`             | Executes a pcode script string; returns ScriptResponse with results, session_changed, display_changed |
| `debug_buffer_info`      | Returns buffer metadata including display_width and color_space                                       |
| `delete_macro`           | Deletes a .phs macro file from the Macros directory                                                   |
| `get_analysis_results`   | Returns per-frame metrics, flags, triggered_by, and session stats                                     |
| `get_autostretch_frame`  | Computes Auto-STF stretch on current frame, returns JPEG data URL; does not cache                     |
| `get_blink_cache_status` | Returns blink cache build status: idle / building / ready                                             |
| `get_blink_frame`        | Returns a blink frame as JPEG data URL from blink cache (by index + resolution)                       |
| `get_current_frame`      | Returns current image as raw (unstretched) JPEG data URL, rendered on the fly                         |
| `get_frame_flags`        | Returns PXFLAG values for all loaded frames (used by blink overlay)                                   |
| `get_full_frame`         | Returns current image at full resolution with last STF params applied; cached after first call        |
| `get_histogram`          | Computes and returns histogram bins + stats for current frame (per-channel for RGB)                   |
| `get_keywords`           | Returns all keywords for current frame as a keyed map                                                 |
| `get_macros_dir`         | Returns the Macros directory path as a forward-slash string                                           |
| `get_pixel`              | Returns raw pixel value(s) at source coordinates (x, y) from the raw image buffer                     |
| `get_session`            | Returns current session state (directory, file list, current frame)                                   |
| `get_star_positions`     | Re-runs star detection on current frame; returns {cx, cy, fwhm, r} per star for annotation overlay    |
| `get_variable`           | Returns a pcode variable value from ctx.variables by name                                             |
| `list_log_files`         | Lists available log files in the logs directory, sorted newest first                                  |
| `list_macros`            | Lists .phs files in the Macros directory with name, path, line count, and tooltip                     |
| `list_plugins`           | Returns list of registered plugins with name, version, and type                                       |
| `load_file`              | Reads a single image file from disk, injects into session, returns JPEG data URL                      |
| `read_log_file`          | Reads and parses a log file into structured {timestamp, level, module, message} lines                 |
| `rename_macro`           | Renames a .phs macro file; validates name, returns new path                                           |
| `start_background_cache` | Spawns background task to build blink cache JPEGs                                                     |

---

## 8. Path Conventions

| Convention      | Rule                                                                              |
| --------------- | --------------------------------------------------------------------------------- |
| Separator       | Forward slash `/` always; backend translates to OS-native before filesystem calls |
| Absolute paths  | `D:/Astrophotos/M31` (Windows) or `/home/user/photos` (macOS/Linux)               |
| Relative paths  | Resolved against current active directory set by `SelectDirectory`                |
| Home shorthand  | `~` expands to current user's home directory on all platforms                     |
| UNC paths       | `//192.168.1.100/Astrophotos/M31` — useful for ASIAir Pro over local network      |
| Spaces in paths | Must be enclosed in double quotes                                                 |

---

## 9. Plugin Designation

All plugins are Built-in Native in the initial release. WASM user plugins are supported via the plugin framework but none are shipped by default.

| Plugin              | Category        | Status     |
| ------------------- | --------------- | ---------- |
| AddKeyword          | Keyword         | ✅ Complete |
| AnalyzeFrames       | Frame Analysis  | ✅ Complete |
| Assert              | Scripting       | ✅ Complete |
| AutoStretch         | Processing      | ✅ Complete |
| CacheFrames         | Blink           | ✅ Complete |
| ClearSession        | Session         | ✅ Complete |
| ComputeEccentricity | Analysis        | ✅ Complete |
| ComputeFWHM         | Analysis        | ✅ Complete |
| ContourHeatmap      | Analysis        | ✅ Complete |
| CopyKeyword         | Keyword         | ✅ Complete |
| CountFiles          | Scripting       | ✅ Complete |
| CountStars          | Analysis        | ✅ Complete |
| DeleteKeyword       | Keyword         | ✅ Complete |
| GetHistogram        | Analysis        | ✅ Complete |
| GetKeyword          | Scripting       | ✅ Complete |
| ListKeywords        | Keyword         | ✅ Complete |
| LoadFile            | File Management | ✅ Complete |
| ModifyKeyword       | Keyword         | ✅ Complete |
| MoveFile            | File Management | ✅ Complete |
| Print               | Scripting       | ✅ Complete |
| ReadAll             | I/O Reader      | ✅ Complete |
| ReadFIT             | I/O Reader      | ✅ Complete |
| ReadTIFF            | I/O Reader      | ✅ Complete |
| ReadXISF            | I/O Reader      | ✅ Complete |
| RunMacro            | Scripting       | ✅ Complete |
| SelectDirectory     | File Management | ✅ Complete |
| SetFrame            | Navigation      | ✅ Complete |
| WriteCurrent        | I/O Writer      | ✅ Complete |
| WriteFIT            | I/O Writer      | ✅ Complete |
| WriteFrame          | I/O Writer      | ✅ Complete |
| WriteTIFF           | I/O Writer      | ✅ Complete |
| WriteXISF           | I/O Writer      | ✅ Complete |
| BinImage            | Processing      | ⬜ Planned  |
| CropImage           | Processing      | ⬜ Planned  |
| DebayerImage        | Processing      | ⬜ Planned  |
| FilterByKeyword     | File Management | ⬜ Planned  |
| GetImageProperty    | Interrogation   | ⬜ Planned  |
| GetSessionProperty  | Interrogation   | ⬜ Planned  |
| ListFiles           | File Management | ⬜ Planned  |
| MedianValue         | Analysis        | ⬜ Planned  |
| SetZoom             | Blink & View    | ⬜ Planned  |
| Test                | Interrogation   | ⬜ Planned  |
