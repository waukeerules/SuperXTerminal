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

/** How many columns the terminal has now */
const cols = (page: Page) => page.evaluate(() => (window as any).terminalDev.term.cols as number);
/** How far the view is panned, in pixels (the side-to-side scroll bar follows it) */
const pan = (page: Page) => page.evaluate(() => document.getElementById("hscroll")!.scrollLeft);
/** Pan as the user does, with the side-to-side scroll bar, and wait for the view to follow it */
async function panTo(page: Page, x: number) {
  await page.evaluate((x) => (document.getElementById("hscroll")!.scrollLeft = x), x);
  await expect.poll(() => page.evaluate(() => document.querySelector("#terminal .xterm-screen")!.getAttribute("style") ?? "")).toContain("translateX");
}

/** A command longer than the terminal is wide, for Wrap Lines off */
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

test("Follow the App's Look is Light while the app is light, and Dark once it goes dark", async ({ page }) => {
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  // the real theme files, as the app gives them
  const themes = await page.evaluate(async () =>
    Promise.all(["light", "dark", "putty"].map((n) => fetch(`/web/output/themes/${n}.theme.json`).then((r) => r.json()))),
  );
  await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", prefs: { theme: "@look" }, themes, dark: false }), themes);
  const background = () => page.evaluate(() => (window as any).terminalDev.term.options.theme.background as string);
  await expect.poll(background).toBe("#ffffff");
  await page.evaluate(() => (window as any).terminalDev.message({ type: "look", dark: true }));
  await expect.poll(background).not.toBe("#ffffff");
  await page.evaluate(() => (window as any).terminalDev.message({ type: "look", dark: false }));
  await expect.poll(background).toBe("#ffffff");
});

test("the find bar and menu follow the terminal's theme: light on Light, dark on PuTTY's", async ({ page }) => {
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  const themes = await page.evaluate(async () =>
    Promise.all(["light", "dark", "putty"].map((n) => fetch(`/web/output/themes/${n}.theme.json`).then((r) => r.json()))),
  );
  const findBackground = () => page.evaluate(() => getComputedStyle(document.getElementById("find")!).backgroundColor);
  await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", prefs: { theme: "Light" }, themes }), themes);
  await expect.poll(findBackground).toBe("rgb(243, 243, 243)");
  await page.evaluate(() => (window as any).terminalDev.message({ type: "prefs", prefs: { theme: "PuTTY" } }));
  await expect.poll(findBackground).toBe("rgb(43, 43, 43)");
});

test("the terminal's own colours go over the theme's, and a light background gets the light menu", async ({ page }) => {
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  const themes = await page.evaluate(async () => [await fetch("/web/output/themes/putty.theme.json").then((r) => r.json())]);
  const colours = () => page.evaluate(() => {
    const t = (window as any).terminalDev.term.options.theme;
    return [t.background, t.foreground, t.selectionBackground, document.documentElement.classList.contains("light-theme")];
  });
  await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", prefs: { theme: "PuTTY" }, themes }), themes);
  await expect.poll(colours).toEqual(["#000000", "#bbbbbb", "#264f78", false]);
  await page.evaluate(() => (window as any).terminalDev.message({ type: "prefs",
    prefs: { theme: "PuTTY", colorBackground: "#fdf6e3", colorForeground: "#333333", colorSelection: "#eee8d5" } }));
  await expect.poll(colours).toEqual(["#fdf6e3", "#333333", "#eee8d5", true]);
  // the theme's again
  await page.evaluate(() => (window as any).terminalDev.message({ type: "prefs", prefs: { theme: "PuTTY" } }));
  await expect.poll(colours).toEqual(["#000000", "#bbbbbb", "#264f78", false]);
});

test("a profile's colours win over Settings > Terminal's", async ({ page }) => {
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  const themes = await page.evaluate(async () => [await fetch("/web/output/themes/putty.theme.json").then((r) => r.json())]);
  await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", themes,
    prefs: { theme: "PuTTY", colorBackground: "#fdf6e3", colorForeground: "#333333" },
    colors: { background: "#3a0000", foreground: "", selection: "" } }), themes);
  await expect.poll(() => page.evaluate(() => {
    const t = (window as any).terminalDev.term.options.theme;
    return [t.background, t.foreground, document.documentElement.classList.contains("light-theme")];
  })).toEqual(["#3a0000", "#333333", false]);
});

