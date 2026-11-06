/**
 * The button bars as the core keeps them (`src-tauri/src/buttonbars.rs`, SuperXPuTTY's ButtonBars.xml in the settings
 * folder), and the calls that read and write them.
 */
import { invoke } from "@tauri-apps/api/core";
import { CUSTOM_PREFIX, parseKey, resolveIcon } from "../icons/library";
import { DEFAULT_PROFILE } from "../settings/profiles";

/** One button on a bar, as ButtonBars.xml holds it (SuperXPuTTY's) */
export interface BarButton {
  label: string;
  tooltip: string;
  /** "#RRGGBB": a filled circle when there's no icon */
  color: string;
  /** A session icon key */
  icon: string;
  /** "SendString", "SpslScript" or "RhaiScript" (SuperXTerminal's own) */
  action: string;
  text: string;
  /**
   * Runs at once, without the "Run …?" question Settings > General > Button bar asks for (Edit Button's "Ask before
   * it runs" unticked), for a harmless command. Kept in the button, so it's the same for everyone using the bars
   */
  skipConfirm?: boolean;
}

/** One button bar, as ButtonBars.xml holds it */
export interface Bar {
  name: string;
  buttons: BarButton[];
  /** Session icons whose tabs show this bar */
  tabIcons: string[];
}

/** What picks the bar shown: the active tab, the profiles, and the bar picked by hand */
export interface BarChoice {
  /** The active tab's icon key; "" for no tab */
  icon: string;
  /** The active tab's session's own bar (Edit Session > Terminal); "" for none */
  session: string;
  /** The profile the active tab's session uses; "" for none (no tab, or not started yet) */
  profile: string;
  /** Each profile's name and button bar (Settings > Profiles), Default Settings among them */
  profiles: { name: string; buttonBar: string }[];
  /** The bar last picked from the bar picker (Settings' activeButtonBar); "" for none */
  chosen: string;
  /** Whether it was picked by hand since the active tab changed, which wins until it changes again */
  picked: boolean;
}

/**
 * Which bar shows, as ButtonBarStrip.CurrentBar with sessions' and profiles' added: a bar picked by hand while the tab
 * stays; else the session's own bar; else the profile's own bar; else the first bar whose Tab Icons list the tab's icon (SuperXPuTTY's); else Default
 * Settings' bar; else the bar last picked; else the first. A bar named that isn't there (renamed or deleted) is
 * passed over. Default Settings' comes after the tab icons, so a bar for an icon still shows for profiles without one.
 */
export function barFor<B extends { name: string; tabIcons: string[] }>(bars: B[], choice: BarChoice): B {
  const named = (name: string | undefined) =>
    name?.trim() ? bars.find((b) => b.name.toLowerCase() === name.trim().toLowerCase()) : undefined;
  const chosen = named(choice.chosen) ?? bars[0];
  if (choice.picked) return chosen;
  const session = named(choice.session);
  if (session) return session;
  const profile = (name: string) => choice.profiles.find((p) => p.name.toLowerCase() === name.trim().toLowerCase());
  // a session with no profile of its own reports Default Settings, whose bar comes after the tab icons
  const ownProfile = choice.profile && choice.profile.trim().toLowerCase() !== DEFAULT_PROFILE.toLowerCase();
  const own = ownProfile ? named(profile(choice.profile)?.buttonBar) : undefined;
  if (own) return own;
  const icon = choice.icon.toLowerCase();
  const base = icon.split("#")[0];
  const forIcon = icon ? bars.find((b) => b.tabIcons.some((t) => t.toLowerCase() === icon || t.toLowerCase() === base)) : undefined;
  if (forIcon) return forIcon;
  // Default Settings' only for a tab with a session (not when there's no tab at all)
  const fallback = choice.profile || choice.icon ? named(profile(DEFAULT_PROFILE)?.buttonBar) : undefined;
  return fallback ?? chosen;
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
