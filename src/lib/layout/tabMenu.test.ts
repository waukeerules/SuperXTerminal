// A tab's right-click menu: its Tunnels... item, which shows the session's tunnels, or is greyed saying where to add
// them; Grid View, ticked while it's on; and the Close items, greyed by the tabs they'd close. The tab area is stood in for by an object with what the menu reads of it, the window by the menu's host.
import { describe, expect, it, vi } from "vitest";
import { tabMenu, type TabMenuHost } from "./tabMenu";
import type { TabArea } from "./TabArea";

/**
 * The menu of a terminal tab whose session opened these tunnels, in a window with Grid View on or off; with the
 * window's tunnels and Grid View callbacks
 */
function menuWith(tunnels: { kind: string; listen: string; to: string }[], grid = false) {
  const tab = {
    id: "t1",
    title: "Web",
    acceptCommands: true,
    host: { tunnels, logPath: "/tmp/web.log", isEnded: false, isReconnecting: false, canReceiveCommands: true },
  };
  const tabs = {
    rdpHosts: new Map(),
    tabList: () => [tab],
    paramsOf: () => ({ target: { kind: "session", store: "saved", id: "Web" }, local: false }),
    api: { getPanel: () => ({ group: { panels: [{ id: "t1" }] } }) },
    watchOf: () => ({ activity: false, silence: 0 }),
    groupCount: () => (grid ? 3 : 1),
    inGridView: () => grid,
    // the tab alone in its group; in Grid View, one of the window's three tiles
    closeScope: () => (grid ? [{ id: "t0" }, { id: "t1" }, { id: "t2" }] : [{ id: "t1" }]),
  } as unknown as TabArea;
  const host = { tabs, sessions: [], tunnels: vi.fn(), gridView: vi.fn() } as unknown as TabMenuHost;
  const items = tabMenu(host, "t1");
  type Item = { label?: string; disabled?: string | boolean; checked?: boolean; action?: () => void };
  const named = (start: string) => items.find((i) => "label" in i && i.label!.startsWith(start)) as Item | undefined;
  const item = named("Tunnels...") as Item;
  return { item, named, tunnels: host.tunnels as ReturnType<typeof vi.fn>, gridView: host.gridView as ReturnType<typeof vi.fn> };
}

describe("a tab's Tunnels... item", () => {
  it("shows the session's tunnels", () => {
    const { item, tunnels } = menuWith([{ kind: "local", listen: "8080", to: "localhost:80" }]);
    expect(item.disabled).toBeFalsy();
    item.action!();
    expect(tunnels).toHaveBeenCalledWith("t1");
  });

  it("is greyed for a session with none, saying where to add them", () => {
    const { item } = menuWith([]);
    expect(item.disabled).toBe("The session opened no tunnels: add them in Settings > Profiles > Tunnels");
  });
});

describe("a tab's Grid View item", () => {
  it("is ticked while Grid View is on, and turns it off; off, it turns it on", () => {
    const on = menuWith([], true);
    expect(on.named("Grid View")?.checked).toBe(true);
    on.named("Grid View")!.action!();
    expect(on.gridView).toHaveBeenCalledWith(false);
    // in a grid the tabs are joined by turning Grid View off, so Join All Tabs isn't offered as well
    expect(on.named("Join All Tabs")).toBeUndefined();
    const off = menuWith([], false);
    expect(off.named("Grid View")?.checked).toBe(false);
    off.named("Grid View")!.action!();
    expect(off.gridView).toHaveBeenCalledWith(true);
  });
});

describe("a tab's Close items", () => {
  it("in Grid View, where each tab is a tile of its own, close the window's other tabs, so they aren't greyed", () => {
    const { named } = menuWith([], true);
    expect(["Close others", "Close others to the right", "Close all"].map((label) => named(label)?.disabled)).toEqual([false, false, false]);
  });

  it("out of Grid View go by the tab's own group: greyed for a tab alone in it", () => {
    const { named } = menuWith([], false);
    expect(["Close others", "Close others to the right", "Close all"].map((label) => named(label)?.disabled)).toEqual([true, true, true]);
  });
});
