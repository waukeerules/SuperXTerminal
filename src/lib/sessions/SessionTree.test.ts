// The Sessions tree's selection of folders and sessions together; an open session's state on its icon (a yellow dot
// while connecting, green once connected, or the tab's red cross once its session has dropped); Cmd+click and Shift+click pick both, and the
// selection's menu moves them to a folder; a search hides the roots it leaves no matches in; Show in Sessions tree opens a Recent session's folders and scrolls to it;
// on a clean install, the first session saved shows; and a folder's menu edits and turns on or off the local
// overrides of every session in it. The Rust side is stood in for by mockIPC; moves are
// recorded. jsdom doesn't scroll, so scrollIntoView is stood in for, recording the rows it's asked to show.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import SessionTree from "./SessionTree.svelte";
import { settings } from "../settings/store.svelte";
import type { SessionView } from "./api";
import type { SessionState } from "../layout/TabArea";

/** A Saved SSH session with this id, its name and folder taken from it */
function session(id: string): SessionView {
  const name = id.slice(id.lastIndexOf("/") + 1);
  const folder = id.includes("/") ? id.slice(0, id.lastIndexOf("/")) : "";
  return {
    id, store: "saved", name, folder, protocol: "SSH", host: "10.0.0.1", port: 22, username: "", profile: "", icon: "switch",
    color: "", note: "", extraArgs: "", script: "", buttonBar: "", updatedBy: "", locked: false, lockedFields: "", pending: false, pendingNote: "",
  };
}

/** Every command the tree sent, with its arguments, for checking what a move asked for */
let calls: { cmd: string; args: Record<string, unknown> }[];
/** One user for a whole test, so a key held down (Cmd, Shift) is still down for the click after it */
let user: ReturnType<typeof userEvent.setup>;

beforeEach(() => {
  user = userEvent.setup();
  calls = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
    if (cmd === "sessions_load")
      return { sessions: [session("Imported/Core/sw1"), session("Imported/Core/sw2"), session("Imported/r1"), session("Lab/x")], folders: [], problems: [] };
    if (cmd === "shortcuts_get") return { Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false };
    if (cmd === "overrides_get") return {};
    if (cmd === "icons_custom") return [];
    if (cmd === "plugin:dialog|ask") return true;
    if (cmd === "folder_move" || cmd === "session_move") return "moved";
    return null;
  });
  Object.assign(settings, { loaded: true, treeSearchFields: [], treeSearchCase: false, treeSearchRegex: false });
});

