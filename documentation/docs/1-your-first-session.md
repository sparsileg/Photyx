# Tutorial 1: Your First Session

**What you'll learn:** how to load a session, review frames, run
quality analysis, reject the bad ones, and produce a quick-look stack
— the core Photyx workflow, start to finish.

**Before you begin:** you'll need a folder of your own FITS light
frames from a real imaging session. This tutorial will modify those
files — it adds a `FILTER` keyword to every frame partway through, and
removes it again at the end. If you'd rather not modify your files at
all, make a copy of the session folder first and work from the copy.

---

## 1. Launch Photyx

When Photyx opens you'll see:

- **Menu Bar** across the top (File, Session, Edit, View, Analyze, Tools, Help)
- **Toolbar** below it, with viewer controls and a file/directory count
- **Icon Sidebar** on the left — File Browser, Keyword Editor, Macro
  Library, and Plugin Manager panels live behind these icons
- **Viewer** filling the center — this is where your images display
- **Status Bar** along the bottom, which shows what Photyx is doing

Nothing is loaded yet, so the toolbar's file count is empty. That
changes in the next step.

## 2. Add Files

Load your session two different ways, so you've seen both:

- **File browser:** `Session > Add Files…` (or Ctrl+O), then select
  roughly half of your frames.
- **Drag-and-drop:** drag the remaining files directly from your
  operating system's file explorer into the Photyx window to add them
  to the same session.

Either way, watch the toolbar — it updates to show `N files · M
directories` once your frames are loaded. Note that after the first
set of files is loaded, subsequent loads do not show progress
updates. This is a known issue.

> **Note:** Photyx works best with FITS (`.fit`/`.fits`/`.fts`) files.
> XISF is also fully supported if that's what your capture software
> produces.

The set of files loaded is known as a `session`.

## 3. View a Frame

Click the `Linear` button on the top toolbar to autostretch the
current image. Adjust the Black and Background (BG) sliders to adjust
the stretch. This only changes the display, not your underlying pixel
data.

Click through a few frames using the viewer's navigation arrows,
located near the bottom right of the viewer. Try zooming in on a star
field.

**NOTE** that the display image is a raw compressed JPEG image that is
 uncalibrated and not debayered. You will definitely see artifacts in
 the raw subs.

## 4. Blink for a Quick Review

Open the **Blink** tab in the bottom **Info Panel** and press
play. Blink rapidly cycles through your frames at reduced resolution
so you can eyeball focus, tracking, and cloud drift across the whole
session in seconds — well before running any formal analysis. Try
stepping frame-by-frame as well as continuous playback, and adjust the
Min Delay if it's cycling too fast or slow to follow.

**Tip**: You can manually reject frames while in Blink mode. Clearing
out extreme outliers by eye before running Analyze Frames gives the
statistical analysis a cleaner population to work from — and produces
better results.

## 5. Tag the Session with a Keyword

The Keyword Editor panel is great for looking at one frame's header,
but it can only edit one file at a time. To add a keyword across an
entire session at once, use the console instead. Later you will learn
how to write and save macros for operations you repeat regularly.

Maximize the console (the collapsible panel at the bottom of the
viewer) by clicking on the line that says "PCODE CONSOLE" and typing:

In the console, type `ListKeywords`. This will retrieve all the FIT
keywords from the currently displayed image and print them to the
screen and in a scrollable popup window.

In the console, type (note the autocomplete while typing):

```
help AddKeyword
```

Click the `X` to close the help window.

When typing a command at the console, hit `<TAB>` to auto-complete. Once
you've typed enough characters to get a unique string, hitting `<TAB>`
will display the possible command arguments.

```
AddKeyword name=FILTER value="duo" comment="Tutorial test tag" scope=all
WriteCurrent
ListKeywords
```

`scope=all` applies the keyword to every loaded frame in one command
and is the default value.  `WriteCurrent` writes the change back to
your files — for FITS files this only rewrites header keywords, the
pixel data on disk is untouched.

**TIP**: Use double-quotes around an argument that has spaces in
it. It's a good habit to just wrap everything in double quotes even if
you don't have to. The keyword name is always upper case, but the
value and comment can be mixed case.

