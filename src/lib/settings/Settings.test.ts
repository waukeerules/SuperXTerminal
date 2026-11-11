// Settings' search: it finds a setting in any tab, not only the open one, and a result opens its tab with the
// setting flashed. And General's Use PrivateSessions.xml, which can be turned off only while Saved sessions are in use,
// and the Show in Finder / File Explorer beside each sessions file; choosing a settings folder, or typing one (which must be there), that can't be saved to
// asks first; the seconds between checks of the session files; Look's themes of your own, and Tabs' Longest tab name, Grid View, Grid View off and Remote Desktop in Grid View.
// The Rust side is stood in for by mockIPC, which saves the settings and notes the files asked to be shown.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import Settings from "./Settings.svelte";
import { emptyProfile, DEFAULT_PROFILE } from "./profiles";
import { noFolderMessage, settings } from "./store.svelte";
import { uiThemes } from "./uiThemes.svelte";
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
  settings.onePassword = false;
  opCheck = null;
  opProblem = "";
  saved = [];
  revealed = [];
  opened = [];
  mockIPC((cmd, args) => {
    if (cmd === "open_link") return void opened.push((args as { url: string }).url);
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

describe("Settings > General > Settings folder", () => {
  /** The folder chosen before, and three to choose: Program Files, a read-only share, and one of the user's own */
  const TEAM = String.raw`C:\Users\me\Team`;
  const PROGRAM_FILES = String.raw`C:\Program Files\SuperXTerminal`;
  const SHARE = String.raw`\\fileserver\team-settings`;
  const OWN = String.raw`C:\Users\me\OneDrive\Team`;

  /** The folders settings_set_folder was given, and the warnings shown */
  let used: string[];
  let warned: { message: string; buttons?: unknown }[];

  /** Choose `folder` with Choose…, the core saying whether this account can write there; `answer` to the warning */
  async function choose(folder: string, canWrite: boolean, answer: "Use It" | "Cancel"): Promise<void> {
    used = [];
    warned = [];
    mockIPC((cmd, args) => {
      if (cmd === "plugin:dialog|open") return folder;
      if (cmd === "folder_writable") return canWrite;
      if (cmd === "plugin:dialog|message") {
        warned.push(args as { message: string; buttons?: unknown });
        return answer;
      }
      if (cmd === "settings_set_folder") return void used.push((args as { path: string }).path);
      if (cmd === "profiles_list") return [emptyProfile(DEFAULT_PROFILE)];
      if (cmd === "shortcuts_keys" || cmd === "shortcuts_reserved") return [];
      if (cmd === "highlighting_load") return { themes: [], ruleSets: [], problems: [] };
      return null;
    });
    Object.assign(settings, { settingsFolder: TEAM });
    render(Settings, { onClose: () => {} });
    const row = screen.getByRole("textbox", { name: "Settings folder" }).parentElement!;
    await userEvent.click(within(row).getByRole("button", { name: "Choose…" }));
  }

  /** The folder Settings shows */
  const shown = () => (screen.getByRole("textbox", { name: "Settings folder" }) as HTMLInputElement).value;

  it("warns before using Program Files, which can't be saved to, and keeps the folder it had on Cancel", async () => {
    await choose(PROGRAM_FILES, false, "Cancel");
    await expect.poll(() => warned.length).toBe(1);
    expect(warned[0].message).toMatch(/^You can't save to this folder: changes to sessions, button bars and layouts won't be kept\./);
    expect(warned[0].buttons).toEqual({ OkCancelCustom: ["Use It", "Cancel"] });
    await new Promise((r) => setTimeout(r, 20));
    expect(used).toEqual([]);
    expect(shown()).toBe(TEAM);
  });

  it("uses it once told to: a team's folder may be read-only on purpose", async () => {
    await choose(SHARE, false, "Use It");
    await expect.poll(() => used).toEqual([SHARE]);
    expect(warned).toHaveLength(1);
  });

  it("uses a folder that can be saved to without a word", async () => {
    await choose(OWN, true, "Cancel");
    await expect.poll(() => used).toEqual([OWN]);
    expect(warned).toEqual([]);
  });

  /** Type `folder` in the Settings folder field and press Enter; `there` is whether the core finds a folder at it */
  async function type(folder: string, there: boolean, canWrite = true, answer: "Use It" | "Cancel" = "Cancel"): Promise<void> {
    used = [];
    warned = [];
    mockIPC((cmd, args) => {
      if (cmd === "folder_exists") return there;
      if (cmd === "folder_writable") return canWrite;
      if (cmd === "plugin:dialog|message") {
        warned.push(args as { message: string; buttons?: unknown });
        return answer;
      }
      if (cmd === "settings_set_folder") return void used.push((args as { path: string }).path);
      if (cmd === "profiles_list") return [emptyProfile(DEFAULT_PROFILE)];
      if (cmd === "shortcuts_keys" || cmd === "shortcuts_reserved") return [];
      if (cmd === "highlighting_load") return { themes: [], ruleSets: [], problems: [] };
      return null;
    });
    Object.assign(settings, { settingsFolder: TEAM });
    render(Settings, { onClose: () => {} });
    const field = screen.getByRole("textbox", { name: "Settings folder" }) as HTMLInputElement;
    await fireEvent.input(field, { target: { value: folder } });
    await fireEvent.change(field);
  }

  it("takes a folder typed or pasted, without the quotes Copy as path puts round it", async () => {
    await type(`"${SHARE}" `, true);
    await expect.poll(() => used).toEqual([SHARE]);
    expect(warned).toEqual([]);
    // the field shows the folder as it's used, without the quotes
    await expect.poll(shown).toBe(SHARE);
  });

  it("doesn't use a typed folder that isn't there, and says so, the folder it had kept", async () => {
    await type(String.raw`\\fileserver\team-setings`, false);
    expect(await screen.findByText(/^There's no folder at \\\\fileserver\\team-setings, or it can't be reached now\./)).toBeTruthy();
    expect(used).toEqual([]);
    expect(settings.settingsFolder).toBe(TEAM);
  });

  it("asks before using a typed folder that can't be saved to, as one chosen", async () => {
    await type(PROGRAM_FILES, true, false, "Use It");
    await expect.poll(() => used).toEqual([PROGRAM_FILES]);
    expect(warned).toHaveLength(1);
  });

  it("leaves the folder as it is when the field is emptied", async () => {
    await type("  ", true);
    await new Promise((r) => setTimeout(r, 20));
    expect(used).toEqual([]);
    expect(shown()).toBe(TEAM);
  });

  it("says where a network folder is on the Mac, which has no \\\\server\\share paths", () => {
    const windows = noFolderMessage(SHARE, false);
    expect(windows).toBe(String.raw`There's no folder at \\fileserver\team-settings, or it can't be reached now.`);
    // typed as on Windows, or as Finder's Connect to Server has it: connect to it, then it's under /Volumes
    for (const typed of [String.raw`\\openmediavault.localdomain\public\SuperXTerminal`, "smb://openmediavault.localdomain/public/SuperXTerminal"]) {
      const mac = noFolderMessage(typed, true);
      expect(mac).toContain("smb://openmediavault.localdomain/public");
      expect(mac).toContain("/Volumes/public/SuperXTerminal");
    }
    // a path of the Mac's own gets no such advice
    expect(noFolderMessage("/Volumes/public/SuperXTerminal", true)).toBe("There's no folder at /Volumes/public/SuperXTerminal, or it can't be reached now.");
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
  it("is off until turned on: no Check 1Password, and turning it on saves it and offers the check", async () => {
    render(Settings, { onClose: () => {} });
    const use = screen.getByRole("checkbox", { name: "Use 1Password references" }) as HTMLInputElement;
    expect(use.checked).toBe(false);
    expect(screen.queryByRole("button", { name: "Check 1Password" })).toBeNull();
    expect(screen.getByText(/Off, nothing is read from 1Password and its CLI never runs/)).toBeTruthy();
    await userEvent.click(use);
    await expect.poll(() => saved.at(-1)?.onePassword).toBe(true);
    expect(await screen.findByRole("button", { name: "Check 1Password" })).toBeTruthy();
  });

  it("says what 1Password's CLI can read when it works", async () => {
    settings.onePassword = true;
    opCheck = "1Password's CLI 2.42.0 works, through the 1Password app: 3 vaults it can read.";
    render(Settings, { onClose: () => {} });
    await userEvent.click(screen.getByRole("button", { name: "Check 1Password" }));
    expect(await screen.findByText(opCheck)).toBeTruthy();
    expect(screen.queryByText(/winget install 1password-cli/)).toBeNull();
    // the app's own guide, on GitHub
    await userEvent.click(screen.getByRole("link", { name: "How SuperXTerminal uses 1Password" }));
    expect(opened).toEqual(["https://github.com/waukeerules/SuperXTerminal/blob/main/docs/1PASSWORD.md"]);
  });

  it("says what's wrong, and how to install the CLI, with its getting-started page", async () => {
    settings.onePassword = true;
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

describe("Look's Theme", () => {
  it("lists the themes of your own by name, and Escape in Edit Themes closes it alone, not Settings", async () => {
    const user = userEvent.setup();
    uiThemes.list = [{ name: "Harbour", base: "dark", colors: { accent: "#ff8800" } }];
    let closed = 0;
    render(Settings, { onClose: () => closed++ });
    expect(screen.getByRole("option", { name: "Harbour" })).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Edit Themes…" }));
    expect(screen.getByRole("dialog", { name: "Edit Themes" })).toBeTruthy();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Edit Themes" })).toBeNull();
    expect(closed).toBe(0);
    uiThemes.list = [];
  });
});

describe("General's check of the session files", () => {
  it("saves the seconds between checks, 10 at the least", async () => {
    settings.sessionsCheckSeconds = 10;
    render(Settings, { onClose: () => {} });
    const field = screen.getByLabelText("Seconds between checks") as HTMLInputElement;
    expect(field.value).toBe("10");
    await fireEvent.change(field, { target: { value: "30" } });
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ sessionsCheckSeconds: 30 }));
    // oftener than every 10 seconds isn't offered: each check reads the files whole
    await fireEvent.change(field, { target: { value: "3" } });
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ sessionsCheckSeconds: 10 }));
    await fireEvent.change(field, { target: { value: "" } });
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ sessionsCheckSeconds: 10 }));
    settings.sessionsCheckSeconds = 10;
  });
});

