# Image Processing

Commands for stretching and debayering frames.

---

## `AutoStretch`

Applies an automatic stretch to the current frame for display using
the PixInsight-compatible Auto-STF algorithm. The raw pixel buffer is
not modified.

```
AutoStretch [shadowClip=<float>] [targetBackground=<float>]
```

| Argument | Required | Default | Description |
| ------------------ | -------- | ------- | -------------------------------------- |
| `shadowClip` | No | `-2.8` | Shadow clipping point in sigma units |
| `targetBackground` | No | `0.15` | Target background level (0.0–1.0) |

```
AutoStretch shadowClip=-2.8 targetBackground=0.25
```

---

## `DebayerImage`

Debayers a Bayer CFA image to interleaved RGB using bilinear
interpolation. Operates on the transient stack result if one exists;
otherwise operates on the current session frame. The Bayer pattern is
always read from the `BAYERPAT` (or `BAYER_PATTERN`) keyword,
defaulting to RGGB if neither is present — there is currently no way
to override the pattern or interpolation method from pcode.

```
DebayerImage
```

Takes no arguments. Frames that are already RGB are left unchanged
(reported, not an error).

```
DebayerImage
```