test("an open tab takes its profile's new colours when the profile is saved", async ({ page }) => {
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  const themes = await page.evaluate(async () => [await fetch("/web/output/themes/light.theme.json").then((r) => r.json())]);
  await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", themes, prefs: { theme: "Light" } }), themes);
  const background = () => page.evaluate(() => (window as any).terminalDev.term.options.theme.background as string);
  const light = await background();
  await page.evaluate(() => (window as any).terminalDev.message({ type: "colors", colors: { background: "#3a0000" } }));
  await expect.poll(background).toBe("#3a0000");
  // its colour taken away again: the theme's
  await page.evaluate(() => (window as any).terminalDev.message({ type: "colors", colors: { background: "" } }));
  await expect.poll(background).toBe(light);
});

test("the cursor: the session's environment colour, else the profile's, else Settings > Terminal's", async ({ page }) => {
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  const themes = await page.evaluate(async () => [await fetch("/web/output/themes/putty.theme.json").then((r) => r.json())]);
  const cursor = () => page.evaluate(() => (window as any).terminalDev.term.options.theme.cursor as string);
  await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", themes,
    prefs: { theme: "PuTTY", colorCursor: "#ffcc00" }, colors: { cursor: "#ff00ff" } }), themes);
  await expect.poll(cursor).toBe("#ff00ff");
  await page.evaluate(() => (window as any).terminalDev.message({ type: "colors", colors: {} }));
  await expect.poll(cursor).toBe("#ffcc00");
  // a production session's red wins over both
  await page.evaluate(() => (window as any).terminalDev.message({ type: "cursorColor", color: "#e5534b" }));
  await expect.poll(cursor).toBe("#e5534b");
});

test("a profile's Light or Dark wins over Settings > Terminal's theme, and its tab's menu says so", async ({ page }) => {
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  const themes = await page.evaluate(async () =>
    Promise.all(["light", "dark", "putty"].map((n) => fetch(`/web/output/themes/${n}.theme.json`).then((r) => r.json()))),
  );
  const background = () => page.evaluate(() => (window as any).terminalDev.term.options.theme.background as string);
  await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", themes, prefs: { theme: "PuTTY" }, profileTheme: "Light" }), themes);
  await expect.poll(background).toBe(themes[0].colors.background);
  // the profile saved as Dark
  await page.evaluate(() => (window as any).terminalDev.message({ type: "colors", colors: {}, theme: "Dark" }));
  await expect.poll(background).toBe(themes[1].colors.background);
  // and as following the theme: PuTTY's
  await page.evaluate(() => (window as any).terminalDev.message({ type: "colors", colors: {}, theme: "" }));
  await expect.poll(background).toBe("#000000");
});

test("Clear Scrollback stays cleared when a theme with its own font size refits the terminal", async ({ page }) => {
  // reported on Windows: the cleared lines came back after a theme change. The page itself must not bring them back
  // as the theme's font refits the columns and rows (bigger, then back); Windows' pseudo console is outside this test
  await page.goto("/web/terminal/index.html");
  await page.waitForFunction(() => !!(window as any).terminalDev);
  const themes = [{ name: "PuTTY", colors: {} }, { name: "Big", colors: {}, font: { size: 22 } }];
  await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", prefs: { theme: "PuTTY" }, themes }), themes);
  await page.waitForFunction(() => (window as any).terminalDev.term.cols > 2);
  await feed(page, Array.from({ length: 500 }, (_, i) => `line ${i + 1}\r\n`).join("") + "$ ");
  /** The first line kept, and how many rows the scrollback and screen hold */
  const kept = () => page.evaluate(() => {
    const buffer = (window as any).terminalDev.term.buffer.active;
    return { first: buffer.getLine(0)?.translateToString(true) as string, length: buffer.length as number };
  });
  await expect.poll(async () => (await kept()).first).toBe("line 1");
  await page.evaluate(() => (window as any).terminalDev.message({ type: "action", name: "clearScrollback" }));
  await expect.poll(async () => (await kept()).length).toBe(await page.evaluate(() => (window as any).terminalDev.term.rows));
  const cleared = (await kept()).first;
  const size = () => page.evaluate(() => `${(window as any).terminalDev.term.cols}x${(window as any).terminalDev.term.rows}`);
  for (const theme of ["Big", "PuTTY"]) {
    const before = await size();
    await page.evaluate((theme) => (window as any).terminalDev.message({ type: "prefs", prefs: { theme } }), theme);
    await expect.poll(size).not.toBe(before);
    // a smaller screen pushes its top rows into the scrollback, but nothing from before the clear comes back
    expect((await kept()).first).toBe(cleared);
  }
});

