// Grid View's choice for a Remote Desktop opened: a new window when Grid View is on, it's set to, and this window has
// tabs already that aren't all desktops. Plain values stand in for the settings and the window's tabs.
import { describe, expect, it } from "vitest";
import { rdpOpensInNewWindow } from "./gridView";

describe("a Remote Desktop opened in Grid View", () => {
  it("opens in a new window when this one has tabs, as it's set to by default", () => {
    expect(rdpOpensInNewWindow(true, "window", 4, false)).toBe(true);
  });

  it("opens here in a window with no tabs yet, so the new window keeps it", () => {
    expect(rdpOpensInNewWindow(true, "window", 0, false)).toBe(false);
  });

  it("goes in as another tab in a window of desktops alone, the one Grid View sent the first to", () => {
    expect(rdpOpensInNewWindow(true, "window", 1, true)).toBe(false);
  });

  it("takes a tile when set to, and opens here as usual out of Grid View", () => {
    expect(rdpOpensInNewWindow(true, "tile", 4, false)).toBe(false);
    expect(rdpOpensInNewWindow(false, "window", 4, false)).toBe(false);
  });
});
