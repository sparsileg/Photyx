# Photyx — Reference Document

**Version:** 4 **Last updated:** 13 May 2026

This document is the authoritative lookup reference for pcode
commands, interrogation properties, keyword mappings, analysis
metrics, settings, Tauri commands, and file format support. It is a
companion to `photyx_spec.md` (requirements) and `photyx_development.md` (implementation). Upload this document when
doing work involving pcode scripting, keyword management, analysis, or
settings configuration.

---

## 1. pcode Command Dictionary

All pcode commands in the initial release. Arguments in brackets are optional.

| Command             | Category        | Description                                                                                                                                                 | Key Arguments                                          |
| ------------------- | --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| AddFiles            | Session         | Appends explicit file paths to the session; skips duplicates; checks memory limit before loading                                                            | paths                                                  |
| AddKeyword          | Keyword         | Adds or replaces a keyword on loaded images                                                                                                                 | name, value, [comment], [scope=all\|current]           |
| AnalyzeFrames       | Frame Analysis  | Computes five quality metrics for all loaded frames, classifies each as PASS or REJECT                                                                      | —                                                      |
| Assert              | Scripting       | Halts execution with an error if expression is false; silent on pass in both Trace and No Trace modes                                                       | expression                                             |
| AutoStretch         | Processing      | Applies Auto-STF stretch to current frame (display only — raw buffer unchanged)                                                                             | [shadowClip], [targetBackground]                       |
| BinImage            | Processing      | Bins the image by an integer factor                                                                                                                         | factor                                                 |
| BlinkSequence       | Blink & View    | Starts blinking the loaded image set                                                                                                                        | [fps]                                                  |
| CacheFrames         | Blink & View    | Pre-decodes and caches all frames for blinking at both resolutions                                                                                          | —                                                      |
| ClearSession        | Session         | Clears all loaded images and resets session state                                                                                                           | —                                                      |
| ComputeEccentricity | Analysis        | Calculates eccentricity for detected stars on current frame                                                                                                 | —                                                      |
| ComputeFWHM         | Analysis        | Calculates FWHM for detected stars; displays per-star circle annotations on viewer overlay                                                                  | —                                                      |
| ContourHeatmap      | Analysis        | Generates spatial FWHM heatmap for current frame; writes XISF to source file's directory; stores output path in `$NEW_FILE`                                 | [palette], [contour_levels], [threshold], [saturation] |
| CopyFile            | File Management | Copies a file to a destination directory; defaults to current frame if source= not specified; destination created automatically; stores path in `$NEW_FILE` | [source], destination                                  |
| CopyKeyword         | Keyword         | Copies a keyword value to a new keyword name                                                                                                                | from, to                                               |
| CountFiles          | Scripting       | Stores number of files in current list in `$filecount`                                                                                                      | —                                                      |
| CountStars          | Analysis        | Counts detected stars in current frame                                                                                                                      | —                                                      |
| CropImage           | Processing      | Crops the image to a specified region                                                                                                                       | x, y, width, height                                    |
| DebayerImage        | Processing      | Debayers a Bayer CFA image on demand                                                                                                                        | [pattern], [method=nearest\|bilinear\|vng\|ahd]        |
| DeleteKeyword       | Keyword         | Removes a keyword from loaded images                                                                                                                        | name, [scope=all\|current]                             |
| FilterByKeyword     | File Management | Filters the active file list by keyword value                                                                                                               | name, value                                            |
| GetHistogram        | Processing      | Computes histogram statistics for current frame (median, std dev, clipping %)                                                                               | —                                                      |
| GetImageProperty    | Interrogation   | Retrieves an image property into a variable; see §2 for full property list                                                                                  | property                                               |
| GetKeyword          | Interrogation   | Retrieves a keyword value; auto-stores in `$<NAME>` (uppercase) — e.g. `GetKeyword name=FILTER` stores result in `$FILTER`                                  | name                                                   |
| GetSessionProperty  | Interrogation   | Retrieves a session state value into a variable; see §2 for full property list                                                                              | property                                               |
| ListFiles           | File Management | Lists files in the active file list                                                                                                                         | [filter]                                               |
| ListKeywords        | Keyword         | Lists all keywords for the current image                                                                                                                    | —                                                      |
| LoadFile            | File Management | Loads a single image file for display without adding it to the session; stores path in `$LOAD_FILE_PATH`                                                    | path                                                   |
| Log                 | Scripting       | Writes collected macro output since last Log call to a file                                                                                                 | path, [append]                                         |
| MedianValue         | Analysis        | Returns the median pixel value per channel                                                                                                                  | —                                                      |
| ModifyKeyword       | Keyword         | Changes the value of an existing keyword                                                                                                                    | name, value, [comment], [scope=all\|current]           |
| MoveFile            | File Management | Moves a file to a destination directory; defaults to current frame if source= not specified; stores path in `$NEW_FILE`                                     | [source], destination                                  |
| Print               | Scripting       | Outputs a message to the pcode console; accepts bare expressions — `Print $x + 1` and `Print "hello"` are both valid                                        | message (positional or bare expression)                |
| RunMacro            | Scripting       | Executes a saved macro by name from the database                                                                                                            | name                                                   |
| Set                 | Scripting       | Assigns a value to a variable; string literals on the RHS must use double quotes                                                                            | varname = value                                        |
| SetFrame            | Navigation      | Sets the current active frame by index (0-based)                                                                                                            | index                                                  |
| SetZoom             | Blink & View    | Sets the viewer zoom level                                                                                                                                  | level (fit, 25, 50, 100, 200)                          |
| Test                | Interrogation   | Performs a boolean test and stores result in `$Result`; see §2 for full test list                                                                           | expression                                             |
| WriteCurrent        | I/O             | Writes all buffered images back to their source paths in their original format (atomic temp-rename)                                                         | —                                                      |
| WriteFIT            | I/O             | Writes all buffered images as FITS files (atomic temp-rename)                                                                                               | destination, [overwrite]                               |
| WriteFrame          | I/O             | Writes the currently active frame only back to its source format (atomic temp-rename)                                                                       | —                                                      |
| WriteTIFF           | I/O             | Writes all buffered images as TIFF files with AstroTIFF keyword embedding (atomic temp-rename)                                                              | destination, [overwrite]                               |
| WriteXISF           | I/O             | Writes all buffered images as XISF files (atomic temp-rename)                                                                                               | destination, [overwrite], [compress=true\|false]       |
| pwd                 | Console         | Prints the unique source directories of all files currently loaded in the session (client-side only)                                                        | —                                                      |