test.describe("Line Numbers", () => {
  /** The gutter's numbers, a row each ("" for a row without one) */
  const numbers = (page: Page) => page.evaluate(() => {
    const gutter = document.getElementById("gutter")!;
    return getComputedStyle(gutter).display === "none" ? null : gutter.textContent!.split("\n");
  });
  const lines = (n: number, from = 1) => Array.from({ length: n }, (_, i) => `line ${i + from}`).join("\r\n") + "\r\n";

  test("are off unless turned on", async ({ page }) => {
    await openTerminal(page, {});
    await feed(page, lines(3));
    await expect.poll(() => numbers(page)).toBeNull();
  });

  test("number each line once, the rows a long line wraps onto and those under the cursor not at all", async ({ page }) => {
    await openTerminal(page, { lineNumbers: "true" });
    const cols = await page.evaluate(() => (window as any).terminalDev.term.cols as number);
    await feed(page, "one\r\n" + "x".repeat(cols + 5) + "\r\nthree\r\n$ ");
    await expect.poll(async () => (await numbers(page))?.slice(0, 6)).toEqual(["1", "2", "", "3", "4", ""]);
  });

  test("go while a full-screen program (less) runs, and come back after it", async ({ page }) => {
    await openTerminal(page, { lineNumbers: "true" });
    await feed(page, lines(3));
    await expect.poll(async () => (await numbers(page))?.[0]).toBe("1");
    const colsBefore = await page.evaluate(() => (window as any).terminalDev.term.cols as number);
    await feed(page, "\x1b[?1049h\x1b[Hless's page");
    await expect.poll(() => numbers(page)).toBeNull();
    // the program gets the gutter's room
    await expect.poll(() => page.evaluate(() => (window as any).terminalDev.term.cols as number)).toBeGreaterThan(colsBefore);
    await feed(page, "\x1b[?1049l");
    await expect.poll(async () => (await numbers(page))?.slice(0, 3)).toEqual(["1", "2", "3"]);
  });

  test("keep counting as a full scrollback drops its oldest lines", async ({ page }) => {
    await openTerminal(page, { lineNumbers: "true", scrollback: "20" });
    await feed(page, lines(200));
    const rows = await page.evaluate(() => (window as any).terminalDev.term.rows as number);
    // the last line written is 200, and the cursor's row (where the prompt goes) is 201
    await expect.poll(async () => ((await numbers(page)) ?? []).filter(Boolean).slice(-2)).toEqual(["200", "201"]);
    expect(rows).toBeGreaterThan(2);
  });

  test("count the lines a full scrollback dropped before they were turned on", async ({ page }) => {
    // the dropped lines were only counted from the gutter's first drawing, so output that filled the scrollback
    // before it (a burst as the tab opened, or Line Numbers turned on later) started the numbers again near 1
    await openTerminal(page, { scrollback: "20" });
    await feed(page, lines(200));
    // all written (and the oldest dropped) before the numbers are turned on
    await expect.poll(() => page.evaluate(() => {
      const buffer = (window as any).terminalDev.term.buffer.active;
      return buffer.getLine(buffer.baseY + buffer.cursorY - 1)?.translateToString(true);
    })).toBe("line 200");
    await page.evaluate(() => (window as any).terminalDev.message({ type: "prefs", prefs: { lineNumbers: "true", scrollback: "20" } }));
    await expect.poll(async () => ((await numbers(page)) ?? []).filter(Boolean).slice(-2)).toEqual(["200", "201"]);
  });
});

