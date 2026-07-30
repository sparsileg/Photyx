# pcode Command Reference

pcode is Photyx's built-in scripting language — line-oriented, with
variables, conditionals, loops, and commands covering session
management, file I/O, keywords, analysis, and stacking.

New to pcode? **[Tutorial 2: Automating with pcode](../2-automating-with-pcode.md)**
is the place to start. This section is the full reference to come back
to afterward.

## Sections

- **[Language Basics](language-basics.md)** — syntax, variables,
  arithmetic, flow control, error handling, console output, trace mode
- **[Session Commands](session-commands.md)** — `AddFiles`,
  `ReadImages`, `ClearSession`, `LoadFile`, `CountFiles`,
  `FilterByKeyword`, `RejectFrame`
- **[Write & Export Commands](write-export-commands.md)** —
  `WriteCurrent`, `WriteFrame`, `WriteFIT`, `WriteXISF`, `CopyFile`,
  `MoveFile`
- **[Keyword Commands](keyword-commands.md)** — `AddKeyword`,
  `ModifyKeyword`, `DeleteKeyword`, `CopyKeyword`, `GetKeyword`,
  `ListKeywords`
- **[Analysis Commands](analysis-commands.md)** — `AnalyzeFrames`,
  `CommitAnalysis`, `ExportAnalysisReport`, `ComputeFWHM`,
  `ComputeEccentricity`, `CountStars`, `GetHistogram`,
  `ContourHeatmap`, `BackgroundMedian`
- **[Image Processing](image-processing.md)** — `AutoStretch`,
  `DebayerImage`
- **[Stacking Commands](stacking-commands.md)** — `StackFrames`,
  `CommitStretch`, `ClearStack`
- **[Display & Navigation](display-navigation.md)** — `SetFrame`,
  `CacheFrames`, `ClearAnnotations`, `ShowAnalysisGraph`,
  `ShowAnalysisResults`
- **[Scripting Utilities](scripting-utilities.md)** — `Set`, `Print`,
  `Assert`, `CountMatches`, `GetSystemPath`, `RunMacro`, `Log`,
  `If`/`Else`/`EndIf`, `For`/`EndFor`, Console Built-ins
- **[Complete Examples](examples.md)** — worked, end-to-end scripts