### 1.1 Retired Commands

The following commands have been retired and are no longer available:

| Retired Command | Replacement | Notes                                                                  |
| --------------- | ----------- | ---------------------------------------------------------------------- |
| SelectDirectory | AddFiles    | Directory as a first-class entity is replaced by explicit file paths   |
| ReadAll         | AddFiles    | Use AddFiles with explicit paths; ClearSession first if starting fresh |
| ReadFIT         | AddFiles    | Format filtering is now the user's responsibility at selection time    |
| ReadTIFF        | AddFiles    | Format filtering is now the user's responsibility at selection time    |
| ReadXISF        | AddFiles    | Format filtering is now the user's responsibility at selection time    |

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

| Field                | Description                                                  |
| -------------------- | ------------------------------------------------------------ |
| `NAME`               | Token name; used as `$NAME` inside the script                |
| `"Description"`      | Label shown in the parameter prompt dialog                   |
| `required\|optional` | Whether the user must supply a value                         |
| `default="value"`    | Default value for optional params; used if user leaves blank |

**At run time:**

- Console: `RunMacro name=ProcessLights`
- Macro Library / Quick Launch: parameter prompt dialog appears before execution
- Parameters become pcode variables (`$INPUT_DIR`, etc.) for the duration of the macro

**Quick Launch rule:** buttons store only `RunMacro name=X` — never embedded parameter values. Parameters are always resolved at run time.

### 1.7 Session Model

Photyx uses a **global file context** — a flat list of file paths that persists across operations. There is no concept of an "active directory."

- `AddFiles` appends files to the session; existing files are skipped automatically
- `ClearSession` resets the session entirely
- Files from multiple directories can coexist in a single session
- After Commit Results, rejected files are removed from the session; pass frames remain loaded
- `pwd` lists the unique source directories derived from the current file list

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

### 2.3 Session Properties (GetSessionProperty)

| Property        | Type    | Description                             | Example Value         |
| --------------- | ------- | --------------------------------------- | --------------------- |
| FileCount       | Integer | Number of files in the active file list | 47                    |
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

WCS transformation keywords (`CRPIX1/2`, `CD1_1`, `CD1_2`, `CD2_1`, `CD2_2`, `CDELT1/2`, `CROTA1/2`, `LONPOLE`, `LATPOLE`, `PV1_*`, all PC
matrix keywords) have no XISF Property equivalents and are preserved
verbatim in the FITSKeyword block only.