test("Line Numbers with Wrap Lines off: the gutter stays put while the view pans", async ({ page }) => {
  await openTerminal(page, { lineNumbers: "true", wrap: "false" });
  await feed(page, "Router#show running-config\r\n" + "interface GigabitEthernet1/0/1 description " + "z".repeat(250) + "\r\nRouter#");
  await expect.poll(() => page.evaluate(() => document.getElementById("gutter")!.textContent!.split("\n").slice(0, 3))).toEqual(["1", "2", "3"]);
  expect(await cols(page)).toBe(300);
  await panTo(page, 400);
  const gutterLeft = await page.evaluate(() => document.getElementById("gutter")!.getBoundingClientRect().left);
  expect(gutterLeft).toBe(0);
});

test("Line Numbers take a button's colour for its command's lines, until a command is typed", async ({ page }) => {
  await openTerminal(page, { lineNumbers: "true" });
  /** Each numbered row's number and colour ("" for the usual) */
  const coloured = () => page.evaluate(() => {
    const gutter = document.getElementById("gutter")!;
    const out: string[] = [];
    gutter.childNodes.forEach((node) => {
      if (node.nodeType === Node.TEXT_NODE) node.textContent!.split("\n").filter(Boolean).forEach((n) => out.push(`${n}`));
      else out.push(`${node.textContent}:${(node as HTMLElement).style.color}`);
    });
    return out;
  });
  await feed(page, "$ ");
  // the button "ls -la", purple: its echo and output
  await page.evaluate(() => (window as any).terminalDev.message({ type: "commandColor", color: "#a371f7" }));
  await feed(page, "ls -la\r\ntotal 8\r\nfile\r\n$ ");
  await expect.poll(coloured).toEqual(["1:rgb(163, 113, 247)", "2:rgb(163, 113, 247)", "3:rgb(163, 113, 247)", "4:rgb(163, 113, 247)"]);
  // the user types a command at the prompt: it and its output have the usual colour
  await page.evaluate(() => (window as any).terminalDev.term.input("\r", true));
  await feed(page, "\r\nuptime\r\n$ ");
  await expect.poll(async () => (await coloured()).slice(3)).toEqual(["4", "5", "6"]);
});

test.describe("Shift+Insert and Ctrl+Shift+V", () => {
  for (const keys of ["Shift+Insert", "Control+Shift+V"]) {
    test(`${keys} pastes once, through TerminalControl, not again as the browser's own paste`, async ({ page, context, browserName }) => {
      // the browser's own paste has the clipboard's text too, and pasted it twice. Not every browser pastes on these
      // keys (headless Chromium and WebKit don't on Ctrl+Shift+V; WebView2 does), so the key press is also checked to
      // have stopped it, which holds in any browser
      if (browserName === "chromium") await context.grantPermissions(["clipboard-read", "clipboard-write"]);
      await openTerminal(page, {});
      await page.evaluate(() => {
        // what the page sends TerminalControl, through host/webview.js (to the main window: here, the page itself)
        (window as any).sent = [];
        window.addEventListener("message", (e) => { if (e.data?.channel === "sxt-page") (window as any).sent.push(e.data.message); });
        // whether each key press was stopped from going on to the browser's paste, seen once xterm.js has had it
        (window as any).stopped = [];
        window.addEventListener("keydown", (e) => { if (!["Shift", "Control"].includes(e.key)) (window as any).stopped.push(e.defaultPrevented); });
      });
      if (browserName === "chromium") await page.evaluate(() => navigator.clipboard.writeText("FastEthernet0/5"));
      await page.locator("#terminal").click();
      await page.keyboard.press(keys);
      await page.waitForTimeout(200);
      const pastes = await page.evaluate(() => (window as any).sent.filter((m: any) => m.type === "pasteRequest"));
      expect(pastes).toEqual([{ type: "pasteRequest" }]);
      expect(await page.evaluate(() => (window as any).stopped)).toEqual([true]);
    });
  }
});

