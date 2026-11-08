// Settings > General > Look's themes of your own: Edit Themes… makes one, its colours drawn on the whole app as
// they're picked (the window's background, selected rows, checkboxes...), Save keeps them and Use This Theme makes
// it the app's; Cancel and Escape put the app back; a theme in use that's renamed stays in use, and one deleted goes
// back to Follow the system. The Rust side (UiThemes.json in the user's folder, and the settings) is stood in for by
// mockIPC, which keeps what's saved as the app would.
import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import UiThemesEditor from "./UiThemesEditor.svelte";
import { applyTheme, isDark, settings } from "./store.svelte";
import { uiThemes, type UiTheme } from "./uiThemes.svelte";

/** The themes as UiThemes.json has them, saved by ui_themes_save */
let file: UiTheme[] = [];
/** How many times the window closed */
let closed = 0;

/** A colour token as the page has it set now ("" when it's app.css's own) */
const token = (css: string) => document.documentElement.style.getPropertyValue(css);

beforeEach(() => {
  file = [];
  closed = 0;
  uiThemes.list = [];
  settings.theme = "system";
  applyTheme();
  mockIPC((cmd, args) => {
    if (cmd === "ui_themes_save") {
      file = (args as { themes: UiTheme[] }).themes.map((t) => ({ ...t, name: t.name.trim() }));
      return file;
    }
    if (cmd === "settings_save") return (args as { settings: unknown }).settings;
    return null;
  });
});

/** Open Edit Themes; it goes when closed, as Settings takes it away */
function open() {
  const shown = render(UiThemesEditor, { onClose: () => (closed++, shown.unmount()) });
  return shown;
}

describe("Edit Themes", () => {
  it("makes a theme whose colours show on the app as they're picked, and Use This Theme keeps and uses it", async () => {
    open();
    await userEvent.click(screen.getByRole("button", { name: "New" }));
    await fireEvent.input(screen.getByLabelText("Window background"), { target: { value: "#102030" } });
    await fireEvent.input(screen.getByLabelText("Selected rows and highlights"), { target: { value: "#ff8800" } });
    await fireEvent.input(screen.getByLabelText("Checkboxes"), { target: { value: "#22aa44" } });
    expect(token("--bg-window")).toBe("#102030");
    expect(token("--accent")).toBe("#ff8800");
    expect(token("--check")).toBe("#22aa44");

    const name = screen.getByLabelText("Name");
    await userEvent.clear(name);
    await userEvent.type(name, "Harbour");
    await userEvent.click(screen.getByRole("button", { name: "Use This Theme" }));

    expect(file).toEqual([{ name: "Harbour", base: expect.any(String), colors: { bgWindow: "#102030", accent: "#ff8800", check: "#22aa44" } }]);
    expect(settings.theme).toBe("Harbour");
    expect(token("--check")).toBe("#22aa44");
    expect(closed).toBe(1);
  });

  it("puts the app back as it was on Cancel, and on Escape", async () => {
    uiThemes.list = [{ name: "Harbour", base: "dark", colors: { accent: "#ff8800" } }];
    open();
    await fireEvent.input(screen.getByLabelText("Selected rows and highlights"), { target: { value: "#00ff00" } });
    expect(token("--accent")).toBe("#00ff00");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(token("--accent")).toBe("");
    expect(file).toEqual([]);

    open();
    await fireEvent.input(screen.getByLabelText("Selected rows and highlights"), { target: { value: "#00ff00" } });
    await userEvent.keyboard("{Escape}");
    expect(token("--accent")).toBe("");
    expect(closed).toBe(2);
  });

  it("keeps a renamed theme in use, and goes back to Follow the system for one deleted", async () => {
    uiThemes.list = [{ name: "Harbour", base: "dark", colors: { accent: "#ff8800" } }];
    settings.theme = "Harbour";
    open();
    const name = screen.getByLabelText("Name");
    await userEvent.clear(name);
    await userEvent.type(name, "Night Harbour");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(settings.theme).toBe("Night Harbour");
    expect(token("--accent")).toBe("#ff8800");

    open();
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(file).toEqual([]);
    expect(settings.theme).toBe("system");
    expect(token("--accent")).toBe("");
  });

  it("says why a set can't be saved, and stays open", async () => {
    mockIPC((cmd) => {
      if (cmd === "ui_themes_save") throw "There are two themes named \"Harbour\".";
      return null;
    });
    uiThemes.list = [{ name: "Harbour", base: "dark", colors: {} }];
    open();
    await userEvent.click(screen.getByRole("button", { name: "Duplicate" }));
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText(/two themes named/)).toBeTruthy();
    expect(closed).toBe(0);
  });
});

describe("a theme of your own in use", () => {
  it("draws the app in its base, light or dark, with its own colours over it", () => {
    uiThemes.list = [{ name: "Paper", base: "light", colors: { bgPane: "#fffbe8" } }];
    settings.theme = "paper";
    applyTheme();
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(token("--bg-pane")).toBe("#fffbe8");
    // marked as a theme of your own, for app.css to draw the system's buttons and scroll bars in its colours
    expect(document.documentElement.dataset.ownTheme).toBe("");
    expect(isDark()).toBe(false);

    settings.theme = "dark";
    applyTheme();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(token("--bg-pane")).toBe("");
    expect(document.documentElement.dataset.ownTheme).toBeUndefined();
    expect(isDark()).toBe(true);
  });
});
