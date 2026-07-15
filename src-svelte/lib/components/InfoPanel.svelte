<!-- InfoPanel.svelte — Pixel tracking, metadata, histogram, blink. Spec §8.8 -->
<script lang="ts">
  import { tick, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { currentImage, session } from '../stores/session';
  import { ui } from '../stores/ui';
  import { notifications } from '../stores/notifications';
  import { displayFrame, syncSession } from '../commands';
  import Dropdown from './Dropdown.svelte';

  let activeTab = $state<'pixels' | 'metadata' | 'histogram' | 'blink'>('pixels');

  const { onBlinkFrame, mousePixel }: {
    onBlinkFrame: (filename: string) => void;
    mousePixel: { x: number; y: number } | null;
  } = $props();

  function decToHMS(deg: number): string {
    const h = deg / 15;
    const hh = Math.floor(h);
    const mm = Math.floor((h - hh) * 60);
    const ss = ((h - hh) * 60 - mm) * 60;
    return `${String(hh).padStart(2,'0')}h ${String(mm).padStart(2,'0')}m ${ss.toFixed(1).padStart(4,'0')}s`;
  }

  function decToDMS(deg: number): string {
    const sign = deg < 0 ? '-' : '+';
    const abs = Math.abs(deg);
    const dd = Math.floor(abs);
    const mm = Math.floor((abs - dd) * 60);
    const ss = ((abs - dd) * 60 - mm) * 60;
    return `${sign}${String(dd).padStart(2,'0')}° ${String(mm).padStart(2,'0')}' ${ss.toFixed(1).padStart(4,'0')}"`;
  }

  // ── Pixel tracking state ──────────────────────────────────────────────────
  let pixelX   = $state<number | null>(null);
  let pixelY   = $state<number | null>(null);
  let pixelRaw = $state<string>('—');
  let pixelVal = $state<string>('—');
  let pixelRA  = $state<string | null>(null);
  let pixelDec = $state<string | null>(null);

  function clearPixelTracking() {
    pixelX = null; pixelY = null;
    pixelRaw = '—'; pixelVal = '—';
    pixelRA = null; pixelDec = null;
  }

  function computeWCS(sx: number, sy: number): { ra: string; dec: string } | null {
    const kw = $currentImage?.keywords;
    if (!kw) return null;
    const crpix1 = parseFloat(kw['CRPIX1']?.value ?? '');
    const crpix2 = parseFloat(kw['CRPIX2']?.value ?? '');
    const crval1 = parseFloat(kw['CRVAL1']?.value ?? '');
    const crval2 = parseFloat(kw['CRVAL2']?.value ?? '');
    if (isNaN(crpix1) || isNaN(crpix2) || isNaN(crval1) || isNaN(crval2)) return null;
    const dx = (sx + 1) - crpix1;
    const dy = (sy + 1) - crpix2;
    let dra: number, ddec: number;
    const cd11 = parseFloat(kw['CD1_1']?.value ?? '');
    const cd12 = parseFloat(kw['CD1_2']?.value ?? '');
    const cd21 = parseFloat(kw['CD2_1']?.value ?? '');
    const cd22 = parseFloat(kw['CD2_2']?.value ?? '');
    if (!isNaN(cd11) && !isNaN(cd12) && !isNaN(cd21) && !isNaN(cd22)) {
      dra  = cd11 * dx + cd12 * dy;
      ddec = cd21 * dx + cd22 * dy;
    } else {
      const cdelt1 = parseFloat(kw['CDELT1']?.value ?? '');
      const cdelt2 = parseFloat(kw['CDELT2']?.value ?? '');
      if (isNaN(cdelt1) || isNaN(cdelt2)) return null;
      dra  = cdelt1 * dx;
      ddec = cdelt2 * dy;
    }
    const decRad = crval2 * Math.PI / 180;
    const ra  = crval1 + dra / Math.cos(decRad);
    const dec = crval2 + ddec;
    return { ra: decToHMS(((ra % 360) + 360) % 360), dec: decToDMS(dec) };
  }

  async function updatePixelTracking(px: { x: number; y: number } | null) {
    if (!px || !$currentImage) { clearPixelTracking(); return; }
    pixelX = px.x;
    pixelY = px.y;
    const wcs = computeWCS(px.x, px.y);
    pixelRA  = wcs?.ra  ?? null;
    pixelDec = wcs?.dec ?? null;
    try {
      const result = await invoke<{ raw: string; val: string; channels: number }>(
        'get_pixel', { x: px.x, y: px.y }
      );
      pixelRaw = result.raw;
      pixelVal = result.val;
    } catch {
      pixelRaw = '—'; pixelVal = '—';
    }
  }

  $effect(() => {
    updatePixelTracking(mousePixel);
  });

  let wasOnBlinkTab = false;
  // Set right before displayFrame() below re-displays the same frame we
  // were already blinking on. Without this, the frameRefreshToken effect
  // further down can't tell that apart from a genuine frame navigation,
  // and snaps activeTab back to 'pixels' even though the user deliberately
  // clicked Metadata/Histogram to get here.
  let suppressTabResetOnce = false;

  $effect(() => {
    const tab = activeTab;
    if (tab === 'blink') {
      wasOnBlinkTab = true;
      ui.setBlinkTabActive(true);
      ui.setBlinkModeActive(true);
      ui.clearAnnotations();
      blinkFrame = $session.currentFrame;
    } else if (wasOnBlinkTab) {
      wasOnBlinkTab = false;
      ui.setBlinkTabActive(false);
      ui.setBlinkModeActive(false);
      onBlinkFrame('');
      if (blinkPlaying) pause();
      ui.setBlinkFrame(null);
      if (!$ui.displayImageUrl) {
        if ($ui.blinkCached) {
          suppressTabResetOnce = true;
          displayFrame(blinkFrame);
        }
      }
    }
  });

  // ── Blink state ───────────────────────────────────────────────────────────
  let blinkPlaying    = $state(false);
  let blinkFrame      = $state(0);
  let blinkDelay      = $state(0.1);
  let blinkTimer: ReturnType<typeof setTimeout> | null = null;
  let playInProgress  = false;
  let rejecting       = $state(false);

  const DELAY_OPTIONS = [0, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0];

  const frameCount = $derived($session.fileList.length);

  async function buildCache(): Promise<boolean> {
    ui.setBlinkCaching(true);
    ui.setBlinkCached(false);
    try {
      // No resolution arg — CacheFrames defaults to building both 12.5% and
      // 25% caches, so whichever resolution gets auto-selected (see
      // Viewer.svelte) is always already cached, even after a resize.
      const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
        'dispatch_command',
        { request: { command: 'CacheFrames', args: {} } }
      );
      if (!result.success) {
        notifications.error(result.error ?? 'CacheFrames failed');
        return false;
      }
      notifications.info(result.output ?? 'Frames cached');
      ui.setBlinkCached(true);
    } catch (e) {
      notifications.error(`CacheFrames error: ${e}`);
      return false;
    } finally {
      ui.setBlinkCaching(false);
    }
    return true;
  }

  async function showBlinkFrame(index: number) {
    console.log('showBlinkFrame resolution:', $ui.blinkResolution);
    try {
      const dataUrl = await invoke<string>('get_blink_frame', { index, resolution: $ui.blinkResolution });
      ui.setBlinkFrame(dataUrl);
      ui.setBlinkFrameIndex(index);
      const filename = $session.fileList[index]?.split(/[\\/]/).pop() ?? '';
      onBlinkFrame(filename);
    } catch (e) {
      console.error('get_blink_frame error:', e);
    }
  }

  let cachePollInterval: ReturnType<typeof setInterval> | null = null;

  function stopCachePoll() {
    if (cachePollInterval !== null) {
      clearInterval(cachePollInterval);
      cachePollInterval = null;
    }
  }

  async function ensureCached(): Promise<boolean> {
    if ($ui.blinkCached) return true;
    if ($ui.blinkCaching) return false;
    // Check if background cache already built it
    const status = await invoke<string>('get_blink_cache_status');
    if (status === 'ready') {
      ui.setBlinkCached(true);
      return true;
    }
    if (status === 'building') {
      notifications.running('Cache is being built in the background, please wait…');
      // Poll until complete then notify
      stopCachePoll();
      cachePollInterval = setInterval(async () => {
        const s = await invoke<string>('get_blink_cache_status');
        if (s === 'ready') {
          stopCachePoll();
          ui.setBlinkCached(true);
          notifications.success('Caching complete. Ready for operation.');
        }
      }, 500);
      return false;
    }
    return await buildCache();
  }

  async function play() {
    if (playInProgress) return;
    if (frameCount === 0) { notifications.error('No frames loaded.'); return; }
    if ($ui.blinkCaching) return;
    playInProgress = true;
    try {
      const ok = await ensureCached();
      if (!ok) return;
      blinkPlaying = true;
      ui.setBlinkPlaying(true);
      blinkLoop();
    } finally {
      playInProgress = false;
    }
  }

  function pause() {
    blinkPlaying = false;
    ui.setBlinkPlaying(false);
    if (blinkTimer) { clearTimeout(blinkTimer); blinkTimer = null; }
    // Do NOT clear blinkImageUrl — keep last blink frame visible
  }

  onDestroy(() => {
    pause();
    stopCachePoll();
  });

  async function stepBack() {
    if (blinkPlaying || frameCount === 0 || $ui.blinkCaching) return;
    const ok = await ensureCached();
    if (!ok) return;
    blinkFrame = (blinkFrame - 1 + frameCount) % frameCount;
    await showBlinkFrame(blinkFrame);
  }

  async function stepForward() {
    if (blinkPlaying || frameCount === 0 || $ui.blinkCaching) return;
    const ok = await ensureCached();
    if (!ok) return;
    blinkFrame = (blinkFrame + 1) % frameCount;
    await showBlinkFrame(blinkFrame);
  }

  async function blinkLoop() {
    if (!blinkPlaying) return;
    await showBlinkFrame(blinkFrame);
    blinkTimer = setTimeout(() => {
      if (!blinkPlaying) return;
      blinkFrame = (blinkFrame + 1) % frameCount;
      blinkLoop();
    }, blinkDelay * 1000);
  }

  async function rejectCurrentBlinkFrame() {
    if (blinkPlaying || frameCount === 0 || $ui.blinkCaching || rejecting) return;
    rejecting = true;
    try {
      const result = await invoke<{
        success: boolean;
        output: string | null;
        error: string | null;
        data: { rejected_path?: string; new_index?: number; frame_count?: number } | null;
      }>('dispatch_command', {
        request: { command: 'RejectCurrentFrame', args: { index: String(blinkFrame) } }
      });

      if (!result.success) {
        notifications.error(result.error ?? 'RejectCurrentFrame failed');
        return;
      }

      notifications.success(result.output ?? 'Frame rejected');
      await syncSession();

      // Compute Blink's own next index locally rather than trusting the
      // backend's new_index — that value now only reflects ctx.current_frame
      // (the Pixels/pcode notion of "current"), which is intentionally left
      // untouched when the rejected index isn't the one it was pointing at.
      // Blink has its own separate "what's next" question.
      const newCount = result.data?.frame_count ?? 0;
      blinkFrame = newCount === 0 ? 0 : blinkFrame % newCount;

      if (newCount === 0) {
        ui.setBlinkFrame(null);
        onBlinkFrame('');
      } else {
        await showBlinkFrame(blinkFrame);
      }
    } catch (e) {
      notifications.error(`RejectCurrentFrame error: ${e}`);
    } finally {
      rejecting = false;
    }
  }

  // Invalidate cache only when file list length actually changes
  let lastFileCount = $state(0);
  $effect(() => {
    const count = $session.fileList.length;
    if (count !== lastFileCount) {
      const wasBlinking = lastFileCount > 0;
      lastFileCount = count;
      ui.setBlinkCached(false);
      blinkFrame = 0;
      // If we were already actively displaying a blink frame (e.g. a
      // console/pcode RejectCurrentFrame just changed the list out from
      // under us), refresh what's on screen now rather than leaving the
      // pre-reject bitmap visible until the next natural lap.
      if (wasBlinking && $ui.blinkModeActive && count > 0) {
        showBlinkFrame(0);
      }
    }
  });

  // ── Histogram ─────────────────────────────────────────────────────────────
  let histogramCanvas = $state<HTMLCanvasElement>();
  interface HistStats {
    bins: number[];
    bins_g: number[] | null;
    bins_b: number[] | null;
    median: number;
    median_g: number | null;
    median_b: number | null;
    mean: number;
    std_dev: number;
    std_dev_g: number | null;
    std_dev_b: number | null;
    clipping_pct: number;
  }

  let histStats = $state<HistStats | null>(null);
  let histogramLoading = $state(false);

  async function updateHistogram() {
    if (activeTab !== 'histogram' || !$currentImage) return;
    histogramLoading = true;
    try {
      const data = await invoke<HistStats>('get_histogram');
      histStats = data;
      histogramLoading = false;
      await tick();
      drawHistogram(data);
    } catch (e) {
      console.error('get_histogram error:', e);
      histogramLoading = false;
    }
  }

  function drawHistogram(data: HistStats) {
    histStats = data;
    const canvas = histogramCanvas;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    canvas.width = canvas.parentElement?.clientWidth || canvas.offsetWidth || 400;
    canvas.height = 80;

    const w = canvas.width;
    const h = canvas.height;
    const isRGB = data.bins_g !== null && data.bins_b !== null;

    // Background
    ctx.fillStyle = '#001100';
    ctx.fillRect(0, 0, w, h);

    const barW = w / 256;
    if (isRGB) {
      const allBins = [data.bins, data.bins_g!, data.bins_b!];
      const allMax = Math.max(...allBins.flatMap(b => b));
      if (allMax === 0) return;

      const colors = ['rgba(255,60,60,0.7)', 'rgba(60,255,60,0.7)', 'rgba(60,120,255,0.7)'];
      ctx.globalCompositeOperation = 'lighter';
      for (let ch = 0; ch < 3; ch++) {
        ctx.fillStyle = colors[ch];
        const bins = allBins[ch];
        for (let i = 0; i < 256; i++) {
          if (bins[i] === 0) continue;
          const logVal = Math.log1p(bins[i]) / Math.log1p(allMax);
          const barH = logVal * h;
          ctx.fillRect(i * barW, h - barH, Math.ceil(barW), barH);
        }
      }
      ctx.globalCompositeOperation = 'source-over';

      const statsLine = `Med R/G/B: ${(data.median * 65535).toFixed(0)}/${(data.median_g! * 65535).toFixed(0)}/${(data.median_b! * 65535).toFixed(0)}  σ: ${(data.std_dev * 65535).toFixed(0)}/${(data.std_dev_g! * 65535).toFixed(0)}/${(data.std_dev_b! * 65535).toFixed(0)}  Clip: ${data.clipping_pct.toFixed(3)}%`;
      drawStatsOverlay(ctx, statsLine, w, h);
    } else {
      const max = Math.max(...data.bins);
      if (max === 0) return;

      ctx.fillStyle = '#00ff00';
      for (let i = 0; i < 256; i++) {
        if (data.bins[i] === 0) continue;
        const logVal = Math.log1p(data.bins[i]) / Math.log1p(max);
        const barH = logVal * h;
        ctx.fillRect(i * barW, h - barH, Math.ceil(barW), barH);
      }

      const statsLine = `Med: ${(data.median * 65535).toFixed(0)}  σ: ${(data.std_dev * 65535).toFixed(0)}  Clip: ${data.clipping_pct.toFixed(3)}%`;
      drawStatsOverlay(ctx, statsLine, w, h);
    }
  }

  function drawStatsOverlay(ctx: CanvasRenderingContext2D, statsLine: string, w: number, h: number) {
    const fontSize = 11;
    ctx.font = `${fontSize}px monospace`;
    const padding = 4;
    const textW = ctx.measureText(statsLine).width;
    const boxX = w * 0.25;
    const boxY = 4;
    const boxW = textW + padding * 2;
    const boxH = fontSize + padding * 2;

    ctx.fillStyle = 'rgba(0, 0, 0, 0.65)';
    ctx.fillRect(boxX, boxY, boxW, boxH);
    ctx.fillStyle = '#ffffff';
    ctx.fillText(statsLine, boxX + padding, boxY + padding + fontSize);
  }

  function drawHoverOverlay(ctx: CanvasRenderingContext2D, hoverLine: string, w: number, h: number) {
    const fontSize = 11;
    ctx.font = `${fontSize}px monospace`;
    const padding = 4;
    const statsRowH = fontSize + padding * 2;  // matches drawStatsOverlay box height
    const textW = ctx.measureText(hoverLine).width;
    const boxX = w * 0.25;
    const boxY = 4 + statsRowH + 2;  // just below the stats row
    const boxW = textW + padding * 2;
    const boxH = fontSize + padding * 2;

    ctx.fillStyle = 'rgba(0, 0, 0, 0.65)';
    ctx.fillRect(boxX, boxY, boxW, boxH);
    ctx.fillStyle = '#00ff88';
    ctx.fillText(hoverLine, boxX + padding, boxY + padding + fontSize);
  }

  function redrawWithHover(hoverLine: string | null) {
    if (!histStats || !histogramCanvas) return;
    drawHistogram(histStats);
    if (hoverLine) {
      const ctx = histogramCanvas.getContext('2d');
      if (ctx) drawHoverOverlay(ctx, hoverLine, histogramCanvas.width, histogramCanvas.height);
    }
  }

  function onHistogramMouseMove(e: MouseEvent) {
    if (!histStats || !histogramCanvas || !$currentImage) return;
    const rect = histogramCanvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const bin = Math.max(0, Math.min(255, Math.floor((x / rect.width) * 256)));
    const normalized = (bin / 255).toFixed(3);
    const aduScale = $currentImage.bitDepth === 'U8' ? 255 : 65535;
    const adu = Math.round((bin / 255) * aduScale);
    const samplePixels = histStats.bins.reduce((a, b) => a + b, 0);

    let hoverLine: string;
    if (histStats.bins_g !== null && histStats.bins_b !== null) {
      const pctR = (histStats.bins[bin] / samplePixels * 100).toFixed(2);
      const pctG = (histStats.bins_g[bin] / samplePixels * 100).toFixed(2);
      const pctB = (histStats.bins_b[bin] / samplePixels * 100).toFixed(2);
      hoverLine = `Val: ${normalized} / ${adu}  R: ${pctR}%  G: ${pctG}%  B: ${pctB}%`;
    } else {
      const pct = (histStats.bins[bin] / samplePixels * 100).toFixed(2);
      hoverLine = `Val: ${normalized} / ${adu}  Count: ${pct}%`;
    }
    redrawWithHover(hoverLine);
  }

  function onHistogramMouseLeave() {
    redrawWithHover(null);
  }

  // Update histogram when tab changes or frame changes
  let lastFrameToken = 0;
  $effect(() => {
    const tab = activeTab;
    const frame = $ui.frameRefreshToken;
    if (frame !== lastFrameToken && frame > 0) {
      lastFrameToken = frame;
      if (suppressTabResetOnce) {
        suppressTabResetOnce = false;
      } else if (activeTab !== 'pixels') {
        activeTab = 'pixels';
      }
    }
    if (tab === 'histogram') {
      updateHistogram();
    }
  });

