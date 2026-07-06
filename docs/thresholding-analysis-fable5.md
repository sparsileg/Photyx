# Photyx Analysis Algorithm & Results

Evaluated filtering algorithms' effectiveness and identified marginal
rejection cases warranting threshold recalibration Evaluated filtering
algorithms' effectiveness and identified marginal rejection cases
warranting threshold recalibration Went through both reports frame by
frame, including per-session timelines. Short version: the algorithms
are measuring something real and the rejections are overwhelmingly
justified — but the StarCount threshold at 1.5σ is slightly
over-rejecting at the margin, and there's one specific reject in the
ircut set worth your eyeballs.

1) Do the metrics adequately measure "goodness"? Yes — the evidence is
in the temporal structure.

The strongest validation isn't the numbers themselves, it's that
rejections cluster in physically coherent episodes. The duo set's
20240214 session tells the story: frame 0001 starts at sc=143, FWHM
4.2, elevated background, and the metrics improve monotonically
through the evening — background falls steadily all night, star count
climbs to 405, FWHM drops to 2.6 — with rejections ending exactly
where the frames become indistinguishable from the session's
best. Same pattern in ircut's 20240206 session: the first ten frames
(twilight — background visibly decaying frame-over-frame from 0.0491)
are rejected, then passes begin. That's the algorithm tracking real
sky conditions, not noise.

The metrics are also internally consistent: in the duo set,
corr(star_count, FWHM) = −0.92 and corr(star_count, background) =
−0.95. Multi-triggered rejects (BG+FWHM+StarCount) land on the
objectively worst frames, StarCount-only rejects populate the
transition band. One more piece of good design evidence: frame 0054 in
the duo set appears in the pre-clip outliers list but passed — because
it's an outlier on the good side (sc=451, FWHM 2.25, the best frame in
its session). Stats-clipping being symmetric while rejection is
directional is exactly right.

Two metrics did essentially nothing: SignalWeight never triggered a
single rejection in either dataset (386 frames), and eccentricity
fired zero times on its own (the 0.85 absolute cap was never reached;
the worst frame at 0.834 was caught by StarCount anyway). Neither is
broken — they're backstops for failure modes these sessions didn't
have (guiding excursions, focus-fine-but-transparency-dead frames) —
but be aware that in practice StarCount is doing almost all the work,
with FWHM and BackgroundMedian as corroborators. What no metric covers
is local/transient defects — which leads to the one frame worth
checking.

2) Too many rejects, or not enough? Slightly too many at the margin;
definitely not too few.

Not too few: nothing that passed looks damaging. Worst passing FWHM in
duo is 3.44 (vs. cut at 4.25), no passing frame has high eccentricity
or elevated background, and marginal passes are exactly what
SubframeSelector's weighting exists to handle downstream.

The over-rejection is concentrated at the StarCount 1.5σ boundary. In
the duo set, 7 rejects sit within 0.25σ of the cut (sc 323–326 vs. cut
327.3) with FWHM 2.99–3.46 — overlapping the passing
population. Frames 0039–0046 in that session literally alternate
REJECT/PASS as star count wobbles around the line; sc=326 rejected and
sc=328 passed with essentially identical FWHM is a cliff, not a
quality distinction. Ircut has 3 equivalent cases. Given your division
of labor — Photyx does binary rejection, SFS does weighting — these
~10 frames (≈30 min of integration) are ones SFS would have retained
and down-weighted; hard rejection sacrifices a little SNR for no real
resolution protection. The rest of the rejects (the early-twilight
runs, the first 33 frames of 20240214 with FWHM 3.4–5.2) are
unambiguous keeps-out.

The one frame I'd pull up and look at: ircut 20240206 frame 0012 —
rejected solely on BackgroundMedian (0.04901 vs. cut 0.04873), yet it
has sc=375 and FWHM 3.03, better than the passing average, and it sits
between two passing frames. That's a transient background bump — thin
cloud wisp, aircraft glow, something momentary — on an otherwise
top-tier frame. Whether that's a good reject depends on what the bump
physically was, which the metrics can't tell you; it's also a preview
of what the deferred trail/gradient detection would disambiguate.

Since you keep paired SFS CSVs for exactly this kind of validation:
the concrete test is to look up those ~10 borderline StarCount rejects
in the SFS output — if PixInsight weights them at, say, >70% of median
weight, that's empirical grounds to relax star_count_reject_sigma to
~1.75–2.0 in a broadband profile while keeping 1.5 for narrowband,
which maps cleanly onto your existing threshold-profile system. One
caveat on everything above: I'm judging from metric separation only —
I can't see the frames, so "would have harmed" is statistical
inference, not visual confirmation.

