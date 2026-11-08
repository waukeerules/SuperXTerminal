// Settings > General > Look's themes of your own: Edit Themes… makes one, its colours drawn on the whole app as
// they're picked (the window's background, selected rows, checkboxes..., and a script's words), Save keeps them and
// Use This Theme makes it the app's; Cancel and Escape put the app back; a theme in use that's renamed stays in use,
// and one deleted goes back to Follow the system. The Rust side (UiThemes.json in the user's folder, and the settings) is stood in for by
// mockIPC, which keeps what's saved as the app would.
import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import UiThemesEditor from "./UiThemesEditor.svelte";
import { applyTheme, isDark, ownThemeColours, settings } from "./store.svelte";
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

  it("gives a script's words colours of its own, under Script colours, for the script box", async () => {
    open();
    await userEvent.click(screen.getByRole("button", { name: "New" }));
    await fireEvent.input(screen.getByLabelText("Comments"), { target: { value: "#00ff00" } });
    await userEvent.click(screen.getByRole("button", { name: "Use This Theme" }));
    expect(file[0].colors).toEqual({ scriptComment: "#00ff00" });
    expect(ownThemeColours()).toMatchObject({ scriptComment: "#00ff00" });
  });

  it("takes a colour's code typed or pasted, and puts back one that isn't a colour", async () => {
    open();
    await userEvent.click(screen.getByRole("button", { name: "New" }));
    const code = screen.getByLabelText("Window background code") as HTMLInputElement;
    const picker = screen.getByLabelText("Window background") as HTMLInputElement;
    // typed: taken as soon as it's whole, the picker following
    await userEvent.clear(code);
    await userEvent.type(code, "#102030");
    expect(token("--bg-window")).toBe("#102030");
    expect(picker.value).toBe("#102030");
    // pasted as copied from elsewhere: no #, capitals, a space
    const accent = screen.getByLabelText("Selected rows and highlights code") as HTMLInputElement;
    await userEvent.clear(accent);
    await userEvent.click(accent);
    await userEvent.paste(" FF8800 ");
    expect(token("--accent")).toBe("#ff8800");
    // the short form, once the field is left
    const check = screen.getByLabelText("Checkboxes code") as HTMLInputElement;
    await userEvent.clear(check);
    await userEvent.type(check, "#2a4{Enter}");
    expect(token("--check")).toBe("#22aa44");
    expect(check.value).toBe("#22aa44");
    // not a colour: nothing changes, and leaving the field puts the colour's code back
    await userEvent.clear(code);
    await userEvent.type(code, "blue");
    await userEvent.tab();
    expect(token("--bg-window")).toBe("#102030");
    expect(code.value).toBe("#102030");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(file[0].colors).toEqual({ bgWindow: "#102030", accent: "#ff8800", check: "#22aa44" });
  });

  it("shows a sample script in the Preview, drawn in the theme being edited as its colours are picked", async () => {
    open();
    await userEvent.click(screen.getByRole("button", { name: "New" }));
    // the Preview's page (Monaco) stood in for: what the box sends it recorded, and the page saying it's ready
    const frame = document.querySelector<HTMLIFrameElement>('iframe[title="Preview"]')!;
    const sent: { type: string; [key: string]: unknown }[] = [];
    frame.contentWindow!.postMessage = ((envelope: { message: { type: string } }) => sent.push(envelope.message)) as typeof window.postMessage;
    const tab = new URL(frame.src, location.href).searchParams.get("tab");
    window.dispatchEvent(new MessageEvent("message", {
      data: { channel: "sxt-page", tab, message: { type: "ready" } }, source: frame.contentWindow, origin: location.origin,
    }));
    await expect.poll(() => sent[0]).toMatchObject({ type: "set", language: "spsl", text: expect.stringContaining("WAITFOR /re") });

    // a script colour picked shows in the Preview, though the app's theme is still Follow the system
    await fireEvent.input(screen.getByLabelText("Comments"), { target: { value: "#00ff00" } });
    await expect.poll(() => sent.at(-1)).toMatchObject({ type: "dark", colors: { scriptComment: "#00ff00" } });
    expect(settings.theme).toBe("system");
    // and the window's colours too, which the box is drawn in (its background is the fields')
    await fireEvent.input(screen.getByLabelText("Fields and buttons"), { target: { value: "#203040" } });
    await expect.poll(() => sent.at(-1)).toMatchObject({ colors: { bgInput: "#203040", scriptComment: "#00ff00" } });
    // Based on: light or dark
    await userEvent.selectOptions(screen.getByLabelText("Based on"), "dark");
    await expect.poll(() => sent.at(-1)).toMatchObject({ type: "dark", dark: true });
    await userEvent.selectOptions(screen.getByLabelText("Based on"), "light");
    await expect.poll(() => sent.at(-1)).toMatchObject({ type: "dark", dark: false });

    // Rhai's sample instead
    await userEvent.selectOptions(screen.getByLabelText("Preview script"), "rhai");
    await expect.poll(() => sent.filter((m) => m.type === "language").at(-1)).toEqual({ type: "language", language: "rhai" });
    expect(sent.filter((m) => m.type === "text").at(-1)).toMatchObject({ text: expect.stringContaining("send_line(user);") });
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
    // and its colours, its base's where it leaves one alone, for the script box's Monaco, which app.css can't reach
    expect(ownThemeColours()).toMatchObject({ bgPane: "#fffbe8", bgInput: "#ffffff", text: "#1e1e1e" });
    // a script colour it leaves alone is left to Monaco's own
    expect(ownThemeColours()).not.toHaveProperty("scriptComment");

    settings.theme = "dark";
    applyTheme();
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(token("--bg-pane")).toBe("");
    expect(document.documentElement.dataset.ownTheme).toBeUndefined();
    expect(isDark()).toBe(true);
    expect(ownThemeColours()).toBeNull();
  });
});
