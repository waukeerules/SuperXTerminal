// Settings' search: it finds a setting in any tab, not only the open one, and a result opens its tab with the
// setting flashed. And General's Use PrivateSessions.xml, which can be turned off only while Saved sessions are in use,
// and the Show in Finder / File Explorer beside each sessions file.
// The Rust side is stood in for by mockIPC, which saves the settings and notes the files asked to be shown.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import Settings from "./Settings.svelte";
import { emptyProfile, DEFAULT_PROFILE } from "./profiles";
import { settings } from "./store.svelte";
import { FILE_MANAGER } from "../platform";

/** The settings saved, as settings_save was given them */
let saved: Record<string, unknown>[] = [];
/** The sessions files asked to be shown in the file manager (sessions_reveal's store) */
let revealed: string[] = [];
/** The links opened in the browser (the opener plugin's open_url) */
let opened: string[] = [];

/** What Check 1Password says when op works; null for it to fail with opProblem */
let opCheck: string | null = null;
let opProblem = "";

beforeEach(() => {
  Element.prototype.scrollIntoView = () => {};
  opCheck = null;
  opProblem = "";
  saved = [];
  revealed = [];
  opened = [];
  mockIPC((cmd, args) => {
    if (cmd === "plugin:opener|open_url") return void opened.push((args as { url: string }).url);
    // 1Password's CLI as Check 1Password finds it: working, or what's wrong
    if (cmd === "onepassword_check") return opCheck ? Promise.resolve(opCheck) : Promise.reject(opProblem);
    if (cmd === "sessions_reveal") {
      const store = (args as { store: string }).store;
      revealed.push(store);
      // Sessions.xml not made yet: its folder is shown, with a note
      return store === "saved" ? "There's no Sessions.xml yet: it's made in this folder when the first Saved session is saved." : null;
    }
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

  it("has the tab list (▾) open Switch Session when ticked under Tabs", async () => {
    const user = userEvent.setup();
    Object.assign(settings, { tabListSwitchSession: false });
    render(Settings, { onClose: () => {} });
    const box = screen.getByRole("checkbox", { name: /The tab list \(▾\) opens Switch Session/ }) as HTMLInputElement;
    expect(box.checked).toBe(false);
    await user.click(box);
    await expect.poll(() => saved.at(-1)?.tabListSwitchSession).toBe(true);
  });

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

describe("Scripts", () => {
  it("says what each kind of script is, and its links open in the browser, not in the app's window", async () => {
    const user = userEvent.setup();
    render(Settings, { onClose: () => {} });
    // each kind the New scripts are list offers is named, with what it is under it
    for (const kind of ["Rhai scripts", "SPSL scripts", "Lines to send"]) expect(screen.getByText(kind, { selector: "dt" })).toBeTruthy();
    expect(screen.getByText(/each line is typed into the session as it is/)).toBeTruthy();
    await user.click(screen.getByRole("link", { name: "rhai.rs" }));
    await user.click(screen.getByRole("link", { name: "Rhai in SuperXTerminal" }));
    await user.click(screen.getByRole("link", { name: "SPSL in SuperXTerminal" }));
    expect(opened).toEqual([
      "https://rhai.rs/",
      "https://github.com/waukeerules/SuperXTerminal/blob/main/docs/RHAI.md",
      "https://github.com/waukeerules/SuperXTerminal/blob/main/docs/SPSL.md",
    ]);
  });
});

describe("Command bar's days", () => {
  it("are saved as typed, 0 keeping the history only until the app closes", async () => {
    const user = userEvent.setup();
    render(Settings, { onClose: () => {} });
    const days = screen.getByRole("spinbutton", { name: "Keep sent commands for" });
    await user.clear(days);
    await user.type(days, "7");
    await user.tab();
    await expect.poll(() => saved.at(-1)?.commandHistoryDays).toBe(7);
    await user.clear(days);
    await user.type(days, "0");
    await user.tab();
    await expect.poll(() => saved.at(-1)?.commandHistoryDays).toBe(0);
  });
});

describe("Show in the file manager beside the sessions files", () => {
  /** The Show in Finder / File Explorer buttons, Sessions.xml's first */
  const showButtons = () => screen.getAllByRole("button", { name: `Show in ${FILE_MANAGER}` }) as HTMLButtonElement[];

  it("shows Sessions.xml and PrivateSessions.xml, each by its own button", async () => {
    const user = userEvent.setup();
    Object.assign(settings, { settingsFolder: "/team", useSessionsXml: true, usePrivateSessions: true });
    render(Settings, { onClose: () => {} });
    const [savedButton, privateButton] = showButtons();
    await user.click(savedButton);
    await user.click(privateButton);
    expect(revealed).toEqual(["saved", "private"]);
  });

  it("says why only the folder showed when the file isn't made yet, and not once it's there", async () => {
    const user = userEvent.setup();
    Object.assign(settings, { settingsFolder: "/team", useSessionsXml: true, usePrivateSessions: true });
    render(Settings, { onClose: () => {} });
    const [savedButton, privateButton] = showButtons();
    await user.click(savedButton);
    expect(await screen.findByText(/There's no Sessions\.xml yet/)).toBeTruthy();
    await user.click(privateButton);
    await expect.poll(() => screen.queryByText(/There's no Sessions\.xml yet/)).toBeNull();
  });

  it("can't show Sessions.xml before a settings folder is chosen, but can show PrivateSessions.xml", () => {
    Object.assign(settings, { settingsFolder: null, useSessionsXml: true, usePrivateSessions: true });
    render(Settings, { onClose: () => {} });
    const [savedButton, privateButton] = showButtons();
    expect(savedButton.disabled).toBe(true);
    expect(privateButton.disabled).toBe(false);
  });
});

describe("Settings > General > 1Password", () => {
  it("says what 1Password's CLI can read when it works", async () => {
    opCheck = "1Password's CLI 2.42.0 works, through the 1Password app: 3 vaults it can read.";
    render(Settings, { onClose: () => {} });
    await userEvent.click(screen.getByRole("button", { name: "Check 1Password" }));
    expect(await screen.findByText(opCheck)).toBeTruthy();
    expect(screen.queryByText(/winget install 1password-cli/)).toBeNull();
    // the app's own guide, on GitHub
    await userEvent.click(screen.getByRole("link", { name: "How SuperXTerminal uses 1Password" }));
    expect(opened).toEqual(["https://github.com/waukeerules/SuperXTerminal/blob/main/docs/ONEPASSWORD.md"]);
  });

  it("says what's wrong, and how to install the CLI, with its getting-started page", async () => {
    opProblem = "1Password's CLI (op) isn't installed: winget install 1password-cli, then turn on Integrate with 1Password CLI in 1Password's Settings > Developer (https://www.1password.dev/cli/get-started).";
    render(Settings, { onClose: () => {} });
    await userEvent.click(screen.getByRole("button", { name: "Check 1Password" }));
    expect(await screen.findByText(opProblem)).toBeTruthy();
    // this test's jsdom isn't a Mac: winget's command
    expect(screen.getByText("winget install 1password-cli").tagName).toBe("CODE");
    await userEvent.click(screen.getByRole("link", { name: "1Password CLI: get started" }));
    expect(opened).toEqual(["https://www.1password.dev/cli/get-started"]);
  });
});