# SubFrameSelector Algorithm and Results

Measurement settings. The settings that actually matter for
comparability with Photyx:

Subframe scale: 206.265 × pixel size / focal length. For the ASI533MC
Pro (3.76 µm) on the AT115EDT that's ~0.96″/px at native 805 mm, or
~1.20″/px if you're running the 0.8× reducer/flattener. That said — if
your goal is comparing against Photyx's FWHM numbers (which I believe
are in pixels), it's actually simpler to leave scale at 1 and work in
pixels on both sides, so the columns line up without conversion.  PSF
fit: Moffat (SFS's default Moffat4 is fine) rather than Gaussian —
real star profiles have heavier wings, and since Photyx also fits
Moffat, this keeps the two FWHM estimates methodologically comparable
rather than systematically offset.  Structure/noise layers: defaults
are generally fine, but if hot pixels or noise are being detected as
stars (star counts wildly above Photyx's), raise noise layers to 1.
The big methodological one — measure the same data state Photyx
measured. Photyx triages raw lights; WBPP runs SFS on calibrated
frames. Calibration shifts background medians and noise absolutely,
though rank ordering mostly survives. For a clean validation, run SFS
standalone on the same raw lights, not inside WBPP. If you use
calibrated frames instead, restrict yourself to rank-based comparisons
(more on that below) and don't compare absolute values.

Weighting formula. Honest framing first: since PI 1.8.9, the best
single pure-SNR quality estimator in SFS is PSF Signal Weight — it's
what WBPP defaults to, and if all you wanted was optimal integration
weighting, PSFSignalWeight alone is a defensible modern answer. But
for your purpose — validating a five-metric rejection system — a
composite formula is more useful, because it makes the
FWHM/eccentricity/SNR trade-offs explicit and tunable, mirroring what
Photyx's thresholds are doing. The standard normalized form:

```
(35*(1 -
(FWHM - FWHMMin)/(FWHMMax - FWHMMin))
+ 15*(1 - (Eccentricity - EccentricityMin)/(EccentricityMax -
  EccentricityMin))
+ 30*(PSFSignalWeight - PSFSignalWeightMin)/(PSFSignalWeightMax -
  PSFSignalWeightMin))
+ 20
```

Each term is normalized 0–1 across the session, weighted, and the
+20 pedestal guarantees no frame zeroes out. The 35/15/30 split is a
detail-priority weighting appropriate for a galaxy target like M82
(resolution matters most, guiding is demonstrably not your problem —
eccentricity never fired in either dataset).

For a faint-nebula target
where depth beats resolution, flip toward something like
20/10/50+20.

```
(20*(1 -
(FWHM - FWHMMin)/(FWHMMax - FWHMMin))
+ 10*(1 - (Eccentricity - EccentricityMin)/(EccentricityMax -
  EccentricityMin))
+ 50*(PSFSignalWeight - PSFSignalWeightMin)/(PSFSignalWeightMax -
  PSFSignalWeightMin))
+ 20
```


One honest caveat: normalization by session min/max makes
weights relative to that session's range — a session with one terrible
frame stretches the scale for everything else — which is fine for
validation but worth remembering when comparing across sessions.
Using it to validate the Photyx profiles — this is where it gets
concrete. Export the SFS measurements to CSV alongside the weight, and
three checks fall out:

Rank correlation per metric — Spearman between Photyx's
fwhm/star_count/background_median/signal_weight/eccentricity and SFS's
FWHM/Stars/Median/PSFSignalWeight/Eccentricity per frame. High rank
correlation (>0.9) says Photyx's measurement is sound and any
disagreement is threshold placement, not measurement error. This
directly addresses question 1 from your earlier analysis with external
ground truth instead of internal consistency.  The borderline-reject
test from my last message: look up the ~10 StarCount-borderline
rejects (the sc 322–326 frames) in the SFS output. If their composite
weights land above ~70% of the session median weight, that's empirical
confirmation the 1.5σ cut is clipping frames SFS considers solidly
usable — grounds for a broadband profile at 1.75–2.0σ.
Threshold-from-weight derivation — the more ambitious version: pick a
weight percentile you consider the "harm floor" (say, frames below 40%
of median weight genuinely hurt), find where that boundary sits in
each Photyx metric's sigma space, and set the profile sigmas to
match. That turns your threshold profiles from hand-tuned values into
ones anchored to SFS's assessment, which is exactly the empirical
validation loop your paired-JSON/CSV reference folder was set up for.

One practical note: leave SFS's approval expression empty for these
runs. Photyx already did the binary rejection; you want SFS weighting
everything Photyx passed — and, for the validation runs specifically,
also weighting the frames Photyx rejected, since the whole point is
seeing what weights the rejects would have received.

# SFS Duo-band Filter Analysis and Conclusions

This is a genuinely useful dataset — 160 duo frames, all of them
(including rejects) measured by SFS on the raw lights, under two
weighting schemes. Ran the full cross-validation. Findings, then
profile recommendations.

**First, an important result about your two schemes: they rank the
frames almost identically.** Spearman between the two weight columns
is **0.980**. Your favorite formula doesn't disagree with the
composite about which frames are better — it applies a much steeper
contrast curve to the same ordering (max/min ratio 39.3 vs. 2.64). The
composite's narrow spread is baked into its design: each term is
normalized to [0,1] and the +20 pedestal floors everything, so it's
bounded ~20–100 by construction. Your favorite is a product of power
laws (`FWHM^2.6`, `SNRWeight^1.8`), so ratios compound. For
*integration*, that steepness matters — under your scheme a mediocre
frame at weight 1.4 contributes ~26% as much as a median frame, versus
~78% under the composite — so your formula effectively does
soft-rejection on top of Photyx's hard rejection. For *validation*,
the agreement between them is good news: every conclusion below holds
under both schemes, so nothing hinges on the weighting choice.

**Photyx's measurements are externally validated — with two
exceptions.** Rank correlations, Photyx metric vs. SFS equivalent
across all 160 frames:

| Photyx | SFS | Spearman |
|---|---|---|
| fwhm | FWHM | **+0.987** |
| star_count | Stars | **+0.984** |
| background_median | Median | **+0.967** |
| eccentricity | Eccentricity | +0.676 |
| signal_weight | PSF Signal Weight | **+0.365** |

FWHM, star count, and background are essentially measuring the same
thing SFS measures — your three workhorse metrics are
solid. Eccentricity's 0.68 is mediocre; the two tools clearly compute
it differently (Photyx's second-moment approach vs. SFS's
PSF-fit). Since it's a never-firing backstop that's tolerable, but I
wouldn't tighten the 0.85 threshold based on Photyx's eccentricity
numbers alone. **The real flag is `signal_weight`: 0.365 against PSF
Signal Weight, and I checked it against every other SFS signal-ish
column (SNR +0.17, M* −0.10, PSF Total Flux −0.45, PSF Total Mean Flux
+0.47) — it doesn't strongly track *any* of them.** Combined with the
earlier finding that SignalWeight triggered zero rejections across all
386 frames in both filters, you have a metric that is both
non-discriminating and non-validated. Worth a look at what it actually
computes at some point; until then it's harmless but dead weight in
the profile.

**The rejection boundary itself is well-placed.** Under your favorite
scheme, PASS frames have median weight 5.32, REJECT frames 1.44 —
clean separation, and only 2 of 51 rejects score above the PASS 25th
percentile. In the other direction, only 2 PASS frames land in the
bottom weight quintile (0029 and 0034 — softish FWHM, ecc ~0.7), and
those are precisely the frames SFS down-weighting exists to
handle. So: no under-rejection problem at all, and the over-rejection
is confined to exactly the borderline band I flagged from the JSON
alone. Now with SFS weights attached to those borderline StarCount
rejects:

| Frame | sc | fwhm | Favorite weight (% of session median) |
|---|---|---|---|
| 0031 | 323 | 3.05 | **132%** |
| 0041 | 323 | 2.99 | **106%** |
| 0045 | 323 | 3.10 | 94% |
| 0046 | 323 | 3.18 | 88% |
| 0030 | 323 | 3.10 | 72% |
| 0039 | 326 | 3.17 | 64% |
| 0011 (2/07) | 324 | 3.46 | 30% |

Two rejected frames are *above-median* quality; five of seven clear
the 70%-of-median bar. That's the empirical confirmation of the
over-rejection hypothesis. Note the instructive failure mode in that
table: frames 0031 and 0011 have nearly identical star counts (323 vs
324) but weights of 132% vs 30% — at the boundary, star count stops
carrying information and FWHM is what separates good from bad. A hard
star-count cut fundamentally can't make that distinction; the
weighting engine can. Which argues for letting the boundary breathe
and trusting SFS with the marginal cases.

**Profile recommendations** (for the duo/narrowband profile — this
dataset is all duo; the ircut set deserves its own pass with the same
method):

- **star_count_reject_sigma: 1.5 → 1.75.** The borderline frames sit
  at 1.52–1.58σ below the mean; a 1.75σ cut flips exactly this band to
  PASS (about 7–8 frames, ~20 min of integration recovered) while
  still rejecting the sc<314 frames that are genuinely
  conditions-damaged. The one mediocre frame that slips through (0011)
  gets crushed to 30% weight by your formula anyway — that's the
  division of labor working as designed.
- **fwhm 2.5σ, bg_median 2.5σ: keep.** Neither mis-fired once; the
  bg-triggered rejects were all twilight ramps and one transient, and
  FWHM triggers landed only on genuinely soft frames.
- **eccentricity 0.85 abs: keep as backstop**, but don't invest tuning
  effort in it given the measurement divergence from SFS.
- **signal_weight 2.5σ: leave loose / deprioritize** pending a look at
  its implementation.

And on the SFS side: your instinct about the composite scheme is right
for your workflow. Since Photyx already amputates the bad tail, what
remains benefits from a weighting function with real dynamic range
across the surviving quality spread — which your power-law formula
provides and the pedestal-compressed composite doesn't. The one thing
I'd *not* conclude is that the composite is "wrong" — it ranks
identically; it just weights timidly.

# SFS IRCUT Filter Analysis and Conclusions

Ran the identical cross-validation on the ircut set (226 frames,
perfect overlap with the Photyx JSON). The picture differs from the
duo set in ways that matter for the profiles — including one place
where the data reverses a guess I made earlier, and one where it
overturns my own prior finding.

**Measurement validation — and an eccentricity surprise.** Rank
correlations vs. SFS: fwhm **+0.983**, star_count **+0.960**,
background_median **+0.968** — same excellent agreement as duo. But
eccentricity comes in at **+0.955** here, versus 0.676 on the duo
set. So my earlier read ("the two tools clearly compute it
differently") was wrong as a blanket statement — on broadband data
they agree nearly perfectly. The duo divergence is evidently
data-driven, not methodological: narrowband stars are dimmer and
fewer, so shape estimates get noisier on both sides. Photyx's
eccentricity is fine; just trust it less on narrowband. Meanwhile
`signal_weight` scores **+0.321** — nearly identical to duo's
0.365. Two filters, same result: that metric isn't tracking anything
SFS recognizes as signal. The case for investigating its
implementation is now solid.

**Scheme agreement drops here** — Spearman 0.817 between your two
weight columns (vs. 0.980 on duo), with the favorite spreading
45:1. The divergence makes sense: your formula is FWHM-dominated
(power 2.6) and SNR-sensitive (power 1.8), while the composite caps
FWHM's influence at 35%. The ircut population has a wide FWHM range
with decent star counts throughout, which is exactly where the two
philosophies separate. The conclusions below hold under both, but
where they differ I'll say so.

**The rejection boundary: essentially perfect on this filter.** Under
the composite, *zero* rejects score above the PASS 25th percentile;
under your favorite, exactly one (frame 0080, at 77% of median). The
borderline StarCount rejects tell the real story — compare with duo:

| Frame | sc | fwhm | Favorite weight (% of median) |
|---|---|---|---|
| 0080 (3/12) | 322 | 4.25 | 77% |
| 0114 (3/07) | 324 | 4.39 | 71% |
| 0113 (3/07) | 330 | 4.43 | 44% |
| 0006 (3/06) | 328 | 3.36 | 26% |
| 0005 (3/06) | 320 | 3.26 | 21% |
| 0004 (3/06) | 315 | 3.37 | 20% |

In the duo set, borderline star-count rejects were frames with *good*
FWHM that SFS rated at up to 132% of median — clear
over-rejection. Here, the borderline frames are mostly soft (fwhm
4.25–4.43) or twilight-contaminated, and your own weighting scheme
rates most of them 20–44%. At most two frames are arguable, and both
sit right at the 70% line. **So the 1.5σ star-count cut is correctly
placed for ircut** — which means my earlier speculation had the
direction backwards: the data says relax star_count for the
*duo/narrowband* profile (→1.75σ) and keep 1.5σ for
*broadband*. Physically that tracks: on broadband, a depressed star
count genuinely signals a degraded frame; on narrowband, star count
wobbles near the threshold even on frames that are otherwise
excellent.

**Frame 0012 — I retract my earlier flag.** I'd called it "an
otherwise top-tier frame lost to a transient background bump" and
suggested you eyeball it. Your favorite scheme rates it **39% of
median** — the SNRWeight^1.8 term punishes exactly the elevated
background that Photyx's BackgroundMedian trigger caught. The
composite (83%) is more forgiving, but the scheme you actually trust
corroborates Photyx's rejection. That transient bump was real signal
degradation, not cosmetic.

**The one genuine observation on the other side:** 26 PASS frames land
in your favorite scheme's bottom weight quintile (as low as 28% of
median — soft frames at fwhm 4.2–4.4, under the 4.61 cut). That's not
under-rejection per your architecture — these are usable frames your
weighting formula duly crushes — but it's worth knowing the lever
exists: if you ever want Photyx to prune broadband more aggressively,
`fwhm_reject_sigma` 2.5→2.0 (cut ~4.37) is the change, not
star_count. I wouldn't make it; the current division of labor is doing
its job.

# Bottom line profile settings, both filters, now fully validated:

| Setting | Duo/narrowband profile | Ircut/broadband profile |
|---|---|---|
| star_count_reject_sigma | **1.75** (change from 1.5) | **1.5** (keep) |
| fwhm_reject_sigma | 2.5 (keep) | 2.5 (keep; 2.0 optional if you want harder pruning) |
| bg_median_reject_sigma | 2.5 (keep — twilight + transients caught correctly) | 2.5 (keep) |
| eccentricity_reject_abs | 0.85 (keep; metric is noisier on this filter) | 0.85 (keep; metric validated) |
| signal_weight_reject_sigma | leave loose — metric needs investigation | same |

This is exactly the per-filter split your threshold-profile system was
built for — the empirical loop (Photyx JSON + SFS CSV) has now closed
on both filters.

# Conclusions on SFS Weighting

Good framing — and "losing valuable photons to underweighting" is
exactly the right question to make quantitative. The standard tool for
it is the **effective frame count**: N_eff = (Σw)²/Σw². If a weighted
stack has N_eff = 82% of N, your depth is roughly what you'd get from
equal-weighting 82% of your frames — that's the photon cost of weight
dispersion, bounded from above. I computed it on the PASS populations
only (post-Photyx, since that's what you actually integrate), for both
of your schemes plus PixInsight's own SNR-oriented estimators as a
reference:

| | Duo (n=108) | Ircut (n=200) |
|---|---|---|
| Composite 35-15-30-20 | N_eff 106.9 (**99.0%**) | 197.3 (**98.6%**) |
| Your favorite | N_eff 90.8 (**84.1%**) | 164.1 (**82.1%**) |
| Pure PSFSignalWeight | 87.4 (81.0%) | 187.5 (93.8%) |
| Pure PSFSNR | 99.7 (92.3%) | 184.0 (92.0%) |

**The composite, post-Photyx, is nearly a no-op.** At 99% N_eff it's
within half a percent of straight equal-weight stacking. Your original
complaint — not enough spread — is now quantified: once Photyx has
removed the bad tail, the composite's pedestal-and-normalize structure
flattens what remains into near-uniformity. That's only a flaw if you
want the weighting stage to do real work; if you fully trusted binary
rejection, near-equal weighting is actually the depth-maximizing
choice. But it does mean the composite adds essentially nothing on top
of your Photyx pipeline.

**Your favorite costs at most ~8–9% SNR-equivalent depth — and that's
an upper bound, not the real cost.** The N_eff penalty assumes the
down-weighted frames were secretly as good as the rest. They aren't —
they're your soft 4.2–4.4″ FWHM frames — so the true depth cost is
meaningfully smaller than 8%, and what you're buying with it is
resolution protection in the stack. That's not "losing photons,"
that's spending them on sharpness, deliberately.

The filter split matters again, though. On **duo**, your favorite's
dispersion (84%) is nearly identical to pure PSFSignalWeight (81%) and
correlates with it at 0.83 — meaning on narrowband, your formula
behaves almost exactly like theoretically SNR-optimal
weighting. Nothing to fix; it's doing the job with essentially zero
waste. On **ircut**, your favorite diverges: N_eff 82% vs. 94% for
PSFSignalWeight, and only 0.52 rank correlation with the SNR-proxy. On
broadband, the FWHM^2.6 term dominates and punishes softness well
beyond what SNR-weighting would — that's where your "lost photons"
intuition has actual teeth, on the order of a few percent depth traded
for sharpness.

So my honest answer: **keep your favorite as the default — your year
of instinct is validated by the numbers — but it's a detail-priority
prior, not a universal optimum.** For M82-class targets where
resolution is the point, it's the right tool and the photon cost is
modest and purposeful. Two refinements worth considering: for
depth-priority targets (faint nebulosity, IFN, anything where you'd
never crop to core detail), switch that run to plain `PSFSNR` — it
recovers most of the depth while still ordering frames sensibly.

And if you want one formula that softens the broadband over-punishment
without changing character, drop the FWHM exponent from 2.6 to ~2.0
for broadband sessions — on duo it won't matter (SNR term dominates
there anyway), on ircut it claws back a chunk of that N_eff gap. The
composite I'd retire: post-Photyx it's neither weighting meaningfully
nor protecting anything your rejection thresholds don't already
handle.

# In Summary

**Duo star-count to 1.75σ — yes, confirmed.** That's the change with
direct empirical backing (the sc 323–326 rejects that your own
weighting scheme rated at 88–132% of median). Ircut stays at 1.5σ.

**The weighting swap is inverted — or at least, the swap as stated
doesn't buy what you think.** On *duo*, your favorite formula and
PSFSignalWeight are nearly the same animal: 0.83 rank correlation,
N_eff 84% vs 81%. Swapping one for the other there is a lateral move —
PSFSignalWeight is actually slightly *more* dispersed on duo, not
gentler. Broadband is where the two philosophies genuinely diverge
(0.52 correlation, 82% vs 94% N_eff) and where your favorite spends
the photons.

Now, there's a defensible rationale hiding in your version: the duo
data carries the faint Hα signal where depth matters most, so wanting
SNR-style weighting on duo and detail-priority weighting on broadband
is a coherent goal. But if that's the intent, the right duo choice is
**PSFSNR, not PSFSignalWeight** — PSFSNR came in at 92% N_eff on duo
(vs 81% for PSFSignalWeight, 84% for your favorite), so it's the one
that actually preserves depth there. So the corrected version of your
plan: favorite formula on broadband (accepting its deliberate
sharpness-for-depth trade), **PSFSNR** on duo if depth is the priority
— or honestly, keep the favorite on both, since on duo it's already
within a few percent of SNR-optimal and one formula is simpler to
maintain.

**Dropping Photyx's signal_weight — the evidence supports it, but a
scope flag first.** Zero rejections in 386 frames, no correlation with
any SFS signal metric on either filter — as a *decision input* it's
contributing nothing. But actually removing it is not a small edit:
it's wired through the analysis pipeline, `AnalysisThresholds`, the
threshold-profile DB schema, the JSON export, and the results
UI. That's real multi-layer surgery in an app you've declared
feature-frozen. Three options in ascending effort: (1) leave it
computed but set the threshold loose enough to never matter — zero
risk, but violates your no-dead-code instinct; (2) remove it
end-to-end — cleanest, but it's a proper change session with a DB
migration question for existing profiles; (3) investigate what it's
actually computing first — there's a chance it's a fixable bug rather
than a useless concept, and a *working* signal metric would be worth
having. If you do want it gone, I'd treat that as its own session with
the usual discussion-first process.

**On a fifth metric: the four you'd keep are close to the ceiling for
what global statistics can measure — the real headroom is spatial, not
another scalar.** Your four survivors each own an axis: FWHM =
seeing/focus, star count = transparency, background median = sky glow,
eccentricity = guiding. The SFS cross-validation showed those
measurements rank-match professional tooling at 0.96–0.99, and the
weight analysis showed the resulting decisions are sound. Another
global number would mostly re-measure one of those axes (frame noise
correlates with background; any honest SNR metric correlates with star
count × background).

What none of the four can see is *localized* defects: background
**gradients** (moon direction, light-pollution wedge, thin cloud on
one side of the frame — a frame can have a perfect global median and a
nasty tilt), reflections, dew halos, and trails. If you ever add a
fifth metric, the one with genuine headroom is **background
uniformity** — e.g., quadrant-median spread or a fitted-plane slope
over the existing pixel buffers. It's cheap, it reuses data you
already load, and it's the only category where these two datasets
showed even a hint of blindness (the frame-0012-style transient, which
global median caught only because it was strong enough to move the
whole frame). Trail detection is the other spatial gap, but you've
already correctly deferred that — and pixel-level sigma clipping
during integration mops up most trail damage anyway.

Both of those are new-feature territory, though — so under
release-mode rules, the practical answer to your question is: yes, the
four are as good as it gets for now, and that's a validated,
defensible place to stop.