/** Show the tree on the stand-in sessions, with Saved sessions' folders open */
async function open() {
  render(SessionTree, { openSessions: new Map(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
  await screen.findByText(/Imported/);
  // open Saved sessions' folders so their rows show
  await user.click(screen.getByRole("button", { name: "Expand all" }));
}

/** A tree row, by the text in it */
const row = (text: string | RegExp) => screen.getByText(text).closest(".row") as HTMLElement;

describe("selecting folders and sessions together", () => {
  it("Cmd+click adds a folder and a session, and the menu moves both to a folder", async () => {
    await open();
    await user.click(row(/^Core \(/));
    await user.keyboard("{Meta>}");
    await user.click(row("r1"));
    await user.keyboard("{/Meta}");
    expect(row(/^Core \(/).classList.contains("selected")).toBe(true);
    expect(row("r1").classList.contains("selected")).toBe(true);

    await user.pointer({ keys: "[MouseRight]", target: row("r1") });
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Move 2 items to folder…" }));
    const input = screen.getByRole("textbox", { name: "" }) as HTMLInputElement;
    await user.clear(input);
    await user.type(input, "Lab{Enter}");
    expect(calls.find((c) => c.cmd === "folder_move")?.args).toMatchObject({ store: "saved", from: "Imported/Core", to: "Lab/Core" });
    expect(calls.find((c) => c.cmd === "session_move")?.args).toMatchObject({ store: "saved", id: "Imported/r1", folder: "Lab" });
  });

  it("Shift+click selects the folders and sessions shown between", async () => {
    await open();
    await user.click(row(/^Core \(/));
    await user.keyboard("{Shift>}");
    await user.click(row("r1"));
    await user.keyboard("{/Shift}");
    for (const name of [/^Core \(/, "sw1", "sw2", "r1"]) expect(row(name).classList.contains("selected"), String(name)).toBe(true);
    expect(row("x").classList.contains("selected")).toBe(false);
  });
});

describe("searching", () => {
  /** The roots' rows the tree shows, by their names */
  const rootsShown = () => [...document.querySelectorAll(".row.folder.root .label")].map((l) => l.textContent?.replace(/\s*\(.*$/, ""));

  it("shows only the roots with matches, and none at all when nothing matches", async () => {
    mockIPC((cmd) => {
      if (cmd === "sessions_load") return { sessions: [session("Imported/r1"), { ...session("Home/nas"), store: "private" }], folders: [], problems: [] };
      if (cmd === "shortcuts_get") return { Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false };
      if (cmd === "overrides_get") return {};
      if (cmd === "icons_custom" || cmd === "profiles_list") return [];
      return null;
    });
    render(SessionTree, { openSessions: new Map(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
    await expect.poll(rootsShown).toEqual(["Saved sessions", "Private sessions"]);
    const search = screen.getByPlaceholderText(/Type to search/);
    await user.type(search, "r1");
    await expect.poll(rootsShown).toEqual(["Saved sessions"]);
    await user.clear(search);
    await user.type(search, "sdfsdfsdf");
    await expect.poll(rootsShown).toEqual([]);
    // and back when the search is cleared
    await user.clear(search);
    await expect.poll(rootsShown).toEqual(["Saved sessions", "Private sessions"]);
  });
});

describe("with Private sessions turned off", () => {
  it("shows no Private sessions, offers none to copy to, and makes new sessions in Saved sessions", async () => {
    let made: string | null = null;
    mockIPC((cmd, args) => {
      if (cmd === "sessions_load") return { sessions: [session("Lab/sw1")], folders: [], problems: [], savedFile: "/team/Sessions.xml", privateFile: "/me/PrivateSessions.xml", savedOff: false, privateOff: true };
      if (cmd === "shortcuts_get") return { Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false };
      if (cmd === "overrides_get") return {};
      if (cmd === "icons_custom" || cmd === "profiles_list" || cmd === "variables_list" || cmd === "variables_built_in" || cmd === "buttonbars_load") return [];
      if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
      if (cmd === "session_save") {
        made = (args as { store: string }).store;
        return "Lab/new";
      }
      return null;
    });
    Object.assign(settings, { settingsFolder: "/team", useSessionsXml: true, usePrivateSessions: false });
    render(SessionTree, { openSessions: new Map(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
    await screen.findByText(/^Saved sessions/);
    expect(screen.queryByText(/^Private sessions/)).toBeNull();
    // Copy to lists only the roots in use
    await user.click(screen.getByRole("button", { name: "Expand all" }));
    await user.pointer({ keys: "[MouseRight]", target: row("sw1") });
    await user.hover(within(screen.getByRole("menu")).getByRole("menuitem", { name: /^Copy to/ }));
    expect(screen.queryByRole("menuitem", { name: /Private sessions/ })).toBeNull();
    await user.keyboard("{Escape}");
    // nothing selected: the toolbar's New session goes in Saved sessions
    await user.click(row(/^Saved sessions/));
    await user.click(screen.getByRole("button", { name: "New session (in the selected folder)" }));
    await user.type(await screen.findByRole("textbox", { name: /Session name/ }), "sw9");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => made).toBe("saved");
    Object.assign(settings, { usePrivateSessions: true });
  });
});

describe("turning Private sessions off and on", () => {
  it("takes effect at once, without restarting", async () => {
    /** Whether Private sessions are on, as the core answers sessions_load */
    let privateOn = true;
    mockIPC((cmd) => {
      if (cmd === "sessions_load") {
        return {
          sessions: [session("Lab/sw1"), { ...session("Home/nas"), store: "private" }], folders: [], problems: [],
          savedFile: "/team/Sessions.xml", privateFile: "/me/PrivateSessions.xml", savedOff: false, privateOff: !privateOn,
        };
      }
      if (cmd === "shortcuts_get") return { Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false };
      if (cmd === "overrides_get") return {};
      if (cmd === "icons_custom" || cmd === "profiles_list") return [];
      return null;
    });
    Object.assign(settings, { loaded: true, settingsFolder: "/team", useSessionsXml: true, usePrivateSessions: true });
    render(SessionTree, { openSessions: new Map(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
    await screen.findByText(/^Private sessions/);
    // Settings > General saves the change, as the core then answers
    privateOn = false;
    settings.usePrivateSessions = false;
    await expect.poll(() => screen.queryByText(/^Private sessions/)).toBeNull();
    privateOn = true;
    settings.usePrivateSessions = true;
    await screen.findByText(/^Private sessions/);
  });
});

describe("Show in Sessions tree", () => {
  /** The rows scrollIntoView was asked to show */
  let scrolled: Element[];
  /** jsdom's Element has no scrollIntoView of its own */
  const own = Element.prototype.scrollIntoView;
  beforeEach(() => {
    scrolled = [];
    Element.prototype.scrollIntoView = function (this: Element) { scrolled.push(this); };
  });
  afterEach(() => {
    Element.prototype.scrollIntoView = own;
  });

  it("opens a Recent session's folders, selects it there and scrolls it into view", async () => {
    mockIPC((cmd) => {
      if (cmd === "sessions_load") return { sessions: [session("Imported/Core/sw1"), session("Imported/r1"), session("Lab/x")], folders: [], problems: [] };
      if (cmd === "shortcuts_get") return { Favorites: [], Recent: ["Imported/Core/sw1"], FavoritesExpanded: false, RecentExpanded: true };
      if (cmd === "overrides_get") return {};
      if (cmd === "icons_custom" || cmd === "profiles_list") return [];
      return null;
    });
    render(SessionTree, { openSessions: new Map(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
    // only Recent's sw1 shows: its folders are closed
    const inRecent = await screen.findByText("sw1");
    await user.pointer({ keys: "[MouseRight]", target: inRecent });
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Show in Sessions tree" }));
    const inFolder = document.querySelector<HTMLElement>('.row.session[data-key="saved:Imported/Core/sw1"]');
    expect(inFolder?.classList.contains("selected")).toBe(true);
    await expect.poll(() => scrolled).toEqual([inFolder]);
  });
});

describe("the first session on a clean install", () => {
  /**
   * Show the tree on a clean install: no sessions anywhere, and a settings folder or not. The stand-in files keep what
   * session_save is given, and sessions_load gives it back, as the Rust side reads the files again
   */
  async function cleanInstall(settingsFolder: boolean): Promise<SessionView[]> {
    const stored: SessionView[] = [];
    mockIPC((cmd, args) => {
      const a = (args ?? {}) as { store?: "saved" | "private"; fields?: { name: string; folder: string } };
      if (cmd === "sessions_load")
        return { sessions: [...stored], folders: [], problems: [], savedFile: settingsFolder ? "/team/Sessions.xml" : null, privateFile: "/me/PrivateSessions.xml", savedOff: false };
      if (cmd === "session_save") {
        const id = a.fields!.folder ? `${a.fields!.folder}/${a.fields!.name}` : a.fields!.name;
        stored.push({ ...session(id), store: a.store! });
        return id;
      }
      if (cmd === "shortcuts_get") return { Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false };
      if (cmd === "overrides_get") return {};
      if (cmd === "icons_custom" || cmd === "profiles_list" || cmd === "variables_list" || cmd === "variables_built_in") return [];
      if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
      return null;
    });
    render(SessionTree, { openSessions: new Map(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
    await screen.findByText(/Private sessions/);
    return stored;
  }

  /** The toolbar's New session, named and saved as the user does */
  async function newSession(name: string): Promise<void> {
    await user.click(screen.getByTitle("New session (in the selected folder)"));
    await user.type(screen.getByText("Session name:").closest("label")!.querySelector("input")!, name);
    await user.click(screen.getByRole("button", { name: "Save" }));
  }

  it("shows in Saved sessions once saved, with a settings folder", async () => {
    const stored = await cleanInstall(true);
    await newSession("sw1");
    await expect.poll(() => stored.map((s) => `${s.store}:${s.id}`)).toEqual(["saved:sw1"]);
    expect(within(row(/^Saved sessions/).parentElement!).getByText("sw1")).toBeTruthy();
  });

  it("shows in Private sessions once saved, before a settings folder is chosen", async () => {
    const stored = await cleanInstall(false);
    await newSession("sw1");
    await expect.poll(() => stored.map((s) => `${s.store}:${s.id}`)).toEqual(["private:sw1"]);
    expect(within(row(/^Private sessions/).parentElement!).getByText("sw1")).toBeTruthy();
  });
});

describe("an open session's state on its icon", () => {
  it("shows a yellow dot while connecting, green while connected, and the tab's red cross once dropped", async () => {
    render(SessionTree, {
      openSessions: new Map<string, SessionState>([
        ["saved:Imported/Core/sw1", "disconnected"], ["saved:Imported/Core/sw2", "connected"], ["saved:Lab/x", "connecting"],
      ]),
      onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {},
    });
    await screen.findByText(/Imported/);
    await user.click(screen.getByRole("button", { name: "Expand all" }));
    const dropped = row("sw1");
    expect(within(dropped).getByLabelText("Disconnected")).toBeTruthy();
    expect(dropped.querySelector(".open")).toBeNull();
    const connected = row("sw2");
    expect(connected.querySelector(".open")).not.toBeNull();
    expect(within(connected).queryByLabelText("Disconnected")).toBeNull();
    const connecting = row("x");
    expect(within(connecting).getByLabelText("Connecting")).toBeTruthy();
    expect(connecting.querySelector(".open")).toBeNull();
    expect(within(connected).queryByLabelText("Connecting")).toBeNull();
    // not open: none of them
    expect(row("r1").querySelector(".open")).toBeNull();
    expect(within(row("r1")).queryByLabelText("Disconnected")).toBeNull();
    expect(within(row("r1")).queryByLabelText("Connecting")).toBeNull();
  });
});

describe("a folder's local overrides", () => {
  /** The overrides stored, by session id, as overrides_get gives them and overrides_set changes them */
  let stored: Record<string, { Port: number; Username?: string; Disabled?: boolean }>;

  beforeEach(() => {
    stored = { "Imported/Core/sw1": { Port: 0, Username: "tim" }, "Imported/Core/sw2": { Port: 2222, Disabled: true } };
    mockIPC((cmd, args) => {
      calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      if (cmd === "sessions_load")
        return { sessions: [session("Imported/Core/sw1"), session("Imported/Core/sw2"), session("Imported/r1"), session("Lab/x")], folders: [], problems: [] };
      if (cmd === "shortcuts_get") return { Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false };
      if (cmd === "overrides_get") return stored;
      if (cmd === "overrides_set") {
        for (const [id, o] of (args as { changes: [string, { Port: number }][] }).changes) stored[id] = o;
        return { ...stored };
      }
      if (cmd === "icons_custom") return [];
      return null;
    });
    Object.assign(settings, { useLocalOverrides: true });
  });

  /** The folder's menu item with this name (after its tick, when it has one) */
  async function folderItem(folder: RegExp, name: string) {
    await user.pointer({ keys: "[MouseRight]", target: row(folder) });
    return within(screen.getByRole("menu")).getByRole("menuitem", { name: new RegExp(`^(✓\\s*)?${name}`) });
  }

  it("edits the overrides of every session in the folder and the folders below it", async () => {
    await open();
    await user.click(await folderItem(/^Imported \(/, "Local overrides…"));
    expect(screen.getByRole("dialog", { name: "Local overrides for 3 sessions" })).toBeTruthy();
  });

  it("turns the folder's overrides all on, then all off, keeping their values", async () => {
    await open();
    // one on and one off: not ticked, and a click turns them all on
    const mixed = await folderItem(/^Core \(/, "Use Local Overrides");
    expect(mixed.textContent).not.toContain("✓");
    await user.click(mixed);
    expect(stored["Imported/Core/sw1"]).toEqual({ Port: 0, Username: "tim", Disabled: false });
    expect(stored["Imported/Core/sw2"]).toEqual({ Port: 2222, Disabled: false });
    // all on: ticked, and a click turns them all off
    const on = await folderItem(/^Core \(/, "Use Local Overrides");
    expect(on.textContent).toContain("✓");
    await user.click(on);
    expect(stored["Imported/Core/sw1"].Disabled).toBe(true);
    expect(stored["Imported/Core/sw2"]).toEqual({ Port: 2222, Disabled: true });
    // a session with none (r1) is never given any
    expect(stored["Imported/r1"]).toBeUndefined();
  });

  it("has no Use Local Overrides for a folder whose sessions have none", async () => {
    await open();
    await user.pointer({ keys: "[MouseRight]", target: row(/^Lab \(/) });
    expect(within(screen.getByRole("menu")).queryByRole("menuitem", { name: /^Use Local Overrides/ })).toBeNull();
  });
});
