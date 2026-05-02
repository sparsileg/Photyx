# To Do List



1. **Iterative sigma clipping** — two-pass session stats + Analysis Graph visual indicator
2. **AnalyzeFrames metric caching** — skip Pass 1 when metrics already cached
3. **Zoom buttons on sidebar** — add Fit/25%/50%/100%/200% buttons to the icon sidebar below the Plugin Manager icon
4. **Remove channel switching** — remove R/G/B channel buttons from toolbar, delete `activeChannel` from `ui.ts` and all related code (`$ui.activeChannel`, any channel extraction logic)
5. **Toolbar repurposing** — free up toolbar space for active threshold profile name, active theme indicator, etc. Put the zoom buttons Fit, 25%, 50%, 100% on the sidebar immediately below the Plugin Manager. Remove the Channel buttons from the tool bar and delete any code that is related to displaying the R, G, B channels separately. T

Items 3 and 4 are related — do them together since removing the channel buttons changes the toolbar layout and we can think about what replaces them at the same time.
