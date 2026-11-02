/**
 * Session icon keys, as SuperXPuTTY's IconLibrary: a name plus an optional colour, "switch#1E63B4". The name is a
 * vector template (`templates.ts`), a brand logo (`brands.ts`) or a custom icon ("custom:juniper", a one-colour PNG
 * in `<SettingsFolder>/icons`). Old PNG keys from SuperPuTTY's Silk icons map onto templates, so existing
 * Sessions.xml files show the right icons without being changed.
 */
import { logos } from "./brands";
import { templates } from "./templates";

/** Old PNG icon keys and the template (and colour) that replaces each, as IconLibrary's legacy table */
const LEGACY: Record<string, string> = {
  computer: "workstation",
  monitor: "workstation",
  application_xp_terminal: "terminal",
  application_osx_terminal: "terminal#263238",
  drive_network: "storage",
  telephone: "phone",
  webcam: "camera",
  wifi_device: "wireless",
  green_switch: "switch#2E7D32",
  blue_switch: "switch#1E63B4",
  red_switch: "switch#C62828",
  orange_switch: "switch#EF6C00",
  blue_router: "router#1E63B4",
  red_router: "router#C62828",
  orange_router: "router#EF6C00",
  purple_router: "router#6A3D9A",
  voice_router: "voicerouter",
  green_nexus: "nexus#2E7D32",
  red_nexus: "nexus#C62828",
  red_big_nexus: "nexus#8E1B1B",
  orange_nexus: "nexus#EF6C00",
  new_firewall: "firewall#78909C",
  flag_blue: "flag#1E63B4",
  flag_green: "flag#2E7D32",
  flag_orange: "flag#EF6C00",
  flag_pink: "flag#D81B60",
  flag_purple: "flag#6A3D9A",
  flag_red: "flag#C62828",
  flag_yellow: "flag#F9A825",
  arch_linux: "archlinux",
  linux_mint: "linuxmint",
  puffy: "openbsd",
  suse_linux: "opensuse",
  tux: "linux",
  time: "clock",
  comment: "chat",
  compress: "archive",
  feed: "rss",
  information: "info",
  ipod: "mobile",
  lightning: "bolt",
  lorry: "truck",
  report: "document",
  television: "tv",
  world: "globe",
};

/** The icon a session gets when its key is empty or names nothing drawable */
export const FALLBACK_ICON = "workstation";

/** Prefix of custom icon keys */
export const CUSTOM_PREFIX = "custom:";

/** A key resolved to what draws it */
export type ResolvedIcon =
  | { kind: "template"; name: string; color: string }
  | { kind: "logo"; name: string; color: string }
  | { kind: "custom"; name: string; color: string | null };

/** Split a key into its name and colour, after mapping an old PNG key; null if it isn't a key at all */
export function parseKey(key: string): { name: string; color: string | null } | null {
  const mapped = LEGACY[key.trim().toLowerCase()] ?? key.trim();
  const match = /^([^#]+)(?:#([0-9A-Fa-f]{6}))?$/.exec(mapped);
  if (!match) return null;
  return { name: match[1], color: match[2] ? `#${match[2].toUpperCase()}` : null };
}

/**
 * What draws a key: its template, logo or custom icon in its colour (the key's own, else the default); keys that
 * name nothing drawable fall back to the workstation template. Custom icons' default colour is the PNG's own, so
 * `color` is null for them when the key has none.
 */
export function resolveIcon(key: string, customNames: ReadonlySet<string>): ResolvedIcon {
  const parsed = parseKey(key || FALLBACK_ICON) ?? { name: FALLBACK_ICON, color: null };
  const name = parsed.name.toLowerCase();
  if (templates[name] && !templates[name].glyph) {
    return { kind: "template", name, color: parsed.color ?? templates[name].color };
  }
  if (logos[name]) return { kind: "logo", name, color: parsed.color ?? logos[name].color };
  if (name.startsWith(CUSTOM_PREFIX) && customNames.has(name.slice(CUSTOM_PREFIX.length))) {
    return { kind: "custom", name: name.slice(CUSTOM_PREFIX.length), color: parsed.color };
  }
  return { kind: "template", name: FALLBACK_ICON, color: templates[FALLBACK_ICON].color };
}
