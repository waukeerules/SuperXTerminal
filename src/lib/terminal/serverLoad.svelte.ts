/**
 * Server load in the status bar: CPU, memory, the root disk and network traffic of the Linux server the tab in front
 * is connected to (src-tauri/src/server_load.rs reads them over the tab's own ssh connection). Only for a session
 * whose profile has Server load on (Settings > Profiles); the core says so, by watching or not.
 *
 * The tab in front is followed: its readings start once its session is connected (the far end has answered; until it
 * has logged in the core waits for it), and stop when another tab comes to the front, the session drops or the tab
 * closes. One tab at a time: a server is only asked while you're looking at it.
 *
 * Each tab's readings are kept for HISTORY_MS (its connection's, by pseudo terminal id), for the graph the status
 * bar's figures open (ServerLoadChart.svelte): a tab brought back to the front goes on from where it was, with a gap
 * for the time it wasn't read. They go when the window closes; nothing is written to disk.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import type { TabArea } from "../layout/TabArea";

/** A reading as the core sends it (server_load.rs Load): percents, and bytes a second; null for what it hasn't */
export interface Load {
  cpu: number | null;
  mem: number | null;
  disk: number | null;
  down: number | null;
  up: number | null;
}

/** What the core sends: a reading, or why there are none for this tab */
export type LoadEvent = ({ kind: "load" } & Load) | { kind: "unavailable"; why: string };

/** One reading kept for the graph: when it came (ms since 1970) and its figures */
export interface Sample extends Load {
  at: number;
}

/** How long a tab's readings are kept for the graph: 15 minutes, 300 readings at one every 3 seconds */
export const HISTORY_MS = 15 * 60 * 1000;

/**
 * What the status bar shows: the readings, and what they mean (its tooltip); "" for nothing. `samples` is the tab in
 * front's kept readings (oldest first) and `tab` its title, for the graph
 */
export const serverLoad = $state<{ text: string; title: string; samples: Sample[]; tab: string }>({ text: "", title: "", samples: [], tab: "" });

/** Each connection's kept readings, by pseudo terminal id */
const histories = new Map<number, Sample[]>();

/**
 * Keep a reading in a connection's history, dropping those older than HISTORY_MS; the history as it is now. A copy
 * each time, so the graph (which holds the array) redraws
 */
export function keep(id: number, load: Load, at = Date.now()): Sample[] {
  const kept = (histories.get(id) ?? []).filter((s) => at - s.at <= HISTORY_MS);
  const now = [...kept, { ...load, at }];
  histories.set(id, now);
  return now;
}

/** Forget every connection's readings (the tests' fresh start) */
export function forgetHistories(): void {
  histories.clear();
}

/** A rate as the status bar shows it: B/s, KB/s, MB/s or GB/s (of 1024), one decimal past bytes */
export function rate(bytes: number): string {
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return unit === 0 ? `${Math.round(value)} ${units[0]}` : `${value.toFixed(1)} ${units[unit]}`;
}

/**
 * A reading as the status bar shows it: "CPU 2% · Mem 16% · Disk 50% · ↓10.6 KB/s ↑28.3 KB/s". The
 * first reading has no CPU or traffic (they're changes since the reading before), so those come 3 seconds later
 */
export function shown(load: Load): string {
  const parts: string[] = [];
  if (load.cpu !== null) parts.push(`CPU ${Math.round(load.cpu)}%`);
  if (load.mem !== null) parts.push(`Mem ${Math.round(load.mem)}%`);
  if (load.disk !== null) parts.push(`Disk ${load.disk}%`);
  if (load.down !== null && load.up !== null) parts.push(`↓${rate(load.down)} ↑${rate(load.up)}`);
  return parts.join(" · ");
}

/** What the readings are, for the status bar's tooltip */
const MEANING =
  "The server's load, every 3 seconds: CPU busy since the last reading; memory in use (less what it can free); the " +
  "root filesystem's use; and network traffic in (↓) and out (↑) on its own interfaces (not loopback, containers or " +
  "VMs). Read over this tab's connection with cat and df: nothing on the server changes. Settings > Profiles > Server load";

/** The pseudo terminal being watched, so a change of tab that keeps it doesn't start again; null for none */
let watched: number | null = null;

/** The connected session of the tab in front, by its pseudo terminal's id; null for none (or still connecting) */
function frontSession(tabs: TabArea): number | null {
  const id = tabs.api.activePanel?.id;
  const host = id ? tabs.hosts.get(id) : undefined;
  if (!host || host.isEnded || host.isConnecting) return null;
  return host.connection();
}

/** Watch the tab in front's server load, or stop: as the tabs change, and as its session connects or drops */
function follow(tabs: TabArea): void {
  const id = frontSession(tabs);
  if (id === watched) return;
  watched = id;
  serverLoad.text = "";
  serverLoad.title = "";
  serverLoad.samples = id === null ? [] : (histories.get(id) ?? []);
  serverLoad.tab = id === null ? "" : (tabs.api.activePanel?.title ?? "");
  if (id === null) {
    void invoke("server_load_stop").catch(() => {});
    return;
  }
  const channel = new Channel<LoadEvent>();
  channel.onmessage = (event) => {
    // a reading for a tab no longer in front, come after the change
    if (watched !== id) return;
    if (event.kind === "load") {
      const { kind: _, ...load } = event;
      serverLoad.text = shown(load);
      serverLoad.title = `${MEANING}. Click for the last 15 minutes as a graph`;
      serverLoad.samples = keep(id, load);
    } else {
      serverLoad.text = "Server load: none";
      serverLoad.title = `No server load for this tab: ${event.why}. Settings > Profiles > Server load`;
    }
  };
  void invoke<boolean>("server_load_watch", { id, onEvent: channel }).catch(() => {});
}

/** Follow the tab area's tab in front from now on; the function stops following */
export function followServerLoad(tabs: TabArea): () => void {
  follow(tabs);
  const unfollow = tabs.onChange(() => follow(tabs));
  return () => {
    unfollow();
    watched = null;
    void invoke("server_load_stop").catch(() => {});
  };
}