---

## 4. AnalyzeFrames Metrics & Classification

### 4.1 Metrics

| Metric            | Description                                                                                                        | Threshold Type           | Default Reject | Rejection Driver |
| ----------------- | ------------------------------------------------------------------------------------------------------------------ | ------------------------ | -------------- | ---------------- |
| Background Median | Sigma-clipped sky background level                                                                                 | Sigma (session-relative) | > +2.5σ        | ✓                |
| FWHM              | Median FWHM derived from elliptical Moffat PSF fitting                                                             | Sigma (session-relative) | > +2.5σ        | ✓                |
| Eccentricity      | Median eccentricity derived from Moffat PSF fitted ellipse semi-axes: e = sqrt(1 − (b/a)²)                         | Absolute                 | > 0.85         | ✓                |
| Star Count        | Stars accepted by Moffat PSF fitting; bimodal-aware anchoring applied when cloud-induced population split detected | Sigma (session-relative) | < 1.5σ         | ✓                |
| Signal Weight     | PSF-based signal quality: A² / (A + B·π·a·b); derived from Moffat fit parameters                                   | Sigma (session-relative) | < 2.5σ         | ✓                |

### 4.2 Classification

- **PASS / REJECT only** — no SUSPECT classification
- A frame is REJECT if any single metric exceeds its threshold
- `triggered_by` records which metrics caused the REJECT (visible in Analysis Graph tooltip)
- Each REJECT frame is assigned a rejection category: O (Optical), T (Transparency), B (Sky Brightness), or combinations (OT, OB, BT, OBT)
- On Commit: REJECT files are moved to a `rejected/` subfolder within their own source directory and renamed `<name>.<ext>.rejected`; PXFLAG is **not** written to files
- After Commit: rejected files are removed from the session; pass frames remain loaded

### 4.3 Commit Results Behavior

Commit Results is a fast, non-destructive operation:

1. Collects all REJECT paths from `ctx.analysis_results`
2. Moves each REJECT file to `<source_dir>/rejected/<filename>.rejected`
3. Removes rejected paths from `ctx.file_list` and all caches
4. Clears analysis results — pass frames remain loaded and ready for subsequent operations (e.g. stacking)

PXFLAG keywords are **not** written to files on commit. The move itself is the persistence action.

---

## 5. Settings Reference

### 5.1 Persistence Model

| Persisted | User Pref | Behavior         |
| --------- | --------- | ---------------- |
|           |           | Always default   |
| X         |           | Last used        |
| X         | X         | Always user pref |

If a setting is **not persisted**, it always resets to its hard-coded
default at startup. If it is **persisted but not a user pref**, the
last-used value is restored automatically. If it is **persisted and a
user pref**, it appears in Edit > Preferences and the user-set value
is always used until changed.

All defaults and bounds are defined as constants in `src-tauri/src/settings/defaults.rs`. Bounds are enforced in `AppSettings` on read — the database stores raw values and Rust clamps
them. This allows bounds to change without a schema migration.

### 5.2 UI / Viewer Settings

| Setting              | Default        | Persisted | Pref | DB Key  |
| -------------------- | -------------- | --------- | ---- | ------- |
| Color theme          | Matrix         | X         |      | `theme` |
| Default zoom level   | Fit            |           |      | —       |
| Default blink rate   | 0.1s per frame |           |      | —       |
| Default channel view | RGB            |           |      | —       |

### 5.3 File & Path Settings

| Setting                | Default | Persisted | Pref | DB Key                   | Min | Max |
| ---------------------- | ------- | --------- | ---- | ------------------------ | --- | --- |
| JPEG quality           | 75%     | X         | X    | `jpeg_quality`           | 1   | 100 |
| Overwrite behavior     | Prompt  |           |      | —                        | —   | —   |
| Recent directories max | 10      | X         | X    | `recent_directories_max` | 1   | 50  |

### 5.4 pcode / Macro Settings

| Setting                | Default          | Persisted | Pref | DB Key                   | Min | Max  |
| ---------------------- | ---------------- | --------- | ---- | ------------------------ | --- | ---- |
| Backup directory       | Downloads folder | X         | X    | `backup_directory`       | —   | —    |
| Console history size   | 500              | X         | X    | `console_history_size`   | 100 | 5000 |
| Error behavior         | Halt on error    |           |      | —                        | —   | —    |
| Macro editor font size | 13px             | X         | X    | `macro_editor_font_size` | 8   | 24   |

### 5.5 Performance Settings

