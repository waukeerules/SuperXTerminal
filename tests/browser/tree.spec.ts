// The Sessions tree with thousands of sessions (npm run test:browser), mounted on a page of its own through
// tests/browser/fixtures/tree.svelte.ts with the Rust side stood in for, in the engines the app runs in: how many of
// a large folder's rows are in the page, that the ones scrolled to are there, that a selection reaches across rows
// that aren't, and that Show in Sessions tree scrolls to one far down.
//
// The last test measures rather than checks: `npm run measure:tree` (docs/TESTING.md) prints how long the tree takes
// to show, to read its sessions again, to open a large folder, to follow a tab connecting and to search.
import { expect, test, type Page } from "@playwright/test";

/** Show the tree with `count` sessions, `perFolder` to a folder, `recent` (ids) in Recent, and wait for its roots */
async function showTree(page: Page, count: number, perFolder: number, recent: string[] = []) {
  await page.goto("/tests/browser/fixtures/tree.html");
  await page.evaluate(async ([count, perFolder, recent]) => {
    const { showTree } = await import(/* @vite-ignore */ "/tests/browser/fixtures/tree.svelte.ts");
    (window as any).tree = showTree(count, perFolder, recent);
  }, [count, perFolder, recent] as const);
  await expect(page.getByText(/^Saved sessions/)).toBeVisible();
}

/** Two frames on: what was asked for has been worked out and drawn */
const drawn = (page: Page) => page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));

/** How long `work` and the drawing after it take, in milliseconds */
async function timed(page: Page, work: () => Promise<unknown>): Promise<number> {
  const started = Date.now();
  await work();
  await drawn(page);
  return Date.now() - started;
}

test.describe("a folder of thousands of sessions", () => {
  /** How many rows are in the page */
  const rowsInPage = (page: Page) => page.evaluate(() => document.querySelectorAll("#pane .row").length);
  /** Scroll the tree to its end, and wait for it to be drawn */
  async function scrollToEnd(page: Page) {
    await page.evaluate(() => {
      const tree = document.querySelector("#pane .tree") as HTMLElement;
      tree.scrollTop = tree.scrollHeight;
    });
    await drawn(page);
  }

  test.beforeEach(async ({ page }) => {
    await showTree(page, 5000, 5000);
    await page.getByRole("button", { name: "Expand all" }).click();
    await expect(page.getByText("sw-001-00001")).toBeVisible();
  });

  test("has only the rows in view in the page, and scrolls as far as all of them reach", async ({ page }) => {
    // the root, the folder and 5,000 sessions: a screenful and a few either side are drawn
    expect(await rowsInPage(page)).toBeLessThan(80);
    const reach = await page.evaluate(() => {
      const tree = document.querySelector("#pane .tree") as HTMLElement;
      return { scroll: tree.scrollHeight, row: (document.querySelector("#pane .row") as HTMLElement).offsetHeight };
    });
    expect(reach.scroll).toBeGreaterThanOrEqual(5002 * reach.row);
    expect(reach.scroll).toBeLessThan(5002 * reach.row + 100);
  });

  test("shows the sessions scrolled to, the last one at the end", async ({ page }) => {
    await expect(page.getByText("sw-001-05000")).toHaveCount(0);
    await scrollToEnd(page);
    await expect(page.getByText("sw-001-05000")).toBeVisible();
    await expect(page.getByText("sw-001-00001")).toHaveCount(0);
    expect(await rowsInPage(page)).toBeLessThan(80);
    // and back at the top
    await page.evaluate(() => ((document.querySelector("#pane .tree") as HTMLElement).scrollTop = 0));
    await expect(page.getByText("sw-001-00001")).toBeVisible();
  });

  test("Shift+click selects from a session to one far below it, the rows between not in the page", async ({ page }) => {
    await page.getByText("sw-001-00001").click();
    await scrollToEnd(page);
    await page.getByText("sw-001-05000").click({ modifiers: ["Shift"] });
    await page.getByText("sw-001-05000").click({ button: "right" });
    await expect(page.getByRole("menuitem", { name: "Delete 5000 sessions" })).toBeVisible();
  });

  test("marks a tab's session when it's scrolled to, though its row wasn't in the page when the tab connected", async ({ page }) => {
    await page.evaluate(() => (window as any).tree.setOpen([["saved:Site 001/sw-001-05000", "connected"]]));
    await scrollToEnd(page);
    const row = page.locator("#pane .row.session", { hasText: "sw-001-05000" });
    await expect(row).toBeVisible();
    // the green dot of a connected session, which the row above it hasn't
    const dots = (text: string) => page.locator("#pane .row.session", { hasText: text }).locator(".dot.open").count();
    expect([await dots("sw-001-05000"), await dots("sw-001-04999")]).toEqual([1, 0]);
  });
});

