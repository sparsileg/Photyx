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
  operating system's file browser into the Photyx window to add them
  to the same session.

Either way, watch the toolbar — it updates to show
`N files · M directories` once your frames are loaded.

> **Note:** Photyx works best with FITS (`.fit`/`.fits`/`.fts`) files.
> XISF is also fully supported if that's what your capture software
> produces.

## 3. View a Frame

Click through a few frames using the viewer's navigation controls.
Try zooming in on a star field. Then apply **AutoStretch** to bring
out detail in an otherwise flat-looking linear image — this only
changes the display, not your underlying pixel data.

## 4. Blink for a Quick Review

Open the **Blink** tab and press play. Blink rapidly cycles through
your frames at reduced resolution so you can eyeball focus, tracking,
and cloud drift across the whole session in seconds — well before
running any formal analysis. Try stepping frame-by-frame as well as
continuous playback, and adjust the Min Delay if it's cycling too fast
or slow to follow.

## 5. Tag the Session with a Keyword

The Keyword Editor panel is great for looking at one frame's header,
but it can only edit one file at a time. To add a keyword across an
entire session at once, use the console instead.

Open the console (the collapsible panel at the bottom of the viewer)
and run:

```
AddKeyword name=FILTER value=duo comment="Tutorial test tag" scope=all
WriteCurrent
```

`scope=all` applies the keyword to every loaded frame in one command.
`WriteCurrent` writes the change back to your files — for FITS files
this only rewrites header keywords, the pixel data on disk is
untouched.

> This keyword isn't required by anything downstream in this
> tutorial — it's here so you get comfortable with the console before
> Tutorial 2. We'll remove it again in the final step.

## 6. Run Analyze Frames

Open `Analyze > Analyze Frames`. You'll be prompted to pick a
threshold profile — choose **Default** for this run and confirm.

Photyx computes four quality metrics per frame — background median,
FWHM, eccentricity, and star count — and classifies every frame as
**PASS** or **REJECT** relative to the rest of *this* session. A frame
that would pass in bright moonlight might reject in a darker session,
and that's intentional: classification is always session-relative,
never an absolute standard.

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

## 8. Read Analysis Graph

Open `Analyze > Analysis Graph` for the same results, visually. Each
dot is a frame — white for PASS, colored per the table above for
REJECT — plotted against sigma bands with the reject threshold lines
drawn in. Click any dot to jump straight to that frame in the viewer.

## 9. Toggle a Flag

Right-click any row in Analysis Results. If it's a REJECT, you'll see
"Set to PASS" — if it's a PASS, "Set to REJECT." Try toggling one.

Nothing is written to disk yet — this is a local, reversible override
that only takes effect when you commit.

## 10. Commit Results

Click **Commit Results**. Here's exactly what happens:

- Any toggle you made in step 9 is applied first
- Every frame still flagged REJECT is moved into a `rejected/`
  subfolder inside its own source directory, with `.reject` appended
  to the filename (e.g. `frame001.fit.reject`)
- This is a **move, not a delete** — nothing is destroyed
- PASS frames stay loaded and the session stays open, ready for the
  next step

## 11. Stack the Surviving Frames

Open the Stacking Workspace (`Analyze` menu) and run a stack on your
remaining PASS frames.

At a high level, Photyx groups frames by imaging session/rotation,
aligns each frame to a reference using star matching, and combines
them with a sigma-clipped mean — outlier pixels (satellite trails,
cosmic ray hits) are automatically excluded from the final result on a
per-pixel basis.

> *Reviewer note for Stan: I don't have exact button/control labels
> for the Stacking Workspace UI in the reference docs — please correct
> this step with the real control names before publishing.*

## 12. View and Export the Stack

Preview the stretched result in the Stacking Workspace. When you're
happy with it, export it from the console:

```
WriteXISF destination="<your output folder>" stack=true
Print "Stack written to: " + $STACKED
```

## 13. Clean Up the Test Keyword

Back in the console, remove the `FILTER` keyword you added in step 5:

```
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
