// A tab's right-click menu: its Tunnels... item, which shows the session's tunnels, or is greyed saying where to add
// them. The tab area is stood in for by an object with what the menu reads of it, the window by the menu's host.
import { describe, expect, it, vi } from "vitest";
import { tabMenu, type TabMenuHost } from "./tabMenu";
import type { TabArea } from "./TabArea";

/** The menu of a terminal tab whose session opened these tunnels, and the window's tunnels callback */
function menuWith(tunnels: { kind: string; listen: string; to: string }[]) {
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
    groupCount: () => 1,
  } as unknown as TabArea;
  const host = { tabs, sessions: [], tunnels: vi.fn() } as unknown as TabMenuHost;
  const items = tabMenu(host, "t1");
  const item = items.find((i) => "label" in i && i.label === "Tunnels...") as { disabled?: string | boolean; action?: () => void };
  return { item, tunnels: host.tunnels as ReturnType<typeof vi.fn> };
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
