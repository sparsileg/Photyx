<!-- PreferencesDialog.svelte — Edit > Preferences modal overlay. -->
<!-- Spec §8.13. Reads from settings store; writes only on OK or Apply. -->

<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import { invoke } from '@tauri-apps/api/core';
  import { settings } from '../stores/settings';
  import { notifications } from '../stores/notifications';
  import {
    PREF_FIELDS,
    PREF_SECTIONS,
    type PrefFieldMeta,
  } from '../settings/constants';

  let { onclose }: { onclose: () => void } = $props();

  // 0 = not yet loaded. Was previously a hardcoded 32 placeholder that
  // could reach a saved value if the draft init ran (and Apply was
  // clicked) before get_cpu_count resolved. Issue 121.
  let cpuCount = $state(0);

  // Fetched once on mount, deliberately independent of the draft-init
  // effect below — it must never re-run draft init or clobber an
  // in-progress edit when it resolves. Issue 121. Still needed post-171:
  // the Auto button computes cpuCount - 1 from this.
  $effect(() => {
    invoke<number>('get_cpu_count').then(n => { cpuCount = n; });
  });

  // Draft copy — edited freely; nothing is written until OK or Apply.
  // buffer_pool_memory_limit is converted to GB for display.
  let draft = $state<Record<string, number | string>>({});

  // Validation errors keyed by preference key
  let errors = $state<Record<string, string>>({});

  // Initialise draft from current store values on mount
  $effect(() => {
    const s = $settings;
    draft = {
      jpeg_quality:             s.jpeg_quality,
      backup_directory:         s.backup_directory,
      console_history_size:     s.console_history_size,
      macro_editor_font_size:   s.macro_editor_font_size,
      autostretch_shadow_clip:  s.autostretch_shadow_clip,
      autostretch_target_bg:    s.autostretch_target_bg,
      // rayon_thread_count is a plain explicit value now (Issue 171) — no
      // -1 sentinel, no auto mode. The Auto button just writes a computed
      // number into this same field, same as typing one in by hand.
      rayon_thread_count:       s.rayon_thread_count,
    };
  });

  function fieldMeta(key: string): PrefFieldMeta | undefined {
    return PREF_FIELDS.find(f => f.key === key);
  }

  function effectiveMax(field: PrefFieldMeta): number | undefined {
    if (field.key === 'rayon_thread_count') {
      // 0 means cpuCount hasn't loaded yet — treat max as unknown rather
      // than capping manual entry at a bogus 0. Issue 121.
      return cpuCount > 0 ? cpuCount : undefined;
    }
    return field.max;
  }

  // Sets the field to cpuCount - 1, same value the field would hold if the
  // user typed it in manually. Issue 171: no mode, no sentinel — this is a
  // one-shot fill, not a toggle.
  function setAuto() {
    draft.rayon_thread_count = Math.max(1, cpuCount - 1 || 1);
  }

  function validate(): boolean {
    errors = {};
    for (const field of PREF_FIELDS) {
      const raw = draft[field.key];
      if (field.type === 'integer' || field.type === 'float') {
        const v = Number(raw);
        if (isNaN(v)) {
          errors[field.key] = `Must be a number.`;
          continue;
        }
        const max = effectiveMax(field);
        if (field.min !== undefined && v < field.min) {
          errors[field.key] = `Minimum is ${field.min}${field.unit ? ' ' + field.unit : ''}.`;
        } else if (max !== undefined && v > max) {
          errors[field.key] = `Maximum is ${max}${field.unit ? ' ' + field.unit : ''}.`;
        }
      }
    }
    return Object.keys(errors).length === 0;
  }

  function buildChanged(): Record<string, number | string> {
    const s = $settings;
    const changed: Record<string, number | string> = {};

    for (const field of PREF_FIELDS) {
      let draftVal: number | string = draft[field.key];
      let storeVal: number | string;

      if (field.type === 'integer') {
        draftVal = Math.round(Number(draftVal));
        storeVal = (s as unknown as Record<string, number | string>)[field.key];
      } else if (field.type === 'float') {
        draftVal = Number(draftVal);
        storeVal = (s as unknown as Record<string, number | string>)[field.key];
      } else {
        storeVal = (s as unknown as Record<string, number | string>)[field.key];
      }

      if (draftVal !== storeVal) {
        changed[field.key] = draftVal;
      }
    }
    return changed;
  }

  async function apply() {
    if (!validate()) return;
    const changed = buildChanged();
    if (Object.keys(changed).length === 0) return;
    try {
      await settings.savePreferences(changed as never);
      notifications.success('Preferences saved.');
    } catch (e) {
      notifications.error(`Failed to save preferences: ${e}`);
    }
  }

  async function ok() {
    if (!validate()) return;
    await apply();
    onclose();
  }

  function cancel() {
    onclose();
  }

  async function browseBackupDir() {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === 'string') {
        draft['backup_directory'] = selected;
      }
    } catch (e) {
      notifications.error(`Failed to open folder picker: ${e}`);
    }
  }

  // Close on Escape
  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Escape') cancel();
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="pref-backdrop">
  <div class="pref-dialog" onclick={(e) => e.stopPropagation()}>

    <div class="pref-header">
      <span class="pref-title">Preferences</span>
      <button class="pref-close-btn" onclick={cancel}>✕</button>
    </div>

    <div class="pref-body">
      {#each PREF_SECTIONS as section}
        <div class="pref-section">
          <div class="pref-section-title">{section.title}</div>

          {#each section.keys as key}
            {@const meta = fieldMeta(key)}
          {#if meta}
            <div class="pref-row" class:has-error={!!errors[key]}>
              <label class="pref-label" for={`pref-${key}`}>{meta.label}</label>
              <div class="pref-control">
                {#if meta.type === 'path'}
                  <div class="pref-path-row">
                    <input
                      id={`pref-${key}`}
                      class="pref-input pref-input-path"
                      type="text"
                      value={draft[key] ?? ''}
                      oninput={(e) => draft[key] = (e.target as HTMLInputElement).value}
                    />
                    <button class="pref-browse-btn" onclick={browseBackupDir}>Browse…</button>
                  </div>
                {:else if key === 'rayon_thread_count'}
                  <div class="pref-numeric-row">
                    <input
                      id={`pref-${key}`}
                      class="pref-input pref-input-numeric"
                      type="number"
                      step="1"
                      min={meta.min}
                      max={effectiveMax(meta)}
                      value={draft[key] ?? meta.default}
                      oninput={(e) => draft[key] = (e.target as HTMLInputElement).value}
                    />
                    <button class="pref-browse-btn" onclick={setAuto}>Auto</button>
                    {#if meta.unit}
                      <span class="pref-unit">{meta.unit}</span>
                    {/if}
                  </div>
                {:else}
                  <div class="pref-numeric-row">
                    <input
                      id={`pref-${key}`}
                      class="pref-input pref-input-numeric"
                      type="number"
                      step={meta.step ?? (meta.type === 'float' ? '0.01' : '1')}
                      min={meta.min}
                      max={effectiveMax(meta)}
                      value={draft[key] ?? meta.default}
                      oninput={(e) => draft[key] = (e.target as HTMLInputElement).value}
                    />
                    {#if meta.unit}
                      <span class="pref-unit">{meta.unit}</span>
                    {/if}
                  </div>
                {/if}
                <div class="pref-helper">
                  {meta.helper}
                </div>
                {#if errors[key]}
                  <div class="pref-error">{errors[key]}</div>
                {/if}
              </div>
            </div>
          {/if}
        {/each}
      </div>
    {/each}
  </div>

    <div class="pref-footer">
      <button class="pref-btn pref-btn-secondary" onclick={cancel}>Cancel</button>
      <button class="pref-btn pref-btn-secondary" onclick={apply}>Apply</button>
      <button class="pref-btn pref-btn-primary" onclick={ok}>OK</button>
    </div>

  </div>
</div>
