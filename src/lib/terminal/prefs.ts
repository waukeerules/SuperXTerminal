/**
 * The terminal's preferences, as SuperXPuTTY's TerminalPreferences: one set for every tab, by name, as strings. Each
 * tab's page gets all of them (defaults filled in) in its config, and again whenever one changes, whether in
 * Settings > Terminal or from the page's own menu (font size, theme, copy on select...).
 */
import { isDark, saveSettings, settings } from "../settings/store.svelte";

/** Every preference and its default */
export const PREF_DEFAULTS: Record<string, string> = {
  /** "auto", "none" or a rule set's id */
  ruleSet: "auto",
  /**
   * "false": the rule sets that come with the app (Cisco, Junos, Generic) aren't used, only the team's and the
   * user's own (highlighting.ts), for someone whose own rules they get in the way of
   */
  builtInRules: "true",
  /** A theme's name; "" for PuTTY's; "@look" for Light or Dark as the app is (Settings > General > Theme) */
  theme: "",
  /** The terminal's own colours over the theme's, as "#rrggbb"; "" for the theme's */
  colorBackground: "",
  colorForeground: "",
  colorSelection: "",
  /** The cursor's; a session's or folder's environment colour wins over it */
  colorCursor: "",
  /**
   * "profile" (the default: the profile's, Settings > Profiles > Font, else the theme's), "" for the theme's whatever
   * the profile says, or a font's name. Was "" by default: with no profile font the two draw the same
   */
  fontFamily: "profile",
  /** Pixels; "0" for the theme's or profile's */
  fontSize: "0",
  /** "profile", "block", "underline" or "bar" */
  cursorShape: "profile",
  /** "profile", "on" or "off" */
  cursorBlink: "profile",
  /** "profile", "visual", "sound" or "none" */
  bell: "profile",
  copyOnSelect: "true",
  /**
   * The characters a word ends at when it's double-clicked, besides a space (always one): xterm.js's own by default.
   * With # added, a double-click on a switch's "sw1#show" selects "show"; with ( and ) taken out, "sw1(config)" whole.
   * "" for none but the space
   */
  wordSeparators: "()[]{}',\"`",
  markMatches: "true",
  /** "false": lines aren't wrapped at the tab's width; the terminal is 300 columns wide and the view pans along them */
  wrap: "true",
  /** "true": the output's line numbers in a gutter at the left (none while a full-screen program runs) */
  lineNumbers: "false",
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
  /** Whether the script box (Edit Button, the Script Editor) wraps long lines: "true" or "false" */
  scriptWrap: "true",
};

/** Every preference, with its default where it isn't set */
export function allPrefs(): Record<string, string> {
  return { ...PREF_DEFAULTS, ...settings.terminalPrefs };
}

/** One preference's value */
export function pref(name: string): string {
  return allPrefs()[name] ?? "";
}

/** Who to tell when a preference changes */
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

/** The theme the terminal draws with: PuTTY's when none is chosen, Light or Dark when following the app's look */
export function drawnTheme(): string {
  const theme = pref("theme");
  return theme === "@look" ? (isDark() ? "Dark" : "Light") : theme || "PuTTY";
}

/** A colour as "#rrggbb" in lower case, or "" when it isn't one */
export function hexColour(value: string | undefined | null): string {
  const c = (value ?? "").trim();
  return /^#[0-9a-f]{6}$/i.test(c) ? c.toLowerCase() : "";
}
