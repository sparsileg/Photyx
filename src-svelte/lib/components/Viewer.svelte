<!-- Viewer.svelte — Image viewer canvas. Spec §8.7 -->
<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { ui } from '../stores/ui';
    import { session, currentImage } from '../stores/session';
    import { applyAutoStretch } from '../commands';

    const { onMousePixel }: {
        onMousePixel: (px: { x: number; y: number } | null) => void;
    } = $props();

    // ── Starfield canvas ──────────────────────────────────────────────────────
    let starCanvas = $state<HTMLCanvasElement>();
    let starCtx: CanvasRenderingContext2D | null = null;
    let animFrame: number | null = null;
    let running = false;

    // ── Image display canvas ──────────────────────────────────────────────────
    let imageCanvas = $state<HTMLCanvasElement>();
    let imageCtx: CanvasRenderingContext2D | null = null;
    let hasImage = $state(false);

    // ── Quality flag overlay canvas ───────────────────────────────────────────
    let overlayCanvas = $state<HTMLCanvasElement>();
    let overlayCtx: CanvasRenderingContext2D | null = null;

    // Current bitmap held in memory for zoom/pan redraws without re-fetching
    let currentBitmap: ImageBitmap | null = null;

    // Cached star annotation positions — populated by drawStarAnnotations, reused by paintStarAnnotations
    let cachedStars: Array<{ cx: number; cy: number; fwhm: number; r: number }> = [];

    // ── Pan state ─────────────────────────────────────────────────────────────
    let panX = 0;
    let panY = 0;
    let isPanning = $state(false);
    let panStartX = 0;
    let panStartY = 0;

    // Momentum tracking
    let velX = 0;
    let velY = 0;
    let lastMoveTime = 0;
    let lastMoveX = 0;
    let lastMoveY = 0;
    let momentumFrame: number | null = null;
    const FRICTION = 0.88; // velocity multiplier per frame — lower = more friction
    const MIN_VELOCITY = 0.3; // px/frame below which momentum stops

    function resetPan() {
        panX = 0;
        panY = 0;
        velX = 0;
        velY = 0;
        if (momentumFrame !== null) {
            cancelAnimationFrame(momentumFrame);
            momentumFrame = null;
        }
    }

    // Clamp pan so image edge never goes beyond canvas edge.
    // If image is smaller than canvas in a dimension, lock pan to 0 in that dimension.
    function clampPan(bitmap: ImageBitmap, px: number, py: number): { x: number; y: number } {
        if (!imageCanvas) return { x: px, y: py };
        const cw = imageCanvas.width;
        const ch = imageCanvas.height;
        const { dw, dh } = getDrawDimensions(bitmap);

        let maxPanX = 0;
        let maxPanY = 0;

        if (dw > cw) {
            maxPanX = (dw - cw) / 2;
        }
        if (dh > ch) {
            maxPanY = (dh - ch) / 2;
        }

        return {
            x: Math.max(-maxPanX, Math.min(maxPanX, px)),
            y: Math.max(-maxPanY, Math.min(maxPanY, py)),
        };
    }

    // Returns the drawn dimensions without position — used by clampPan
    function getDrawDimensions(bitmap: ImageBitmap): { dw: number; dh: number } {
        if (!imageCanvas) return { dw: 0, dh: 0 };
        const cw = imageCanvas.width;
        const ch = imageCanvas.height;
        const bw = bitmap.width;
        const bh = bitmap.height;

        if ($ui.blinkModeActive) {
            // Blink resolution: 12 = 12.5%, 25 = 25% of source size
            const blinkFactor = $ui.blinkResolution === '25' ? 0.25 : 0.125;
            const img      = $session.loadedImages[$session.fileList[$session.currentFrame]];
            const srcWidth = img?.width ?? bw;
            const scale    = blinkFactor * (srcWidth / bw);
            const dw = Math.round(bw * scale);
            const dh = Math.round(bh * scale);
            return { dw, dh };
        } else if ($ui.zoomLevel === 'fit') {
            const scale = Math.min(cw / bw, ch / bh);
            return { dw: Math.round(bw * scale), dh: Math.round(bh * scale) };
        } else {
            const img      = $session.loadedImages[$session.fileList[$session.currentFrame]];
            const srcWidth = img?.width ?? bw;
            const factor   = ZOOM_FACTORS[$ui.zoomLevel] ?? 1;
            const scale    = factor * (srcWidth / bw);
            return { dw: Math.round(bw * scale), dh: Math.round(bh * scale) };
        }
    }

    function startMomentum() {
        if (momentumFrame !== null) cancelAnimationFrame(momentumFrame);

        function step() {
            if (!currentBitmap) return;
            velX *= FRICTION;
            velY *= FRICTION;

            if (Math.abs(velX) < MIN_VELOCITY && Math.abs(velY) < MIN_VELOCITY) {
                momentumFrame = null;
                return;
            }

            const clamped = clampPan(currentBitmap, panX + velX, panY + velY);

            // If we hit a wall, kill velocity in that direction
            if (clamped.x === panX + velX) {
                panX += velX;
            } else {
                panX = clamped.x;
                velX = 0;
            }
            if (clamped.y === panY + velY) {
                panY += velY;
            } else {
                panY = clamped.y;
                velY = 0;
            }

            renderBitmap(currentBitmap);
            momentumFrame = requestAnimationFrame(step);
        }

        momentumFrame = requestAnimationFrame(step);
    }

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
    let viewerHeight = $state(0);

    let viewerWrap = $state<HTMLDivElement>();

    function resize() {
        const container = viewerWrap;
        if (!container) return;
        const w = container.offsetWidth;
        const h = container.offsetHeight;
        if (w === 0 || h === 0) return;

        viewerWidth  = w;
        viewerHeight = h;

        if (starCanvas) {
            starCanvas.width  = w;
            starCanvas.height = h;
            starCtx = starCanvas.getContext('2d');
            stars = Array.from({ length: STAR_COUNT }, () => makeStar(w, h));
        }

        if (imageCanvas) {
            imageCanvas.width  = w;
            imageCanvas.height = h;
            imageCtx = imageCanvas.getContext('2d');
            if (currentBitmap) renderBitmap(currentBitmap);
        }

        if (overlayCanvas) {
            overlayCanvas.width  = w;
            overlayCanvas.height = h;
            overlayCtx = overlayCanvas.getContext('2d');
            if (cachedStars.length > 0) {
                paintStarAnnotations();
            } else {
                drawFlagOverlay($ui.currentBlinkFlag);
            }
        }
    }

    function drawSpikes(s: Star) {
        if (!starCtx) return;
        const len = s.r * 10 + 6;
        for (const [dx, dy] of [[1,0],[-1,0],[0,1],[0,-1]] as [number,number][]) {
            const grad = starCtx.createLinearGradient(s.x, s.y, s.x + dx * len, s.y + dy * len);
            grad.addColorStop(0, hexToRgba(s.color, s.lum * 0.5));
            grad.addColorStop(1, hexToRgba(s.color, 0));
            starCtx.strokeStyle = grad;
            starCtx.lineWidth = 0.5;
            starCtx.beginPath();
            starCtx.moveTo(s.x, s.y);
            starCtx.lineTo(s.x + dx * len, s.y + dy * len);
            starCtx.stroke();
        }
    }

    function drawStarfield() {
        if (!starCtx || !starCanvas) return;
        const w = starCanvas.width;
        const h = starCanvas.height;

        const grad = starCtx.createRadialGradient(w * 0.4, h * 0.35, 0, w * 0.5, h * 0.5, Math.max(w, h) * 0.8);
        grad.addColorStop(0,   '#050810');
        grad.addColorStop(0.5, '#030507');
        grad.addColorStop(1,   '#000000');
        starCtx.fillStyle = grad;
        starCtx.fillRect(0, 0, w, h);

        const band = starCtx.createLinearGradient(0, h * 0.2, w, h * 0.8);
        band.addColorStop(0,    'rgba(20,25,40,0)');
        band.addColorStop(0.35, 'rgba(20,25,40,0.18)');
        band.addColorStop(0.5,  'rgba(25,30,50,0.28)');
        band.addColorStop(0.65, 'rgba(20,25,40,0.18)');
        band.addColorStop(1,    'rgba(20,25,40,0)');
        starCtx.fillStyle = band;
        starCtx.fillRect(0, 0, w, h);

        for (const s of stars) {
            if (Math.random() < TWINKLE_RATE) s.dlum = (Math.random() - 0.5) * 0.02;
            s.lum += s.dlum;
            if (s.lum > 1)                { s.lum = 1;              s.dlum *= -1; }
            if (s.lum < s.baseLum * 0.5)  { s.lum = s.baseLum*0.5; s.dlum *= -1; }

            starCtx.save();
            if (s.diffuse) {
                const grd = starCtx.createRadialGradient(s.x, s.y, 0, s.x, s.y, s.r * 6);
                grd.addColorStop(0,   hexToRgba(s.color, s.lum));
                grd.addColorStop(0.3, hexToRgba(s.color, s.lum * 0.4));
                grd.addColorStop(1,   hexToRgba(s.color, 0));
                starCtx.fillStyle = grd;
                starCtx.beginPath();
                starCtx.arc(s.x, s.y, s.r * 6, 0, Math.PI * 2);
                starCtx.fill();
                starCtx.fillStyle = hexToRgba(s.color, Math.min(1, s.lum * 1.2));
                starCtx.beginPath();
                starCtx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
                starCtx.fill();
                drawSpikes(s);
            } else {
                starCtx.fillStyle = hexToRgba(s.color, s.lum);
                starCtx.beginPath();
                starCtx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
                starCtx.fill();
            }
            starCtx.restore();
        }
    }

    // ── Image rendering ───────────────────────────────────────────────────────
    function getDrawRect(bitmap: ImageBitmap): { dx: number; dy: number; dw: number; dh: number } {
        if (!imageCanvas) return { dx: 0, dy: 0, dw: 0, dh: 0 };
        const cw = imageCanvas.width;
        const ch = imageCanvas.height;
        const { dw, dh } = getDrawDimensions(bitmap);

        if ($ui.blinkModeActive || $ui.zoomLevel === 'fit') {
            return {
                dx: Math.round((cw - dw) / 2),
                dy: Math.round((ch - dh) / 2),
                dw,
                dh,
            };
        } else {
            return {
                dx: Math.round((cw - dw) / 2) + panX,
                dy: Math.round((ch - dh) / 2) + panY,
                dw,
                dh,
            };
        }
    }

    function renderBitmap(bitmap: ImageBitmap) {
        if (!imageCanvas || !imageCtx) return;
        const cw = imageCanvas.width;
        const ch = imageCanvas.height;

        imageCtx.imageSmoothingEnabled = true;
        imageCtx.imageSmoothingQuality = 'high';
        imageCtx.fillStyle = '#000';
        imageCtx.fillRect(0, 0, cw, ch);

        const { dx, dy, dw, dh } = getDrawRect(bitmap);
        imageCtx.drawImage(bitmap, dx, dy, dw, dh);

        // Repaint cached annotations to stay in sync with pan/zoom
        if (cachedStars.length > 0) {
            paintStarAnnotations();
        }
    }

    async function drawImageFromUrl(dataUrl: string) {
        if (!imageCanvas || !imageCtx) return;
        try {
            const response = await fetch(dataUrl);
            const blob = await response.blob();
            const bitmap = await createImageBitmap(blob);

            if (currentBitmap) currentBitmap.close();
            currentBitmap = bitmap;

            renderBitmap(bitmap);
            drawFlagOverlay($ui.currentBlinkFlag);
            hasImage = true;

            // Repaint annotations after new frame is loaded
            if (cachedStars.length > 0) {
                paintStarAnnotations();
            }

            running = false;
            if (animFrame) { cancelAnimationFrame(animFrame); animFrame = null; }
        } catch (e) {
            console.error('drawImageFromUrl error:', e);
        }
    }

    function clearImageCanvas() {
        if (currentBitmap) { currentBitmap.close(); currentBitmap = null; }
        if (imageCanvas && imageCtx) {
            imageCtx.clearRect(0, 0, imageCanvas.width, imageCanvas.height);
        }
        hasImage = false;
        resetPan();
    }

    function drawFlagOverlay(flag: string) {
        if (!overlayCanvas) return;
        overlayCtx = overlayCanvas.getContext('2d');
        if (!overlayCtx) return;

        const cw = overlayCanvas.width;
        const ch = overlayCanvas.height;
        overlayCtx.clearRect(0, 0, cw, ch);

        if (!$ui.showQualityFlags || !$ui.blinkModeActive) return;
        if (!currentBitmap) return;

        const { dx, dy, dw, dh } = getDrawRect(currentBitmap);

        if (flag === 'REJECT') {
            // Red border ~5px inside the image rect
            overlayCtx.save();
            overlayCtx.strokeStyle = 'rgba(255, 40, 40, 0.85)';
            overlayCtx.lineWidth = 5;
            overlayCtx.strokeRect(dx + 3, dy + 3, dw - 6, dh - 6);
            overlayCtx.restore();
        }
    }

    // Redraw overlay when flag or toggle changes
    $effect(() => {
        const flag = $ui.currentBlinkFlag;
        const show = $ui.showQualityFlags;
        drawFlagOverlay(flag);
    });

    // ── Frame loading ─────────────────────────────────────────────────────────
    async function loadCurrentFrame() {
        console.log('loadCurrentFrame called');
        try {
            const result = await invoke<string>('get_current_frame');
            console.log('got frame, length:', result.length);
            resetPan();
            await drawImageFromUrl(result);
        } catch (e) {
            console.error('get_current_frame error:', e);
            clearImageCanvas();
        }
    }

    async function loadFullFrame() {
        try {
            const result = await invoke<string>('get_full_frame');
            resetPan();
            await drawImageFromUrl(result);
        } catch (e) {
            console.error('get_full_frame error:', e);
            await loadCurrentFrame();
        }
    }

    // ── Blink frame drawing ───────────────────────────────────────────────────
    let lastBlinkUrl: string | null = null;
    $effect(() => {
        const url = $ui.blinkImageUrl;
        if (url && url !== lastBlinkUrl) {
            lastBlinkUrl = url;
            ui.clearAnnotations();
            drawImageFromUrl(url);
        } else if (!url) {
            lastBlinkUrl = null;
        }
    });

    // ── AutoStretch frame drawing ─────────────────────────────────────────────
    let lastAutostretchUrl: string | null = null;
    $effect(() => {
        const url = $ui.autostretchImageUrl;
        if (url && url !== lastAutostretchUrl) {
            lastAutostretchUrl = url;
            drawImageFromUrl(url);
        } else if (!url) {
            lastAutostretchUrl = null;
        }
    });

    // ── Frame refresh effects ─────────────────────────────────────────────────
    let lastToken = 0;
    let lastNeedsFullRes = false;

    $effect(() => {
        const token = $ui.frameRefreshToken;
        console.log('frameRefreshToken changed:', token);
        if (token > 0 && token !== lastToken) {
            lastToken = token;
            lastNeedsFullRes = false;
            loadCurrentFrame();
        }
    });

    $effect(() => {
        const full = needsFullRes;
        if (full === lastNeedsFullRes) return;
        lastNeedsFullRes = full;
        if (!hasImage) return;
        if ($ui.autostretchImageUrl) {
            applyAutoStretch();
        } else if (full) {
            loadFullFrame();
        } else {
            loadCurrentFrame();
        }
    });

    // Redraw and reset pan when zoom changes
    $effect(() => {
        const _ = $ui.zoomLevel;
        resetPan();
        if (currentBitmap && hasImage) {
            renderBitmap(currentBitmap);
        }
    });

    // React to viewer clear requests
    let lastClearToken = 0;
    $effect(() => {
        const token = $ui.viewerClearToken;
        if (token > 0 && token !== lastClearToken) {
            lastClearToken = token;
            clearImageCanvas();
            ui.setBlinkFrame(null);
            running = true;
            loop();
        }
    });

