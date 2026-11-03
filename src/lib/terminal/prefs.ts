/**
 * The terminal's preferences, as SuperXPuTTY's TerminalPreferences: one set for every tab, by name, as strings. Each
 * tab's page gets all of them (defaults filled in) in its config, and again whenever one changes, whether in
 * Settings > Terminal or from the page's own menu (font size, theme, copy on select...).
 */
import { saveSettings, settings } from "../settings/store.svelte";

/** Every preference and its default */
export const PREF_DEFAULTS: Record<string, string> = {
  /** "auto", "none" or a rule set's id */
  ruleSet: "auto",
  /** A theme's name; "" for PuTTY's; "@look" for Light or Dark as the app is (Settings > General > Theme) */
  theme: "",
  /** The terminal's own colours over the theme's, as "#rrggbb"; "" for the theme's */
  colorBackground: "",
  colorForeground: "",
  colorSelection: "",
  /** "" for the theme's, "profile", or a font's name */
  fontFamily: "",
  /** Pixels; "0" for the theme's or profile's */
  fontSize: "0",
  /** "profile", "block", "underline" or "bar" */
  cursorShape: "profile",
  /** "profile", "on" or "off" */
  cursorBlink: "profile",
  /** "profile", "visual", "sound" or "none" */
  bell: "profile",
  copyOnSelect: "true",
  markMatches: "true",
  /** "false": lines aren't wrapped at the tab's width; the terminal is 300 columns wide and the view pans along them */
  wrap: "true",
  /** A paste of this many lines or more is checked first; "0" never */
  pasteLines: "2",
  /** Lines kept; "0" for unlimited */
  scrollback: "10000",
  /** "profile" (the PuTTY profile's Backspace, Home/End and function keys) or "xterm" */
  keys: "profile",
  /** Seconds between time stamps in session logs; "0" for none */
  logTimestamps: "30",
  /** "webgl" or "dom" */
  renderer: "webgl",
};

/** Every preference, with its default where it isn't set */
export function allPrefs(): Record<string, string> {
  return { ...PREF_DEFAULTS, ...settings.terminalPrefs };
}

/** One preference's value */
export function pref(name: string): string {
  return allPrefs()[name] ?? "";
}

const listeners = new Set<(prefs: Record<string, string>) => void>();

/** Be told every preference whenever one changes; returns how to stop */
export function onPrefsChanged(listener: (prefs: Record<string, string>) => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Change preferences (unknown names are ignored; null puts one back to its default), save, and tell every tab */
export async function setPrefs(changes: Record<string, string | null>): Promise<void> {
  const next = { ...settings.terminalPrefs };
  let changed = false;
  for (const [name, value] of Object.entries(changes)) {
    if (!(name in PREF_DEFAULTS)) continue;
    const old = next[name] ?? PREF_DEFAULTS[name];
    if (value === null || value === PREF_DEFAULTS[name]) delete next[name];
    else next[name] = value;
    changed ||= (next[name] ?? PREF_DEFAULTS[name]) !== old;
  }
  if (!changed) return;
  await saveSettings({ terminalPrefs: next });
  const all = allPrefs();
  for (const listener of listeners) listener(all);
}
