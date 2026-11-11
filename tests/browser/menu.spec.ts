// The app's menus (src/lib/ui/Menu.svelte) in the engines the app runs in, mounted on a page of their own through
// tests/browser/fixtures/menu.ts: a terminal's menu, taller than a short window, scrolls with its scroll bar shown.
import { expect, test, type Page } from "@playwright/test";

/** A terminal's menu as tall as its own, about thirty rows with submenus, at the top of the test's 400-pixel page */
async function tallMenu(page: Page) {
  // a blank page: the terminal page's own menu styles would apply to it too (its class is "menu" as well)
  await page.goto("/tests/browser/fixtures/menu.html");
  await page.evaluate(async () => {
    const { showMenu } = await import(/* @vite-ignore */ "/tests/browser/fixtures/menu.ts");
    const rows = Array.from({ length: 30 }, (_, i) => (i === 10 ? { label: "Font", items: [{ label: "Menlo" }] } : { label: `Row ${i + 1}` }));
    showMenu([{ label: "core-sw-01", heading: true }, { separator: true }, ...rows, { label: "Close All Tabs" }], 20, 20);
  });
  // the menu itself; a submenu, once open, is a second one
  return page.getByRole("menu").first();
}

test("a menu taller than the window scrolls within it, its last row still in reach", async ({ page }) => {
  const menu = await tallMenu(page);
  const fits = await menu.evaluate((m) => {
    const box = m.getBoundingClientRect();
    return { top: box.top, bottom: box.bottom, height: window.innerHeight, scrolls: m.scrollHeight > m.clientHeight };
  });
  expect(fits.top).toBeGreaterThanOrEqual(0);
  expect(fits.bottom).toBeLessThanOrEqual(fits.height);
  expect(fits.scrolls).toBe(true);
  const last = page.getByRole("menuitem", { name: "Close All Tabs" });
  await last.scrollIntoViewIfNeeded();
  await expect(last).toBeInViewport();
});

test("a menu that scrolls shows its scroll bar all the time, not only while it's scrolled (the Mac's way)", async ({ page, browserName }) => {
  // Playwright's headless Chromium hides every scroll bar (--hide-scrollbars), so there's none to measure there; the
  // Mac's WebKit is the one that hid it, and is checked
  test.skip(browserName === "chromium", "headless Chromium draws no scroll bars");
  const menu = await tallMenu(page);
  // a scroll bar the Mac hides floats over the rows and takes no room; one always shown takes its own 10 pixels
  const bar = await menu.evaluate((m) => {
    const style = getComputedStyle(m);
    return m.offsetWidth - m.clientWidth - parseFloat(style.borderLeftWidth) - parseFloat(style.borderRightWidth);
  });
  expect(bar).toBe(10);
});

test("scrolled under a pointer held still, an item's submenu opens, as the item's highlight shows", async ({ page }) => {
  // the pointer on Row 5; the wheel scrolls the menu until Font (row 11) is where the pointer is
  const menu = await tallMenu(page);
  const row5 = page.getByRole("menuitem", { name: "Row 5", exact: true });
  await row5.hover();
  const font = page.getByRole("menuitem", { name: /^Font/ });
  const step = (await font.boundingBox())!.y - (await row5.boundingBox())!.y;
  await expect(page.getByRole("menu")).toHaveCount(1);
  await menu.evaluate((m, by) => (m.scrollTop = by), step);
  // Font's submenu, beside Font where it is now
  await expect(page.getByRole("menu")).toHaveCount(2);
  const row = (await font.boundingBox())!;
  const sub = (await page.getByRole("menu").nth(1).boundingBox())!;
  expect(Math.round(sub.y - row.y)).toBe(-5);
  // scrolled on, another row under the pointer: the submenu closes
  await menu.evaluate((m, by) => (m.scrollTop = by + 48), step);
  await expect(page.getByRole("menu")).toHaveCount(1);
});
