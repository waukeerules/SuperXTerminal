<!--
  The server load graph: the tab in front's last 15 minutes of readings (serverLoad.svelte.ts), opened by clicking
  the figures in the status bar, over the window's bottom right. It goes on drawing as readings come.

  Two charts on one time axis rather than one with two scales: "Use" (CPU, memory and the root disk, all percent of
  0 to 100) and "Network" (in and out, bytes a second, its top rounded up). 2px lines that break where no readings
  were taken; a legend, and each line's latest value at its end, so no line is told by colour alone; a crosshair
  that follows the pointer (or the arrow keys) with every figure at that moment; and Table, the same readings as
  rows. Colours are the reference data-viz palette's first three, checked against the window's own pane colours
  (light #ffffff, dark #252526): the light aqua is under 3:1 there, which the end labels and Table make up for.
  Escape, a click elsewhere or × closes it.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { isDark } from "../settings/store.svelte";
  import { rate, type Sample } from "./serverLoad.svelte";
  import { ago, axisRate, linePath, nearest, niceMax, spread, timeSpan, type Measure } from "./loadChart";

  interface Props {
    /** The readings, oldest first */
    samples: Sample[];
    /** The tab they're the server of, for the title */
    tab: string;
    /** Close it */
    onClose: () => void;
  }

  let { samples, tab, onClose }: Props = $props();

  /** The categorical palette's first three hues, stepped for the light or dark pane they're drawn on */
  const COLOURS = {
    light: { cpu: "#2a78d6", mem: "#eb6834", disk: "#1baf7a" },
    dark: { cpu: "#3987e5", mem: "#d95926", disk: "#199e70" },
  };
  const colours = $derived(isDark() ? COLOURS.dark : COLOURS.light);
  /** Network in and out take the first two hues, in a chart of their own with its own legend */
  const lines = $derived({
    use: [
      { measure: "cpu" as Measure, name: "CPU", colour: colours.cpu },
      { measure: "mem" as Measure, name: "Memory", colour: colours.mem },
      { measure: "disk" as Measure, name: "Disk", colour: colours.disk },
    ],
    net: [
      { measure: "down" as Measure, name: "In ↓", colour: colours.cpu },
      { measure: "up" as Measure, name: "Out ↑", colour: colours.mem },
    ],
  });

  /** The charts' size in the SVG's own units: room on the left for the axis, on the right for the end labels */
  const WIDTH = 560;
  const HEIGHT = 128;
  const LEFT = 58;
  /** Wide enough for the longest end label, "Out ↑ 999.9 KB/s" */
  const RIGHT = 128;
  const TOP = 10;
  const BOTTOM = 20;
  const plotRight = WIDTH - RIGHT;
  const plotBottom = HEIGHT - BOTTOM;

  /** Now, moved on each second while open, so the time axis keeps up between readings */
  let now = $state(Date.now());
  const ticker = setInterval(() => (now = Date.now()), 1000);
  onDestroy(() => clearInterval(ticker));

  const span = $derived(timeSpan(samples, now));
  /** A time's place across the charts */
  const x = (at: number) => LEFT + ((at - span[0]) / Math.max(1, span[1] - span[0])) * (plotRight - LEFT);
  /** Where a percent goes up a chart */
  const yPercent = (value: number) => plotBottom - (Math.min(100, Math.max(0, value)) / 100) * (plotBottom - TOP);
  /** The network chart's top: the busiest moment shown, rounded up */
  const netTop = $derived(niceMax(Math.max(0, ...samples.flatMap((s) => [s.down ?? 0, s.up ?? 0]))));
  const yRate = (value: number) => plotBottom - (Math.max(0, value) / netTop) * (plotBottom - TOP);

  /** The time axis' marks: every 5 minutes back from now, as far as the readings go, and now */
  const timeTicks = $derived.by(() => {
    const ticks = [];
    for (let back = 0; now - back >= span[0]; back += 5 * 60_000) ticks.push(now - back);
    return ticks;
  });

  /** The latest reading of a measure, for its end label */
  function latest(measure: Measure): Sample | undefined {
    return [...samples].reverse().find((s) => s[measure] !== null);
  }

  /** Each line's end label: its latest value, beside the line's end, spread so labels don't overlap */
  function endLabels(set: { measure: Measure; name: string; colour: string }[], y: (v: number) => number, format: (v: number) => string) {
    const found = set.map((line) => ({ line, sample: latest(line.measure) })).filter((f) => f.sample);
    const heights = spread(found.map((f) => y(f.sample![f.line.measure] as number)), 13, TOP + 4, plotBottom);
    return found.map((f, i) => ({ ...f.line, y: heights[i], text: `${f.line.name} ${format(f.sample![f.line.measure] as number)}` }));
  }
  const useLabels = $derived(endLabels(lines.use, yPercent, (v) => `${Math.round(v)}%`));
  const netLabels = $derived(endLabels(lines.net, yRate, rate));

  /** The reading the crosshair is on: the pointer's or the arrow keys'; -1 for none */
  let pointed = $state(-1);
  const pointedSample = $derived(pointed >= 0 ? samples[pointed] : undefined);

  /** The pointer over a chart: the crosshair to the reading nearest it in time */
  function point(event: PointerEvent): void {
    const box = (event.currentTarget as SVGSVGElement).getBoundingClientRect();
    if (!box.width) return;
    const unit = ((event.clientX - box.left) / box.width) * WIDTH;
    const at = span[0] + ((unit - LEFT) / (plotRight - LEFT)) * (span[1] - span[0]);
    pointed = nearest(samples, at);
  }

  /** The arrow keys move the crosshair a reading at a time; Home and End go to the oldest and latest */
  function key(event: KeyboardEvent): void {
    const last = samples.length - 1;
    const from = pointed >= 0 ? pointed : last;
    const to = { ArrowLeft: from - 1, ArrowRight: from + 1, Home: 0, End: last }[event.key];
    if (to === undefined || last < 0) return;
    event.preventDefault();
    pointed = Math.max(0, Math.min(last, to));
  }

  /** A reading's figures in a chart's lines, said: "5 min ago: CPU 12%, Memory 16%, Disk 50%" */
  function readoutText(sample: Sample | undefined, set: { measure: Measure; name: string }[]): string {
    if (!sample) return "no readings";
    const figure = (m: Measure) => (m === "down" || m === "up" ? bytes(sample[m]) : percent(sample[m]));
    return `${ago(sample.at, now)}: ${set.map((l) => `${l.name} ${figure(l.measure)}`).join(", ")}`;
  }

  /** Whether the readings show as rows (Table) rather than charts */
  let table = $state(false);

  /** A figure for the readout and Table: "—" for one the reading hasn't */
  const percent = (v: number | null) => (v === null ? "—" : `${Math.round(v)}%`);
  const bytes = (v: number | null) => (v === null ? "—" : rate(v));

  // Escape, or a click outside the panel and the status bar's figures that opened it, closes it
  let panel: HTMLElement | undefined = $state();
  $effect(() => {
    const escape = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    const outside = (e: PointerEvent) => {
      const target = e.target as Element | null;
      if (panel && target && !panel.contains(target) && !target.closest(".load")) onClose();
    };
    window.addEventListener("keydown", escape);
    window.addEventListener("pointerdown", outside, true);
    return () => {
      window.removeEventListener("keydown", escape);
      window.removeEventListener("pointerdown", outside, true);
    };
  });
</script>

{#snippet figure(line: { measure: Measure; name: string; colour: string }, sample: Sample)}
  <span class="value"><svg width="10" height="8" aria-hidden="true"><line x1="0" y1="4" x2="10" y2="4" stroke={line.colour} stroke-width="2" stroke-linecap="round" /></svg><b>{line.measure === "down" || line.measure === "up" ? bytes(sample[line.measure]) : percent(sample[line.measure])}</b> {line.name}</span>
{/snippet}

{#snippet chart(title: string, set: { measure: Measure; name: string; colour: string }[], y: (v: number) => number, ticks: { value: number; label: string }[], labels: { name: string; colour: string; y: number; text: string }[])}
  <figure>
    <figcaption>
      <span class="chart-title">{title}</span>
      <!-- the legend: a line key per series, its name in text ink -->
      {#each set as line}<span class="key"><svg width="14" height="8" aria-hidden="true"><line x1="0" y1="4" x2="14" y2="4" stroke={line.colour} stroke-width="2" stroke-linecap="round" /></svg>{line.name}</span>{/each}
    </figcaption>
    <!-- a slider through the readings for the keyboard and screen readers: the arrow keys move the crosshair -->
    <svg viewBox="0 0 {WIDTH} {HEIGHT}" role="slider" aria-label="{title}, the last {ago(span[0], now).replace(' ago', '')}"
      aria-valuemin={0} aria-valuemax={Math.max(0, samples.length - 1)} aria-valuenow={pointed >= 0 ? pointed : samples.length - 1}
      aria-valuetext={readoutText(pointed >= 0 ? samples[pointed] : samples[samples.length - 1], set)}
      tabindex="0" onpointermove={point} onpointerleave={() => (pointed = -1)} onkeydown={key} onblur={() => (pointed = -1)}>
      {#each ticks as tick}
        <line class="grid" x1={LEFT} x2={plotRight} y1={y(tick.value)} y2={y(tick.value)} />
        <text class="axis" x={LEFT - 6} y={y(tick.value)} text-anchor="end" dominant-baseline="middle">{tick.label}</text>
      {/each}
      {#each timeTicks as at}
        <text class="axis" x={x(at)} y={HEIGHT - 4} text-anchor={at === now ? "end" : "middle"}>{ago(at, now)}</text>
      {/each}
      <line class="baseline" x1={LEFT} x2={plotRight} y1={plotBottom} y2={plotBottom} />
      {#each set as line}
        <path d={linePath(samples, line.measure, x, y)} fill="none" stroke={line.colour} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
      {/each}
      {#each labels as label}
        <line x1={plotRight + 6} x2={plotRight + 16} y1={label.y} y2={label.y} stroke={label.colour} stroke-width="2" stroke-linecap="round" />
        <text class="end" x={plotRight + 20} y={label.y} dominant-baseline="middle">{label.text}</text>
      {/each}
      {#if pointedSample}
        <line class="crosshair" x1={x(pointedSample.at)} x2={x(pointedSample.at)} y1={TOP} y2={plotBottom} />
        {#each set as line}
          {#if pointedSample[line.measure] !== null}
            <circle cx={x(pointedSample.at)} cy={y(pointedSample[line.measure] as number)} r="4" fill={line.colour} class="dot" />
          {/if}
        {/each}
      {/if}
    </svg>
  </figure>
{/snippet}

<div class="panel" bind:this={panel} role="dialog" aria-label="Server load of {tab}">
  <header>
    <span class="title">Server load: <b>{tab}</b></span>
    <span class="grow"></span>
    <button class="toggle" aria-pressed={table} onclick={() => (table = !table)}>{table ? "Graph" : "Table"}</button>
    <button class="close" aria-label="Close" onclick={onClose}>×</button>
  </header>
  {#if samples.length === 0}
    <p class="muted">No readings yet: they come every 3 seconds once the session has logged in.</p>
  {:else if table}
    <div class="rows">
      <table>
        <thead><tr><th>When</th><th>CPU</th><th>Memory</th><th>Disk</th><th>In ↓</th><th>Out ↑</th></tr></thead>
        <tbody>
          {#each [...samples].reverse() as sample}
            <tr><td>{ago(sample.at, now)}</td><td>{percent(sample.cpu)}</td><td>{percent(sample.mem)}</td><td>{percent(sample.disk)}</td><td>{bytes(sample.down)}</td><td>{bytes(sample.up)}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  {:else}
    {@render chart("Use", lines.use, yPercent, [0, 50, 100].map((v) => ({ value: v, label: `${v}%` })), useLabels)}
    {@render chart("Network", lines.net, yRate, [0, netTop / 2, netTop].map((v) => ({ value: v, label: axisRate(v) })), netLabels)}
    <!--
      the crosshair's readout, values first: two rows as the charts are (Use's figures, then Network's), each kept to
      one line and the pair always the same height, hint or figures, so the panel doesn't grow and shrink (it's
      anchored at the bottom, so it jumped up a line) as the pointer goes on and off the charts
    -->
    <div class="readout" aria-live="polite">
      {#if pointedSample}
        <p><span class="when">{ago(pointedSample.at, now)}</span>{#each lines.use as line}{@render figure(line, pointedSample)}{/each}</p>
        <p><span class="when"></span>{#each lines.net as line}{@render figure(line, pointedSample)}{/each}</p>
      {:else}
        <p class="muted">Point at a chart (or focus it and use the arrow keys) for the figures at that moment.</p>
        <p></p>
      {/if}
    </div>
  {/if}
</div>

<style>
  /* over the window's bottom right, above the status bar it opens from */
  .panel {
    position: fixed;
    right: 12px;
    bottom: 30px;
    z-index: 140;
    width: min(600px, calc(100vw - 24px));
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    background: var(--bg-pane);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 10px 32px rgb(0 0 0 / 0.35);
    color: var(--text);
    font-size: 12px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .title,
  .muted,
  .when {
    color: var(--text-muted);
  }

  .title b {
    color: var(--text);
  }

  .grow {
    flex: 1;
  }

  header button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 8px;
    font: inherit;
    color: inherit;
  }

  header .close {
    border: 0;
    background: none;
    font-size: 16px;
    line-height: 1;
  }

  figure {
    margin: 0;
  }

  figcaption {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-muted);
  }

  .chart-title {
    color: var(--text);
    font-weight: 600;
    margin-right: 4px;
  }

  .key,
  .value {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  figure svg[role="slider"] {
    display: block;
    width: 100%;
    height: auto;
    outline: none;
  }

  figure svg[role="slider"]:focus-visible {
    outline: 1px solid var(--accent);
    border-radius: 4px;
  }

  /* recessive chrome: hairline grid and baseline, muted labels; text in text ink, never the series colour */
  .grid {
    stroke: var(--border);
    stroke-width: 1;
    opacity: 0.6;
  }

  .baseline {
    stroke: var(--border);
    stroke-width: 1;
  }

  .axis {
    fill: var(--text-muted);
    font-size: 10px;
  }

  .end {
    fill: var(--text);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .crosshair {
    stroke: var(--text-muted);
    stroke-width: 1;
  }

  /* each dot on the crosshair ringed in the pane's colour, so it reads where it crosses a line */
  .dot {
    stroke: var(--bg-pane);
    stroke-width: 2;
  }

  /* two rows of one line each, the same height with figures or the hint: no jump as the pointer comes and goes */
  .readout {
    display: grid;
    grid-template-rows: 18px 18px;
    gap: 2px;
    font-variant-numeric: tabular-nums;
  }

  .readout p {
    display: flex;
    align-items: center;
    gap: 14px;
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* the time's column, so each row's figures start at the same place, under each other */
  .readout .when {
    flex: none;
    width: 92px;
  }

  .rows {
    max-height: 300px;
    overflow: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }

  th,
  td {
    padding: 2px 8px;
    text-align: right;
    border-bottom: 1px solid var(--border);
  }

  th:first-child,
  td:first-child {
    text-align: left;
  }

  th {
    color: var(--text-muted);
    font-weight: normal;
    position: sticky;
    top: 0;
    background: var(--bg-pane);
  }
</style>
