w<!-- AnalysisGraph.svelte — Analysis graph displayed in the viewer region. Spec §15 -->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { ui } from '../stores/ui';
  import { displayFrame } from '../commands';
  import Dropdown from './Dropdown.svelte';

  // ── Data types ────────────────────────────────────────────────────────────
  interface FrameData {
    index:                number;
    filename:             string;
    label:                string;
    short_name:           string;
    background_median?:   number;
    background_stddev?:   number;
    background_gradient?: number;
    snr_estimate?:        number;
    fwhm?:                number;
    eccentricity?:        number;
    star_count?:          number;
    flag:                 string;
    triggered?:           string[];
  }

  interface MetricStats { mean: number; stddev: number; }

  interface AppliedThreshold {
    value:     number;
    direction: 'high' | 'low';
  }

  interface AnalysisData {
    frames:              FrameData[];
    session_stats:       Record<string, MetricStats>;
    applied_thresholds:  Record<string, AppliedThreshold> | null;
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
  ];

  function metricDef(key: string) {
    return METRICS.find(m => m.key === key) ?? METRICS[0];
  }

  // ── Reject threshold lookup ───────────────────────────────────────────────
  // Eccentricity is absolute; all others are sigma-based.
  const ABSOLUTE_METRICS = new Set(['eccentricity']);

  function getRejectThresholds(d: AnalysisData | null): Record<string, { type: 'sigma' | 'absolute'; value: number; direction: 'high' | 'low' }> | null {
    const applied = d?.applied_thresholds;
    if (!applied) return null;
    return Object.fromEntries(
      Object.entries(applied).map(([key, t]) => [
        key,
        {
          type:      ABSOLUTE_METRICS.has(key) ? 'absolute' : 'sigma',
          value:     t.value,
          direction: t.direction,
        }
      ])
    );
  }

  // ── State ─────────────────────────────────────────────────────────────────
  let data    = $state<AnalysisData | null>(null);
  let loading = $state(false);
  let error   = $state('');
  let metric1 = $state('fwhm');
  let metric2 = $state('none');
  let canvas  = $state<HTMLCanvasElement>();
  let wrap    = $state<HTMLDivElement>();

  interface TooltipState {
    x:      number;
    y:      number;
    line1:  string;
    line2:  string;
    region: 'left' | 'center' | 'right';
  }
  let tooltip = $state<TooltipState | null>(null);

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
      setTimeout(resizeCanvas, 10);
    }
  }

  $effect(() => {
    if ($ui.activeView === 'analysisGraph' && !data) loadData();
  });

  $effect(() => {
    const _d  = data;
    const _m1 = metric1;
    const _m2 = metric2;
    if ($ui.activeView === 'analysisGraph' && canvas && data) {
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
    if ($ui.activeView === 'analysisGraph') setTimeout(resizeCanvas, 50);
  });

  // ── Chart ─────────────────────────────────────────────────────────────────
  function getVal(f: FrameData, key: string): number | undefined {
    return (f as any)[key];
  }

  // Read theme colors from CSS variables at draw time
  function getThemeColors(el: HTMLCanvasElement) {
    const s = getComputedStyle(el);
    const get = (v: string) => s.getPropertyValue(v).trim() || null;
    return {
      bg:          get('--bg-color')         ?? '#000000',
      cardBg:      get('--card-bg')           ?? '#001100',
      border:      get('--border-color')      ?? '#004400',
      borderLight: get('--border-color-light') ?? '#002200',
      primary:     get('--primary-color')     ?? '#00ff00',
      secondary:   get('--text-secondary')    ?? '#00aa00',
      warning:     get('--warning-color')     ?? '#ffaa00',
      error:       get('--error-color')       ?? '#ff0000',
    };
  }

  function rejectThresholdY(
    key:    string,
    stats:  MetricStats,
    lo:     number,
    hi:     number,
    PT:     number,
    CH:     number,
    thresholds: ReturnType<typeof getRejectThresholds>,
  ): number | null {
    if (!thresholds) return null;
    const thresh = thresholds[key];
    if (!thresh || !stats || stats.stddev === 0) return null;

    let threshVal: number;
    if (thresh.type === 'sigma') {
      threshVal = thresh.direction === 'high'
        ? stats.mean + thresh.value * stats.stddev
        : stats.mean - thresh.value * stats.stddev;
    } else {
      threshVal = thresh.value;
    }

    return PT + CH - ((threshVal - lo) / (hi - lo)) * CH;
  }

function drawRejectLine(
    ctx:       CanvasRenderingContext2D,
    y:         number,
    PL:        number,
    CW:        number,
    color:     string,
    lineWidth: number,
    labelSide: 'left' | 'right',
    fontSize:  number,
  ) {
    // Black border lines either side
    ctx.strokeStyle = 'rgba(0,0,0,0.85)';
    ctx.lineWidth = lineWidth > 2 ? 2 : 1;
    ctx.setLineDash([]);
    const offset = lineWidth > 2 ? 3 : 2;
    ctx.beginPath(); ctx.moveTo(PL, y - offset); ctx.lineTo(PL + CW, y - offset); ctx.stroke();
    ctx.beginPath(); ctx.moveTo(PL, y + offset); ctx.lineTo(PL + CW, y + offset); ctx.stroke();

    // Main dotted reject line
    ctx.strokeStyle = color;
    ctx.lineWidth = lineWidth;
    ctx.setLineDash([6, 3]);
    ctx.beginPath(); ctx.moveTo(PL, y); ctx.lineTo(PL + CW, y); ctx.stroke();
    ctx.setLineDash([]);

    // REJECT label with semi-opaque black background
    ctx.font = `${fontSize}px monospace`;
    const labelText = 'REJECT';
    const textW = ctx.measureText(labelText).width;
    const labelX = labelSide === 'left' ? PL + 4 : PL + CW - 4;
    const labelY = y - (fontSize < 14 ? 8 : 10);
    ctx.textAlign = labelSide;
    const rectX = labelSide === 'left' ? labelX - 2 : labelX - textW - 2;
    ctx.fillStyle = 'rgba(0,0,0,0.60)';
    ctx.fillRect(rectX, labelY - fontSize, textW + 6, fontSize + 4);
    ctx.fillStyle = color;
    ctx.fillText(labelText, labelX, labelY);
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
    const C = getThemeColors(canvas);

    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = C.bg;
    ctx.fillRect(0, 0, W, H);

    const m1def = metricDef(metric1);
    const m2def = metric2 !== 'none' ? metricDef(metric2) : null;

    const m1vals = frames.map(f => getVal(f, metric1));
    const m2vals = m2def ? frames.map(f => getVal(f, metric2)) : [];
    const m1valid = m1vals.filter(v => v !== undefined) as number[];
    const m2valid = m2vals.filter(v => v !== undefined) as number[];

    const stats1 = data.session_stats[metric1];

    const calcRange = (vals: number[], extraVal?: number) => {
      if (!vals.length) return { lo: 0, hi: 1 };
      const allVals = extraVal !== undefined ? [...vals, extraVal] : vals;
      const mn = Math.min(...allVals), mx = Math.max(...allVals);
      const pad = (mx - mn) * 0.15 || Math.abs(mn) * 0.1 || 0.1;
      return { lo: mn - pad, hi: mx + pad };
    };

    const thresholds = getRejectThresholds(data);

    // Calculate reject threshold value for metric1 so axis always includes it
    const thresh1 = thresholds?.[metric1];
    let rejectVal: number | undefined;
    if (stats1 && thresh1) {
      if (thresh1.type === 'sigma') {
        rejectVal = thresh1.direction === 'high'
          ? stats1.mean + thresh1.value * stats1.stddev
          : stats1.mean - thresh1.value * stats1.stddev;
      } else {
        rejectVal = thresh1.value;
      }
    }

    const r1 = calcRange(m1valid, rejectVal);

    // Calculate reject threshold value for metric2 so right axis always includes it
    let rejectVal2: number | undefined;
    if (m2def && thresholds) {
      const stats2 = data.session_stats[metric2];
      const thresh2 = thresholds[metric2];
      if (stats2 && thresh2) {
        if (thresh2.type === 'sigma') {
          rejectVal2 = thresh2.direction === 'high'
            ? stats2.mean + thresh2.value * stats2.stddev
            : stats2.mean - thresh2.value * stats2.stddev;
        } else {
          rejectVal2 = thresh2.value;
        }
      }
    }

    const r2 = metric1 === metric2 ? r1 : calcRange(m2valid, rejectVal2);

    const toX  = (i: number) => n === 1 ? PL + CW / 2 : PL + (i / (n - 1)) * CW;
    const toY1 = (v: number) => PT + CH - ((v - r1.lo) / (r1.hi - r1.lo)) * CH;
    const toY2 = (v: number) => PT + CH - ((v - r2.lo) / (r2.hi - r2.lo)) * CH;

    // Sigma bands (metric 1 only)
    if (stats1 && stats1.stddev > 0) {
      // Parse primary color for sigma band tinting
      [[3, 0.20], [2, 0.13], [1, 0.07]].forEach(([sigma, alpha]) => {
        const blo = Math.max(stats1.mean - sigma * stats1.stddev, r1.lo);
        const bhi = Math.min(stats1.mean + sigma * stats1.stddev, r1.hi);
        const y1 = toY1(bhi), y2 = toY1(blo);
        ctx.fillStyle = C.primary + Math.round((alpha as number) * 255).toString(16).padStart(2, '0');
        ctx.fillRect(PL, y1, CW, y2 - y1);
      });
    }

    // Grid lines
    ctx.strokeStyle = C.borderLight;
    ctx.lineWidth = 1;
    const ticks = 5;
    for (let t = 0; t <= ticks; t++) {
      const y = PT + (t / ticks) * CH;
      ctx.beginPath(); ctx.moveTo(PL, y); ctx.lineTo(PL + CW, y); ctx.stroke();
    }

    // Mean line (metric 1)
    if (stats1 && stats1.mean >= r1.lo && stats1.mean <= r1.hi) {
      const my = toY1(stats1.mean);
      ctx.strokeStyle = C.primary + '73'; // ~45% opacity
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 4]);
      ctx.beginPath(); ctx.moveTo(PL, my); ctx.lineTo(PL + CW, my); ctx.stroke();
      ctx.setLineDash([]);
    }

    // Reject threshold line (metric 1)
    const rejY = rejectThresholdY(metric1, stats1, r1.lo, r1.hi, PT, CH, thresholds);
    if (rejY !== null) {
      drawRejectLine(ctx, rejY, PL, CW, 'rgba(255,60,60,0.75)', 4, 'left', 15);
    }

    // Reject threshold line (metric 2)
    if (m2def && m2valid.length) {
      const stats2 = data.session_stats[metric2];
      const rejY2 = rejectThresholdY(metric2, stats2, r2.lo, r2.hi, PT, CH, thresholds);
      if (rejY2 !== null) {
        drawRejectLine(ctx, rejY2, PL, CW, C.warning, 2, 'right', 13);
      }
    }

    // Chart border
    ctx.strokeStyle = C.border;
    ctx.lineWidth = 1;
    ctx.strokeRect(PL, PT, CW, CH);

    // Left Y labels
    ctx.fillStyle = C.secondary;
    ctx.font = '11px monospace';
    ctx.textAlign = 'right';
    for (let t = 0; t <= ticks; t++) {
      const v = r1.lo + (t / ticks) * (r1.hi - r1.lo);
      ctx.fillText(m1def.fmt(v), PL - 6, toY1(v) + 4);
    }

    // Right Y labels
    if (m2def && m2valid.length) {
      ctx.fillStyle = C.warning;
      ctx.textAlign = 'left';
      for (let t = 0; t <= ticks; t++) {
        const v = r2.lo + (t / ticks) * (r2.hi - r2.lo);
        ctx.fillText(m2def.fmt(v), PL + CW + 6, toY2(v) + 4);
      }
    }

    // X labels
    ctx.fillStyle = C.secondary;
    ctx.textAlign = 'center';
    ctx.font = '11px monospace';
    const maxLabels = Math.floor(CW / 30);
    const step = Math.max(1, Math.ceil(n / maxLabels));
    for (let i = 0; i < n; i += step) {
      ctx.fillText(frames[i].label, toX(i), PT + CH + 16);
    }

    // X title
    ctx.fillStyle = C.secondary;
    ctx.font = '11px monospace';
    ctx.textAlign = 'center';
    ctx.fillText('Frame', PL + CW / 2, PT + CH + 34);

    // Left axis title
    ctx.save();
    ctx.font = '12px monospace';
    ctx.fillStyle = C.secondary;
    ctx.translate(14, PT + CH / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.textAlign = 'center';
    ctx.fillText(m1def.label, 0, 0);
    ctx.restore();

    // Right axis title
    if (m2def) {
      ctx.save();
      ctx.font = '12px monospace';
      ctx.fillStyle = C.warning;
      ctx.translate(W - 14, PT + CH / 2);
      ctx.rotate(Math.PI / 2);
      ctx.textAlign = 'center';
      ctx.fillText(m2def.label, 0, 0);
      ctx.restore();
    }

    // Metric 1 line (solid)
    ctx.strokeStyle = C.primary;
    ctx.lineWidth = 2.5;
    ctx.setLineDash([]);
    ctx.beginPath();
    let started = false;
    for (let i = 0; i < n; i++) {
      const v = m1vals[i];
      if (v === undefined) { started = false; continue; }
      started ? ctx.lineTo(toX(i), toY1(v)) : ctx.moveTo(toX(i), toY1(v));
      started = true;
    }
    ctx.stroke();

    // Metric 2 line (dotted)
    if (m2def) {
      ctx.strokeStyle = C.warning;
      ctx.lineWidth = 1.0;
      ctx.setLineDash([4, 4]);
      ctx.beginPath();
      started = false;
      for (let i = 0; i < n; i++) {
        const v = m2vals[i];
        if (v === undefined) { started = false; continue; }
        started ? ctx.lineTo(toX(i), toY2(v)) : ctx.moveTo(toX(i), toY2(v));
        started = true;
      }
      ctx.stroke();
      ctx.setLineDash([]);
    }

    // Dots — metric 1 only
    for (let i = 0; i < n; i++) {
      const f = frames[i];
      const x = toX(i);
      const v1 = m1vals[i];
      if (v1 !== undefined) drawDot(ctx, x, toY1(v1), f.flag);
    }
  }

  function drawDot(ctx: CanvasRenderingContext2D, x: number, y: number, flag: string) {
    const r = flag === 'REJECT' ? 8 : 4;
    ctx.fillStyle = flag === 'REJECT' ? '#ff3030' : '#ffffff';
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fill();
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

    // Line 1: metric value + flag + triggered metrics
    let flagStr = frame.flag || '—';
    if (frame.flag === 'REJECT' && frame.triggered && frame.triggered.length > 0) {
      flagStr += ` (${frame.triggered.join(', ')})`;
    }
    const line1 = `${mdef.label}: ${val !== undefined ? mdef.fmt(val) : 'n/a'}  |  ${flagStr}`;
    const line2 = frame.short_name;

    // Determine tooltip position region
    const pct = (e.clientX - rect.left) / rect.width;
    const region: 'left' | 'center' | 'right' =
          pct < 0.33 ? 'left' : pct > 0.66 ? 'right' : 'center';

    const tx = e.clientX - rect.left;
    const ty = e.clientY - rect.top - 56;

    tooltip = { x: tx, y: ty, line1, line2, region };
  }

  function onMouseLeave() { tooltip = null; }

  async function onClick(e: MouseEvent) {
    const hit = hitTest(e);
    if (!hit) return;
    ui.showView(null);
    await displayFrame(hit.frame.index);
  }
