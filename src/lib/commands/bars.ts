/**
 * The button bars as the core keeps them (`src-tauri/src/buttonbars.rs`, SuperXPuTTY's ButtonBars.xml in the settings
 * folder), and the calls that read and write them.
 */
import { invoke } from "@tauri-apps/api/core";
import { CUSTOM_PREFIX, parseKey, resolveIcon } from "../icons/library";

/** One button on a bar, as ButtonBars.xml holds it (SuperXPuTTY's) */
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

/** One button bar, as ButtonBars.xml holds it */
export interface Bar {
  name: string;
  buttons: BarButton[];
  /** Session icons whose tabs show this bar */
  tabIcons: string[];
}

/**
 * The colour a button shows, which its command's line numbers take: its icon's (the colour chosen for it, else the
 * icon's own; none for a custom picture without one), else its circle's; "" for none
 */
export function buttonColour(button: BarButton): string {
  if (button.icon) {
    const key = parseKey(button.icon);
    if (key?.color) return key.color;
    // a custom picture has its own colours, none to take
    if (!key || key.name.toLowerCase().startsWith(CUSTOM_PREFIX)) return "";
    return resolveIcon(button.icon, new Set()).color ?? "";
  }
  return button.color;
}

/** Read ButtonBars.xml again (a missing file is one empty "Default" bar) */
export function loadBars(): Promise<Bar[]> {
  return invoke<Bar[]>("buttonbars_load");
}

/** Write ButtonBars.xml with these bars */
export function saveBars(bars: Bar[]): Promise<void> {
  return invoke("buttonbars_save", { bars });
}

/** Save one bar as SuperXPuTTY's export JSON, at `path` */
export function exportBar(bar: Bar, path: string): Promise<void> {
  return invoke("buttonbars_export", { bar, path });
}

/** Add an export file's bars; the bars and the last one added */
export function importBars(path: string): Promise<[Bar[], string]> {
  return invoke<[Bar[], string]>("buttonbars_import", { path });
}
