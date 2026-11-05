// Settings' search: it finds a setting in any tab, not only the open one, and a result opens its tab with the
// setting flashed. And General's Use PrivateSessions.xml, which can be turned off only while Saved sessions are in use.
// The Rust side is stood in for by mockIPC, which saves the settings.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import Settings from "./Settings.svelte";
import { emptyProfile, DEFAULT_PROFILE } from "./profiles";
import { settings } from "./store.svelte";

/** The settings saved, as settings_save was given them */
let saved: Record<string, unknown>[] = [];

beforeEach(() => {
  Element.prototype.scrollIntoView = () => {};
  saved = [];
  mockIPC((cmd, args) => {
    if (cmd === "profiles_list") return [emptyProfile(DEFAULT_PROFILE)];
    if (cmd === "shortcuts_keys") return [{ id: "newTab", label: "New Local Terminal", key: "CmdOrCtrl+T", default: "CmdOrCtrl+T" }];
    if (cmd === "shortcuts_reserved") return [];
    if (cmd === "highlighting_load") return { themes: [], ruleSets: [], problems: [] };
    if (cmd === "settings_save") {
      const next = (args as { settings: Record<string, unknown> }).settings;
      saved.push(next);
      return next;
    }
    return null;
  });
});

describe("Settings' search", () => {
  it("finds settings in the other tabs and opens one", async () => {
    const user = userEvent.setup();
    render(Settings, { onClose: () => {} });
    await user.type(screen.getByRole("searchbox", { name: "Search settings" }), "cursor blinks");
    const results = await screen.findByRole("list", { name: "Settings found" });
    const hit = await within(results).findByRole("button", { name: /Terminal › Look\s*Cursor blinks/ });
    await user.click(hit);
    expect(screen.getByRole("button", { name: "Terminal" }).classList.contains("active")).toBe(true);
    expect(screen.queryByRole("list", { name: "Settings found" })).toBeNull();
    await expect.poll(() => document.querySelector(".found")?.textContent).toContain("Cursor blinks");
  });

  it("finds a shortcut and a profile's field by name", async () => {
    const user = userEvent.setup();
    render(Settings, { onClose: () => {} });
    const box = screen.getByRole("searchbox", { name: "Search settings" });
    await user.type(box, "local terminal");
    const results = await screen.findByRole("list", { name: "Settings found" });
    expect(await within(results).findByRole("button", { name: /Shortcuts\s*New Local Terminal/ })).toBeTruthy();
    await user.clear(box);
    await user.type(box, "login username");
    expect(await within(await screen.findByRole("list", { name: "Settings found" })).findByRole("button", { name: /Profiles › Login\s*Username/ })).toBeTruthy();
  });

  it("says when nothing matches", async () => {
    const user = userEvent.setup();
    render(Settings, { onClose: () => {} });
    await user.type(screen.getByRole("searchbox", { name: "Search settings" }), "zzzz");
    expect(screen.getByText(/No settings match/)).toBeTruthy();
  });
});

describe("Use PrivateSessions.xml", () => {
  /** The checkbox, in General */
  const privateBox = () => screen.getByRole("checkbox", { name: /Use PrivateSessions\.xml/ }) as HTMLInputElement;

  it("can't be turned off until Saved sessions are in use: sessions need somewhere to go", async () => {
    Object.assign(settings, { settingsFolder: null, useSessionsXml: true, usePrivateSessions: true });
    render(Settings, { onClose: () => {} });
    expect(privateBox().checked).toBe(true);
    expect(privateBox().disabled).toBe(true);
    expect(screen.getByText(/Private sessions stay on until a settings folder is chosen/)).toBeTruthy();
  });

  it("turns off with a settings folder, and then keeps Saved sessions on", async () => {
    const user = userEvent.setup();
    Object.assign(settings, { settingsFolder: "/team", useSessionsXml: true, usePrivateSessions: true });
    render(Settings, { onClose: () => {} });
    expect(privateBox().disabled).toBe(false);
    await user.click(privateBox());
    await expect.poll(() => saved.at(-1)?.usePrivateSessions).toBe(false);
    await expect.poll(() => (screen.getByRole("checkbox", { name: /Use Sessions\.xml/ }) as HTMLInputElement).disabled).toBe(true);
    Object.assign(settings, { usePrivateSessions: true });
  });
});
