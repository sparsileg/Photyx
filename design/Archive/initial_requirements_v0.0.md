# Photyx

An application to read, view, process, and analyze astrophotos.

## High-level Requirements

- Install on Windows, Apple, Linux
- High performance UI and backend processing code
- Initially read FIT, XISF, and TIFF image files (leave hooks for more)
- Write FIT, XISF, TIFF, PNG, and JPEG image files (leave hooks for more)
- Update keywords for FIT, XISF, (AstroTiff?)
- Read in a series of files and be able to review them rapidly (blink)
- Variable zoom levels for viewing
- Write short scripts and save them as a function (select directory,
  specify file type, read files, write files, add keyword, delete keyword,
  modify keyword, etc.)
- External API that can be called from an external program (or command
  line) to execute a script.
- Perform analysis. For instance FWHM median calculation and contour
  plots. Other analysis might be calculating Eccentricity of stars, Median
  value, and count stars. Open architecture (plugins, modules?) to allow
  additional functionality.
