<!-- AnalyzeFramesProfileDialog.svelte — Required threshold-profile picker
     shown when Analyze > Analyze Frames is triggered from the menu.
     Issue 101: the menu trigger was silently using whatever profile
     happened to be active, with no on-screen indication. Selecting a
     profile here runs AnalyzeFrames with that profile for this run only
     (via runAnalyzeFramesWithProfile) — it does NOT change the saved
     active profile (thresholdProfiles.setActiveProfile is deliberately
     not called here). Quick Launch, saved macros, RunMacro, and the
     console are unaffected by this dialog entirely — they continue to
     dispatch AnalyzeFrames directly, unattended, as before. -->

<script lang="ts">
  import { thresholdProfiles } from '../stores/thresholdProfiles';
  import { runAnalyzeFramesWithProfile } from '../commands';

  let { onclose }: { onclose: () => void } = $props();

  // Pre-selected to the currently active profile so repeat use isn't
  // extra friction, but still requires a confirming click every time.
  let selectedId = $state<number | null>($thresholdProfiles.activeProfileId);

  async function confirm() {
    const profile = $thresholdProfiles.profiles.find(p => p.id === selectedId);
    if (!profile) return;
    // Issue 177 diagnostic: previously called onclose() synchronously right
    // after firing runAnalyzeFramesWithProfile (fire-and-forget), tearing
    // down this full-viewport overlay at the exact moment AnalyzeFrames'
    // compute burst begins — every other dialog in the app closes only
    // after its async work resolves (see commitAnalysis in commands.ts).
    // Now waits for the run to actually finish before dismissing, to test
    // whether that mount/unmount timing was the deterministic trigger.
    await runAnalyzeFramesWithProfile(profile.name);
    onclose();
  }

  function cancel() {
    onclose();
  }
</script>

<div class="afp-overlay">
  <div class="afp-dialog">
    <h2 class="afp-title">Select Threshold Profile</h2>
    <p class="afp-subtitle">Choose which profile to use for this AnalyzeFrames run.</p>

    <ul class="afp-list">
      {#each $thresholdProfiles.profiles as p (p.id)}
        <li class="afp-item">
          <label class="afp-label">
            <input
              type="radio"
              name="afp-profile"
              value={p.id}
              bind:group={selectedId}
            />
            <span class="afp-name">{p.name}</span>
          </label>
        </li>
      {/each}
    </ul>

    <div class="afp-actions">
      <button class="afp-btn afp-btn-secondary" onclick={cancel}>Cancel</button>
      <button
        class="afp-btn afp-btn-primary"
        onclick={confirm}
        disabled={selectedId === null}
      >
        Run Analysis
      </button>
    </div>
  </div>
</div>

<!-- ----------------------------------------------------------------------
     Styling note (flag for review): this follows the general
     overlay + centered-dialog pattern used by the app's other modals
     (draft-copy, does not close on outside click, per the existing
     Analysis Parameters / Preferences dialog convention). I have not
     seen the exact CSS of an existing dialog to match class-for-class —
     please verify static/css/analyzeFramesProfileDialog.css (below)
     against your established modal styling and adjust if it doesn't
     match visually. Uses only theme variables already established
     elsewhere in this engagement (--bg-color, --text-color,
     --primary-color, --border-color, --card-bg, --card-hover) — swap
     any that don't exist in your actual theme files.
     ---------------------------------------------------------------------- -->
