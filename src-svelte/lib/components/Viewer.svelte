<!-- Viewer.svelte — Image viewer canvas. Spec §8.7 -->
<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { ui } from '../stores/ui';
    import { session, currentImage } from '../stores/session';

    const { onMousePixel }: {
        onMousePixel: (px: { x: number; y: number } | null) => void;
    } = $props();

    let canvas = $state<HTMLCanvasElement>();
    let ctx: CanvasRenderingContext2D | null = null;
    let animFrame: number | null = null;
    let running = false;
    let imageDataUrl = $state<string | null>(null);

    // ── Starfield config ──────────────────────────────────────────────────────
    const STAR_COUNT = 420;
    const TWINKLE_RATE = 0.012;

    interface Star {
        x: number; y: number; r: number;
        baseLum: number; lum: number; dlum: number;
        color: string; diffuse: boolean;
    }

    let stars: Star[] = [];

    const STAR_COLORS = [
        '#ccd8ff', '#e8eeff', '#fff4e8',
        '#ffe8c8', '#ffd4a0', '#ffb080', '#ffffff',
    ];

    function starColor(mag: number): string {
        if (mag < 0.05) return STAR_COLORS[Math.floor(Math.random() * 3)];
        if (mag < 0.4)  return STAR_COLORS[Math.floor(Math.random() * STAR_COLORS.length)];
        return STAR_COLORS[2 + Math.floor(Math.random() * 3)];
    }

    function makeStar(w: number, h: number): Star {
        const mag = Math.random();
        return {
            x: Math.random() * w,
            y: Math.random() * h,
            r: mag < 0.05 ? 1.4 + Math.random() * 1.0
             : mag < 0.25 ? 0.9 + Math.random() * 0.6
             :               0.3 + Math.random() * 0.4,
            baseLum: 0.3 + Math.random() * 0.7,
            lum: 0.3 + Math.random() * 0.7,
            dlum: (Math.random() - 0.5) * 0.015,
            color: starColor(mag),
            diffuse: mag < 0.08,
        };
    }

    function hexToRgba(hex: string, alpha: number): string {
        const r = parseInt(hex.slice(1, 3), 16);
        const g = parseInt(hex.slice(3, 5), 16);
        const b = parseInt(hex.slice(5, 7), 16);
        return `rgba(${r},${g},${b},${alpha.toFixed(3)})`;
    }

    let viewerWidth = $state(0);

    function resize() {
        if (!canvas) return;
        canvas.width  = canvas.offsetWidth;
        canvas.height = canvas.offsetHeight;
        viewerWidth = canvas.offsetWidth;
        stars = Array.from({ length: STAR_COUNT }, () => makeStar(canvas!.width, canvas!.height));
    }

    function drawSpikes(s: Star) {
        if (!ctx) return;
        const len = s.r * 10 + 6;
        for (const [dx, dy] of [[1,0],[-1,0],[0,1],[0,-1]] as [number,number][]) {
            const grad = ctx.createLinearGradient(s.x, s.y, s.x + dx * len, s.y + dy * len);
            grad.addColorStop(0, hexToRgba(s.color, s.lum * 0.5));
            grad.addColorStop(1, hexToRgba(s.color, 0));
            ctx.strokeStyle = grad;
            ctx.lineWidth = 0.5;
            ctx.beginPath();
            ctx.moveTo(s.x, s.y);
            ctx.lineTo(s.x + dx * len, s.y + dy * len);
            ctx.stroke();
        }
    }

    function drawFrame() {
        if (!ctx || !canvas) return;
        const w = canvas.width;
        const h = canvas.height;

        const grad = ctx.createRadialGradient(w * 0.4, h * 0.35, 0, w * 0.5, h * 0.5, Math.max(w, h) * 0.8);
        grad.addColorStop(0,   '#050810');
        grad.addColorStop(0.5, '#030507');
        grad.addColorStop(1,   '#000000');
        ctx.fillStyle = grad;
        ctx.fillRect(0, 0, w, h);

        const band = ctx.createLinearGradient(0, h * 0.2, w, h * 0.8);
        band.addColorStop(0,    'rgba(20,25,40,0)');
        band.addColorStop(0.35, 'rgba(20,25,40,0.18)');
        band.addColorStop(0.5,  'rgba(25,30,50,0.28)');
        band.addColorStop(0.65, 'rgba(20,25,40,0.18)');
        band.addColorStop(1,    'rgba(20,25,40,0)');
        ctx.fillStyle = band;
        ctx.fillRect(0, 0, w, h);

        for (const s of stars) {
            if (Math.random() < TWINKLE_RATE) s.dlum = (Math.random() - 0.5) * 0.02;
            s.lum += s.dlum;
            if (s.lum > 1)                { s.lum = 1;              s.dlum *= -1; }
            if (s.lum < s.baseLum * 0.5)  { s.lum = s.baseLum*0.5; s.dlum *= -1; }

            ctx.save();
            if (s.diffuse) {
                const grd = ctx.createRadialGradient(s.x, s.y, 0, s.x, s.y, s.r * 6);
                grd.addColorStop(0,   hexToRgba(s.color, s.lum));
                grd.addColorStop(0.3, hexToRgba(s.color, s.lum * 0.4));
                grd.addColorStop(1,   hexToRgba(s.color, 0));
                ctx.fillStyle = grd;
                ctx.beginPath();
                ctx.arc(s.x, s.y, s.r * 6, 0, Math.PI * 2);
                ctx.fill();
                ctx.fillStyle = hexToRgba(s.color, Math.min(1, s.lum * 1.2));
                ctx.beginPath();
                ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
                ctx.fill();
                drawSpikes(s);
            } else {
                ctx.fillStyle = hexToRgba(s.color, s.lum);
                ctx.beginPath();
                ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
                ctx.fill();
            }
            ctx.restore();
        }
    }

    async function loadCurrentFrame() {
        console.log('loadCurrentFrame called');
        try {
            const result = await invoke<string>('get_current_frame');
            console.log('got frame, length:', result.length);
            imageDataUrl = result;
            // Stop starfield when image is displayed
            running = false;
            if (animFrame) { cancelAnimationFrame(animFrame); animFrame = null; }
        } catch (e) {
            console.error('get_current_frame error:', e);
            imageDataUrl = null;
        }
    }

    async function loadFullFrame() {
        try {
            const result = await invoke<string>('get_full_frame');
            imageDataUrl = result;
        } catch (e) {
            console.error('get_full_frame error:', e);
            // Fall back to display cache
            await loadCurrentFrame();
        }
    }

    // React to frame refresh requests — only fire when token actually increases
    let lastToken = 0;
    let lastNeedsFullRes = false;

    $effect(() => {
        const token = $ui.frameRefreshToken;
        console.log('frameRefreshToken changed:', token);
        if (token > 0 && token !== lastToken) {
            lastToken = token;
            lastNeedsFullRes = false; // reset so zoom effect re-evaluates for new frame
            loadCurrentFrame();
        }
    });

    // Switch between display cache and full-res cache when zoom crosses threshold
    $effect(() => {
        const full = needsFullRes;
        if (full === lastNeedsFullRes) return;
        lastNeedsFullRes = full;
        if (imageDataUrl === null) return; // no image loaded yet
        if (full) {
            loadFullFrame();
        } else {
            loadCurrentFrame();
        }
    });

    // React to viewer clear requests
    let lastClearToken = 0;
    $effect(() => {
        const token = $ui.viewerClearToken;
        if (token > 0 && token !== lastClearToken) {
            lastClearToken = token;
            imageDataUrl = null;
            ui.setBlinkFrame(null);
            running = true;
            loop();
        }
    });

    onMount(() => {
        if (!canvas) return;
        ctx = canvas.getContext('2d');
        resize();
        window.addEventListener('resize', resize);
        running = true;
        loop();
    });

    onDestroy(() => {
        running = false;
        if (animFrame) cancelAnimationFrame(animFrame);
        window.removeEventListener('resize', resize);
    });

    function loop() {
        if (!running) return;
        drawFrame();
        animFrame = requestAnimationFrame(loop);
    }

    // ── Zoom ──────────────────────────────────────────────────────────────────
    const ZOOM_FACTORS: Record<string, number> = {
        'fit': 1, '25': 0.25, '50': 0.5, '100': 1.0, '200': 2.0
    };

    let zoomScale = $derived((() => {
        if ($ui.zoomLevel === 'fit') return 1;
        const img = $session.loadedImages[$session.fileList[$session.currentFrame]];
        const srcWidth = img?.width ?? 1;
        const dispWidth = img?.displayWidth || srcWidth;
        return (ZOOM_FACTORS[$ui.zoomLevel] ?? 1) * (srcWidth / dispWidth);
    })());

    // True when the current zoom level needs more resolution than the display cache provides
    let needsFullRes = $derived((() => {
        const img = $session.loadedImages[$session.fileList[$session.currentFrame]];
        if (!img) return false;
        const displayCacheWidth = img.displayWidth || img.width;
        if ($ui.zoomLevel === 'fit') {
            // At fit zoom, full-res needed if viewer is wider than display cache
            return viewerWidth > displayCacheWidth;
        }
        // At other zoom levels, full-res needed if scaled source exceeds display cache
        const factor = ZOOM_FACTORS[$ui.zoomLevel] ?? 1;
        return factor * img.width > displayCacheWidth;
    })());

    // ── Mouse pixel tracking — always on ─────────────────────────────────────
    let lastSrcX = -1;
    let lastSrcY = -1;

    function getSourceCoords(e: MouseEvent): { x: number; y: number } | null {
        if (!$currentImage) return null;
        const img = (e.currentTarget as HTMLElement).querySelector('#viewer-image') as HTMLImageElement;
        if (!img) return null;
        const rect = img.getBoundingClientRect();
        const displayX = e.clientX - rect.left;
        const displayY = e.clientY - rect.top;
        if (displayX < 0 || displayY < 0 || displayX >= rect.width || displayY >= rect.height) return null;
        return {
            x: Math.floor((displayX / rect.width)  * $currentImage.width),
            y: Math.floor((displayY / rect.height) * $currentImage.height),
        };
    }

    function onScrollMouseMove(e: MouseEvent) {
        if ($ui.blinkPlaying) return;
        const coords = getSourceCoords(e);
        if (!coords) {
            onMousePixel(null);
            return;
        }
        if (coords.x === lastSrcX && coords.y === lastSrcY) return;
        lastSrcX = coords.x;
        lastSrcY = coords.y;
        onMousePixel(coords);
    }

    function onScrollMouseLeave() {
        lastSrcX = -1;
        lastSrcY = -1;
        onMousePixel(null);
    }
