<!-- InfoPanel.svelte — Pixel tracking, metadata, histogram, blink. Spec §8.8 -->
<script lang="ts">
    import { currentImage } from '../stores/session';

    let activeTab = $state<'pixels' | 'metadata' | 'histogram' | 'blink'>('pixels');
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
                <button class="blink-btn">◀</button>
                <button class="blink-btn">▶</button>
                <button class="blink-btn">▶▶</button>
                <span class="blink-counter">0 / 0</span>
                <div class="fps-control">
                    <span>FPS</span>
                    <input type="range" id="fps-slider" min="0.5" max="10" step="0.5" value="2" />
                    <span id="fps-display">2</span>
                </div>
            </div>
            <p style="font-size:10px;color:var(--text-secondary);margin-top:12px;">
                Blink engine — Phase 2.
            </p>
        </div>
    {/if}
</div>
