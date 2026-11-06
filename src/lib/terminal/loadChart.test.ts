// The server load graph's geometry (loadChart.ts): lines that break where no readings were taken, a round top for
// the network axis, the reading nearest the pointer, end labels spread apart, and how long ago a time was.
import { describe, expect, it } from "vitest";
import { ago, axisRate, linePath, nearest, niceMax, spread, timeSpan } from "./loadChart";
import type { Sample } from "./serverLoad.svelte";

/** A reading at `at` seconds, its CPU `cpu` (null: the first reading, which has none) */
const sample = (at: number, cpu: number | null): Sample => ({ at: at * 1000, cpu, mem: 16, disk: 50, down: null, up: null });
/** Time in seconds and values as they are, so paths are easy to read */
const x = (at: number) => at / 1000;
const y = (v: number) => v;

describe("a line through the readings", () => {
  it("joins readings 3 s apart, and breaks at one with no value and at a gap of missed readings", () => {
    const samples = [sample(0, null), sample(3, 10), sample(6, 20), sample(9, 30), sample(60, 40), sample(63, 50)];
    // the first has no CPU; 9 → 60 s is a gap (the tab wasn't in front): a new piece, not a slope across it
    expect(linePath(samples, "cpu", x, y)).toBe("M3.0,10.0L6.0,20.0L9.0,30.0M60.0,40.0L63.0,50.0");
    expect(linePath([], "cpu", x, y)).toBe("");
  });
});

describe("the axes", () => {
  it("rounds the network axis' top up to 1, 2 or 5 of a power of ten of the units its labels show, at least 1 KB/s", () => {
    const KB = 1024;
    const MB = 1024 * 1024;
    expect([niceMax(0), niceMax(800), niceMax(1.5 * KB), niceMax(190 * KB), niceMax(600 * KB), niceMax(3 * MB)]).toEqual([KB, KB, 2 * KB, 200 * KB, 1000 * KB, 5 * MB]);
    // labelled as the axis reads: round figures without a ".0"
    expect([axisRate(0), axisRate(200 * KB), axisRate(100 * KB), axisRate(5 * MB), axisRate(1.5 * KB)]).toEqual(["0", "200 KB/s", "100 KB/s", "5 MB/s", "1.5 KB/s"]);
  });

  it("shows at least a minute, so a new graph isn't a dot, and its readings from the first", () => {
    expect(timeSpan([], 100_000)).toEqual([40_000, 100_000]);
    expect(timeSpan([sample(10, 1)], 400_000)).toEqual([10_000, 400_000]);
  });

  it("says how long ago a time was", () => {
    const now = 1_000_000;
    expect([0, 3, 45, 60, 150, 600].map((s) => ago(now - s * 1000, now))).toEqual(["now", "now", "45 s ago", "1 min ago", "2 min 30 s ago", "10 min ago"]);
  });
});

describe("the crosshair and labels", () => {
  it("finds the reading nearest the pointer's time", () => {
    const samples = [sample(0, 1), sample(3, 2), sample(6, 3)];
    expect([nearest(samples, 1000), nearest(samples, 2000), nearest(samples, 99_000), nearest([], 0)]).toEqual([0, 1, 2, -1]);
  });

  it("spreads end labels that would overlap, in their order, inside the chart", () => {
    // CPU at 90 and disk at 92 would overlap: disk moves down to keep 13 apart; memory alone stays
    expect(spread([90, 40, 92], 13, 10, 108)).toEqual([90, 40, 103]);
    // three at the bottom move up together rather than off the chart
    expect(spread([108, 108, 108], 13, 10, 108)).toEqual([82, 95, 108]);
  });
});
