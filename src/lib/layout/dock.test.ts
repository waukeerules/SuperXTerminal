// Docking the Sessions pane (dock.ts), as DockPanelSuite's in SuperXPuTTY: where the pane lands when its title is
// dragged and let go (the window's left, right or bottom edge, or nowhere new), and the sizes it can take there.
// Pure functions, so no window is needed: the area is a plain rectangle.
import { describe, expect, it } from "vitest";
import { clampPaneHeight, clampPaneWidth, dockTarget } from "./dock";

/** The area beside the bars where the pane and the tabs are: 1200 by 800, from (0, 100) */
const area = { left: 0, top: 100, width: 1200, height: 800 };
/** The pane's size: 280 wide beside the tabs, 260 high under them, as its drop outlines are drawn */
const pane = { width: 280, height: 260 };

describe("docking the Sessions pane", () => {
  it("lands on the side whose outline the pointer is anywhere in: the pane's width from the left or right", () => {
    expect(dockTarget(10, 400, area, pane)).toBe("left");
    // the far side of the left outline, not only its edge
    expect(dockTarget(279, 400, area, pane)).toBe("left");
    expect(dockTarget(1190, 400, area, pane)).toBe("right");
    expect(dockTarget(921, 400, area, pane)).toBe("right");
  });

  it("lands at the bottom anywhere in the pane's height from the foot, the full width", () => {
    expect(dockTarget(600, 890, area, pane)).toBe("bottom");
    // the top of the bottom outline: 260 above the foot (900)
    expect(dockTarget(600, 641, area, pane)).toBe("bottom");
  });

  it("lands nowhere between the outlines, or outside the area, so letting go there leaves it where it was", () => {
    expect(dockTarget(600, 400, area, pane)).toBeNull();
    expect(dockTarget(281, 400, area, pane)).toBeNull();
    expect(dockTarget(600, 50, area, pane)).toBeNull();
    expect(dockTarget(-5, 400, area, pane)).toBeNull();
  });

  it("takes the bottom under the side outlines, which stop above it, as they're drawn", () => {
    expect(dockTarget(10, 895, area, pane)).toBe("bottom");
    expect(dockTarget(1190, 700, area, pane)).toBe("bottom");
  });

  it("keeps the outlines from meeting in a narrow window: each side at most a third of it", () => {
    const narrow = { left: 0, top: 0, width: 600, height: 800 };
    expect(dockTarget(199, 100, narrow, pane)).toBe("left");
    expect(dockTarget(300, 100, narrow, pane)).toBeNull();
    expect(dockTarget(401, 100, narrow, pane)).toBe("right");
  });

  it("keeps the pane's width and height in reach, and room for the tabs", () => {
    expect(clampPaneWidth(50, 1200)).toBe(160);
    expect(clampPaneWidth(1100, 1200)).toBe(880);
    expect(clampPaneWidth(300, 1200)).toBe(300);
    expect(clampPaneHeight(40, 800)).toBe(120);
    expect(clampPaneHeight(700, 800)).toBe(560);
    expect(clampPaneHeight(250, 800)).toBe(250);
  });
});