test.describe("Windows' pseudo console redrawing its screen after a resize (ConPTY)", () => {
  /** A 60-line listing, then the prompt */
  const LISTING = Array.from({ length: 60 }, (_, i) => `old ${i + 1}\r\n`).join("") + "$ ";
  /**
   * What ConPTY sends after the tab's new size reaches it, as seen from cmd on Windows 11: its whole screen again,
   * from its own copy, between hiding and showing the cursor. Its copy still holds the listing, whatever the tab cleared.
   * Only the first redraw of a quick run of resizes names a size (ESC[8;rows;cols t), and it's the size that redraw
   * was for, not the one the terminal ends up; `named` is false for the ones after it.
   */
  const redraw = (cols: number, rows: number, named = true) =>
    `\x1b[?25l${named ? `\x1b[8;${rows};${cols}t` : ""}\x1b[H` +
    Array.from({ length: rows - 1 }, (_, i) => `old ${60 - rows + 2 + i}\x1b[K\r\n`).join("") +
    `$ \x1b[K\x1b[${rows};3H\x1b[?25h`;
  /** Every row the terminal holds, scrollback and screen */
  const rows = (page: Page) => page.evaluate(() => {
    const buffer = (window as any).terminalDev.term.buffer.active;
    return Array.from({ length: buffer.length as number }, (_, i) => (buffer.getLine(i).translateToString(true) as string).trimEnd());
  });
  /** The terminal's size, as "colsxrows" */
  const size = (page: Page) => page.evaluate(() => `${(window as any).terminalDev.term.cols}x${(window as any).terminalDev.term.rows}`);

  /** Open the page with a theme that has its own font size, and the listing on screen */
  async function listed(page: Page) {
    await page.goto("/web/terminal/index.html");
    await page.waitForFunction(() => !!(window as any).terminalDev);
    const themes = [{ name: "PuTTY", colors: {} }, { name: "Big", colors: {}, font: { size: 22 } }];
    await page.evaluate((themes) => (window as any).terminalDev.message({ type: "config", prefs: { theme: "PuTTY" }, themes }), themes);
    await page.waitForFunction(() => (window as any).terminalDev.term.cols > 2);
    await feed(page, LISTING);
    await expect.poll(async () => (await rows(page)).some((r) => r === "old 1")).toBe(true);
  }

  /**
   * Change to the big theme, which refits the terminal, then have ConPTY redraw as it did for two quick resizes: once
   * naming a size on the way, then again at the new size without naming it
   */
  async function themeChanged(page: Page) {
    const before = await size(page);
    await page.evaluate(() => (window as any).terminalDev.message({ type: "prefs", prefs: { theme: "Big" } }));
    await expect.poll(() => size(page)).not.toBe(before);
    const [cols, rowCount] = (await size(page)).split("x").map(Number);
    await feed(page, redraw(cols + 10, rowCount + 5) + redraw(cols, rowCount, false));
  }

  test("Clear Screen and Scrollback stays cleared when a theme change resizes the terminal", async ({ page }) => {
    await listed(page);
    await page.locator("#terminal").click({ button: "right" });
    await page.locator("#menu").getByText("Clear Screen and Scrollback", { exact: true }).click();
    await expect.poll(async () => (await rows(page)).filter(Boolean)).toEqual(["$"]);
    await themeChanged(page);
    // what's typed and printed after it still shows, on the prompt's line
    await feed(page, "ls\r\nfile\r\n$ ");
    await expect.poll(async () => (await rows(page)).filter(Boolean)).toEqual(["$ ls", "file", "$"]);
  });

  test("without a clear, the redraw is drawn as ConPTY sent it", async ({ page }) => {
    await listed(page);
    await themeChanged(page);
    const [, rowCount] = (await size(page)).split("x").map(Number);
    await expect.poll(async () => (await rows(page)).slice(-rowCount, -rowCount + 1)).toEqual([`old ${60 - rowCount + 2}`]);
  });
});
