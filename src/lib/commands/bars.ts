/**
 * The button bars as the core keeps them (`src-tauri/src/buttonbars.rs`, SuperXPuTTY's ButtonBars.xml in the settings
 * folder), and the calls that read and write them.
 */
import { invoke } from "@tauri-apps/api/core";

export interface BarButton {
  label: string;
  tooltip: string;
  /** "#RRGGBB": a filled circle when there's no icon */
  color: string;
  /** A session icon key */
  icon: string;
  /** "SendString" or "SpslScript" */
  action: string;
  text: string;
}

export interface Bar {
  name: string;
  buttons: BarButton[];
  /** Session icons whose tabs show this bar */
  tabIcons: string[];
}

/** Read ButtonBars.xml again (a missing file is one empty "Default" bar) */
export function loadBars(): Promise<Bar[]> {
  return invoke<Bar[]>("buttonbars_load");
}

export function saveBars(bars: Bar[]): Promise<void> {
  return invoke("buttonbars_save", { bars });
}

export function exportBar(bar: Bar, path: string): Promise<void> {
  return invoke("buttonbars_export", { bar, path });
}

/** Add an export file's bars; the bars and the last one added */
export function importBars(path: string): Promise<[Bar[], string]> {
  return invoke<[Bar[], string]>("buttonbars_import", { path });
}
