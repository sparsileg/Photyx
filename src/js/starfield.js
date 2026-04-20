// starfield.js — Photyx image viewer starfield placeholder renderer

'use strict';

const Starfield = (() => {

    let canvas, ctx;
    let stars = [];
    let animFrame = null;
    let running = false;

    // Config
    const STAR_COUNT   = 420;
    const TWINKLE_RATE = 0.012;   // fraction that flicker per frame
    const BG_COLOR     = '#000000';

    // Star object pool
    function makeStar(w, h) {
        const mag = Math.random(); // 0 = bright, 1 = dim
        return {
            x:         Math.random() * w,
            y:         Math.random() * h,
            r:         mag < 0.05 ? (1.4 + Math.random() * 1.0) :   // bright giant
                       mag < 0.25 ? (0.9 + Math.random() * 0.6) :   // mid
                                    (0.3 + Math.random() * 0.4),     // faint
            baseLum:   0.3 + Math.random() * 0.7,
            lum:       0,        // set below
            dlum:      (Math.random() - 0.5) * 0.015,
            color:     starColor(mag),
            diffuse:   mag < 0.08,   // bloomed halo for brightest
        };
    }

    // Subtle BVRI colour palette — astrophotographically plausible
    const STAR_COLORS = [
        '#ccd8ff',   // blue-white (O/B)
        '#e8eeff',   // white (A)
        '#fff4e8',   // yellow-white (F)
        '#ffe8c8',   // yellow (G)
        '#ffd4a0',   // orange (K)
        '#ffb080',   // red-orange (M)
        '#ffffff',   // pure white
    ];

    function starColor(mag) {
        if (mag < 0.05) return STAR_COLORS[Math.floor(Math.random() * 3)];   // hot
        if (mag < 0.4)  return STAR_COLORS[Math.floor(Math.random() * STAR_COLORS.length)];
        return STAR_COLORS[2 + Math.floor(Math.random() * 3)];   // cooler, dimmer
    }

    function init(canvasEl) {
        canvas = canvasEl;
        ctx    = canvas.getContext('2d');
        resize();
        window.addEventListener('resize', resize);
    }

    function resize() {
        const w = canvas.offsetWidth;
        const h = canvas.offsetHeight;
        canvas.width  = w;
        canvas.height = h;
        stars = Array.from({ length: STAR_COUNT }, () => makeStar(w, h));
        stars.forEach(s => { s.lum = s.baseLum; });
    }

    function drawFrame() {
        const w = canvas.width;
        const h = canvas.height;

        // Deep sky background — very subtle blue-black gradient
        const grad = ctx.createRadialGradient(w * 0.4, h * 0.35, 0, w * 0.5, h * 0.5, Math.max(w, h) * 0.8);
        grad.addColorStop(0,   '#050810');
        grad.addColorStop(0.5, '#030507');
        grad.addColorStop(1,   BG_COLOR);
        ctx.fillStyle = grad;
        ctx.fillRect(0, 0, w, h);

        // Faint Milky Way band — diagonal soft glow
        const band = ctx.createLinearGradient(0, h * 0.2, w, h * 0.8);
        band.addColorStop(0,    'rgba(20,25,40,0)');
        band.addColorStop(0.35, 'rgba(20,25,40,0.18)');
        band.addColorStop(0.5,  'rgba(25,30,50,0.28)');
        band.addColorStop(0.65, 'rgba(20,25,40,0.18)');
        band.addColorStop(1,    'rgba(20,25,40,0)');
        ctx.fillStyle = band;
        ctx.fillRect(0, 0, w, h);

        // Draw stars
        for (const s of stars) {
            // Twinkle
            if (Math.random() < TWINKLE_RATE) {
                s.dlum = (Math.random() - 0.5) * 0.02;
            }
            s.lum += s.dlum;
            if (s.lum > 1)           { s.lum = 1;          s.dlum *= -1; }
            if (s.lum < s.baseLum * 0.5) { s.lum = s.baseLum * 0.5; s.dlum *= -1; }

            ctx.save();

            if (s.diffuse) {
                // Bloom / diffraction spike for bright stars
                const grd = ctx.createRadialGradient(s.x, s.y, 0, s.x, s.y, s.r * 6);
                grd.addColorStop(0,   hexToRgba(s.color, s.lum));
                grd.addColorStop(0.3, hexToRgba(s.color, s.lum * 0.4));
                grd.addColorStop(1,   hexToRgba(s.color, 0));
                ctx.fillStyle = grd;
                ctx.beginPath();
                ctx.arc(s.x, s.y, s.r * 6, 0, Math.PI * 2);
                ctx.fill();

                // Core
                ctx.fillStyle = hexToRgba(s.color, Math.min(1, s.lum * 1.2));
                ctx.beginPath();
                ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
                ctx.fill();

                // Diffraction spikes
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

    function drawSpikes(s) {
        const len   = s.r * 10 + 6;
        const alpha = s.lum * 0.5;
        ctx.strokeStyle = hexToRgba(s.color, alpha);
        ctx.lineWidth   = 0.5;

        // Horizontal + vertical spikes
        for (const [dx, dy] of [[1,0],[-1,0],[0,1],[0,-1]]) {
            const grad = ctx.createLinearGradient(s.x, s.y, s.x + dx * len, s.y + dy * len);
            grad.addColorStop(0, hexToRgba(s.color, alpha));
            grad.addColorStop(1, hexToRgba(s.color, 0));
            ctx.strokeStyle = grad;
            ctx.beginPath();
            ctx.moveTo(s.x, s.y);
            ctx.lineTo(s.x + dx * len, s.y + dy * len);
            ctx.stroke();
        }
    }

    function hexToRgba(hex, alpha) {
        const r = parseInt(hex.slice(1, 3), 16);
        const g = parseInt(hex.slice(3, 5), 16);
        const b = parseInt(hex.slice(5, 7), 16);
        return `rgba(${r},${g},${b},${alpha.toFixed(3)})`;
    }

    function loop() {
        if (!running) return;
        drawFrame();
        animFrame = requestAnimationFrame(loop);
    }

    function start() {
        if (running) return;
        running = true;
        loop();
    }

    function stop() {
        running = false;
        if (animFrame) { cancelAnimationFrame(animFrame); animFrame = null; }
    }

    return { init, start, stop, resize };

})();
