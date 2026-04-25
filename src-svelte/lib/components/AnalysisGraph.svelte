<!-- AnalysisGraph.svelte — Analysis graph displayed in the viewer region. Spec §15 -->
<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { ui } from '../stores/ui';

    // ── Data types ────────────────────────────────────────────────────────────
    interface FrameData {
        index:                number;
        filename:             string;
        label:                string;
        short_name:           string;
        background_median?:   number;
        background_stddev?:   number;
        background_gradient?: number;
        highlight_clipping?:  number;
        snr_estimate?:        number;
        fwhm?:                number;
        eccentricity?:        number;
        star_count?:          number;
        score?:               number;
        flag:                 string;
    }

    interface MetricStats { mean: number; stddev: number; }

    interface AnalysisData {
        frames:        FrameData[];
        session_stats: Record<string, MetricStats>;
    }

    // ── Metrics ───────────────────────────────────────────────────────────────
    const METRICS = [
        { key: 'fwhm',                label: 'FWHM (px)',           fmt: (v: number) => v.toFixed(2) },
        { key: 'eccentricity',        label: 'Eccentricity',        fmt: (v: number) => v.toFixed(3) },
        { key: 'snr_estimate',        label: 'SNR Estimate',        fmt: (v: number) => v.toFixed(2) },
        { key: 'star_count',          label: 'Star Count',          fmt: (v: number) => Math.round(v).toString() },
        { key: 'background_median',   label: 'Background Median',   fmt: (v: number) => v.toFixed(4) },
        { key: 'background_stddev',   label: 'Background Std Dev',  fmt: (v: number) => v.toFixed(4) },
        { key: 'background_gradient', label: 'Background Gradient', fmt: (v: number) => v.toFixed(4) },
        { key: 'highlight_clipping',  label: 'Highlight Clipping',  fmt: (v: number) => (v * 100).toFixed(4) + '%' },
        { key: 'score',               label: 'PXSCORE',             fmt: (v: number) => Math.round(v).toString() },
    ];

    function metricDef(key: string) {
        return METRICS.find(m => m.key === key) ?? METRICS[0];
    }

    // ── State ─────────────────────────────────────────────────────────────────
    let data    = $state<AnalysisData | null>(null);
    let loading = $state(false);
    let error   = $state('');
    let metric1 = $state('fwhm');
    let metric2 = $state('eccentricity');
    let canvas  = $state<HTMLCanvasElement>();
    let wrap    = $state<HTMLDivElement>();
    let tooltip = $state<{ x: number; y: number; text: string } | null>(null);

    // ── Load data ─────────────────────────────────────────────────────────────
    async function loadData() {
        loading = true;
        error = '';
        try {
            data = await invoke<AnalysisData>('get_analysis_results');
        } catch (e) {
            error = `${e}`;
        } finally {
            loading = false;
        }
    }

    // Load when first shown
    $effect(() => {
        if ($ui.showAnalysisGraph && !data) loadData();
    });

    // Redraw when data or metrics change
    $effect(() => {
        const _d  = data;
        const _m1 = metric1;
        const _m2 = metric2;
        if ($ui.showAnalysisGraph && canvas && data) {
            requestAnimationFrame(() => drawChart());
        }
    });

    // ── Resize ────────────────────────────────────────────────────────────────
    function resizeCanvas() {
        if (!canvas || !wrap) return;
        canvas.width  = wrap.clientWidth;
        canvas.height = wrap.clientHeight;
        if (data) drawChart();
    }

    onMount(() => {
        window.addEventListener('resize', resizeCanvas);
        setTimeout(resizeCanvas, 50);
    });

    onDestroy(() => {
        window.removeEventListener('resize', resizeCanvas);
    });

    $effect(() => {
        if ($ui.showAnalysisGraph) setTimeout(resizeCanvas, 50);
    });

    // ── Chart ─────────────────────────────────────────────────────────────────
    function getVal(f: FrameData, key: string): number | undefined {
        return (f as any)[key];
    }

    function drawChart() {
        if (!canvas || !data || data.frames.length === 0) return;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;

        const W = canvas.width;
        const H = canvas.height;
        const PL = 72, PR = metric2 !== 'none' ? 72 : 24, PT = 30, PB = 50;
        const CW = W - PL - PR;
        const CH = H - PT - PB;
        if (CW <= 0 || CH <= 0) return;

        const frames = data.frames;
        const n = frames.length;

        ctx.clearRect(0, 0, W, H);
        ctx.fillStyle = '#000d00';
        ctx.fillRect(0, 0, W, H);

        const m1def = metricDef(metric1);
        const m2def = metric2 !== 'none' ? metricDef(metric2) : null;

        const m1vals = frames.map(f => getVal(f, metric1));
        const m2vals = m2def ? frames.map(f => getVal(f, metric2)) : [];
        const m1valid = m1vals.filter(v => v !== undefined) as number[];
        const m2valid = m2vals.filter(v => v !== undefined) as number[];

        const calcRange = (vals: number[]) => {
            if (!vals.length) return { lo: 0, hi: 1 };
            const mn = Math.min(...vals), mx = Math.max(...vals);
            const pad = (mx - mn) * 0.15 || Math.abs(mn) * 0.1 || 0.1;
            return { lo: mn - pad, hi: mx + pad };
        };

        const r1 = calcRange(m1valid);
        const r2 = calcRange(m2valid);

        const toX  = (i: number) => n === 1 ? PL + CW / 2 : PL + (i / (n - 1)) * CW;
        const toY1 = (v: number) => PT + CH - ((v - r1.lo) / (r1.hi - r1.lo)) * CH;
        const toY2 = (v: number) => PT + CH - ((v - r2.lo) / (r2.hi - r2.lo)) * CH;

        // Sigma bands (metric 1 only)
        const stats1 = data.session_stats[metric1];
        if (stats1 && stats1.stddev > 0) {
            [[3, 0.20], [2, 0.13], [1, 0.07]].forEach(([sigma, alpha]) => {
                const blo = Math.max(stats1.mean - sigma * stats1.stddev, r1.lo);
                const bhi = Math.min(stats1.mean + sigma * stats1.stddev, r1.hi);
                const y1 = toY1(bhi), y2 = toY1(blo);
                ctx.fillStyle = `rgba(0,180,70,${alpha})`;
                ctx.fillRect(PL, y1, CW, y2 - y1);
            });
        }

        // Grid lines
        ctx.strokeStyle = '#0d200d';
        ctx.lineWidth = 1;
        const ticks = 5;
        for (let t = 0; t <= ticks; t++) {
            const y = PT + (t / ticks) * CH;
            ctx.beginPath(); ctx.moveTo(PL, y); ctx.lineTo(PL + CW, y); ctx.stroke();
        }

        // Mean line (metric 1)
        if (stats1 && stats1.mean >= r1.lo && stats1.mean <= r1.hi) {
            const my = toY1(stats1.mean);
            ctx.strokeStyle = 'rgba(0,255,80,0.45)';
            ctx.lineWidth = 1;
            ctx.setLineDash([4, 4]);
            ctx.beginPath(); ctx.moveTo(PL, my); ctx.lineTo(PL + CW, my); ctx.stroke();
            ctx.setLineDash([]);
        }

        // Chart border
        ctx.strokeStyle = '#1a3a1a';
        ctx.lineWidth = 1;
        ctx.strokeRect(PL, PT, CW, CH);

        // Left Y labels
        ctx.fillStyle = '#00cc44';
        ctx.font = '11px monospace';
        ctx.textAlign = 'right';
        for (let t = 0; t <= ticks; t++) {
            const v = r1.lo + (t / ticks) * (r1.hi - r1.lo);
            ctx.fillText(m1def.fmt(v), PL - 6, toY1(v) + 4);
        }

        // Right Y labels
        if (m2def && m2valid.length) {
            ctx.fillStyle = '#00cccc';
            ctx.textAlign = 'left';
            for (let t = 0; t <= ticks; t++) {
                const v = r2.lo + (t / ticks) * (r2.hi - r2.lo);
                ctx.fillText(m2def.fmt(v), PL + CW + 6, toY2(v) + 4);
            }
        }

        // X labels
        ctx.fillStyle = '#5a8a5a';
        ctx.textAlign = 'center';
        ctx.font = '11px monospace';
        const maxLabels = Math.floor(CW / 30);
        const step = Math.max(1, Math.ceil(n / maxLabels));
        for (let i = 0; i < n; i += step) {
            ctx.fillText(frames[i].label, toX(i), PT + CH + 16);
        }

        // X title
        ctx.fillStyle = '#3a6a3a';
        ctx.font = '11px monospace';
        ctx.textAlign = 'center';
        ctx.fillText('Frame', PL + CW / 2, PT + CH + 34);

        // Left axis title
        ctx.save();
        ctx.font = '12px monospace';
        ctx.fillStyle = '#00cc44';
        ctx.translate(14, PT + CH / 2);
        ctx.rotate(-Math.PI / 2);
        ctx.textAlign = 'center';
        ctx.fillText(m1def.label, 0, 0);
        ctx.restore();

        // Right axis title
        if (m2def) {
            ctx.save();
            ctx.font = '12px monospace';
            ctx.fillStyle = '#00cccc';
            ctx.translate(W - 14, PT + CH / 2);
            ctx.rotate(Math.PI / 2);
            ctx.textAlign = 'center';
            ctx.fillText(m2def.label, 0, 0);
            ctx.restore();
        }

        // Metric 1 line
        ctx.strokeStyle = '#00ff44';
        ctx.lineWidth = 1.5;
        ctx.beginPath();
        let started = false;
        for (let i = 0; i < n; i++) {
            const v = m1vals[i];
            if (v === undefined) { started = false; continue; }
            started ? ctx.lineTo(toX(i), toY1(v)) : ctx.moveTo(toX(i), toY1(v));
            started = true;
        }
        ctx.stroke();

        // Metric 2 line
        if (m2def) {
            ctx.strokeStyle = '#00cccc';
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            started = false;
            for (let i = 0; i < n; i++) {
                const v = m2vals[i];
                if (v === undefined) { started = false; continue; }
                started ? ctx.lineTo(toX(i), toY2(v)) : ctx.moveTo(toX(i), toY2(v));
                started = true;
            }
            ctx.stroke();
        }

        // Dots
        for (let i = 0; i < n; i++) {
            const f = frames[i];
            const x = toX(i);
            const v1 = m1vals[i];
            if (v1 !== undefined) drawDot(ctx, x, toY1(v1), f.flag);
            if (m2def) {
                const v2 = m2vals[i];
                if (v2 !== undefined) drawDot(ctx, x, toY2(v2), f.flag);
            }
        }
    }

    function drawDot(ctx: CanvasRenderingContext2D, x: number, y: number, flag: string) {
        const r = 4;
        ctx.beginPath();
        ctx.arc(x, y, r, 0, Math.PI * 2);
        ctx.fillStyle = flag === 'REJECT' ? '#ff3030' : flag === 'SUSPECT' ? '#ffdd00' : '#ffffff';
        ctx.fill();
        if (flag === 'REJECT') {
            ctx.strokeStyle = '#ff2020';
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.moveTo(x - r, y - r); ctx.lineTo(x + r, y + r);
            ctx.moveTo(x + r, y - r); ctx.lineTo(x - r, y + r);
            ctx.stroke();
        }
    }

    // ── Hit test ──────────────────────────────────────────────────────────────
    function hitTest(e: MouseEvent): { frame: FrameData; which: 1 | 2 } | null {
        if (!canvas || !data || data.frames.length === 0) return null;
        const rect = canvas.getBoundingClientRect();
        const mx = (e.clientX - rect.left) * (canvas.width  / rect.width);
        const my = (e.clientY - rect.top)  * (canvas.height / rect.height);

        const frames = data.frames;
        const n = frames.length;
        const PL = 72, PR = metric2 !== 'none' ? 72 : 24, PT = 30, PB = 50;
        const CW = canvas.width - PL - PR;
        const CH = canvas.height - PT - PB;

        let ci = -1, cd = Infinity;
        for (let i = 0; i < n; i++) {
            const x = n === 1 ? PL + CW / 2 : PL + (i / (n - 1)) * CW;
            const d = Math.abs(mx - x);
            if (d < cd) { cd = d; ci = i; }
        }
        if (ci === -1 || cd > 20) return null;

        const f = frames[ci];
        const m1vals = frames.map(fr => getVal(fr, metric1));
        const m2vals = metric2 !== 'none' ? frames.map(fr => getVal(fr, metric2)) : [];
        const m1valid = m1vals.filter(v => v !== undefined) as number[];
        const m2valid = m2vals.filter(v => v !== undefined) as number[];

        const rng = (vals: number[]) => {
            if (!vals.length) return { lo: 0, hi: 1 };
            const mn = Math.min(...vals), mx2 = Math.max(...vals);
            const pad = (mx2 - mn) * 0.15 || 0.1;
            return { lo: mn - pad, hi: mx2 + pad };
        };
        const r1 = rng(m1valid);
        const r2 = rng(m2valid);
        const toY1 = (v: number) => PT + CH - ((v - r1.lo) / (r1.hi - r1.lo)) * CH;
        const toY2 = (v: number) => PT + CH - ((v - r2.lo) / (r2.hi - r2.lo)) * CH;

        const v1 = getVal(f, metric1);
        const v2 = metric2 !== 'none' ? getVal(f, metric2) : undefined;
        const d1 = v1 !== undefined ? Math.abs(my - toY1(v1)) : Infinity;
        const d2 = v2 !== undefined ? Math.abs(my - toY2(v2)) : Infinity;

        return { frame: f, which: d1 <= d2 ? 1 : 2 };
    }

    function onMouseMove(e: MouseEvent) {
        const hit = hitTest(e);
        if (!hit) { tooltip = null; return; }
        const { frame, which } = hit;
        const key  = which === 1 ? metric1 : metric2;
        const mdef = metricDef(key);
        const val  = getVal(frame, key);
        const rect = canvas!.getBoundingClientRect();
        tooltip = {
            x: e.clientX - rect.left + 14,
            y: e.clientY - rect.top  - 36,
            text: `${mdef.label}: ${val !== undefined ? mdef.fmt(val) : 'n/a'}  [${frame.flag || '—'}]  ${frame.short_name}`,
        };
    }

    function onMouseLeave() { tooltip = null; }

    async function onClick(e: MouseEvent) {
        const hit = hitTest(e);
        if (!hit) return;
        try {
            await invoke('dispatch_command', {
                request: { command: 'SetFrame', args: { index: hit.frame.index.toString() } }
            });
            ui.setShowAnalysisGraph(false);
        } catch {}
    }