| Setting                  | Default      | Persisted | Pref | DB Key                     | Min    | Max   |
| ------------------------ | ------------ | --------- | ---- | -------------------------- | ------ | ----- |
| Buffer pool memory limit | 4 GB         | X         | X    | `buffer_pool_memory_limit` | 512 MB | 32 GB |
| Blink pre-cache frames   | All loaded   |           |      | —                          | —      | —     |
| Rayon thread count       | num_cpus - 1 |           |      | —                          | —      | —     |

### 5.6 Quick Launch Settings

The user can pin as many macros as they wish to the Quick Launch panel;
buttons automatically wrap to the next row. Stored in `quick_launch_buttons` table — not in the `preferences` key/value
store.

### 5.7 Threshold Profile Settings (AnalyzeFrames)

Named sets of rejection thresholds stored in the `threshold_profiles` table. The active profile is tracked by `preferences.active_threshold_profile_id`.

| Setting                  | Type     | Default | Min   | Max   | Notes                                                |
| ------------------------ | -------- | ------- | ----- | ----- | ---------------------------------------------------- |
| Name                     | String   | Default | —     | —     |                                                      |
| Background Median reject | Sigma    | +2.5σ   | +0.5σ | +4.0σ |                                                      |
| FWHM reject              | Sigma    | +2.5σ   | +0.5σ | +4.0σ |                                                      |
| Eccentricity reject      | Absolute | 0.85    | 0.10  | 1.00  |                                                      |
| Star Count reject        | Sigma    | 1.5σ    | 0.5σ  | 5.0σ  | Bimodal-anchored; 1.5σ relative to clear-sky cluster |
| Signal Weight reject     | Sigma    | 2.5σ    | 0.5σ  | 5.0σ  |                                                      |

### 5.8 AutoStretch Settings

| Setting             | Default | Persisted | Pref | DB Key                    | Min  | Max  |
| ------------------- | ------- | --------- | ---- | ------------------------- | ---- | ---- |
| AutoStretch enabled | Off     |           |      | —                         | —    | —    |
| Shadow clip         | -2.8    | X         | X    | `autostretch_shadow_clip` | -5.0 | 0.0  |
| Target background   | 0.15    | X         | X    | `autostretch_target_bg`   | 0.01 | 0.50 |

### 5.9 Internal / Non-User-Facing Settings

These settings are persisted but do not appear in Edit > Preferences.

| Setting                     | Default | DB Key                         | Min | Max | Notes                               |
| --------------------------- | ------- | ------------------------------ | --- | --- | ----------------------------------- |
| Crash recovery interval     | 60s     | `crash_recovery_interval_secs` | 15  | 300 | How often recovery state is written |
| Active threshold profile ID | null    | `active_threshold_profile_id`  | —   | —   | FK → threshold_profiles.id          |
| localStorage migrated       | false   | `localStorage_migrated`        | —   | —   | One-time migration flag             |

---

## 6. File Format Coverage

### 6.1 Read Support

All format reading is handled by `plugins/image_reader.rs`, which consolidates FITS, XISF, and TIFF readers. Files are loaded via `AddFiles` with explicit paths.

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

