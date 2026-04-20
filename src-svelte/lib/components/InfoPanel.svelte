<!-- InfoPanel.svelte — Pixel tracking, metadata, histogram, blink. Spec §8.8 -->
<script lang="ts">
    import { invoke } from '@tauri-apps/api/core';
    import { currentImage, session } from '../stores/session';
    import { ui } from '../stores/ui';
    import { notifications } from '../stores/notifications';

    let activeTab = $state<'pixels' | 'metadata' | 'histogram' | 'blink'>('pixels');

    // ── Blink state ───────────────────────────────────────────────────────────
    let blinkPlaying    = $state(false);
    let blinkFrame      = $state(0);
    let blinkResolution = $state<'12' | '25'>('25');
    let blinkDelay      = $state(0.25);
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
            const dataUrl = await invoke<string>('get_blink_frame', { index });
            ui.setBlinkFrame(dataUrl);
        } catch (e) {
            console.error('get_blink_frame error:', e);
        }
    }

    async function ensureCached(): Promise<boolean> {
        if ($ui.blinkCached) return true;
        if ($ui.blinkCaching) return false;
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
            blinkLoop();
        } finally {
            playInProgress = false;
        }
    }

    function pause() {
        blinkPlaying = false;
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
                <div class="meta-grid">
                    <div class="meta-field"><span class="meta-key">File</span><span class="meta-val">{$currentImage.filename}</span></div>
                    <div class="meta-field"><span class="meta-key">Size</span><span class="meta-val">{$currentImage.width} × {$currentImage.height}</span></div>
                    <div class="meta-field"><span class="meta-key">BitDepth</span><span class="meta-val">{$currentImage.bitDepth}</span></div>
                    <div class="meta-field"><span class="meta-key">ColorSpace</span><span class="meta-val">{$currentImage.colorSpace}</span></div>
                    {#each ['OBJECT','FILTER','EXPTIME','GAIN','TEMP','DATE-OBS','INSTRUME','TELESCOP'] as kw}
                        {#if $currentImage.keywords[kw]}
                            <div class="meta-field">
                                <span class="meta-key">{kw}</span>
                                <span class="meta-val">{$currentImage.keywords[kw].value}</span>
                            </div>
                        {/if}
                    {/each}
                </div>
            {:else}
                <p style="font-size:11px;color:var(--text-secondary);padding:8px;">No image loaded.</p>
            {/if}
        </div>

    <!-- Histogram -->
    {:else if activeTab === 'histogram'}
        <div class="info-panel-body active" id="ip-histogram">
            <canvas id="mini-histogram"></canvas>
            <div class="histogram-label">No image loaded</div>
        </div>

    <!-- Blink -->
    {:else if activeTab === 'blink'}
        <div class="info-panel-body active" id="ip-blink">
            <div id="blink-controls">
                <!-- Navigation -->
                <button
                    class="blink-btn"
                    disabled={blinkPlaying || frameCount === 0}
                    onclick={stepBack}
                    title="Previous frame"
                >←</button>

                <!-- Play/Pause -->
                <button
                    class="blink-btn blink-play"
                    disabled={frameCount === 0 || $ui.blinkCaching}
                    onclick={() => blinkPlaying ? pause() : play()}
                    title={blinkPlaying ? 'Pause' : 'Play'}
                >{blinkPlaying ? '⏸' : '▶'}</button>

                <button
                    class="blink-btn"
                    disabled={blinkPlaying || frameCount === 0}
                    onclick={stepForward}
                    title="Next frame"
                >→</button>

                <!-- Frame counter -->
                <span class="blink-counter">{frameCount > 0 ? `${blinkFrame + 1} / ${frameCount}` : '0 / 0'}</span>
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
