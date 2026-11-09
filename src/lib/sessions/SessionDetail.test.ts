// The Session Detail pane (View > Session Detail): moving from one session to another in the tree keeps the rows on
// screen until the next session's are read, rather than emptying the pane meanwhile, which made the pane's height
// jump and the tree above it shift (a flicker at every click). The Rust side is stood in for by mockIPC, whose
// session_get answers only when the test lets it (`answer`), so the moment between is seen.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import { mockIPC } from "@tauri-apps/api/mocks";
import SessionDetail from "./SessionDetail.svelte";
import type { SessionFields, SessionView } from "./api";

/** The session_get calls waiting for their answer, by session id */
let waiting: Map<string, (fields: SessionFields) => void>;

/** A session's fields: its name and host, the rest empty */
function fields(name: string, host: string): SessionFields {
  return {
    name, folder: "Lab", protocol: "SSH", host, port: 22, username: "admin", profile: "", color: "", icon: "",
    highlightRules: "", extraArgs: "", jumpHost: "", script: "", note: "", localPath: "", remotePath: "", rdpOptions: "",
    sshAlgorithms: "", buttonBar: "",
  };
}

/** The tree's row for a session, as the pane is given it */
function view(id: string, name: string): SessionView {
  return { id, store: "saved", name, folder: "Lab", protocol: "SSH", host: "", port: 22, username: "", profile: "", icon: "", color: "", note: "" } as SessionView;
}

/** Answer the waiting session_get for `id` */
async function answer(id: string, value: SessionFields): Promise<void> {
  await expect.poll(() => waiting.has(id)).toBe(true);
  waiting.get(id)!(value);
  waiting.delete(id);
}

beforeEach(() => {
  waiting = new Map();
  mockIPC((cmd, args) => {
    if (cmd === "session_get") {
      const id = String((args as Record<string, unknown>).id);
      return new Promise<SessionFields>((resolve) => waiting.set(id, resolve));
    }
    return null;
  });
});

describe("Session Detail", () => {
  it("keeps the session shown until the next one clicked is read, so the pane doesn't empty and jump", async () => {
    const { rerender } = render(SessionDetail, { session: view("sw1", "core-sw1"), customs: new Map(), onEdit: () => {}, onClose: () => {} });
    await answer("sw1", fields("core-sw1", "10.0.0.5"));
    expect(await screen.findByText("10.0.0.5")).toBeTruthy();

    await rerender({ session: view("sw2", "core-sw2") });
    // while core-sw2 is read: still core-sw1's rows, never the empty "Reading…"
    expect(screen.queryByText("Reading…")).toBeNull();
    expect(screen.getByText("10.0.0.5")).toBeTruthy();

    await answer("sw2", fields("core-sw2", "10.0.0.6"));
    expect(await screen.findByText("10.0.0.6")).toBeTruthy();
    expect(screen.queryByText("10.0.0.5")).toBeNull();
  });

  it("says to select a session when none is", async () => {
    render(SessionDetail, { session: null, customs: new Map(), onEdit: () => {}, onClose: () => {} });
    expect(screen.getByText("Select a session in the tree.")).toBeTruthy();
  });

  it("doesn't show an answer that came too late, for a session no longer selected", async () => {
    const { rerender } = render(SessionDetail, { session: view("sw1", "core-sw1"), customs: new Map(), onEdit: () => {}, onClose: () => {} });
    await expect.poll(() => waiting.has("sw1")).toBe(true);
    await rerender({ session: view("sw2", "core-sw2") });
    await answer("sw2", fields("core-sw2", "10.0.0.6"));
    expect(await screen.findByText("10.0.0.6")).toBeTruthy();
    // core-sw1's answer, late: ignored
    await answer("sw1", fields("core-sw1", "10.0.0.5"));
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByText("10.0.0.5")).toBeNull();
  });
});
