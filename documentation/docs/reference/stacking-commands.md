# Stacking Commands

Commands for producing, stretching, and clearing the transient stack
result.

---

## `StackFrames`

Stacks all session frames into a single result image using
meridian-flip-aware group reference selection, FFT phase-correlation +
triangle rigid alignment, and two-pass sigma-clipped mean
combination. Color-aware: if the reference frame is Bayer or RGB, the
stack accumulates all three channels.

```
StackFrames
```

Takes no arguments — calibration is applied separately before frames
are loaded into the session, not as part of this command.

```
StackFrames
```

---

## `CommitStretch`

Permanently applies the Auto-STF stretch to the stack result pixel
buffer. After committing, the stack buffer holds non-linear
(stretched) data. Use `WriteXISF stack=true` to export.

```
CommitStretch [shadow_clip=<float>] [target_bg=<float>]
```

| Argument | Required | Description |
| ------------- | -------- | ------------------------------------------------------- |
| `shadow_clip` | No | Shadow clipping factor (default: current context value) |
| `target_bg` | No | Target background value 0.0–1.0 (default: current context value) |

```
CommitStretch shadow_clip=-3.5 target_bg=0.10
```

---

## `ClearStack`

Discards the transient stack result and per-frame contribution data,
returning the viewer to the normal session image.

```
ClearStack
```