| Command                     | Description                                                                                                              |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `backup_database`           | Creates a timestamped ZIP backup of photyx.db in the configured backup directory                                         |
| `check_crash_recovery`      | Returns crash recovery candidate if written_at is recent and a session is open                                           |
| `close_session`             | Sets closed_at on the current session_history row                                                                        |
| `commit_analysis_results`   | Moves REJECT files to rejected/ subfolders; removes them from session; leaves pass frames loaded                         |
| `debug_buffer_info`         | Returns buffer metadata including display_width and color_space                                                          |
| `delete_macro`              | Deletes a macro and its version history from the database                                                                |
| `dispatch_command`          | Dispatches a single pcode command to the plugin registry (legacy interactive path)                                       |
| `get_all_preferences`       | Returns all preferences as HashMap<String, String>; called at startup to hydrate frontend                                |
| `get_analysis_results`      | Returns per-frame metrics, flags, triggered_by, and session stats                                                        |
| `get_autostretch_frame`     | Computes Auto-STF stretch on current frame, returns JPEG data URL; does not cache                                        |
| `get_blink_cache_status`    | Returns blink cache build status: idle / building / ready                                                                |
| `get_blink_frame`           | Returns a blink frame as JPEG data URL from blink cache (by index + resolution)                                          |
| `get_current_frame`         | Returns current image as raw (unstretched) JPEG data URL, rendered on the fly                                            |
| `get_frame_flags`           | Returns PXFLAG values for all loaded frames (used by blink overlay)                                                      |
| `get_full_frame`            | Returns current image at full resolution with last STF params applied; cached after first call                           |
| `get_histogram`             | Computes and returns histogram bins + stats for current frame (per-channel for RGB)                                      |
| `get_keywords`              | Returns all keywords for current frame as a keyed map                                                                    |
| `get_macro_versions`        | Returns version history for a macro ordered newest first                                                                 |
| `get_macros`                | Returns all macros with name, display_name, script, run_count, last_run_at                                               |
| `get_pixel`                 | Returns raw pixel value(s) at source coordinates (x, y) from the raw image buffer                                        |
| `get_quick_launch_buttons`  | Returns ordered list of Quick Launch button assignments                                                                  |
| `get_session`               | Returns current session state (file list, current frame)                                                                 |
| `get_star_positions`        | Re-runs star detection on current frame; returns {cx, cy, fwhm, r} per star for annotation overlay                       |
| `get_variable`              | Returns a pcode variable value from ctx.variables by name                                                                |
| `increment_macro_run_count` | Updates run_count and last_run_at for a macro after successful execution                                                 |
| `list_log_files`            | Lists available log files in the logs directory, sorted newest first                                                     |
| `list_plugins`              | Returns list of registered plugins with name, version, and type                                                          |
| `load_analysis_json`        | Clears session; populates analysis_results, session_stats, thresholds from JSON payload; sets is_imported_session = true |
| `load_file`                 | Reads a single image file from disk, injects into session, returns JPEG data URL                                         |
| `open_session`              | Inserts a session_history row with closed_at = NULL; returns session id                                                  |
| `read_log_file`             | Reads and parses a log file into structured {timestamp, level, module, message} lines                                    |
| `rename_macro`              | Renames a macro; validates name uniqueness                                                                               |
| `restore_database`          | Restores photyx.db from a ZIP backup; reopens connection in-place without app restart                                    |
| `restore_macro_version`     | Restores a previous macro version as the current script                                                                  |
| `run_script`                | Executes a pcode script string; returns ScriptResponse with results, session_changed, display_changed, client_actions    |
| `save_macro`                | Inserts or updates a macro; saves previous version to macro_versions before overwriting                                  |
| `save_quick_launch_buttons` | Replaces all Quick Launch button assignments                                                                             |
| `set_frame_flag`            | Updates PASS/REJECT flag for a single frame in ctx.analysis_results by path; used before Commit to sync toggled flags    |
| `set_preference`            | Upserts a single preference key/value; writes through AppSettings struct                                                 |
| `start_background_cache`    | Spawns background task to build blink cache JPEGs                                                                        |
| `write_crash_recovery`      | Upserts the single crash_recovery row with current session state (file list, current frame)                              |

---

## 8. Path Conventions

| Convention      | Rule                                                                              |
| --------------- | --------------------------------------------------------------------------------- |
| Separator       | Forward slash `/` always; backend translates to OS-native before filesystem calls |
| Absolute paths  | `D:/Astrophotos/M31` (Windows) or `/home/user/photos` (macOS/Linux)               |
| Relative paths  | Resolved against `common_parent()` of the current file list                       |
| Home shorthand  | `~` expands to current user's home directory on all platforms                     |
| UNC paths       | `//192.168.1.100/Astrophotos/M31` — useful for ASIAir Pro over local network      |
| Spaces in paths | Must be enclosed in double quotes                                                 |

---

## 9. Plugin Status

All plugins are Built-in Native in the initial release. WASM user plugins are supported via the plugin framework but none are shipped by default.

| Plugin              | Category        | Status     |
| ------------------- | --------------- | ---------- |
| AddFiles            | Session         | ✅ Complete |
| AddKeyword          | Keyword         | ✅ Complete |
| AnalyzeFrames       | Frame Analysis  | ✅ Complete |
| Assert              | Scripting       | ✅ Complete |
| AutoStretch         | Processing      | ✅ Complete |
| CacheFrames         | Blink           | ✅ Complete |
| ClearSession        | Session         | ✅ Complete |
| ComputeEccentricity | Analysis        | ✅ Complete |
| ComputeFWHM         | Analysis        | ✅ Complete |
| ContourHeatmap      | Analysis        | ✅ Complete |
| CopyFile            | File Management | ✅ Complete |
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
| RunMacro            | Scripting       | ✅ Complete |
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