describe("Tabs' Longest tab name", () => {
  it("saves the characters a tab's name may take, 0 for the whole name", async () => {
    render(Settings, { onClose: () => {} });
    const field = screen.getByLabelText("Longest tab name") as HTMLInputElement;
    expect(field.value).toBe("30");
    await fireEvent.change(field, { target: { value: "18" } });
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ tabNameMax: 18 }));
    await fireEvent.change(field, { target: { value: "0" } });
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ tabNameMax: 0 }));
    // more than a strip could show is kept to 200
    await fireEvent.change(field, { target: { value: "5000" } });
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ tabNameMax: 200 }));
  });
});

describe("Tabs' Grid View", () => {
  it("turns Grid View on and off, as the View menu does", async () => {
    render(Settings, { onClose: () => {} });
    const box = screen.getByLabelText(/Grid View: each tab in a tile/) as HTMLInputElement;
    expect(box.checked).toBe(false);
    await fireEvent.click(box);
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ gridView: true }));
  });
});

describe("Tabs' Grid View off", () => {
  it("puts back the splits by default, and saves joining them instead", async () => {
    render(Settings, { onClose: () => {} });
    const choice = screen.getByLabelText("Grid View off") as HTMLSelectElement;
    expect(choice.value).toBe("restore");
    await fireEvent.change(choice, { target: { value: "join" } });
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ gridViewOff: "join" }));
  });
});

describe("Tabs' Remote Desktop in Grid View", () => {
  it("opens in a new window by default, and saves taking a tile instead", async () => {
    render(Settings, { onClose: () => {} });
    const choice = screen.getByLabelText("Remote Desktop in Grid View") as HTMLSelectElement;
    expect(choice.value).toBe("window");
    await fireEvent.change(choice, { target: { value: "tile" } });
    await vi.waitFor(() => expect(saved.at(-1)).toMatchObject({ gridViewRdp: "tile" }));
  });
});
