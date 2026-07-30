# Complete Examples

Worked, end-to-end scripts combining commands from the other reference
pages.

---

## Batch format conversion: FITS → XISF

```
# Convert all lights in a directory from FITS to XISF
ClearSession
ReadImages path="/data/M31/lights"
WriteXISF destination="/data/M31/xisf" overwrite=false compress=false
Print "Conversion complete."
```

---

## Quality analysis and review workflow

```
# Standard analysis workflow
ClearSession
ReadImages path="/data/NGC7331/lights"

CountFiles
Assert expression="$filecount > 0"
Print "Loaded " + $filecount + " frames"

AnalyzeFrames
ShowAnalysisResults
```

After reviewing results and committing, pass frames remain loaded and
are ready to stack.

---

## Filter session by keyword then write

```
# Keep only Ha frames, write to a separate directory
FilterByKeyword name=FILTER value=Ha
CountFiles
Print "Ha frames: " + $filecount
WriteFIT destination="/data/Ha-only" overwrite=true
```

---

## Per-frame FWHM report with log

```
# Measure FWHM on every frame and write results to a log file
ReadImages path="/data/lights"

CountFiles
For i = 0 To $filecount - 1
  SetFrame index=$i
  GetKeyword name=DATE-OBS
  ComputeFWHM
  Print $DATE-OBS + "  FWHM=" + $fwhm
EndFor

Log path="/logs/fwhm_report.log"
```

---

## Numeric loop: step through frames by index

```
# Visit the first five frames by index
For i = 0 To 4
  SetFrame index=$i
  ComputeFWHM
  Print "Frame " + $i + ": FWHM=" + $fwhm
EndFor
```

---

## Conditional processing based on keyword

```
# Apply different stretch depending on filter
ReadImages path="/data/session"

CountFiles
For i = 0 To $filecount - 1
  SetFrame index=$i
  GetKeyword name=FILTER
  If $FILTER == "Ha"
    AutoStretch shadowClip=-2.4 targetBackground=0.10
  Else
    AutoStretch shadowClip=-2.8 targetBackground=0.20
  EndIf
EndFor
```

---

## Heatmap generation with file capture

```
# Generate a contour heatmap and copy it to a review folder
SetFrame index=0
ContourHeatmap palette=plasma contour_levels=12
Print "Heatmap written to: " + $NEW_FILE
CopyFile source=$NEW_FILE destination="/data/review"
```

---

## Full stack pipeline

```
# Load, analyze, stack, stretch, and export
ClearSession
ReadImages path="/data/M31/lights"
AnalyzeFrames
ShowAnalysisResults
# After manually committing rejects in the UI, continue:
StackFrames
CommitStretch shadow_clip=-3.5 target_bg=0.10
WriteXISF destination="/data/M31/stacked" stack=true
Print "Stack complete."
```

---

## Calling a sub-macro

```
# Main workflow delegates to reusable sub-macros
RunMacro name="load-and-check"
RunMacro name="analyze-and-report"
```

This allows building libraries of composable macros pinned to the
Quick Launch bar.
