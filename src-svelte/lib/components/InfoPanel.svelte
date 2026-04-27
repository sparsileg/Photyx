<!-- InfoPanel.svelte — Pixel tracking, metadata, histogram, blink. Spec §8.8 -->
<script lang="ts">
    import { tick } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { currentImage, session } from '../stores/session';
    import { ui } from '../stores/ui';
    import { notifications } from '../stores/notifications';
    import { displayFrame } from '../commands';

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

    $effect(() => {
        const tab = activeTab;
        if (tab === 'blink') {
            wasOnBlinkTab = true;
            ui.setBlinkTabActive(true);
            ui.setBlinkModeActive(true);
            ui.clearAnnotations();
            blinkFrame = $session.currentFrame;
            fetchFrameFlags();
        } else if (wasOnBlinkTab) {
            wasOnBlinkTab = false;
            ui.setBlinkTabActive(false);
            ui.setBlinkModeActive(false);
            onBlinkFrame('');
            if (blinkPlaying) pause();
            ui.setBlinkFrame(null);
            if ($ui.blinkCached) {
                displayFrame(blinkFrame);
            }
        }
    });

    // ── Blink state ───────────────────────────────────────────────────────────
    let blinkPlaying    = $state(false);
    let blinkFrame      = $state(0);
    let blinkResolution = $state<'12' | '25'>('12');
    let blinkDelay      = $state(0.1);
    let blinkTimer: ReturnType<typeof setTimeout> | null = null;
    let playInProgress  = false;
    let frameFlags      = $state<string[]>([]);

    async function fetchFrameFlags() {
        try {
            frameFlags = await invoke<string[]>('get_frame_flags');
        } catch (e) {
            console.error('get_frame_flags error:', e);
            frameFlags = [];
        }
    }

    const DELAY_OPTIONS = [0, 0.05, 0.1, 0.25, 0.5, 1.0, 2.0];

    const frameCount = $derived($session.fileList.length);

    async function buildCache(): Promise<boolean> {
        ui.setBlinkCaching(true);
        ui.setBlinkCached(false);
        try {
            const result = await invoke<{ success: boolean; output: string | null; error: string | null }>(
                'dispatch_command',
                { request: { command: 'CacheFrames', args: { resolution: blinkResolution } } }
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
        try {
            const dataUrl = await invoke<string>('get_blink_frame', { index, resolution: blinkResolution });
            ui.setBlinkFrame(dataUrl);
            ui.setCurrentBlinkFlag(frameFlags[index] ?? '');
            const filename = $session.fileList[index]?.split(/[\\/]/).pop() ?? '';
            onBlinkFrame(filename);
        } catch (e) {
            console.error('get_blink_frame error:', e);
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
            notifications.info('Cache is being built in the background, please wait…');
            // Poll until complete then notify
            const poll = setInterval(async () => {
                const s = await invoke<string>('get_blink_cache_status');
                if (s === 'ready') {
                    clearInterval(poll);
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
        blinkFrame = (blinkFrame + 1) % frameCount;
        blinkTimer = setTimeout(() => blinkLoop(), blinkDelay * 1000);
    }

    // Invalidate cache only when file list length actually changes
    let lastFileCount = $state(0);
    $effect(() => {
        const count = $session.fileList.length;
        if (count !== lastFileCount) {
            lastFileCount = count;
            ui.setBlinkCached(false);
            blinkFrame = 0;
        }
    });

    // Invalidate cache when resolution changes — skip initial run
    let resolutionInitialized = false;
    $effect(() => {
        const _ = blinkResolution;
        if (!resolutionInitialized) { resolutionInitialized = true; return; }
        ui.setBlinkCached(false);
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
            // Draw R, G, B channels with additive blending
            const allBins = [data.bins, data.bins_g!, data.bins_b!];
            const allMax = Math.max(
                ...allBins.flatMap(b => b)
            );
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

            // Stats overlay
            const statsLines = [
                `Med R/G/B: ${(data.median * 65535).toFixed(0)}/${(data.median_g! * 65535).toFixed(0)}/${(data.median_b! * 65535).toFixed(0)}`,
                `σ R/G/B: ${(data.std_dev * 65535).toFixed(0)}/${(data.std_dev_g! * 65535).toFixed(0)}/${(data.std_dev_b! * 65535).toFixed(0)}`,
                `Clip: ${data.clipping_pct.toFixed(3)}%`,
            ];
            drawStatsOverlay(ctx, statsLines, w);
        } else {
            // Mono
            const max = Math.max(...data.bins);
            if (max === 0) return;

            ctx.fillStyle = '#00ff00';
            for (let i = 0; i < 256; i++) {
                if (data.bins[i] === 0) continue;
                const logVal = Math.log1p(data.bins[i]) / Math.log1p(max);
                const barH = logVal * h;
                ctx.fillRect(i * barW, h - barH, Math.ceil(barW), barH);
            }

            const statsLines = [
                `Med: ${(data.median * 65535).toFixed(0)}`,
                `σ: ${(data.std_dev * 65535).toFixed(0)}`,
                `Clip: ${data.clipping_pct.toFixed(3)}%`,
            ];
            drawStatsOverlay(ctx, statsLines, w);
        }
    }

    function drawStatsOverlay(ctx: CanvasRenderingContext2D, lines: string[], w: number) {
        const fontSize = 12.5;
        ctx.font = `${fontSize}px monospace`;
        const padding = 4;
        const lineH = fontSize + 3;
        const textW = Math.max(...lines.map(s => ctx.measureText(s).width));
        const boxX = w * 0.30;
        const boxY = 4;
        const boxW = textW + padding * 2;
        const boxH = lines.length * lineH + padding;

        ctx.fillStyle = 'rgba(0, 0, 0, 0.65)';
        ctx.fillRect(boxX, boxY, boxW, boxH);

        ctx.fillStyle = '#ffffff';
        lines.forEach((s, i) => {
            ctx.fillText(s, boxX + padding, boxY + padding + fontSize + i * lineH);
        });
    }

    // Update histogram when tab changes or frame changes
    $effect(() => {
        const tab = activeTab;
        const frame = $ui.frameRefreshToken;
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
                <div class="pt-row" style="margin-top:4px;">
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
                <div class="pt-row" style="margin-top:4px;">
                    <div class="pt-field">
                        <span class="pt-label">RA / Dec</span>
                        <span class="pt-value" style="color:var(--text-secondary)">no WCS</span>
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
                            <span class="meta-pill"><span class="meta-key">Image Center</span><span class="meta-val" style="color:var(--text-secondary)">not available</span></span>
                        {/if}
                    </div>
                </div>
            {:else}
                <p style="font-size:11px;color:var(--text-secondary);padding:8px;">No image loaded.</p>
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
                </div>

                <!-- Row 2: Res + Min Delay + Quality Flags toggle -->
                <div class="blink-row">
                    <span class="blink-inline-label">Res</span>
                    <select
                        class="blink-select"
                        value={blinkResolution}
                        disabled={blinkPlaying}
                        onchange={(e) => {
                            blinkResolution = (e.target as HTMLSelectElement).value as '12' | '25';
                            ui.setBlinkResolution(blinkResolution);
                        }}
                    >
                        <option value="25">25%</option>
                        <option value="12">12.5%</option>
                    </select>

                    <span class="blink-inline-label" style="margin-left:12px;">Min Delay</span>
                    <select
                        class="blink-select"
                        value={blinkDelay}
                        disabled={blinkPlaying}
                        onchange={(e) => blinkDelay = parseFloat((e.target as HTMLSelectElement).value)}
                    >
                        {#each DELAY_OPTIONS as d}
                            <option value={d}>{d === 0 ? 'Max' : `${d}s`}</option>
                        {/each}
                    </select>

                    <label class="blink-flag-toggle" style="margin-left:12px;" title="Show quality flag overlays">
                        <input
                            type="checkbox"
                            checked={$ui.showQualityFlags}
                            onchange={(e) => ui.setShowQualityFlags((e.target as HTMLInputElement).checked)}
                        />
                        <span class="blink-inline-label">Flags</span>
                    </label>
                </div>
            </div>
        </div>
    {/if}
</div>
