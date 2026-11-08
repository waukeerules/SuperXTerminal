// The main window's stylesheet (src/app.css) on a page of its own, in the engines the app runs in: a theme of the
// user's own (Edit Themes…) draws the buttons a window leaves to the system, the scroll bars and the tab strip
// (dockview's) in its colours, while a button a window styles itself keeps its style, and Light and Dark keep the
// system's and dockview's. The theme is put on the page as uiThemes.svelte.ts's applyUiTheme puts it (data-own-theme
// and the colour tokens), and dockview's Light theme as its stylesheet has it, with no app around them.
import { expect, test, type Page } from "@playwright/test";

/** A page with app.css, a button no window styles, one styled as a window's scoped style would, and a tab strip */
async function openLook(page: Page) {
  await page.setContent(`<button id="plain">Choose…</button>
    <style>button.own { background: rgb(1, 2, 3); }</style><button id="own" class="own">OK</button>
    <div id="strip" class="dockview-theme-light"></div>`);
  // dockview's Light theme, as its own stylesheet has it: its colours, and nothing of the app's
  await page.addStyleTag({ content: `.dockview-theme-light { --dv-tabs-and-actions-container-background-color: #f3f3f3;
    --dv-activegroup-visiblepanel-tab-background-color: white; --dv-activegroup-visiblepanel-tab-color: rgb(51, 51, 51); }` });
  await page.addStyleTag({ path: "src/app.css" });
}

/** Put the page in a theme of the user's own: Light-based, a red window with white text, as applyUiTheme does */
const ownTheme = (page: Page) => page.evaluate(() => {
  const root = document.documentElement;
  root.dataset.theme = "light";
  root.dataset.ownTheme = "";
  for (const [token, value] of [["--bg-input", "#882b2b"], ["--bg-bar", "#7a2a2a"], ["--border", "#000000"], ["--text", "#ffffff"], ["--bg-pane", "#bb3e3e"], ["--text-muted", "#cdb1b1"]])
    root.style.setProperty(token, value);
});

/** A computed style of an element */
const style = (page: Page, selector: string, property: string) =>
  page.evaluate(({ selector, property }) => getComputedStyle(document.querySelector(selector)!).getPropertyValue(property), { selector, property });

test("a theme of your own draws the system's buttons, the scroll bars and the tab strip in its colours, so its text can be read on them", async ({ page }) => {
  await openLook(page);
  await ownTheme(page);
  expect(await style(page, "#plain", "background-color")).toBe("rgb(136, 43, 43)");
  expect(await style(page, "#plain", "color")).toBe("rgb(255, 255, 255)");
  expect(await style(page, "#plain", "border-top-color")).toBe("rgb(0, 0, 0)");
  // a window's own style for a button still wins
  expect(await style(page, "#own", "background-color")).toBe("rgb(1, 2, 3)");
  // and the tab strip: the strip the bars' colour, the tab in front the panes', its text the theme's
  expect(await style(page, "#strip", "--dv-tabs-and-actions-container-background-color")).toBe("#7a2a2a");
  expect(await style(page, "#strip", "--dv-activegroup-visiblepanel-tab-background-color")).toBe("#bb3e3e");
  expect(await style(page, "#strip", "--dv-activegroup-visiblepanel-tab-color")).toBe("#ffffff");
  // WebKit (the Mac's) has no scrollbar-color: its scroll bars are the system's overlay ones
  if (test.info().project.name === "chromium") expect(await style(page, "html", "scrollbar-color")).toBe("rgb(205, 177, 177) rgb(187, 62, 62)");
});

test("Light keeps the system's own buttons and scroll bars, and dockview's tab strip", async ({ page }) => {
  await openLook(page);
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  expect(await style(page, "#plain", "background-color")).not.toBe("rgb(255, 255, 255)");
  if (test.info().project.name === "chromium") expect(await style(page, "html", "scrollbar-color")).toBe("auto");
  expect(await style(page, "#strip", "--dv-tabs-and-actions-container-background-color")).toBe("#f3f3f3");
});
