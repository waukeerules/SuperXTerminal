// The Sessions tree's selection of folders and sessions together: Cmd+click and Shift+click pick both, and the
// selection's menu moves them to a folder. The Rust side is stood in for by mockIPC; moves are recorded.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import SessionTree from "./SessionTree.svelte";
import { settings } from "../settings/store.svelte";
import type { SessionView } from "./api";

function session(id: string): SessionView {
  const name = id.slice(id.lastIndexOf("/") + 1);
  const folder = id.includes("/") ? id.slice(0, id.lastIndexOf("/")) : "";
  return {
    id, store: "saved", name, folder, protocol: "SSH", host: "10.0.0.1", port: 22, username: "", profile: "", icon: "switch",
    color: "", note: "", extraArgs: "", script: "", updatedBy: "", locked: false, lockedFields: "", pending: false, pendingNote: "",
  };
}

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
    if (cmd === "custom_icons_list" || cmd === "icons_list") return [];
    if (cmd === "plugin:dialog|ask") return true;
    if (cmd === "folder_move" || cmd === "session_move") return "moved";
    return null;
  });
  Object.assign(settings, { loaded: true, treeSearchFields: [], treeSearchCase: false, treeSearchRegex: false });
});

async function open() {
  render(SessionTree, { openSessions: new Set<string>(), onOpen: () => {}, onLook: () => {}, onOpenProfiles: () => {} });
  await screen.findByText(/Imported/);
  // open Saved sessions' folders so their rows show
  await user.click(screen.getByRole("button", { name: "Expand all" }));
}

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
