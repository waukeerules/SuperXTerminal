// A tab's label: its name, cut short at Settings > General > Tabs' Longest tab name with the whole of it on hover;
// its tunnel icon, shown when the session's ssh opened tunnels, marked when the profile's have changed
// since, and clicked to show them. The tab's state is set in tabStates as PageHost reports it; the window's part is
// stood in for by the onTunnels callback. And Watch for Activity's alert: the tab pulsing in its own colour.
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

/** The Web tab's label, its icon clicks going to `onTunnels` */
function label(onTunnels = vi.fn()) {
  render(SessionTab, { title: "Web", icon: "server", environment: null, iconSize: 24, customs: new Map(), tabId: "t1", onClose: () => {}, onTunnels });
  return onTunnels;
}

describe("a tab's tunnel icon", () => {
  it("shows when the session opened tunnels, and a click shows them", async () => {
    tabStates.t1 = { connecting: false, tunnels: 2 };
    const onTunnels = label();
    const icon = screen.getByRole("button", { name: "Tunnels of Web" });
    expect(icon.title).toBe("Tunnels (2): what they open, and how to test them");
    await userEvent.click(icon);
    expect(onTunnels).toHaveBeenCalledOnce();
  });

  it("isn't there without tunnels, nor once the session is down", () => {
    tabStates.t1 = { connecting: false, tunnels: 0 };
    label();
    expect(screen.queryByRole("button", { name: "Tunnels of Web" })).toBeNull();
    cleanup();
    tabStates.t1 = { ended: true, tunnels: 1 };
    label();
    expect(screen.queryByRole("button", { name: "Tunnels of Web" })).toBeNull();
  });

  it("is marked when the profile's tunnels have changed, saying a restart uses them", () => {
    tabStates.t1 = { connecting: false, tunnels: 1, tunnelsChanged: true };
    label();
    const icon = screen.getByRole("button", { name: "Tunnels of Web" });
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

describe("new output in a watched tab (Watch for Activity)", () => {
  /** The tab's label element, which carries its colour */
  const tab = () => document.querySelector(".tab") as HTMLElement;

  it("pulses the tab in its own colour, with no dot on its icon", () => {
    tabStates.t1 = { connecting: false, alert: "activity", tabColor: "#2e7d32" };
    label();
    expect(tab().classList.contains("pulsing")).toBe(true);
    expect(tab().style.getPropertyValue("--pulse")).toBe("#2e7d32");
    // the green dot of a connected session stays: the pulse says there's new output, not a dot in its place
    expect(document.querySelector(".badge.activity")).toBeNull();
    expect(document.querySelector(".badge.connected")).not.toBeNull();
    // said in words too, for a screen reader and for the pointer
    expect(tab().title).toBe("New output");
  });

  it("pulses a tab with no colour of its own in the app's accent colour", () => {
    tabStates.t1 = { connecting: false, alert: "activity" };
    label();
    expect(tab().classList.contains("pulsing")).toBe(true);
    expect(tab().style.getPropertyValue("--pulse")).toBe("var(--accent)");
  });

  it("is still when there's none, and Watch for Silence keeps its badge", () => {
    tabStates.t1 = { connecting: false };
    label();
    expect(tab().classList.contains("pulsing")).toBe(false);
    expect(tab().title).toBe("");
    cleanup();
    tabStates.t1 = { connecting: false, alert: "silence" };
    label();
    expect(tab().classList.contains("pulsing")).toBe(false);
    expect(document.querySelector(".badge.silence")).not.toBeNull();
  });
});
