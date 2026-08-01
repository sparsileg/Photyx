# Photyx RC3 — Tester Smoke Test Plan

**Purpose:** Quick functional pass to confirm core functionality is intact
before wider RC3 distribution. Not a full regression suite — aimed at
30–45 minutes per platform.

**Supported formats for this test pass:** FITS (`.fit`/`.fits`/`.fts`) and
XISF (`.xisf`) only.

**Known issue — do not file:** Separate RGB channel views are known
broken.

---

## 1. Session Basics

- [ ] `Session > Add Files…` on a directory of FITS lights — confirm
      `N files · M directories` in the toolbar
- [ ] `Clear Session`, then `File > Load Single Image…` — confirm the
      loaded file also appears in the session file list (this is by
      design, not a bug)
- [ ] Load an XISF file directly — confirm it opens and displays
- [ ] Mixed session: add files from two different directories in one
      session — confirm both directories show in the count and no
      cross-directory issues appear

## 2. Keywords

- [ ] Open Keyword Editor on a frame — confirm values look correct
- [ ] Add a keyword, modify a keyword, delete a keyword — `WriteCurrent`
      — reopen the file — confirm changes persisted
- [ ] Confirm `WriteCurrent` on a `.fit` file does **not** alter pixel
      data (keyword-only rewrite) — reopen and spot-check the image
      looks unchanged

## 3. Analysis

- [ ] `Analyze Frames` on a real session with a selected threshold
      profile — confirm it completes and the progress indicator shows
      then clears
- [ ] Open Analysis Results — sort by a column, confirm sort works
- [ ] Open Analysis Graph — click a dot, confirm it navigates to that
      frame
- [ ] **PXFLAG toggle:** right-click a row in Analysis Results →
      "Set to PASS" (on a REJECT row) or "Set to REJECT" (on a PASS
      row). Confirm the toggle shows an amber left border, and that
      switching to Analysis Graph reflects the same pending toggle
      before committing (they share state)
- [ ] Commit Results — confirm REJECT frames move to a `rejected/`
      subfolder (suffix `.reject`), pass frames remain loaded, session
      stays open

## 4. Stacking

- [ ] Run `StackFrames` on a small session — confirm it completes and
      shows a preview
- [ ] Check the printed Stack Quality Summary for anything unexpected
      (e.g. an unexplained `low_coverage_pixels` warning)
- [ ] `CommitStretch`, then `WriteXISF destination=<path> stack=true` —
      confirm the file is written and `$STACKED` is populated
- [ ] `ClearStack` — confirm the viewer returns to the normal session
      image

## 5. Export / Import

- [ ] `Export Analysis Results` (Session menu) — confirm a JSON file is
      written
- [ ] `Import Analysis Results` on that same JSON — confirm the
      IMPORTED badge appears, Commit Results is disabled, and display
      (viewer, keyword panel, etc.) still works normally

## 6. Persistence / Preferences

- [ ] Change a preference (theme, thread count) — restart the app —
      confirm it stuck
- [ ] Toggle a Feature Preference flag (reference frame badge) —
      confirm Analysis Graph/Results reflect it immediately

## 7. Blink

- [ ] `CacheFrames` (or trigger via Blink tab) — play/pause/step through
      frames
- [ ] Toggle "Highlight Rejected" — confirm REJECT frames show a red
      border during blink
- [ ] Switch blink resolution (12.5% ↔ 25%) — confirm cache rebuilds

## 8. Macros

- [ ] Open Macro Library, confirm existing macros list correctly
- [ ] Save the complex test macro below under a new name, confirm it
      appears in the library
- [ ] Run it from the console via `RunMacro name="..."`
- [ ] Run the same macro from a pinned Quick Launch button (see §9)
- [ ] Confirm output appears line-by-line in the console for both
      invocation paths
- [ ] Edit the macro (change one line), confirm a new version is saved
      to version history, and that restoring the previous version works

### Complex test macro

Exercises variables, arithmetic, a loop, a conditional, keyword reads
with a default, several analysis commands, string concatenation, and
logging to a file.

**Before running:** edit the `ReadImages path=` line to point at a real
session directory on the test machine.

