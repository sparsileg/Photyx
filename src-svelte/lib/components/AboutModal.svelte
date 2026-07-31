<!-- AboutModal.svelte — About Photyx dialog. Spec §8.2 -->

<script lang="ts">
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { invoke } from '@tauri-apps/api/core';

  let { onclose } = $props<{ onclose: () => void }>();

  // Issue 161: read the live app version and DB schema version rather
  // than a hardcoded string — same pattern already used for the
  // console's Version command in clientCommands.ts (Issue 87).
  let appVersion = $state('');
  let dbVersion  = $state<number | null>(null);

  onMount(async () => {
    try {
      appVersion = await getVersion();
    } catch (e) {
      console.error('Failed to read app version:', e);
    }
    try {
      dbVersion = await invoke<number>('get_db_schema_version');
    } catch (e) {
      console.error('Failed to read DB schema version:', e);
    }
  });
</script>

<div class="modal-overlay" onclick={onclose}>
  <div class="modal-box about-box" onclick={(e) => e.stopPropagation()}>

    <div class="modal-header">
      <span class="modal-title">About Photyx</span>
      <span class="modal-close" onclick={onclose}>✕</span>
    </div>

    <div class="modal-body about-body">
      <div class="about-title">PHOTYX</div>
      <div class="about-version">Version {appVersion || '…'}</div>
      <div class="about-db-version">DB schema v{dbVersion ?? '…'}</div>

      <div class="about-divider"></div>

      <p class="about-text">
        Photyx is a high-performance frame triage tool for
        astrophotographers. It sits between an imaging session and
        processing in PixInsight or Siril, quickly separating the
        frames worth stacking from the ones that would only drag down
        the final result — clouds rolling through, a tracking hiccup,
        a stretch of bad seeing. Fed subs from every session in a
        project, Photyx finds the negative outliers and clears them
        out before they ever reach your stack.
      </p>
      <p class="about-text">
        Photyx reads, displays, and processes astronomical image files
        in FITS and XISF formats. A fast blink engine enables rapid
        sequential comparison of image sets for focus, tracking, and
        quality evaluation, with the ability to reject bad frames at
        the press of a button. A highly optimized analysis engine —
        fast regardless of session size — applies four quality metrics
        through configurable threshold profiles, covering the
        dimensions that matter for rejection. Results are shown as a
        sortable table or plotted graphically, and nothing is final
        until you accept it: confirmed rejects are renamed and moved
        to a separate folder.
      </p>
      <p class="about-text">
        Additional features include configurable auto-stretching,
        batch keyword editing, and fast non-calibrated stacking for
        validation. Every operation is scriptable through pcode, a
        purpose-built macro language supporting variables,
        conditionals, loops, and saved macros — accessible from the
        console, the macro editor, or the Quick Launch bar.
      </p>
      <p class="about-text">
        Photyx delivers native performance on Windows and Debian-based
        Linux. Testers are needed for other Linux distributions and
        macOS.
      </p>

      <div class="about-divider"></div>

      <p class="about-text">
        Testers needed. Questions or suggestions to
        photyx@sparsile.org.
      </p>

      <div class="about-copy">
        © 2026 Photyx Development Team. All rights reserved.
      </div>
    </div>

    <div class="modal-footer">
      <span>Built with Tauri + Svelte + Rust</span>
    </div>

  </div>
</div>
