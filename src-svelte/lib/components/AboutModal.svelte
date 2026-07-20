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
        Photyx is a high-performance desktop application for
        astrophotographers and researchers who demand speed,
        precision, and control. It reads, displays, and processes
        astronomical image files in FITS, XISF, and TIFF formats,
        applying PixInsight-compatible Auto-STF stretching for
        immediate visual assessment of linear data. A fast blink
        engine enables rapid sequential comparison of image sets for
        focus, tracking, and quality evaluation. Photyx automates
        frame triage through its AnalyzeFrames engine, which computes
        four quality metrics per frame — background median, FWHM,
        eccentricity, and star count — classifying each
        frame as PASS or REJECT. Results are visualized in
        the Analysis Graph and Analysis Results table for session-wide
        review.  All operations are scriptable through pcode, a
        purpose-built macro language that supports variables,
        conditionals, loops, and saved macros, accessible
        interactively via the console, from the macro editor, or
        through an external REST API. Photyx is built on Tauri,
        Svelte, and Rust, targeting Windows, macOS, and Linux.
      </p>

      <div class="about-divider"></div>

      <div class="about-stack">
        <span>Tauri v2</span>
        <span>·</span>
        <span>Svelte</span>
        <span>·</span>
        <span>Rust</span>
      </div>

      <div class="about-copy">
        © 2026 Photyx Development Team. All rights reserved.
      </div>
    </div>

    <div class="modal-footer">
      <span>Built with Tauri + Svelte + Rust</span>
    </div>

  </div>
</div>