```
# test-analysis-report — RC3 macro smoke test
# Exercises: ReadImages, CountFiles, Assert, For/EndFor, If/Else/EndIf,
# GetKeyword with default, ComputeFWHM/ComputeEccentricity/CountStars,
# arithmetic, string concatenation, math functions, GetSystemPath, Log

ClearSession
ReadImages path="/path/to/a/test/session"

CountFiles
Assert expression="$filecount > 0"
Print "Loaded " + $filecount + " frames"

Set rejectCount = 0
Set totalFwhm = 0

For i = 0 To $filecount - 1
  SetFrame index=$i
  GetKeyword name=FILTER default="Unknown"
  ComputeFWHM
  ComputeEccentricity
  CountStars
  Set totalFwhm = $totalFwhm + $fwhm

  If $fwhm > 4.0
    Set rejectCount = $rejectCount + 1
    Print "Frame " + $i + " (" + $FILTER + "): POOR FOCUS, FWHM=" + $fwhm
  Else
    Print "Frame " + $i + " (" + $FILTER + "): OK, FWHM=" + $fwhm + " Ecc=" + $eccentricity + " Stars=" + $starcount
  EndIf
EndFor

Set avgFwhm = $totalFwhm / $filecount
Print "Average FWHM: " + round($avgFwhm * 100) / 100
Print "Frames flagged poor focus: " + $rejectCount

GetSystemPath name=downloads
Log path="$downloads/rc3-macro-test-report.log"
Print "Report written to $downloads/rc3-macro-test-report.log"
```

**Pass criteria:** completes without halting, prints one line per
frame plus the two summary lines, and `rc3-macro-test-report.log`
exists in the Downloads folder with matching content.

## 9. Quick Launch Bar

- [ ] Pin the test macro above to Quick Launch from the Macro Library
- [ ] Confirm the button appears and wraps to a new row if the bar
      is full
- [ ] Click it — confirm it runs the macro and output appears in the
      console (same as `RunMacro`)
- [ ] Right-click the button — confirm it can be removed
- [ ] Restart the app — confirm remaining Quick Launch assignments
      persisted

## 10. Backup / Restore

- [ ] `Tools > Backup Database` — confirm a timestamped ZIP appears in
      the configured backup directory
- [ ] Make a small change after the backup (e.g. rename a macro, pin a
      new Quick Launch button, change a preference)
- [ ] `Tools > Restore Database` from the backup ZIP — confirm the
      change made after backup is gone (i.e. the restore actually took
      effect) and the app remains usable without a restart
- [ ] Confirm WAL/SHM files don't linger oddly after restore (quick
      look in the app data directory is enough — not a deep check)

## 11. Standalone pcode Commands to Verify

Run each individually from the console (Trace mode on, so resolved
arguments are visible) against a loaded session:

- [ ] `GetKeyword name=OBJECT default=""` then `Print $OBJECT`
- [ ] `AddKeyword name=TESTKEY value="rc3" comment="smoke test"` →
      `GetKeyword name=TESTKEY` → confirm round-trip
- [ ] `CopyKeyword from=EXPTIME to=EXPOSURE` → confirm both keywords
      now present
- [ ] `DeleteKeyword name=EXPOSURE` → confirm it's gone
- [ ] `CountMatches pattern="<a glob that matches some files>"` →
      `Print $matchcount`
- [ ] `BackgroundMedian` on the current frame → confirm a plausible
      value prints
- [ ] `ContourHeatmap palette=plasma` → confirm `$NEW_FILE` is set and
      the heatmap XISF is written next to the source file
- [ ] `DebayerImage` on an OSC frame → confirm it completes (or reports
      "already RGB" on a mono/already-debayered frame, not an error)
- [ ] Path functions: `Set b = basename($LOAD_FILE_PATH)`,
      `Set d = dirof($LOAD_FILE_PATH)`,
      `Set s = stripext($LOAD_FILE_PATH)` — print all three, confirm
      they look right
- [ ] `RejectFrame append=smoketest` on a non-critical test frame —
      confirm it moves to `rejected/` with the `.smoketest` suffix and
      drops out of the session

## 12. Platform-Specific

- [ ] **macOS:** unsigned build still requires `xattr -cr` — confirm
      the workaround is still accurate for this RC
- [ ] **Windows:** basic launch, load, analyze — confirm no vcpkg-
      related DLL errors on a clean machine
- [ ] **Linux:** basic launch, load, analyze — if using the GTK file
      picker, remember it silently refuses a multi-select mixing files
      and folders (e.g. Ctrl+A with a `rejected/` subfolder present) —
      select files individually instead

---

*Document under configuration control. Update alongside each RC as
functionality changes — do not let this drift silently out of sync
with the app.*
