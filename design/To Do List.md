# To Do List

1. ~~**Iterative sigma clipping** — two-pass session stats + Analysis Graph visual indicator~~
2. ~~**AnalyzeFrames metric caching** — skip Pass 1 when metrics already cached~~
3. **Zoom buttons on sidebar** — add Fit/25%/50%/100%/200% buttons to the icon sidebar below the Plugin Manager icon
4. **Remove channel switching** — remove R/G/B channel buttons from toolbar, delete `activeChannel` from `ui.ts` and all related code (`$ui.activeChannel`, any channel extraction logic)
5. **Toolbar repurposing** — free up toolbar space for active threshold profile name, active theme indicator, etc. Put the zoom buttons Fit, 25%, 50%, 100% on the sidebar immediately below the Plugin Manager. Remove the Channel buttons from the tool bar and delete any code that is related to displaying the R, G, B channels separately.
6. ~~~~The first is Copy to Clipboard. This would be the entire table, including headers with the numbers represented at max precision. ~~~~
7. ~~The second would be Save as CVS. Similar as we include the numbers at max precision, but in CSV format.~~
8. SNR estimator revision. The current estimator rewards integrated star flux rather than signal quality. This artifact appears in all five sessions. The revised estimator should account for PSF size when computing SNR — a frame with 2× the FWHM should not score higher SNR for the same target.
9. Pulse the "Cache is being built" notification via `notifications.running()`
10. ~~Bg Std Dev and Bg Gradient removal from the analysis engine~~
11. **Enhance AnalyzeFrames:**
     - Star count default threshold −1.5σ → −3.0σ
     - SNR removed from rejection classification, kept as diagnostic
     - Three rejection categories (O, T, B) with multi-category support
     - New Category column in Results table
     - Graph dot colors by category with split dots for multi-category, 2px black border on all dots
     - Category legend on graph (always visible)
12- **Redo Menu Structure**
     - Export Session JSON (replaces Export CSV)
     - Import Session JSON (loads full session including images)
     - Please separator between Analyze > Analysis Graph and Analyze > Contour Plot
     - Session menu (new) — moves Select Directory, Close Session from File, adds Export/Import JSON
     - File menu restructured — Read Single Image + Exit only
13- **Commit Enhancement**
     - Commit enhancement — move rejected files to `rejected/` subfolder with `.rejected` appended
     - CSV import removed (superseded by JSON)
     - Toggle PXFLAG value in Results Table and be able to save the result, if desired, when the Commit button is pressed
14- **Deferred:**
     - Memory audit
     - AnalyzeFrames CLI standalone binary