</script>

<div id="ag-root">
  <div id="ag-toolbar">
    <span id="ag-title">Analysis Graph</span>

    <label class="ag-label">Metric 1</label>
    <Dropdown
      className="ag-m1"
      value={metric1}
      openUp={false}
      options={METRICS.map(m => ({ value: m.key, label: m.label }))}
      on:change={(e) => { metric1 = e.detail; }}
      />
      <label class="ag-label">Metric 2</label>
      <Dropdown
        className="ag-m2"
        value={metric2}
        options={[{ value: 'none', label: 'None' }, ...METRICS.map(m => ({ value: m.key, label: m.label }))]}
        on:change={(e) => { metric2 = e.detail; }}
        />

        <button class="ag-btn" onclick={loadData}>↻ Refresh</button>
        <button class="ag-btn ag-close" onclick={() => ui.showView(null)}>✕ Close</button>
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
          class:ag-tooltip-left={tooltip.region === 'left'}
          class:ag-tooltip-center={tooltip.region === 'center'}
          class:ag-tooltip-right={tooltip.region === 'right'}
          style:left="{tooltip.x}px"
          style:top="{tooltip.y}px"
          >
          <div class="ag-tooltip-line1">{tooltip.line1}</div>
          <div class="ag-tooltip-line2">{tooltip.line2}</div>
        </div>
      {/if}
    {/if}
  </div>
</div>