> This keyword isn't required by anything downstream in this
> tutorial — it's here so you get comfortable with the console before
> Tutorial 2. We'll remove it again in the final step.

Clear the console by either clicking the `Clear` at the top right of
the console or typing `Clear` in the console. Minimize the console by
clicking on the title bar.

## 6. Run Analyze Frames

Open `Analyze > Analyze Frames`. You'll be prompted to pick a
threshold profile — choose **Default** for this run and confirm.

Photyx computes four quality metrics per frame — background median,
FWHM, eccentricity, and star count — and classifies every frame as
PASS or REJECT relative to the rest of this session. Three of the four
(background median, FWHM, star count) are session-relative by design:
a frame is judged against the statistics of the frames around it, not
against a fixed number. Eccentricity is the exception — it rejects on
an absolute value, regardless of session. Because most of the
classification depends on what else is in the pool, commit the results
of Analyze Frames only when you're processing all the frames of a
project under the same filter — mixing in a partial or wrong-filter
set skews the very statistics every frame is being judged against.

When the analysis is complete, it will print the number of frames that
passed and the number that were rejected in the console.

## 7. Read Analysis Results

Open `Analyze > Analysis Results`. You'll see a sortable table:
filename, the four metrics, PXFLAG (PASS/REJECT), and a rejection
category.

Category badges tell you *why* a frame was rejected:

| Badge | Meaning | Triggered by |
|-------|---------|--------------|
| **O** (red) | Optical | FWHM and/or eccentricity |
| **T** (yellow) | Transparency | Low star count |
| **B** (blue) | Sky Brightness | High background median |

A frame can show more than one badge if more than one metric fired.

## 8. Toggle a Flag

Right-click any row in Analysis Results. If it's a REJECT, you'll see
"Set to PASS" — if it's a PASS, "Set to REJECT." Try toggling one.

Nothing is written to disk yet — this is a local, reversible override
that only takes effect when you commit.

## 9. Read Analysis Graph

Open `Analyze > Analysis Graph` for the same results, visually. Each
dot is a frame — white for PASS, colored per the table above for
REJECT — plotted against sigma bands with the reject threshold lines
drawn in.

Select the metric that is displayed using the dropdown at the top of
the graph. You can select a second metric to display over the
first. Click any dot to jump straight to that frame in the viewer.

## 10. Commit Results

Click **Commit Results**. Here's exactly what happens:

- Every frame flagged as REJECT is moved into a `rejected/`
  subfolder inside its own source directory, with `.reject` appended
  to the filename (e.g. `frame001.fit.reject`) and is removed from the
  session.
- This is a **move, not a delete** — nothing is destroyed
- PASS frames stay loaded and the session stays open, ready for the
  next step

## 11. Stack the Surviving Frames

Open the Stacking Workspace (`Analyze` menu) and press the `Stack`
button on the top toolbar to stack all the PASS frames.

At a high level, Photyx groups frames by imaging session/rotation,
aligns each frame to a reference using star matching, and combines
them with a sigma-clipped mean — outlier pixels (satellite trails,
cosmic ray hits) are automatically excluded from the final result on a
per-pixel basis.

**NOTE*: This is a fast, rudimentary stack. The final product does not
use calibration frames and white balance is not applied. The stacked
product should only be used for a quick visual validation of the
session images. The stacked image is **NOT** production quality.

## 12. View and Export the Stack

Preview the stretched result in the Stacking Workspace. If you wish,
you can commit the stretch and export the using using the `Stacking
Workspace` UI. Close the `Stacking Workspace` when finished.

## 13. Clean Up the Test Keyword

Back in the console, remove the `FILTER` keyword you added in step 5:

```
help DeleteKeyword
DeleteKeyword name=FILTER scope=all
WriteCurrent
```

Your files are now back to their original header state.

## 14. Wrap-Up

What you just produced is a **quick-look validation stack** — good
enough to confirm your data is worth carrying forward, not a finished
image. From here, take your surviving (PASS) frames into PixInsight or
Siril for real processing: calibration, deeper registration, and
proper stretching.

Everything you just did by clicking can also be done — and automated
— with a script. That's **Tutorial 2: Automating with pcode**.
