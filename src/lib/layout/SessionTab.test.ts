// A tab's label: its name, cut short at Settings > General > Tabs' Longest tab name with the whole of it on hover;
// its tunnel icon, shown when the session's ssh opened tunnels, marked when the profile's have changed
// since, and clicked to show them. The tab's state is set in tabStates as PageHost reports it; the window's part is
// stood in for by the onTunnels callback.
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import SessionTab from "./SessionTab.svelte";
import { tabStates } from "./tabStates.svelte";
import { tabNameWidth } from "./TabArea";

afterEach(() => {
  cleanup();
  delete tabStates.t1;
});

/** The OMV tab's label, its icon clicks going to `onTunnels` */
function label(onTunnels = vi.fn()) {
  render(SessionTab, { title: "OMV", icon: "server", environment: null, iconSize: 24, customs: new Map(), tabId: "t1", onClose: () => {}, onTunnels });
  return onTunnels;
}

describe("a tab's tunnel icon", () => {
  it("shows when the session opened tunnels, and a click shows them", async () => {
    tabStates.t1 = { connecting: false, tunnels: 2 };
    const onTunnels = label();
    const icon = screen.getByRole("button", { name: "Tunnels of OMV" });
    expect(icon.title).toBe("Tunnels (2): what they open, and how to test them");
    await userEvent.click(icon);
    expect(onTunnels).toHaveBeenCalledOnce();
  });

  it("isn't there without tunnels, nor once the session is down", () => {
    tabStates.t1 = { connecting: false, tunnels: 0 };
    label();
    expect(screen.queryByRole("button", { name: "Tunnels of OMV" })).toBeNull();
    cleanup();
    tabStates.t1 = { ended: true, tunnels: 1 };
    label();
    expect(screen.queryByRole("button", { name: "Tunnels of OMV" })).toBeNull();
  });

  it("is marked when the profile's tunnels have changed, saying a restart uses them", () => {
    tabStates.t1 = { connecting: false, tunnels: 1, tunnelsChanged: true };
    label();
    const icon = screen.getByRole("button", { name: "Tunnels of OMV" });
    expect(icon.classList.contains("changed")).toBe(true);
    expect(icon.title).toMatch(/restart the session to use them/);
  });
});

describe("a tab's name", () => {
  it("is as wide as Settings > General > Tabs allows, the whole of it on hover", () => {
    render(SessionTab, { title: "Core switch, building 4, second floor", icon: "server", environment: null, iconSize: 24, customs: new Map(), tabId: "t1", onClose: () => {} });
    expect(screen.getByText("Core switch, building 4, second floor").title).toBe("Core switch, building 4, second floor");
    // Longest tab name: characters, as the tabs' --tab-name-max; 0, the whole name
    expect(tabNameWidth(30)).toBe("30ch");
    expect(tabNameWidth(12.7)).toBe("12ch");
    expect(tabNameWidth(0)).toBe("none");
  });
});
