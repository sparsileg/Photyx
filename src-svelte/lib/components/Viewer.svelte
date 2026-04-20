<!-- Viewer.svelte — Image viewer canvas. Spec §8.7 -->
<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { ui } from '../stores/ui';
    import { session } from '../stores/session';

    let canvas = $state<HTMLCanvasElement>();
    let ctx: CanvasRenderingContext2D | null = null;
    let animFrame: number | null = null;
    let running = false;

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

    function resize() {
        if (!canvas) return;
        canvas.width  = canvas.offsetWidth;
        canvas.height = canvas.offsetHeight;
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

    function loop() {
        if (!running) return;
        drawFrame();
        animFrame = requestAnimationFrame(loop);
    }

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
</script>

<div id="viewer-wrap">
    <canvas id="viewer-canvas" bind:this={canvas}></canvas>
    <div id="zoom-indicator">{$ui.zoomLevel}</div>
    {#if $session.fileList.length === 0}
        <div id="viewer-placeholder">
            <div class="ph-title">PHOTYX</div>
            <div class="ph-sub">Open an image or use SelectDirectory in the console</div>
        </div>
    {/if}
</div>
