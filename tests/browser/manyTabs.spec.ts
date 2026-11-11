// Many terminal tabs at once (npm run test:browser): a tab that's behind another takes its session's output, keeps
// its log going and waits for nothing, but doesn't draw, highlight or resize until it's brought to the front, and
// holds no WebGL context meanwhile (a browser keeps about 16, so the 17th tab took the first one's). The tabs are
// stood in for by tests/browser/fixtures/tabs.html: each terminal page in its own iframe of one window, as the app's
// are, hidden as dockview hides a tab behind another, and told so as PageHost tells it.
//
// The last test measures rather than checks: set SXT_MEASURE=1 (and SXT_TABS, SXT_BUSY for how many) to print how
// many tabs' worth of output the window gets through, how it fares meanwhile, and what a resize costs. Run it before
// and after a change to the terminal page or its host to see what the change did (docs/TESTING.md).
import { expect, test, type Page } from "@playwright/test";

/** A rule set that colours a word, so a test can see whether a line has been highlighted */
const RULES = [{ id: "log", name: "Log", rules: [{ match: "ERROR", style: "error" }] }];

/** Open `count` tabs in the stand-in tab area, the first in front, highlighting with [`RULES`] */
async function openTabs(page: Page, count: number, prefs: Record<string, string> = {}, grid = false) {
  await page.goto("/tests/browser/fixtures/tabs.html");
  await page.evaluate(([count, prefs, rules, grid]) => (window as any).tabs.open(count, prefs, rules, grid), [count, { ruleSet: "log", ...prefs }, RULES, grid] as const);
}

/** A tab's terminal's columns */
const cols = (page: Page, tab: number) => page.evaluate((tab) => (window as any).tabs.dev(tab).term.cols as number, tab);
/** How many "size" messages a tab's page has posted: the first starts its session, each later one resizes it */
const sizes = (page: Page, tab: number) => page.evaluate((tab) => (window as any).tabs.posted(tab, "size").length as number, tab);
/** How many times a tab's terminal has drawn */
const renders = (page: Page, tab: number) => page.evaluate((tab) => (window as any).tabs.renders(tab) as number, tab);
/** How many highlights a tab's terminal has */
const highlights = (page: Page, tab: number) => page.evaluate((tab) => (window as any).tabs.dev(tab).highlights().length as number, tab);
/** The lines a tab's page has sent to its session log */
const logged = (page: Page, tab: number) =>
  page.evaluate((tab) => ((window as any).tabs.posted(tab, "log") as { lines: string[] }[]).flatMap((m) => m.lines), tab);
/** Whether a tab's page has stopped being laid out, as one behind another does once it has had its size a moment */
const behind = (page: Page, tab: number) => page.evaluate((tab) => (window as any).tabs.behind(tab) as boolean, tab);
/** Bring a tab to the front */
const show = (page: Page, tab: number) => page.evaluate((tab) => (window as any).tabs.show(tab), tab);

test("a tab behind another takes its output and logs it without drawing it, and shows it highlighted in front", async ({ page }) => {
  // opening waits for both tabs' sizes: one opened behind another still gets its size, so its session starts
  await openTabs(page, 2);
  await expect.poll(() => behind(page, 1)).toBe(true);
  // xterm.js notices its page is no longer laid out a frame or so later, and stops drawing from then
  await expect.poll(() => page.evaluate(() => (window as any).tabs.dev(1).term._core._renderService._isPaused as boolean)).toBe(true);
  const drawn = await renders(page, 1);

  await page.evaluate(() => (window as any).tabs.feed(1, "link up\r\nERROR power supply 2 failed\r\nswitch# "));
  // the log has it (it's sent after the page would have drawn), so Search Output and a script's WAITFOR see it
  await expect.poll(() => logged(page, 1)).toContain("ERROR power supply 2 failed");
  expect(await renders(page, 1)).toBe(drawn);
  expect(await highlights(page, 1)).toBe(0);
  // and it's in the terminal, to be seen once the tab is in front
  expect(await page.evaluate(() => (window as any).tabs.dev(1).term.buffer.active.getLine(1).translateToString(true))).toBe("ERROR power supply 2 failed");

  await show(page, 1);
  await expect.poll(() => highlights(page, 1)).toBeGreaterThan(0);
  await expect.poll(() => renders(page, 1)).toBeGreaterThan(drawn);
});

test("a tab behind another isn't resized with the window until it's in front, then once", async ({ page }) => {
  await openTabs(page, 2);
  await expect.poll(() => behind(page, 1)).toBe(true);
  // the one behind has the size the one in front has: its session started with the size it would have in front
  const before = await cols(page, 0);
  expect(await cols(page, 1)).toBe(before);
  const told = await sizes(page, 1);

  await page.setViewportSize({ width: 500, height: 300 });
  await expect.poll(() => cols(page, 0)).toBeLessThan(before);
  // the one in front told its session; the one behind is as it was, its session not resized
  expect(await cols(page, 1)).toBe(before);
  expect(await sizes(page, 1)).toBe(told);

  await show(page, 1);
  await expect.poll(() => cols(page, 1)).toBe(await cols(page, 0));
  // its message to the window comes a moment after the terminal has its columns
  await expect.poll(() => sizes(page, 1)).toBe(told + 1);
});