// ── Star annotations ──────────────────────────────────────────────────────
   let lastAnnotationToken = 0;
    $effect(() => {
        const token = $ui.annotationToken;
        if (token === lastAnnotationToken) return;
        lastAnnotationToken = token;
        if (token > 0) {
            drawStarAnnotations();
        } else {
            clearAnnotationOverlay();
        }
    });

    async function drawStarAnnotations() {
        if (!overlayCanvas || !overlayCtx || !currentBitmap) return;
        const result = await invoke<{ stars: Array<{ cx: number; cy: number; fwhm: number; r: number }> }>(
            'get_star_positions'
        );
        cachedStars = result.stars;
        paintStarAnnotations();
    }

    function paintStarAnnotations() {
        if (!overlayCanvas || !overlayCtx || !currentBitmap) return;
        if (cachedStars.length === 0) return;
        const img = $session.loadedImages[$session.fileList[$session.currentFrame]];
        if (!img) return;
        const { dx, dy, dw, dh } = getDrawRect(currentBitmap);
        const scaleX = dw / img.width;
        const scaleY = dh / img.height;
        overlayCtx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
        overlayCtx.save();
        overlayCtx.strokeStyle = 'rgba(0, 255, 100, 0.8)';
        overlayCtx.fillStyle   = 'rgba(0, 255, 100, 0.9)';
        overlayCtx.font        = '10px monospace';
        overlayCtx.lineWidth   = 1;
        overlayCtx.textAlign   = 'left';
        for (const s of cachedStars) {
            const sx = dx + s.cx * scaleX;
            const sy = dy + s.cy * scaleY;
            const sr = Math.max(4, s.fwhm * scaleX);
            overlayCtx.beginPath();
            overlayCtx.arc(sx, sy, sr, 0, Math.PI * 2);
            overlayCtx.closePath();
            overlayCtx.stroke();
            overlayCtx.fillText(s.fwhm.toFixed(1), sx + sr + 2, sy + 3);
        }
        overlayCtx.restore();
    }

    function clearAnnotationOverlay() {
        cachedStars = [];
        if (!overlayCanvas || !overlayCtx) return;
        overlayCtx.clearRect(0, 0, overlayCanvas.width, overlayCanvas.height);
        drawFlagOverlay($ui.currentBlinkFlag);
    }

    onMount(() => {
        if (!starCanvas) return;
        starCtx = starCanvas.getContext('2d');
        if (imageCanvas) {
            imageCtx = imageCanvas.getContext('2d');
            imageCanvas.width  = starCanvas.offsetWidth;
            imageCanvas.height = starCanvas.offsetHeight;
        }
        resize();
        window.addEventListener('resize', resize);
        running = true;
        loop();
    });

    onDestroy(() => {
        running = false;
        if (animFrame) cancelAnimationFrame(animFrame);
        if (momentumFrame !== null) cancelAnimationFrame(momentumFrame);
        window.removeEventListener('resize', resize);
        if (currentBitmap) currentBitmap.close();
    });

    function loop() {
        if (!running) return;
        drawStarfield();
        animFrame = requestAnimationFrame(loop);
    }

    // ── Zoom ──────────────────────────────────────────────────────────────────
    const ZOOM_FACTORS: Record<string, number> = {
        'fit': 1, '25': 0.25, '50': 0.5, '100': 1.0, '200': 2.0
    };

    let needsFullRes = $derived((() => {
        const img = $session.loadedImages[$session.fileList[$session.currentFrame]];
        if (!img) return false;
        const DISPLAY_CACHE_WIDTH = 1200;
        if ($ui.zoomLevel === 'fit') {
            return viewerWidth > DISPLAY_CACHE_WIDTH;
        }
        const factor = ZOOM_FACTORS[$ui.zoomLevel] ?? 1;
        return factor * img.width > DISPLAY_CACHE_WIDTH;
    })());

    // ── Mouse handling ────────────────────────────────────────────────────────
    let lastSrcX = -1;
    let lastSrcY = -1;

    function toCanvasCoords(e: MouseEvent): { x: number; y: number } {
        if (!imageCanvas) return { x: 0, y: 0 };
        const rect = imageCanvas.getBoundingClientRect();
        const cw = imageCanvas.width;
        const ch = imageCanvas.height;
        return {
            x: (e.clientX - rect.left) * (cw / rect.width),
            y: (e.clientY - rect.top)  * (ch / rect.height),
        };
    }

    function getSourceCoords(canvasX: number, canvasY: number): { x: number; y: number } | null {
        if (!$currentImage || !imageCanvas || !currentBitmap) return null;
        const { dx, dy, dw, dh } = getDrawRect(currentBitmap);
        if (canvasX < dx || canvasX >= dx + dw || canvasY < dy || canvasY >= dy + dh) return null;
        const srcX = Math.floor(((canvasX - dx) / dw) * $currentImage.width);
        const srcY = Math.floor(((canvasY - dy) / dh) * $currentImage.height);
        return { x: srcX, y: srcY };
    }

    function onViewerMouseDown(e: MouseEvent) {
        if (e.button !== 0) return;
        if ($ui.blinkPlaying || $ui.zoomLevel === 'fit') return;
        // Cancel any ongoing momentum
        if (momentumFrame !== null) {
            cancelAnimationFrame(momentumFrame);
            momentumFrame = null;
        }
        isPanning = true;
        panStartX = e.clientX - panX;
        panStartY = e.clientY - panY;
        velX = 0;
        velY = 0;
        lastMoveX = e.clientX;
        lastMoveY = e.clientY;
        lastMoveTime = performance.now();
        e.preventDefault();
    }

    function onViewerMouseMove(e: MouseEvent) {
        if (isPanning && !$ui.blinkPlaying && $ui.zoomLevel !== 'fit') {
            const now = performance.now();
            const dt = now - lastMoveTime;

            if (dt > 0) {
                // Track instantaneous velocity (px per ms → px per frame at 60fps ≈ 16.7ms)
                velX = ((e.clientX - lastMoveX) / dt) * 16.7;
                velY = ((e.clientY - lastMoveY) / dt) * 16.7;
            }

            lastMoveX = e.clientX;
            lastMoveY = e.clientY;
            lastMoveTime = now;

            const rawX = e.clientX - panStartX;
            const rawY = e.clientY - panStartY;

            if (currentBitmap) {
                const clamped = clampPan(currentBitmap, rawX, rawY);
                panX = clamped.x;
                panY = clamped.y;
                renderBitmap(currentBitmap);
            }
            return;
        }

        if ($ui.blinkPlaying) return;

        // Pixel tracking
        const { x: canvasX, y: canvasY } = toCanvasCoords(e);
        const coords = getSourceCoords(canvasX, canvasY);
        if (!coords) {
            onMousePixel(null);
            return;
        }
        if (coords.x === lastSrcX && coords.y === lastSrcY) return;
        lastSrcX = coords.x;
        lastSrcY = coords.y;
        onMousePixel(coords);
    }

    function onViewerMouseUp(e: MouseEvent) {
        if (e.button !== 0) return;
        if (!isPanning) return;
        isPanning = false;
        // Launch momentum if velocity is significant
        if (Math.abs(velX) > MIN_VELOCITY || Math.abs(velY) > MIN_VELOCITY) {
            startMomentum();
        }
    }

    function onViewerMouseLeave() {
        if (isPanning) {
            isPanning = false;
            if (Math.abs(velX) > MIN_VELOCITY || Math.abs(velY) > MIN_VELOCITY) {
                startMomentum();
            }
        }
        lastSrcX = -1;
        lastSrcY = -1;
        onMousePixel(null);
    }
