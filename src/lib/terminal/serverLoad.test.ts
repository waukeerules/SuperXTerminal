// The server load in the status bar (serverLoad.svelte.ts): a reading as the status bar shows it, and the tab in front
// followed: its readings asked for once its session is connected, stopped when another tab comes to the front or it
// drops, and a late reading of the tab left behind not shown; and each tab's readings kept 15 minutes for the graph,
// there again when the tab comes back to the front. The tab area is stood in for by its tab in front, its
// tabs' page hosts and its change listeners; the Rust side by mockIPC, which records the watch and stop calls and
// hands over each watch's channel, on which the test sends readings as the core would.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Channel } from "@tauri-apps/api/core";
import { HISTORY_MS, followServerLoad, forgetHistories, keep, rate, serverLoad, shown, type LoadEvent } from "./serverLoad.svelte";
import type { TabArea } from "../layout/TabArea";

/** The calls made to the core: server_load_watch with its pseudo terminal's id, and server_load_stop */
let calls: string[];
/** Each watch's channel, by pseudo terminal id */
let channels: Map<number, Channel<LoadEvent>>;
/** The stand-in tabs: each one's page host, as far as following it needs, and which is in front */
let hosts: Map<string, { isEnded: boolean; isConnecting: boolean; connection: () => number | null }>;
let front: string;
let listeners: (() => void)[];
let unfollow: () => void;

/** A tab whose session is connected, on pseudo terminal `pty` */
const connectedTab = (pty: number) => ({ isEnded: false, isConnecting: false, connection: () => pty });

beforeEach(() => {
  forgetHistories();
  calls = [];
  channels = new Map();
  listeners = [];
  hosts = new Map([["a", connectedTab(7)], ["b", connectedTab(8)]]);
  front = "a";
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as { id?: number; onEvent?: Channel<LoadEvent> };
    if (cmd === "server_load_watch") {
      calls.push(`watch ${payload.id}`);
      channels.set(payload.id!, payload.onEvent!);
      return true;
    }
    if (cmd === "server_load_stop") calls.push("stop");
    return null;
  });
  const tabs = {
    api: { get activePanel() { return { id: front, title: front === "a" ? "web" : "pi" }; } },
    hosts,
    onChange: (listener: () => void) => {
      listeners.push(listener);
      return () => {};
    },
  } as unknown as TabArea;
  unfollow = followServerLoad(tabs);
});

afterEach(() => unfollow());

/** The tab area says something changed (a tab came to the front, a session connected or dropped) */
const changed = () => listeners.forEach((l) => l());
/** The core sends a reading on pseudo terminal `pty`'s watch */
const send = (pty: number, event: LoadEvent) => (channels.get(pty)!.onmessage as (e: LoadEvent) => void)(event);

describe("a reading", () => {
  it("is shown as figures and rates, and without CPU or traffic on the first reading", () => {
    expect(shown({ cpu: 2.4, mem: 15.99, disk: 50, down: 10854, up: 28979 })).toBe("CPU 2% · Mem 16% · Disk 50% · ↓10.6 KB/s ↑28.3 KB/s");
    expect(shown({ cpu: null, mem: 16, disk: 50, down: null, up: null })).toBe("Mem 16% · Disk 50%");
    expect([rate(512), rate(1536), rate(5 * 1024 * 1024), rate(3 * 1024 ** 3)]).toEqual(["512 B/s", "1.5 KB/s", "5.0 MB/s", "3.0 GB/s"]);
  });
});

describe("following the tab in front", () => {
  it("watches its session and shows its readings, and tells why when there are none", async () => {
    await expect.poll(() => calls).toEqual(["watch 7"]);
    send(7, { kind: "load", cpu: null, mem: 16, disk: 50, down: null, up: null });
    expect(serverLoad.text).toBe("Mem 16% · Disk 50%");
    expect(serverLoad.title).toMatch(/nothing on the server changes/);
    send(7, { kind: "unavailable", why: "not a Linux server (no /proc/stat)" });
    expect(serverLoad.text).toBe("Server load: none");
    expect(serverLoad.title).toMatch(/not a Linux server/);
  });

  it("moves to the tab brought to the front, not showing a late reading of the one left", async () => {
    await expect.poll(() => calls).toEqual(["watch 7"]);
    send(7, { kind: "load", cpu: 5, mem: 16, disk: 50, down: 1, up: 1 });
    front = "b";
    changed();
    await expect.poll(() => calls).toEqual(["watch 7", "watch 8"]);
    // the tab left's reading, sent just as the front changed, isn't the new tab's
    expect(serverLoad.text).toBe("");
    send(7, { kind: "load", cpu: 99, mem: 99, disk: 99, down: 1, up: 1 });
    expect(serverLoad.text).toBe("");
    // the same tab still in front (another change: an alert, a title) doesn't start it again
    changed();
    expect(calls).toEqual(["watch 7", "watch 8"]);
  });

  it("stops while the session in front is connecting or has dropped, and starts again once it's back", async () => {
    await expect.poll(() => calls).toEqual(["watch 7"]);
    hosts.get("a")!.isEnded = true;
    changed();
    await expect.poll(() => calls).toEqual(["watch 7", "stop"]);
    expect(serverLoad.text).toBe("");
    // reconnected: a new pseudo terminal, connecting until its far end answers
    hosts.set("a", { ...connectedTab(9), isConnecting: true });
    changed();
    expect(calls).toEqual(["watch 7", "stop"]);
    hosts.set("a", connectedTab(9));
    changed();
    await expect.poll(() => calls).toEqual(["watch 7", "stop", "watch 9"]);
  });
});

describe("the readings kept for the graph", () => {
  it("keeps the tab in front's readings, and gives them back when the tab returns to the front", async () => {
    await expect.poll(() => calls).toEqual(["watch 7"]);
    send(7, { kind: "load", cpu: null, mem: 16, disk: 50, down: null, up: null });
    send(7, { kind: "load", cpu: 4, mem: 17, disk: 50, down: 100, up: 50 });
    expect(serverLoad.tab).toBe("web");
    expect(serverLoad.samples.map((s) => s.mem)).toEqual([16, 17]);
    front = "b";
    changed();
    expect(serverLoad.samples).toEqual([]);
    expect(serverLoad.tab).toBe("pi");
    front = "a";
    changed();
    expect(serverLoad.samples.map((s) => s.mem)).toEqual([16, 17]);
  });

  it("keeps 15 minutes, dropping older readings", () => {
    const load = { cpu: 1, mem: 1, disk: 1, down: 1, up: 1 };
    keep(1, load, 0);
    keep(1, load, 60_000);
    const kept = keep(1, load, HISTORY_MS + 30_000);
    expect(kept.map((s) => s.at)).toEqual([60_000, HISTORY_MS + 30_000]);
    // another connection's are its own
    expect(keep(2, load, 5).map((s) => s.at)).toEqual([5]);
  });
});
