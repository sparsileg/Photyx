<!-- InfoPanel.svelte — Pixel tracking, metadata, histogram, blink. Spec §8.8 -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { currentImage, session } from '../stores/session';
    import { ui } from '../stores/ui';
    import { notifications } from '../stores/notifications';
    import { displayFrame } from '../commands';

    let activeTab = $state<'pixels' | 'metadata' | 'histogram' | 'blink'>('pixels');

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

    let wasOnBlinkTab = false;

    $effect(() => {
        const tab = activeTab;
        if (tab === 'blink') {
            wasOnBlinkTab = true;
        } else if (wasOnBlinkTab) {
            wasOnBlinkTab = false;
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
    let histStats = $state<{ median: number; mean: number; std_dev: number; clipping_pct: number } | null>(null);

    async function updateHistogram() {
        if (activeTab !== 'histogram' || !$currentImage) return;
        try {
            const data = await invoke<{
                bins: number[];
                median: number;
                mean: number;
                std_dev: number;
                clipping_pct: number;
            }>('get_histogram');

            histStats = data;
            drawHistogram(data.bins, data);
        } catch (e) {
            console.error('get_histogram error:', e);
        }
    }

    function drawHistogram(bins: number[], stats?: typeof histStats) {
        if (stats) histStats = stats;
        const canvas = histogramCanvas;
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;

        // Match canvas pixel size to display size
        canvas.width = canvas.offsetWidth || 400;
        canvas.height = 80;

        const w = canvas.width;
        const h = canvas.height;

        const max = Math.max(...bins);
        if (max === 0) return;

        // Background
        ctx.fillStyle = '#001100';
        ctx.fillRect(0, 0, w, h);

        // Bars — log scale for better visibility of dim sky background
        const barW = w / 256;
        ctx.fillStyle = '#00ff00';
        for (let i = 0; i < 256; i++) {
            if (bins[i] === 0) continue;
            const logVal = Math.log1p(bins[i]) / Math.log1p(max);
            const barH = logVal * h;
            ctx.fillRect(i * barW, h - barH, Math.ceil(barW), barH);
        }

        // Stats overlay — top of histogram, starting at 30% from left
        if (histStats) {
            const stats = [
                `Med: ${(histStats.median * 65535).toFixed(0)}`,
                `σ: ${(histStats.std_dev * 65535).toFixed(0)}`,
                `Clip: ${histStats.clipping_pct.toFixed(3)}%`,
            ];
            const fontSize = 12.5;
            ctx.font = `${fontSize}px monospace`;
            const padding = 4;
            const lineH = fontSize + 3;
            const textW = Math.max(...stats.map(s => ctx.measureText(s).width));
            const boxX = w * 0.30;
            const boxY = 4;
            const boxW = textW + padding * 2;
            const boxH = stats.length * lineH + padding;

            // Semi-transparent background
            ctx.fillStyle = 'rgba(0, 0, 0, 0.65)';
            ctx.fillRect(boxX, boxY, boxW, boxH);

            // Text
            ctx.fillStyle = '#ffffff';
            stats.forEach((s, i) => {
                ctx.fillText(s, boxX + padding, boxY + padding + fontSize + i * lineH);
            });
        }
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
                    <div class="pt-field"><span class="pt-label">X</span><span class="pt-value" id="pt-x">—</span></div>
                    <div class="pt-field"><span class="pt-label">Y</span><span class="pt-value" id="pt-y">—</span></div>
                    <div class="pt-field"><span class="pt-label">Raw</span><span class="pt-value" id="pt-raw">—</span></div>
                    <div class="pt-field"><span class="pt-label">Val</span><span class="pt-value" id="pt-val">—</span></div>
                    <div class="pt-field"><span class="pt-label">RA</span><span class="pt-value" style="color:var(--text-secondary)">no WCS</span></div>
                </div>
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
                <canvas
                    id="mini-histogram"
                    bind:this={histogramCanvas}
                    width="400"
                    height="80"
                ></canvas>
            {:else}
                <div class="histogram-label">No image loaded</div>
            {/if}
        </div>

    <!-- Blink -->
    {:else if activeTab === 'blink'}
        <div class="info-panel-body active" id="ip-blink">
            <div id="blink-controls">
                <!-- Navigation -->
                <button
                    class="blink-btn"
                    disabled={blinkPlaying || frameCount === 0}
                    onclick={(e) => { e.stopPropagation(); stepBack(); }}
                    title="Previous frame"
                >←</button>

                <!-- Play/Pause -->
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

                <!-- Frame counter -->
                <span class="blink-counter">{frameCount > 0 ? `${blinkFrame + 1} / ${frameCount}` : '0 / 0'}</span>
            </div>

            <!-- Filename row -->
            {#if frameCount > 0}
                <div class="blink-filename-row">
                    {$session.fileList[blinkFrame]?.split('/').pop() ?? ''}
                </div>
            {/if}

            <div class="blink-settings">

                <!-- Cache status inline -->
                {#if $ui.blinkCaching}
                    <span class="blink-status-inline">Caching…</span>
                {:else if !$ui.blinkCached && frameCount > 0}
                    <span class="blink-status-inline">Press Play to start</span>
                {/if}
            </div>

            <div class="blink-settings">
                <!-- Resolution -->
                <div class="blink-setting-row">
                    <span class="blink-setting-label">Resolution</span>
                    <select
                        class="blink-select"
                        value={blinkResolution}
                        disabled={blinkPlaying}
                        onchange={(e) => blinkResolution = (e.target as HTMLSelectElement).value as '12' | '25'}
                    >
                        <option value="25">25%</option>
                        <option value="12">12.5%</option>
                    </select>
                </div>

                <!-- Min Delay -->
                <div class="blink-setting-row">
                    <span class="blink-setting-label">Min Delay</span>
                    <select
                        class="blink-select"
                        value={blinkDelay}
                        disabled={blinkPlaying}
                        onchange={(e) => blinkDelay = parseFloat((e.target as HTMLSelectElement).value)}
                    >
                        {#each DELAY_OPTIONS as d}
                            <option value={d}>{d === 0 ? 'Max speed' : `${d}s`}</option>
                        {/each}
                    </select>
                </div>
            </div>

            {#if $ui.blinkCaching}
                <div class="blink-status">Caching frames…</div>
            {:else if !$ui.blinkCached && frameCount > 0}
                <div class="blink-status">Press Play to cache and start blink</div>
            {/if}
        </div>
    {/if}
</div>