test("Show in Sessions tree scrolls to a session thousands of rows down its folder, and selects it", async ({ page }) => {
  await showTree(page, 5000, 5000, ["Site 001/sw-001-04000"]);
  // only Recent's shows: its own folder is shut
  await page.getByText("sw-001-04000").click({ button: "right" });
  await page.getByRole("menuitem", { name: "Show in Sessions tree" }).click();
  const row = page.locator('#pane .row.session[data-key="saved:Site 001/sw-001-04000"]');
  await expect(row).toBeInViewport();
  await expect(row).toHaveClass(/selected/);
});

test("measure: the tree with thousands of sessions", async ({ page }) => {
  test.skip(!process.env.SXT_MEASURE, "a measurement, not a check: run with npm run measure:tree");
  test.setTimeout(10 * 60 * 1000);
  const count = Number(process.env.SXT_SESSIONS ?? 10000);
  const perFolder = Number(process.env.SXT_PER_FOLDER ?? count);
  await page.setViewportSize({ width: 1400, height: 800 });

  const shown = await timed(page, () => showTree(page, count, perFolder));
  const rows = () => page.evaluate(() => document.querySelectorAll("#pane .row").length);
  const nodes = () => page.evaluate(() => document.querySelectorAll("#pane *").length);

  // read again with every folder shut, as after a change to a session: the sessions sent and the tree built again
  const reloadShut = await timed(page, () => page.evaluate(() => (window as any).tree.reload()));

  const opened = await timed(page, () => page.getByRole("button", { name: "Expand all" }).click());
  const rowsOpen = await rows();
  const nodesOpen = await nodes();

  // a tab connecting, then connected: the tree marks its session
  const first = `saved:Site 001/sw-001-00001`;
  const connecting = await timed(page, () => page.evaluate((key) => (window as any).tree.setOpen([[key, "connecting"]]), first));
  const connected = await timed(page, () => page.evaluate((key) => (window as any).tree.setOpen([[key, "connected"]]), first));

  const reloadOpen = await timed(page, () => page.evaluate(() => (window as any).tree.reload()));

  // scrolled a long way down, to rows that weren't showing
  const scrolled = await timed(page, () => page.evaluate(() => {
    const tree = document.querySelector("#pane .tree") as HTMLElement;
    tree.scrollTop = tree.scrollHeight / 2;
  }));

  const search = page.getByPlaceholder(/Type to search/);
  const searched = await timed(page, () => search.fill("sw-001-0000"));
  const cleared = await timed(page, () => search.fill(""));

  const memory = await page.evaluate(() => Math.round(((performance as any).memory?.usedJSHeapSize ?? 0) / 1024 / 1024));
  console.log(
    `MEASURE ${test.info().project.name}: the tree with ${count} sessions, ${perFolder} to a folder\n` +
      `  shown at first: ${shown} ms\n` +
      `  sessions read again, folders shut: ${reloadShut} ms\n` +
      `  Expand all: ${opened} ms, ${rowsOpen} rows and ${nodesOpen} elements in the page\n` +
      `  a tab connecting: ${connecting} ms; connected: ${connected} ms\n` +
      `  sessions read again, folders open: ${reloadOpen} ms\n` +
      `  scrolled halfway down: ${scrolled} ms\n` +
      `  a search typed: ${searched} ms; cleared: ${cleared} ms\n` +
      `  JavaScript memory: ${memory || "not told by this browser"} MB`,
  );
});