test("a tab behind another gives up its WebGL context, and draws with WebGL again in front", async ({ page }) => {
  await openTabs(page, 2);
  const webgl = (tab: number) => page.evaluate((tab) => (window as any).tabs.webgl(tab) as boolean, tab);
  test.skip(!(await webgl(0)), "this browser has no WebGL here, so every tab draws with the DOM renderer");
  await expect.poll(() => webgl(1)).toBe(false);
  await show(page, 1);
  await expect.poll(() => webgl(1)).toBe(true);
  await expect.poll(() => webgl(0)).toBe(false);
});

test("a font size chosen while a tab is behind is the tab's once it's in front", async ({ page }) => {
  await openTabs(page, 2);
  const before = await cols(page, 1);
  await page.evaluate(() => {
    for (const tab of [0, 1]) (window as any).tabs.post(tab, { type: "prefs", prefs: { ruleSet: "log", fontSize: "24" } });
  });
  await expect.poll(() => cols(page, 0)).toBeLessThan(before);
  await show(page, 1);
  await expect.poll(() => cols(page, 1)).toBe(await cols(page, 0));
  // and it draws: its rows are the new font's height
  await page.evaluate(() => (window as any).tabs.feed(1, "after the font changed\r\n"));
  await expect.poll(() => page.evaluate(() => (window as any).tabs.dev(1).term.buffer.active.getLine(0).translateToString(true))).toBe("after the font changed");
});

test("output that came while a tab was behind is on its screen, and the keyboard its own, once it's in front", async ({ page }) => {
  // the DOM renderer, whose rows are text a test can read (WebGL draws on a canvas)
  await openTabs(page, 2, { renderer: "dom" });
  await expect.poll(() => behind(page, 1)).toBe(true);
  await page.evaluate(() => (window as any).tabs.feed(1, "core-sw-01#show clock\r\n09:41:07.123 UTC Wed Nov 11 2026\r\ncore-sw-01#"));
  // the window gives a tab the keyboard as it brings it forward; told while still behind, it takes it once in front
  await page.evaluate(() => (window as any).tabs.post(1, { type: "focus" }));
  await show(page, 1);
  const screenText = () => page.evaluate(() => (window as any).tabs.dev(1).term.element.querySelector(".xterm-rows").textContent as string);
  await expect.poll(screenText).toContain("09:41:07.123 UTC Wed Nov 11 2026");
  await expect.poll(() => page.evaluate(() => {
    const doc = (window as any).tabs.dev(1).term.element.ownerDocument as Document;
    return doc.activeElement?.classList.contains("xterm-helper-textarea") ?? false;
  })).toBe(true);
});

