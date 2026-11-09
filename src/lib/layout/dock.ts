/**
 * Docking the Sessions pane, as DockPanelSuite's in SuperXPuTTY: its title dragged to the window's left, right or
 * bottom edge, where it stays (Settings' sessionsDock, and each layout's panes). App.svelte does the dragging and
 * shows where the pane will land; this says where that is, and the sizes the pane can take on each side.
 */

/** Where the Sessions pane is docked: beside the tabs on the left or right, or under them */
export type Dock = "left" | "right" | "bottom";

/** The room the tabs keep beside or above the pane, in points */
const TABS_MIN_WIDTH = 320;
const TABS_MIN_HEIGHT = 240;

/** An area of the window: where the pane and the tabs are (App's .main) */
export interface Area {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * How far each drop outline reaches into the area while the pane's title is dragged: the pane's own width at the
 * sides and its height at the bottom, as it would be docked there, but each at most a third of the area so the
 * outlines never meet. App draws the outlines at these sizes, and dockTarget lands in them, so where the pointer is
 * inside one is where the pane goes.
 * @param area Where the pane and the tabs are
 * @param pane The pane's width beside the tabs and its height under them
 */
export function dockZones(area: Area, pane: { width: number; height: number }): { width: number; height: number } {
  return { width: Math.min(pane.width, area.width / 3), height: Math.min(pane.height, area.height / 3) };
}

/**
 * The edge the pane lands on if let go here: anywhere in the bottom outline (the full width, the pane's height from
 * the foot), else in the left or right one (the pane's width from that side, down to the bottom outline, as they're
 * drawn); null between them or outside the area, which leaves the pane where it was
 * @param x The pointer, in the window's coordinates
 * @param y The pointer, in the window's coordinates
 * @param area Where the pane and the tabs are
 * @param pane The pane's width beside the tabs and its height under them (dockZones)
 */
export function dockTarget(x: number, y: number, area: Area, pane: { width: number; height: number }): Dock | null {
  const right = area.left + area.width;
  const bottom = area.top + area.height;
  if (x < area.left || x > right || y < area.top || y > bottom) return null;
  const zone = dockZones(area, pane);
  if (bottom - y <= zone.height) return "bottom";
  if (x - area.left < zone.width) return "left";
  if (right - x < zone.width) return "right";
  return null;
}

/**
 * A width the pane can take beside the tabs: at least 160 points, and leaving the tabs 320
 * @param width The width wanted
 * @param areaWidth The width of the area it shares with the tabs
 */
export function clampPaneWidth(width: number, areaWidth: number): number {
  return Math.round(Math.max(160, Math.min(width, areaWidth - TABS_MIN_WIDTH)));
}

/**
 * A height the pane can take under the tabs: at least 120 points, and leaving the tabs 240
 * @param height The height wanted
 * @param areaHeight The height of the area it shares with the tabs
 */
export function clampPaneHeight(height: number, areaHeight: number): number {
  return Math.round(Math.max(120, Math.min(height, areaHeight - TABS_MIN_HEIGHT)));
}
