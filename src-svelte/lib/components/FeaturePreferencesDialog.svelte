<!-- FeaturePreferencesDialog.svelte — Edit > Feature Preferences modal overlay. -->
<!-- Draft-copy pattern: nothing written until OK or Apply. One row per -->
<!-- FEATURE_FLAGS registry entry (Issue 130) — a toggle list, no sections, -->
<!-- no add/delete/switch-profile machinery. Reuses the pref-* CSS classes -->
<!-- and the Dropdown component already established by PreferencesDialog -->
<!-- and ThresholdProfilesDialog. -->

<script lang="ts">
  import { featureFlags } from '../stores/featureFlags';
  import { notifications } from '../stores/notifications';
  import Dropdown from './Dropdown.svelte';
  import { FEATURE_FLAGS } from '../settings/constants';

  let { onclose }: { onclose: () => void } = $props();

  // Draft copy — edited freely; nothing is written until OK or Apply.
  let draft = $state<Record<string, boolean>>({});
  let dirty = $state(false);

  // Sync from the store whenever it changes (Pattern 9), but only while
  // not dirty — a hydrate() firing mid-edit should not clobber in-progress
  // changes, same concern PreferencesDialog's cpuCount effect guards
  // against (Issue 121).
  $effect(() => {
    const s = $featureFlags;
    if (dirty) return;
    draft = { ...s.flags };
  });

  function onFieldChange(key: string, value: string) {
    draft = { ...draft, [key]: value === 'yes' };
    dirty = true;
  }

  function buildChanged(): Record<string, boolean> {
    const s = $featureFlags;
    const changed: Record<string, boolean> = {};
    for (const f of FEATURE_FLAGS) {
      if (draft[f.key] !== s.flags[f.key]) {
        changed[f.key] = draft[f.key];
      }
    }
    return changed;
  }

  // featureFlags.setFlag() is fire-and-forget (optimistic local update,
  // persists in the background, logs its own errors) — same pattern as
  // ui.ts's setTheme/toggleQuickLaunch — so this doesn't need to await
  // or catch per-key.
  function apply() {
    const changed = buildChanged();
    if (Object.keys(changed).length === 0) { dirty = false; return; }
    for (const [key, enabled] of Object.entries(changed)) {
      featureFlags.setFlag(key, enabled);
    }
    dirty = false;
    notifications.success('Feature preferences saved.');
  }

  function ok() {
    apply();
    onclose();
  }

  function cancel() {
    onclose();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') cancel();
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="pref-backdrop">
  <div class="pref-dialog" onclick={(e) => e.stopPropagation()}>

    <div class="pref-header">
      <span class="pref-title">Feature Preferences</span>
      <button class="pref-close-btn" onclick={cancel}>✕</button>
    </div>

    <div class="pref-body">
      <div class="pref-section">
        {#each FEATURE_FLAGS as flag}
          <div class="pref-row">
            <label class="pref-label">{flag.label}</label>
            <div class="pref-control">
              <Dropdown
                className="ff-flag-dropdown"
                value={draft[flag.key] ? 'yes' : 'no'}
                options={[{ value: 'yes', label: 'Yes' }, { value: 'no', label: 'No' }]}
                openUp={false}
                width={null}
                on:change={(e) => onFieldChange(flag.key, e.detail)}
              />
              {#if flag.helper}
                <div class="pref-helper">{flag.helper}</div>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>

    <div class="pref-footer">
      <button class="pref-btn pref-btn-secondary" onclick={cancel}>Cancel</button>
      <button class="pref-btn pref-btn-secondary" onclick={apply} disabled={!dirty}>Apply</button>
      <button class="pref-btn pref-btn-primary" onclick={ok}>OK</button>
    </div>

  </div>
</div>