test("measure: many tabs, some printing at once", async ({ page }) => {
  test.skip(!process.env.SXT_MEASURE, "a measurement, not a check: run with SXT_MEASURE=1");
  test.setTimeout(10 * 60 * 1000);
  const count = Number(process.env.SXT_TABS ?? 40);
  const busy = Number(process.env.SXT_BUSY ?? 10);
  // Grid View: every tab showing, side by side
  const grid = !!process.env.SXT_GRID;
  await page.setViewportSize({ width: 1400, height: 800 });

  let started = Date.now();
  await openTabs(page, count, {}, grid);
  const opened = Date.now() - started;
  // let the tabs settle: a tab behind gives up WebGL a moment after it has its size, and one in front whose context
  // the others took as they opened (the browser keeps the newest 16) has asked for it again
  await page.waitForTimeout(9000);
  const tabsList = Array.from({ length: count }, (_, i) => i);
  const onWebgl = async () => (await page.evaluate((tabs) => tabs.map((t) => (window as any).tabs.webgl(t) as boolean), tabsList)).filter(Boolean).length;
  const webglAtStart = await onWebgl();

  // the tab in front and the first few behind it print at once: a switch's log, one line in eight an error
  const chunk = Array.from({ length: 40 }, (_, i) =>
    i % 8 === 0 ? `Nov 11 09:41:${String(i).padStart(2, "0")} ERROR %LINK-3-UPDOWN: Interface GigabitEthernet1/0/${i}, changed state to down\r\n`
      : `Nov 11 09:41:${String(i).padStart(2, "0")} %LINEPROTO-5-UPDOWN: Line protocol on Interface GigabitEthernet1/0/${i}, changed state to up\r\n`).join("");
  const printing = tabsList.slice(process.env.SXT_SKIP_FRONT ? 1 : 0, busy);
  const drawnBefore = await page.evaluate((tabs) => tabs.map((t) => (window as any).tabs.renders(t) as number), tabsList);
  // SXT_PROFILE=1: where the window's thread spent its time while they printed, and in the resize (Chromium only)
  const cdp = process.env.SXT_PROFILE ? await page.context().newCDPSession(page) : null;
  await cdp?.send("Profiler.enable");
  /** Run `work`, printing the functions the window's thread spent longest in meanwhile when profiling */
  async function profiled<T>(what: string, work: () => Promise<T>): Promise<T> {
    await cdp?.send("Profiler.start");
    const result = await work();
    if (!cdp) return result;
    const { profile } = await cdp.send("Profiler.stop");
    const nodes = new Map(profile.nodes.map((n) => [n.id, n]));
    const callers = new Map<number, number>();
    profile.nodes.forEach((n) => (n.children ?? []).forEach((child) => callers.set(child, n.id)));
    const named = (id: number | undefined) => (id === undefined ? "" : nodes.get(id)!.callFrame.functionName || "(anonymous)");
    const self = new Map<string, number>();
    (profile.samples ?? []).forEach((id, i) => {
      const frame = nodes.get(id)!.callFrame;
      // with the two functions that called it: the browser's own (setTimeout) and xterm.js's say little alone
      const caller = callers.get(id);
      const key = `${named(id)} ${frame.url.split("/").pop()}:${frame.lineNumber + 1}  < ${named(caller)} < ${named(caller === undefined ? undefined : callers.get(caller))}`;
      self.set(key, (self.get(key) ?? 0) + (profile.timeDeltas?.[i] ?? 0) / 1000);
    });
    const top = [...self.entries()].sort((a, b) => b[1] - a[1]).slice(0, 18);
    console.log(`PROFILE ${what} (ms in each function itself)\n` + top.map(([name, ms]) => `${ms.toFixed(0).padStart(7)}  ${name}`).join("\n"));
    return result;
  }
  const flood = await profiled("printing", () => page.evaluate(([printing, chunk]) => (window as any).tabs.flood(printing, chunk, 150), [printing, chunk] as const));
  const drawnAfter = await page.evaluate((tabs) => tabs.map((t) => (window as any).tabs.renders(t) as number), tabsList);
  const drawnBehind = drawnAfter.slice(1).reduce((sum, n, i) => sum + n - drawnBefore[i + 1], 0);

  // the window dragged smaller: how long until every tab that's going to resize has, and how often sessions were told
  const sizesBefore = await page.evaluate((tabs) => tabs.map((t) => (window as any).tabs.posted(t, "size").length as number), tabsList);
  const colsBefore = await cols(page, 0);
  started = Date.now();
  await profiled("the resize", async () => {
    // as a drag of the window's corner: eight steps, not one jump
    for (let step = 1; step <= 8; step++) await page.setViewportSize({ width: 1400 - 50 * step, height: 800 - 25 * step });
    await expect.poll(() => cols(page, 0), { timeout: 60_000, intervals: [20] }).toBeLessThan(colsBefore);
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  });
  const resized = Date.now() - started;
  const sizesAfter = await page.evaluate((tabs) => tabs.map((t) => (window as any).tabs.posted(t, "size").length as number), tabsList);
  const told = sizesAfter.reduce((sum, n, i) => sum + n - sizesBefore[i], 0);

  // a tab that printed behind the others, brought to the front: how long until it has drawn and highlighted
  started = Date.now();
  if (!grid) {
    await show(page, busy - 1);
    await expect.poll(() => highlights(page, busy - 1), { intervals: [10] }).toBeGreaterThan(0);
  }
  const shown = Date.now() - started;

  // what draws WebGL here: a graphics card, as in the app, or the browser's software stand-in (SwiftShader), which is
  // many times slower and makes a tab's drawing the whole of what's measured
  const drawnBy = await page.evaluate(() => {
    const gl = document.createElement("canvas").getContext("webgl2");
    const info = gl?.getExtension("WEBGL_debug_renderer_info");
    return gl && info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : "no WebGL";
  });
  const memory = await page.evaluate(() => Math.round(((performance as any).memory?.usedJSHeapSize ?? 0) / 1024 / 1024));
  console.log(
    `MEASURE ${test.info().project.name}: ${count} tabs${grid ? " in a grid" : ""} (${printing.length} printing ${150 * 40} lines each)\n` +
      `  opened in ${opened} ms; drawing with WebGL: ${webglAtStart} of ${count} tabs at the start, ${await onWebgl()} at the end\n` +
      `  output: ${flood.ms} ms; the window drew ${flood.frames} frames meanwhile, the longest wait between two ${flood.longestFrameMs} ms\n` +
      (grid ? "" : `  draws by tabs behind the front one while printing: ${drawnBehind}\n`) +
      `  window dragged smaller in 8 steps: ${resized} ms, sessions told of a new size ${told} times\n` +
      (grid ? "" : `  a tab that printed behind, brought to the front: drawn and highlighted in ${shown} ms\n`) +
      `  JavaScript memory: ${memory || "not told by this browser"} MB
` +
      `  WebGL drawn by: ${drawnBy}`,
  );
});
