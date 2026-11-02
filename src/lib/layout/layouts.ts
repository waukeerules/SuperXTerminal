/**
 * Layouts (`src-tauri/src/layouts.rs`), as SuperXPuTTY's: the tabs open and how they're split, and the panes, saved
 * by name in `<SettingsFolder>/layouts-mac` and opened again, or at start as the default layout. "<Auto Restore>" is
 * the user's own, saved as the tabs change.
 */
import { invoke } from "@tauri-apps/api/core";
import type { SerializedDockview } from "dockview-core";

/** SuperXPuTTY's LayoutData.AutoRestore */
export const AUTO_RESTORE = "<Auto Restore>";

/** A saved layout */
export interface Layout {
  version: 1;
  /** The tab area: its groups and sizes, and each tab's title and what it runs */
  tabs: SerializedDockview;
  /** The panes beside the tabs */
  panes: {
    sessionsPaneWidth: number;
    sessionsAutoHide: boolean;
    showLayouts: boolean;
    showSessionDetail: boolean;
  };
}

/** The layouts' names, "<Auto Restore>" first */
export function listLayouts(): Promise<string[]> {
  return invoke("layouts_list");
}

/** A layout; null when it hasn't been saved or can't be read */
export async function readLayout(name: string): Promise<Layout | null> {
  const text = await invoke<string | null>("layout_read", { name });
  if (!text) return null;
  try {
    const layout = JSON.parse(text) as Layout;
    return layout?.tabs ? layout : null;
  } catch {
    return null;
  }
}

export function saveLayout(name: string, layout: Layout): Promise<void> {
  return invoke("layout_save", { name, text: JSON.stringify(layout, null, 2) });
}

export function deleteLayout(name: string): Promise<void> {
  return invoke("layout_delete", { name });
}

export function renameLayout(from: string, to: string): Promise<void> {
  return invoke("layout_rename", { from, to });
}

/** The window event sent when the layouts change (saved, renamed, deleted), so the Layouts pane lists them again */
export const LAYOUTS_CHANGED = "superxterminal:layouts-changed";
