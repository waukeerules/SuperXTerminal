// Edit sessions… (BulkEdit.svelte), a folder's or the selection's: its Password, set as each ticked session's own
// (in the Keychain on this computer, not the shared data) or forgotten from each when left empty, with or without
// other fields ticked, and beside Login username. The Rust side is stood in for by mockIPC, which records what each
// command was asked.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import BulkEdit from "./BulkEdit.svelte";
import type { SessionView, Store } from "./api";

/** A session as the tree has it */
function session(store: Store, id: string): SessionView {
  const plain = id.replace(/^@db\//, "");
  return {
    id, store, name: plain.split("/").pop()!, folder: plain.split("/").slice(0, -1).join("/"), protocol: "SSH", host: "10.0.0.1",
    port: 22, username: "", profile: "Default Settings", icon: "switch", color: "", note: "", extraArgs: "", script: "", buttonBar: "",
    updatedBy: "", locked: false, lockedFields: "", pending: false, pendingNote: "",
  };
}

let calls: { cmd: string; args: Record<string, unknown> }[];
let done: [number, string[]] | null;

beforeEach(() => {
  calls = [];
  done = null;
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: a });
    if (cmd === "sessions_bulk_edit") return { done: (a.items as unknown[]).length, problems: [] };
    if (cmd === "session_passwords_set") return (a.ids as string[]).length;
    if (cmd === "profiles_list") return [];
    if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
    return null;
  });
});

/** Open Edit sessions for these sessions */
function open(sessions: SessionView[]) {
  render(BulkEdit, {
    title: "in Site", sessions, iconSize: 20, customs: new Map(),
    onDone: (n: number, problems: string[]) => (done = [n, problems]), onCancel: () => {},
  });
}

/** The field ticks, in order, by their labels */
const ticks = () => [...document.querySelectorAll(".fields label.use")].map((l) => l.textContent!.trim());

describe("Edit sessions", () => {
  it("has Password beside Login username, recommending the profile's password for sessions that share a login", () => {
    open([session("saved", "Site/sw1")]);
    expect(ticks().slice(0, 3)).toEqual(["Login username:", "Password:", "Profile:"]);
    expect(screen.getByText(/their profile's password is usually better \(Settings > Profiles > Login\)/)).toBeTruthy();
  });

  it("sets one password as each ticked session's own, without touching their fields", async () => {
    open([session("saved", "Site/sw1"), session("private", "Site/sw2"), session("database", "@db/Site/sw3")]);
    const password = screen.getByLabelText("Password") as HTMLInputElement;
    expect(password.disabled).toBe(true);
    await userEvent.click(screen.getByRole("checkbox", { name: /Password/ }));
    await userEvent.type(password, "site-pw");
    // sw2 left out
    await userEvent.click(screen.getByRole("checkbox", { name: /Site\/sw2/ }));
    await userEvent.click(screen.getByRole("button", { name: "Change 2 sessions" }));
    await expect.poll(() => done).toEqual([2, []]);
    expect(calls.filter((c) => c.cmd === "session_passwords_set").map((c) => c.args)).toEqual([{ ids: ["@db/Site/sw3", "Site/sw1"], password: "site-pw" }]);
    expect(calls.some((c) => c.cmd === "sessions_bulk_edit")).toBe(false);
  });

  it("changes the fields ticked and the password together, and an empty password forgets each one's", async () => {
    open([session("saved", "Site/sw1")]);
    await userEvent.click(screen.getByRole("checkbox", { name: /Login username/ }));
    await userEvent.type(screen.getByPlaceholderText(/e\.g\. admin/), "netops");
    await userEvent.click(screen.getByRole("checkbox", { name: /Password/ }));
    await userEvent.click(screen.getByRole("button", { name: "Change 1 session" }));
    await expect.poll(() => done).toEqual([1, []]);
    expect(calls.find((c) => c.cmd === "sessions_bulk_edit")!.args.change).toEqual({ username: "netops" });
    expect(calls.find((c) => c.cmd === "session_passwords_set")!.args).toEqual({ ids: ["Site/sw1"], password: "" });
  });
});