</script>

<div
    id="viewer-wrap"
    bind:this={viewerWrap}
    onmousedown={onViewerMouseDown}
    onmousemove={onViewerMouseMove}
    onmouseup={onViewerMouseUp}
    onmouseleave={onViewerMouseLeave}
    style:cursor={isPanning ? 'grabbing' : (hasImage && $ui.zoomLevel !== 'fit' ? 'grab' : 'crosshair')}
>
    <!-- Starfield — shown when no image is loaded -->
    <canvas
        id="viewer-canvas"
        bind:this={starCanvas}
        style:display={hasImage ? 'none' : 'block'}
        style:width="100%"
        style:height="100%"
    ></canvas>

    <!-- Image display canvas — fixed size, never resizes, no layout shift -->
    <canvas
        id="viewer-image-canvas"
        bind:this={imageCanvas}
        style:display={hasImage ? 'block' : 'none'}
        style:position="absolute"
        style:top="0"
        style:left="0"
        style:width="100%"
        style:height="100%"
    ></canvas>

    <!-- Quality flag overlay canvas — sits above image, pointer-events:none so mouse still works -->
    <canvas
        id="viewer-overlay-canvas"
        bind:this={overlayCanvas}
        style:display={hasImage ? 'block' : 'none'}
        style:position="absolute"
        style:top="0"
        style:left="0"
        style:width="100%"
        style:height="100%"
        style:pointer-events="none"
    ></canvas>

    {#if hasImage}
        <div id="viewer-disclaimer">Display: JPEG compressed — pixel readout uses raw data</div>
    {/if}

    {#if $session.fileList.length === 0}
        <div id="viewer-placeholder" class:faded={$ui.consoleExpanded}>
            <div class="ph-title">PHOTYX</div>
            <div class="ph-sub">Select a directory and load files to begin</div>
        </div>
    {/if}
</div>
