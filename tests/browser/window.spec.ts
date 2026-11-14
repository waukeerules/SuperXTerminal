// The main window (npm run test:browser), mounted on a page of its own through tests/browser/fixtures/window.ts with
// the Rust side stood in for, in the engines the app runs in: what's shown of the Sessions pane's dialogs while the
// pane is folded to its edge tab (Auto hide), which only the window's own styles decide.
import { expect, test, type Page } from "@playwright/test";

/** Show the main window with these settings, and wait for the tab it starts with */
async function showWindow(page: Page, settings: Record<string, unknown>) {
  await page.goto("/tests/browser/fixtures/window.html");
  await page.evaluate(async (settings) => {
    const { showWindow } = await import(/* @vite-ignore */ "/tests/browser/fixtures/window.ts");
    (window as any).shown = showWindow(settings);
  }, settings);
  // the window is ready once it has opened the Local Shell it starts with: a tab opened before that is lost
  await expect(page.getByRole("tab", { name: "Local Shell" })).toBeVisible();
}

test.describe("the Sessions pane folded to its edge tab", () => {
  test("still shows Edit Session for a Quick connect tab's Save session as...", async ({ page }) => {
    await showWindow(page, { sessionsAutoHide: true });
    // connected until a tab opens: while the window is still starting, the toolbar is drawn again as its settings
    // arrive (emptying a box filled before), and the Local Shell's page takes the keyboard as it loads (and Enter)
    const host = page.getByLabel("Host", { exact: true });
    await expect(async () => {
      await host.fill("192.168.0.50");
      await host.press("Enter");
      await expect(page.getByRole("tab", { name: "192.168.0.50" })).toBeVisible({ timeout: 1000 });
    }).toPass();
    // the new tab takes the keyboard as its page loads, which closes a menu opened just before: opened again then
    // (Escape first, so a menu left open by a try that failed isn't in the way of the next)
    await expect(async () => {
      await page.keyboard.press("Escape");
      await page.getByRole("tab", { name: "192.168.0.50" }).click({ button: "right", timeout: 2000 });
      await page.getByText("Save session as...").click({ timeout: 2000 });
    }).toPass();
    // the dialog, with the tab's host as the session's, though the pane it belongs to is hidden
    await expect(page.getByRole("dialog")).toBeVisible();
    await expect(page.getByRole("dialog").getByLabel("Host address:")).toHaveValue("192.168.0.50");
    // and the pane itself stays folded
    await expect(page.getByLabel("Search sessions")).toBeHidden();
  });
});