</script>

<div id="viewer-wrap">
    <canvas id="viewer-canvas" bind:this={canvas} style:display={imageDataUrl !== null || $ui.blinkImageUrl !== null ? 'none' : 'block'}></canvas>
    {#if imageDataUrl !== null || $ui.blinkImageUrl !== null}
        <div
            id="viewer-scroll"
            class:zoom-fit={$ui.blinkPlaying || $ui.zoomLevel === 'fit'}
            onmousemove={onScrollMouseMove}
            onmouseleave={onScrollMouseLeave}
        >
            <img
                id="viewer-image"
                src={$ui.blinkImageUrl ?? imageDataUrl}
                alt="Current frame"
                style:width={!$ui.blinkPlaying && $ui.zoomLevel !== 'fit' ? `${Math.round(($currentImage?.displayWidth || $currentImage?.width || 1200) * zoomScale)}px` : undefined}
                style:height={!$ui.blinkPlaying && $ui.zoomLevel !== 'fit' ? `${Math.round(($currentImage?.displayWidth || $currentImage?.width || 1200) * zoomScale)}px` : undefined}
            />
        </div>
    {/if}

    {#if imageDataUrl !== null || $ui.blinkImageUrl !== null}
        <div id="viewer-disclaimer">Display: JPEG compressed — pixel readout uses raw data</div>
    {/if}
    {#if $session.fileList.length === 0}
        <div id="viewer-placeholder">
            <div class="ph-title">PHOTYX</div>
            <div class="ph-sub">Select a directory and load files to begin</div>
        </div>
    {/if}
</div>
