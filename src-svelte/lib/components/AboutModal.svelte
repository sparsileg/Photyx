<!-- AboutModal.svelte — About Photyx dialog. Spec §8.2 -->
<script lang="ts">
  let { onclose } = $props<{ onclose: () => void }>();
</script>

<div class="modal-overlay" onclick={onclose}>
  <div class="modal-box about-box" onclick={(e) => e.stopPropagation()}>

    <div class="modal-header">
      <span class="modal-title">About Photyx</span>
      <span class="modal-close" onclick={onclose}>✕</span>
    </div>

    <div class="modal-body about-body">
      <div class="about-title">PHOTYX</div>
      <div class="about-version">Version 1.0.0-dev</div>

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
        five quality metrics per frame — FWHM, eccentricity, star
        count, signal weight, and background median — classifying each
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
