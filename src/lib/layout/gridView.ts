/**
 * Grid View's choices that App makes as sessions open. Here, apart from App, so they can be tested without a window.
 *
 * A Remote Desktop needs room: tiled with terminals it's a small picture of a desktop. So in Grid View, as Settings >
 * General > Tabs > Remote Desktop in Grid View chooses (a new window by default), an RDP session opens in a window of
 * its own, where it takes the whole window; more desktops opened from there go in as its tabs.
 */

/**
 * Whether an RDP session opened now goes to a new window: Grid View is on, it's set to, and this window has tabs that
 * aren't all Remote Desktops. A window with none takes it, so the new window opened for it doesn't send it on again;
 * so does a window of desktops alone (one Grid View sent a desktop to), as another tab, which isn't tiled there
 * (TabArea.desktopsApart).
 * @param grid Whether View > Grid View is on
 * @param rdpInGrid Settings > General > Tabs > Remote Desktop in Grid View: "window" or "tile"
 * @param openTabs How many tabs this window has now
 * @param onlyDesktops Whether every one of them is a Remote Desktop
 * @returns true to open it in a new window, false to open it here
 */
export function rdpOpensInNewWindow(grid: boolean, rdpInGrid: "window" | "tile", openTabs: number, onlyDesktops: boolean): boolean {
  return grid && rdpInGrid === "window" && openTabs > 0 && !onlyDesktops;
}