</script>

<div id="info-panel">
  <div class="info-panel-tabs">
    {#each ['pixels', 'metadata', 'histogram', 'blink'] as tab}
      <div
        class="info-tab"
        class:active={activeTab === tab}
        onclick={() => activeTab = tab as any}
        >{tab.charAt(0).toUpperCase() + tab.slice(1)}</div>
      {/each}
    </div>

  <!-- Pixel Tracking -->
  {#if activeTab === 'pixels'}
    <div class="info-panel-body active" id="ip-pixels">
      <div id="pixel-tracking">
        <div class="pt-row">
          <div class="pt-field">
            <span class="pt-label">X</span>
            <span class="pt-value">{pixelX !== null ? pixelX : '—'}</span>
          </div>
          <div class="pt-field">
            <span class="pt-label">Y</span>
            <span class="pt-value">{pixelY !== null ? pixelY : '—'}</span>
          </div>
          <div class="pt-field">
            <span class="pt-label">Raw</span>
            <span class="pt-value">{pixelRaw}</span>
          </div>
          <div class="pt-field">
            <span class="pt-label">Val</span>
            <span class="pt-value">{pixelVal}</span>
          </div>
        </div>
        {#if pixelRA !== null}
          <div class="pt-row pt-row-spaced">
            <div class="pt-field">
              <span class="pt-label">RA</span>
              <span class="pt-value">{pixelRA}</span>
            </div>
            <div class="pt-field">
              <span class="pt-label">Dec</span>
              <span class="pt-value">{pixelDec}</span>
            </div>
          </div>
        {:else if $currentImage}
          <div class="pt-row pt-row-spaced">
            <div class="pt-field">
              <span class="pt-label">RA / Dec</span>
              <span class="pt-value pt-value-muted">no WCS</span>
            </div>
          </div>
        {/if}
      </div>
    </div>

    <!-- Metadata -->
  {:else if activeTab === 'metadata'}
    <div class="info-panel-body active" id="ip-metadata">
      {#if $currentImage}
        <div class="meta-rows">
          <!-- Row 1: full filename -->
          <div class="meta-row meta-filename">{$currentImage.filename}</div>

          <!-- Row 2: size, bit depth, color space -->
          <div class="meta-row">
            <span class="meta-pill"><span class="meta-key">Size</span><span class="meta-val">{$currentImage.width} × {$currentImage.height}</span></span>
            <span class="meta-pill"><span class="meta-key">Bit</span><span class="meta-val">{$currentImage.bitDepth}</span></span>
            <span class="meta-pill"><span class="meta-key">Color</span><span class="meta-val">{$currentImage.colorSpace}</span></span>
          </div>

          <!-- Row 3: Image center -->
          <div class="meta-row">
            {#if $currentImage.keywords['CRVAL1'] && $currentImage.keywords['CRVAL2']}
              {@const ra = parseFloat($currentImage.keywords['CRVAL1'].value)}
            {@const dec = parseFloat($currentImage.keywords['CRVAL2'].value)}
            <span class="meta-pill">
              <span class="meta-key">Image Center</span>
              <span class="meta-val">{decToHMS(ra)} &nbsp; {decToDMS(dec)}</span>
            </span>
          {:else if $currentImage.keywords['RA'] && $currentImage.keywords['DEC']}
            {@const ra = parseFloat($currentImage.keywords['RA'].value)}
            {@const dec = parseFloat($currentImage.keywords['DEC'].value)}
            <span class="meta-pill">
              <span class="meta-key">Image Center</span>
              <span class="meta-val">{decToHMS(ra)} &nbsp; {decToDMS(dec)}</span>
            </span>
          {:else}
            <span class="meta-pill"><span class="meta-key">Image Center</span><span class="meta-val meta-val-muted">not available</span></span>
          {/if}
        </div>
        </div>
      {:else}
        <p class="info-empty">No image loaded.</p>
      {/if}
    </div>

    <!-- Histogram -->
  {:else if activeTab === 'histogram'}
    <div class="info-panel-body active" id="ip-histogram">
      {#if $currentImage}
        {#if histogramLoading}
          <div class="histogram-label">Computing histogram…</div>
        {:else}
          <canvas
            id="mini-histogram"
            bind:this={histogramCanvas}
            width="400"
            height="80"
            onmousemove={onHistogramMouseMove}
            onmouseleave={onHistogramMouseLeave}
            ></canvas>
          {/if}
        {:else}
          <div class="histogram-label">No image loaded</div>
        {/if}
      </div>

    <!-- Blink -->
  {:else if activeTab === 'blink'}
    <div class="info-panel-body active" id="ip-blink">
      <div id="blink-controls">
        <!-- Row 1: navigation + counter + status -->
        <div class="blink-row">
          <button
            class="blink-btn"
            disabled={blinkPlaying || frameCount === 0}
            onclick={(e) => { e.stopPropagation(); stepBack(); }}
            title="Previous frame"
            >←</button>

          <button
            class="blink-btn blink-play"
            disabled={frameCount === 0 || $ui.blinkCaching}
            onclick={(e) => { e.stopPropagation(); blinkPlaying ? pause() : play(); }}
            title={blinkPlaying ? 'Pause' : 'Play'}
            >{blinkPlaying ? '⏸' : '▶'}</button>

          <button
            class="blink-btn"
            disabled={blinkPlaying || frameCount === 0}
            onclick={(e) => { e.stopPropagation(); stepForward(); }}
            title="Next frame"
            >→</button>

          <span class="blink-counter">{frameCount > 0 ? `${blinkFrame + 1} / ${frameCount}` : '0 / 0'}</span>

          {#if $ui.blinkCaching}
            <span class="blink-status-inline">Caching…</span>
          {:else if !$ui.blinkCached && frameCount > 0}
            <span class="blink-status-inline">Press Play to start</span>
          {/if}

          <button
            class="blink-btn blink-reject-btn"
            disabled={blinkPlaying || frameCount === 0 || $ui.blinkCaching || rejecting}
            onclick={(e) => { e.stopPropagation(); rejectCurrentBlinkFrame(); }}
            title="Move this frame to rejected/ and remove it from the session"
            >Reject</button>
        </div>

        <!-- Row 2: Min Delay -->
        <div class="blink-row">
          <span class="blink-inline-label">Min Delay</span>
          <Dropdown
            className="blink-select"
            value={String(blinkDelay)}
            openUp={true}
            width={70}
            options={DELAY_OPTIONS.map(d => ({ value: String(d), label: d === 0 ? 'Max' : `${d}s` }))}
            on:change={(e) => { blinkDelay = parseFloat(e.detail); }}
            />
        </div>
      </div>
    </div>
  {/if}
</div>
