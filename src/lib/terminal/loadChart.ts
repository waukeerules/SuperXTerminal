/**
 * The server load graph's geometry (ServerLoadChart.svelte), as plain functions so it's tested without a browser:
 * where a reading goes on the time axis, a line through the readings that breaks where none were taken (the tab
 * wasn't in front, or the session dropped), a round top for the network axis, the reading nearest the pointer, and
 * the end-of-line labels spread so they don't overlap.
 */
import { rate, type Sample } from "./serverLoad.svelte";

/** A gap between readings longer than this breaks the line: three missed readings, not one late one */
export const GAP_MS = 10_000;

/** The figures a line can follow */
export type Measure = "cpu" | "mem" | "disk" | "down" | "up";

/** The time span the graph shows: from its first reading (at least a minute back, so a new graph isn't a dot) to now */
export function timeSpan(samples: Sample[], now: number): [number, number] {
  const first = samples[0]?.at ?? now;
  return [Math.min(first, now - 60_000), now];
}

/**
 * The SVG path of one measure through the readings: `x` and `y` place a reading's time and value. A reading without
 * the measure (the first, which has no CPU or traffic) or a gap longer than GAP_MS starts a new piece (M) rather than
 * joining across it, so a time nothing was read isn't drawn as if it were
 */
export function linePath(samples: Sample[], measure: Measure, x: (at: number) => number, y: (value: number) => number): string {
  let path = "";
  let last: number | null = null;
  for (const sample of samples) {
    const value = sample[measure];
    if (value === null) {
      last = null;
      continue;
    }
    const joined = last !== null && sample.at - last <= GAP_MS;
    path += `${joined ? "L" : "M"}${x(sample.at).toFixed(1)},${y(value).toFixed(1)}`;
    last = sample.at;
  }
  return path;
}

/**
 * A round top for a rate axis at or above `max` (bytes a second), round in the units its labels are in (KB/s, MB/s,
 * of 1024, as rate() writes them): 1, 2 or 5 times a power of ten of them, at least 1 KB/s. So the axis reads
 * "200 KB/s" and "100 KB/s", not 195.3 and 97.7
 */
export function niceMax(max: number): number {
  let unit = 1024;
  while (max / unit >= 1000 && unit < 1024 ** 3) unit *= 1024;
  const value = max / unit;
  if (!(value > 1)) return unit;
  const power = 10 ** Math.floor(Math.log10(value));
  const step = [1, 2, 5, 10].find((n) => n * power >= value) ?? 10;
  return step * power * unit;
}

/** A rate as an axis label: rate()'s, without a ".0" (an axis' round figures read "200 KB/s") */
export function axisRate(bytes: number): string {
  return bytes === 0 ? "0" : rate(bytes).replace(/\.0 /, " ");
}

/** The index of the reading nearest a time, or -1 with none */
export function nearest(samples: Sample[], at: number): number {
  let best = -1;
  let distance = Infinity;
  samples.forEach((sample, i) => {
    const d = Math.abs(sample.at - at);
    if (d < distance) {
      best = i;
      distance = d;
    }
  });
  return best;
}

/**
 * Labels' heights, moved apart so none is within `gap` of another, kept between `top` and `bottom`: the end-of-line
 * labels of CPU, memory and disk, which often sit close (a quiet server's CPU and its disk). In the order given
 */
export function spread(ys: number[], gap: number, top: number, bottom: number): number[] {
  const order = ys.map((y, i) => ({ y, i })).sort((a, b) => a.y - b.y);
  for (let k = 1; k < order.length; k++) order[k].y = Math.max(order[k].y, order[k - 1].y + gap);
  // pushed past the bottom: move the lot up, keeping their spacing
  const over = order.length ? order[order.length - 1].y - bottom : 0;
  if (over > 0) for (const o of order) o.y -= over;
  for (let k = order.length - 2; k >= 0; k--) order[k].y = Math.min(order[k].y, order[k + 1].y - gap);
  for (const o of order) o.y = Math.max(o.y, top);
  const out = new Array<number>(ys.length);
  for (const o of order) out[o.i] = o.y;
  return out;
}

/** How long ago a time was, as the axis and the readout say it: "now", "45 s ago", "5 min ago" */
export function ago(at: number, now: number): string {
  const seconds = Math.round((now - at) / 1000);
  if (seconds < 5) return "now";
  if (seconds < 60) return `${seconds} s ago`;
  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest >= 5 && minutes < 5 ? `${minutes} min ${rest} s ago` : `${minutes} min ago`;
}
