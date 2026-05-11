<!-- ThresholdProfilesDialog.svelte — Edit > Analysis Parameters modal. -->
<!-- Manages threshold profiles for AnalyzeFrames. -->

<script lang="ts">
  import { thresholdProfiles, type ThresholdProfile } from '../stores/thresholdProfiles';
  import { notifications } from '../stores/notifications';
  import Dropdown from './Dropdown.svelte';
  import {
    THRESHOLD_FIELDS,
    ECCENTRICITY_ABS_MIN, ECCENTRICITY_ABS_MAX,
  } from '../settings/constants';

  let { onclose }: { onclose: () => void } = $props();

  // ── Local state ──────────────────────────────────────────────────────────

  let selectedId        = $state<number | null>(null);
  let selectedIdStr     = $state<string>('');
  let draft             = $state<Record<string, number | string>>({});
  let dirty             = $state(false);
  let confirmingSwitch  = $state(false);
  let confirmingDelete  = $state(false);
  let pendingSwitchId   = $state<number | null>(null);
  let addingNew         = $state(false);
  let newName           = $state('');
  let errors            = $state<Record<string, string>>({});
  let committedActiveId = $state<number | null>(null);

  // ── Initialise on mount ──────────────────────────────────────────────────

  $effect(() => {
    const s = $thresholdProfiles;
    if (s.profiles.length === 0) return;
    committedActiveId = s.activeProfileId;
    const activeId = s.activeProfileId ?? s.profiles[0].id;
    selectedId = activeId;
    selectedIdStr = String(activeId);
    loadDraft(activeId);
  });

  // Keep selectedIdStr in sync with selectedId
  $effect(() => {
    if (selectedId !== null) selectedIdStr = String(selectedId);
  });

  // React to Dropdown value changes
  let lastHandledIdStr = '';
  $effect(() => {
    const str = selectedIdStr;
    if (str === lastHandledIdStr) return;
    lastHandledIdStr = str;
    const id = parseInt(str, 10);
    if (isNaN(id) || id === selectedId) return;
    if (dirty) {
      pendingSwitchId = id;
      confirmingSwitch = true;
      // Reset dropdown back to current selection
      selectedIdStr = String(selectedId);
      lastHandledIdStr = String(selectedId);
      return;
    }
    selectedId = id;
    loadDraft(id);
  });

  function loadDraft(id: number) {
    const profile = $thresholdProfiles.profiles.find(p => p.id === id);
    if (!profile) return;
    draft = {
      bg_median_reject_sigma:     profile.bg_median_reject_sigma,
      signal_weight_reject_sigma: profile.signal_weight_reject_sigma,
      fwhm_reject_sigma:          profile.fwhm_reject_sigma,
      star_count_reject_sigma:    profile.star_count_reject_sigma,
      eccentricity_reject_abs:    profile.eccentricity_reject_abs,
    };
    dirty = false;
    errors = {};
    confirmingSwitch = false;
    confirmingDelete = false;
  }

  // ── Profile switch confirmation ───────────────────────────────────────────

  function confirmSwitch() {
    if (pendingSwitchId === null) return;
    selectedId = pendingSwitchId;
    selectedIdStr = String(pendingSwitchId);
    lastHandledIdStr = String(pendingSwitchId);
    loadDraft(pendingSwitchId);
    pendingSwitchId = null;
    confirmingSwitch = false;
    addingNew = false;
  }

  function cancelSwitch() {
    pendingSwitchId = null;
    confirmingSwitch = false;
  }

  // ── Add new profile ──────────────────────────────────────────────────────

  function startAdd() {
    if (dirty) {
      pendingSwitchId = -1;
      confirmingSwitch = true;
      return;
    }
    addingNew = true;
    newName = '';
  }

  function cancelAdd() {
    addingNew = false;
    newName = '';
  }

  async function confirmAdd() {
    const name = newName.trim();
    if (!name) return;
    try {
      const base: ThresholdProfile = {
        id:                         0,
        name,
        description:                null,
        bg_median_reject_sigma:     2.5,
        signal_weight_reject_sigma: -2.5,
        fwhm_reject_sigma:          2.5,
        star_count_reject_sigma:    -3.0,
        eccentricity_reject_abs:    0.85,
      };
      const saved = await thresholdProfiles.saveProfile(base);
      selectedId = saved.id;
      selectedIdStr = String(saved.id);
      lastHandledIdStr = String(saved.id);
      loadDraft(saved.id);
      addingNew = false;
      newName = '';
    } catch (e) {
      notifications.error(`Failed to create profile: ${e}`);
    }
  }

  // ── Delete profile ───────────────────────────────────────────────────────

  function startDelete() {
    confirmingDelete = true;
    confirmingSwitch = false;
  }

  async function confirmDelete() {
    if (selectedId === null) return;
    try {
      await thresholdProfiles.deleteProfile(selectedId);
      const s = $thresholdProfiles;
      const newSelected = s.activeProfileId ?? s.profiles[0]?.id ?? null;
      selectedId = newSelected;
      selectedIdStr = String(newSelected ?? '');
      lastHandledIdStr = String(newSelected ?? '');
      committedActiveId = s.activeProfileId;
      if (newSelected !== null) loadDraft(newSelected);
      confirmingDelete = false;
    } catch (e) {
      notifications.error(`Failed to delete profile: ${e}`);
    }
  }

  function cancelDelete() {
    confirmingDelete = false;
  }

  // ── Draft editing ────────────────────────────────────────────────────────

  function onFieldInput(key: string, value: string, min: number, max: number) {
    const v = parseFloat(value);
    if (isNaN(v)) {
      errors = { ...errors, [key]: 'Must be a number.' };
    } else if (v < min) {
      errors = { ...errors, [key]: `Minimum is ${min}.` };
    } else if (v > max) {
      errors = { ...errors, [key]: `Maximum is ${max}.` };
    } else {
      const { [key]: _, ...rest } = errors;
      errors = rest;
    }
    draft = { ...draft, [key]: value };
    dirty = true;
  }

  function hasErrors(): boolean {
    return Object.keys(errors).length > 0;
  }

  // ── Apply / OK ───────────────────────────────────────────────────────────

  async function apply() {
    if (!dirty || selectedId === null || hasErrors()) return;
    console.log('apply draft:', JSON.stringify(draft));
    try {
      const profile = $thresholdProfiles.profiles.find(p => p.id === selectedId);
      if (!profile) return;
      const saved: ThresholdProfile = {
        ...profile,
        bg_median_reject_sigma:     Number(draft.bg_median_reject_sigma),
        signal_weight_reject_sigma: Number(draft.signal_weight_reject_sigma),
        fwhm_reject_sigma:          Number(draft.fwhm_reject_sigma),
        star_count_reject_sigma:    Number(draft.star_count_reject_sigma),
        eccentricity_reject_abs:    Number(draft.eccentricity_reject_abs),
      };
      await thresholdProfiles.saveProfile(saved);
      await thresholdProfiles.setActiveProfile(selectedId);
      committedActiveId = selectedId;
      dirty = false;
      notifications.success('Analysis parameters saved.');
    } catch (e) {
      notifications.error(`Failed to save profile: ${e}`);
    }
  }

  async function ok() {
    if (hasErrors()) return;
    if (dirty) await apply();
    else if (selectedId !== null && selectedId !== committedActiveId) {
      await thresholdProfiles.setActiveProfile(selectedId);
      committedActiveId = selectedId;
    }
    onclose();
  }

  function cancel() {
    onclose();
  }

  // ── Keyboard ─────────────────────────────────────────────────────────────

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') cancel();
    if (e.key === 'Enter' && addingNew) confirmAdd();
  }

  // ── Helpers ──────────────────────────────────────────────────────────────

  function activeProfileName(): string {
    const s = $thresholdProfiles;
    const p = s.profiles.find(x => x.id === committedActiveId);
    return p?.name ?? '—';
  }

  function fieldMin(field: typeof THRESHOLD_FIELDS[number]): number {
    if (field.type === 'sigma' && field.direction === '-') return -field.max;
    return field.min;
  }

  function fieldMax(field: typeof THRESHOLD_FIELDS[number]): number {
    if (field.type === 'sigma' && field.direction === '-') return -field.min;
    return field.max;
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="pref-backdrop">
  <div class="pref-dialog tp-dialog" onclick={(e) => e.stopPropagation()}>

    <!-- Header -->
    <div class="pref-header">
      <span class="pref-title">Analysis Parameters</span>
      <button class="pref-close-btn" onclick={cancel}>✕</button>
    </div>

    <div class="pref-body">

      <!-- Profile selector row -->
      <div class="tp-selector-row">
        <button
          class="tp-icon-btn danger"
          title="Delete selected profile"
          onclick={startDelete}
          disabled={$thresholdProfiles.profiles.length === 0}
        >🗑</button>

        <Dropdown
          className="tp-profile-dropdown"
          value={selectedIdStr}
          options={$thresholdProfiles.profiles.map(p => ({ value: String(p.id), label: p.name }))}
          openUp={false}
          width={null}
          on:change={(e) => { selectedIdStr = e.detail; }}
        />

        <button
          class="tp-icon-btn"
          title="Add new profile"
          onclick={startAdd}
        >＋</button>
      </div>

      <!-- Active profile indicator -->
      <div class="tp-active-label">Active profile: {activeProfileName()}</div>

      <!-- New profile name input row -->
      {#if addingNew}
        <div class="tp-new-row">
          <input
            class="pref-input pref-input-path"
            type="text"
            placeholder="Profile name…"
            maxlength="64"
            bind:value={newName}
            autofocus
          />
          <button
            class="pref-btn pref-btn-primary"
            onclick={confirmAdd}
            disabled={!newName.trim()}
          >OK</button>
          <button class="pref-btn pref-btn-secondary" onclick={cancelAdd}>Cancel</button>
        </div>
      {/if}

      <!-- Unsaved changes confirmation bar -->
      {#if confirmingSwitch}
        <div class="tp-confirm-bar" onclick={(e) => e.stopPropagation()}>
          <span>You have unsaved changes. Discard them?</span>
          <button class="tp-confirm-btn danger" onclick={confirmSwitch}>Discard</button>
          <button class="tp-confirm-btn" onclick={cancelSwitch}>Keep editing</button>
        </div>
      {/if}

      <!-- Delete confirmation bar -->
      {#if confirmingDelete}
        <div class="tp-confirm-bar" onclick={(e) => e.stopPropagation()}>
          <span>Delete "{$thresholdProfiles.profiles.find(p => p.id === selectedId)?.name}"?</span>
          <button class="tp-confirm-btn danger" onclick={confirmDelete}>Delete</button>
          <button class="tp-confirm-btn" onclick={cancelDelete}>Cancel</button>
        </div>
      {/if}

      <!-- Threshold fields -->
      <div class="pref-section">
        <div class="pref-section-title">Rejection Thresholds</div>

        {#each THRESHOLD_FIELDS as field}
          <div class="pref-row" class:has-error={!!errors[field.key]}>
            <label class="pref-label" for={`tp-${field.key}`}>{field.label}</label>
            <div class="pref-control">
              <div class="pref-numeric-row">
                <span class="tp-direction">{field.direction === '+' ? '>' : '<'}</span>
                <input
                  id={`tp-${field.key}`}
                  class="pref-input pref-input-numeric"
                  type="number"
                  step={field.step}
                  value={draft[field.key] ?? field.default}
                  oninput={(e) => onFieldInput(
                    field.key,
                    (e.target as HTMLInputElement).value,
                    field.min,
                    field.max
                  )}
                />
                {#if field.type === 'sigma'}
                  <span class="pref-unit">σ</span>
                {/if}
              </div>
              {#if errors[field.key]}
                <div class="pref-error">{errors[field.key]}</div>
              {/if}
            </div>
          </div>
        {/each}
      </div>

    </div>

    <!-- Footer -->
    <div class="pref-footer">
      <button class="pref-btn pref-btn-secondary" onclick={cancel}>Cancel</button>
      <button class="pref-btn pref-btn-secondary" onclick={apply} disabled={!dirty || hasErrors()}>Apply</button>
      <button class="pref-btn pref-btn-primary" onclick={ok} disabled={hasErrors()}>OK</button>
    </div>

  </div>
</div>
