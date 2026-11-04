// The Sessions tree's selection of folders and sessions together: Cmd+click and Shift+click pick both, and the
// selection's menu moves them to a folder; and on a clean install, the first session saved shows. The Rust side is
// stood in for by mockIPC; moves are recorded.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import SessionTree from "./SessionTree.svelte";
import { settings } from "../settings/store.svelte";
import type { SessionView } from "./api";

/** A Saved SSH session with this id, its name and folder taken from it */
function session(id: string): SessionView {
  const name = id.slice(id.lastIndexOf("/") + 1);
  const folder = id.includes("/") ? id.slice(0, id.lastIndexOf("/")) : "";
  return {
    id, store: "saved", name, folder, protocol: "SSH", host: "10.0.0.1", port: 22, username: "", profile: "", icon: "switch",
    color: "", note: "", extraArgs: "", script: "", updatedBy: "", locked: false, lockedFields: "", pending: false, pendingNote: "",
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
  render(SessionTree, { openSessions: new Set<string>(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
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
    render(SessionTree, { openSessions: new Set<string>(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
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
