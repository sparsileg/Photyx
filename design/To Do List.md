# To Do List

1. **Iterative sigma clipping** — two-pass session stats + Analysis Graph visual indicator
2. **AnalyzeFrames metric caching** — skip Pass 1 when metrics already cached
3. **Zoom buttons on sidebar** — add Fit/25%/50%/100%/200% buttons to the icon sidebar below the Plugin Manager icon
4. **Remove channel switching** — remove R/G/B channel buttons from toolbar, delete `activeChannel` from `ui.ts` and all related code (`$ui.activeChannel`, any channel extraction logic)
5. **Toolbar repurposing** — free up toolbar space for active threshold profile name, active theme indicator, etc. Put the zoom buttons Fit, 25%, 50%, 100% on the sidebar immediately below the Plugin Manager. Remove the Channel buttons from the tool bar and delete any code that is related to displaying the R, G, B channels separately.
6. The first is Copy to Clipboard. This would be the entire table, including headers with the numbers represented at max precision. 
7. The second would be Save as CVS. Similar as we include the numbers at max precision, but in CSV format.
8. Where possible, extract the target from the filenames and show the target name on the on the Analysis Results bar and Analysis Graph bar.
9. The current estimator rewards integrated star flux rather than signal quality. This artifact appears in all five sessions. The revised estimator should account for PSF size when computing SNR — a frame with 2× the FWHM should not score higher SNR for the same target.


