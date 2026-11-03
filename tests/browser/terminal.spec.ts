// The terminal page (public/web/terminal) on its own, as a tab's iframe shows it, fed through its terminalDev hooks:
// Wrap Lines off and the view following the cursor, panning, full-screen programs, and selecting while panned.
import { expect, test, type Page } from "@playwright/test";

/** Open the page with these preferences (a PuTTY theme, so the font and fit apply as in the app) */
async function openTerminal(page: Page, prefs: Record<string, string>) {
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  await page.evaluate((prefs) => (window as any).terminalDev.message({ type: "config", prefs, themes: [{ name: "PuTTY", colors: {} }] }), prefs);
  await page.waitForFunction(() => (window as any).terminalDev.term.cols > 2);
}

/** Write output as if the session sent it; the checks after it wait for it to be drawn (expect.poll) */
async function feed(page: Page, text: string) {
  await page.evaluate((text) => (window as any).terminalDev.feedText(text), text);
}

const cols = (page: Page) => page.evaluate(() => (window as any).terminalDev.term.cols as number);
/** How far the view is panned, in pixels (the side-to-side scroll bar follows it) */
const pan = (page: Page) => page.evaluate(() => document.getElementById("hscroll")!.scrollLeft);
/** Pan as the user does, with the side-to-side scroll bar, and wait for the view to follow it */
async function panTo(page: Page, x: number) {
  await page.evaluate((x) => (document.getElementById("hscroll")!.scrollLeft = x), x);
  await expect.poll(() => page.evaluate(() => document.querySelector("#terminal .xterm-screen")!.getAttribute("style") ?? "")).toContain("translateX");
}

const LONG_COMMAND = "show running-config | include interface GigabitEthernet1/0/24 description uplink-to-core";

test.describe("Wrap Lines off", () => {
  test.beforeEach(async ({ page }) => openTerminal(page, { wrap: "false" }));

  test("the terminal is 300 columns, with the scroll bar along the bottom", async ({ page }) => {
    expect(await cols(page)).toBe(300);
    await expect(page.locator("body")).toHaveClass(/nowrap/);
    await expect(page.locator("#hscroll")).toBeVisible();
    // a 200-character line stays one line
    await feed(page, "x".repeat(200) + "\r\n");
    await expect.poll(() => page.evaluate(() => (window as any).terminalDev.term.buffer.active.cursorY)).toBe(1);
    const wrapped = await page.evaluate(() => (window as any).terminalDev.term.buffer.active.getLine(1)?.isWrapped);
    expect(wrapped).toBeFalsy();
  });

  test("follows the cursor along a long command, and goes back to the start at the prompt", async ({ page }) => {
    await feed(page, "switch# " + LONG_COMMAND);
    await expect.poll(() => pan(page)).toBeGreaterThan(0);
    await feed(page, "\r\nswitch# ");
    await expect.poll(() => pan(page)).toBe(0);
  });

  test("leaves a line the user panned along, until they type", async ({ page }) => {
    await feed(page, "switch# ");
    await expect.poll(() => page.evaluate(() => (window as any).terminalDev.term.buffer.active.cursorX)).toBe(8);
    await panTo(page, 600);
    await feed(page, "x");
    await expect.poll(() => page.evaluate(() => (window as any).terminalDev.term.buffer.active.cursorX)).toBe(9);
    expect(await pan(page)).toBe(600);
    await page.evaluate(() => (window as any).terminalDev.term.input("s"));
    await expect.poll(() => pan(page)).toBe(0);
  });

  test("gives a full-screen program the tab's width, and the wide lines back after it", async ({ page }) => {
    await feed(page, "\x1b[?1049h");
    await expect.poll(() => cols(page)).toBeLessThan(300);
    await expect(page.locator("body")).not.toHaveClass(/nowrap/);
    await feed(page, "\x1b[?1049l");
    await expect.poll(() => cols(page)).toBe(300);
  });

  test("selects the text under the mouse while panned", async ({ page }) => {
    await feed(page, "\x1b[2J\x1b[H" + "0123456789".repeat(20) + "\r\n");
    await expect.poll(() => page.evaluate(() => (window as any).terminalDev.term.buffer.active.cursorY)).toBe(1);
    await panTo(page, 400);
    const cell = await page.evaluate(() => (window as any).terminalDev.term._core._renderService.dimensions.css.cell.width as number);
    const box = (await page.locator("#terminal").boundingBox())!;
    // drag across the first ten visible cells of the first row
    const y = box.y + 8;
    await page.mouse.move(box.x + 1, y);
    await page.mouse.down();
    await page.mouse.move(box.x + cell * 10 - 1, y, { steps: 5 });
    await page.mouse.up();
    const first = Math.round(400 / cell);
    const expected = ("0123456789".repeat(20)).slice(first, first + 10);
    expect(await page.evaluate(() => (window as any).terminalDev.term.getSelection())).toBe(expected);
  });
});

test("Wrap Lines on fits the tab, with no scroll bar", async ({ page }) => {
  await openTerminal(page, { wrap: "true" });
  expect(await cols(page)).toBeLessThan(300);
  await expect(page.locator("body")).not.toHaveClass(/nowrap/);
  await expect(page.locator("#hscroll")).toBeHidden();
});