</script>

<div id="ag-root">
    <div id="ag-toolbar">
        <span id="ag-title">Analysis Graph</span>

        <label class="ag-label">Metric 1</label>
        <select class="ag-select ag-m1" bind:value={metric1}>
            {#each METRICS as m}
                <option value={m.key}>{m.label}</option>
            {/each}
        </select>

        <label class="ag-label">Metric 2</label>
        <select class="ag-select ag-m2" bind:value={metric2}>
            <option value="none">None</option>
            {#each METRICS as m}
                <option value={m.key}>{m.label}</option>
            {/each}
        </select>

        <button class="ag-btn" onclick={loadData}>↻ Refresh</button>
        <button class="ag-btn ag-close" onclick={() => ui.setShowAnalysisGraph(false)}>✕ Close</button>
    </div>

    <div id="ag-canvas-wrap" bind:this={wrap}>
        {#if loading}
            <div class="ag-status">Loading…</div>
        {:else if error}
            <div class="ag-status ag-error">{error}</div>
        {:else if !data || data.frames.length === 0}
            <div class="ag-status">No data — run AnalyzeFrames first.</div>
        {:else}
            <canvas
                id="ag-canvas"
                bind:this={canvas}
                onmousemove={onMouseMove}
                onmouseleave={onMouseLeave}
                onclick={onClick}
            ></canvas>
            {#if tooltip}
                <div
                    class="ag-tooltip"
                    style:left="{tooltip.x}px"
                    style:top="{tooltip.y}px"
                >{tooltip.text}</div>
            {/if}
        {/if}
    </div>
</div>
