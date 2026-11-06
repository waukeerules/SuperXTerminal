/**
 * The user's profiles, as the Rust core keeps them (`src-tauri/src/profiles.rs`). Passwords never come here: a
 * profile only says whether one is stored, and the UI sends a new one (or "" to forget it) when it changes.
 */
import { hexColour } from "../terminal/prefs";
import { invoke } from "@tauri-apps/api/core";

/** The profile every session without one uses, and every profile falls back to */
export const DEFAULT_PROFILE = "Default Settings";

/** One profile; empty fields aren't set and fall back to Default Settings */
export interface Profile {
  name: string;
  username: string;
  port: number;
  keyFile: string;
  /** A command that gets a new key into the key file ({key_file}), run in the tab when it's missing or too old */
  keyFetch: string;
  /** Minutes a fetched key is good for; 0: fetched only when there's none */
  keyLifetime: number;
  /** Whether ssh offers its keys (~/.ssh and the agent) when there's no key file; null: Default Settings', else yes */
  offerKeys: boolean | null;
  /** A session with no username asks for one in the tab ("login as:"); null: Default Settings', else no */
  askUsername: boolean | null;
  jumpHost: string;
  /**
   * Where ssh connects, at the profile's port: for a session with no host of its own (as a PuTTY saved session's
   * Host Name), and for one whose username names its host as {host_address} (a CyberArk gateway)
   */
  host: string;
  keepalive: number;
  /** Which addresses a host name is reached at, as PuTTY's Internet protocol version: "", "auto", "ipv4" or "ipv6" */
  addressFamily: string;
  /** KiTTY's anti-idle string, typed after a quiet spell (terminal/antiIdle.ts); "" for Default Settings' */
  antiIdle: string;
  /** Seconds without typing before it's typed; 0 for Default Settings', else 180 */
  antiIdleSeconds: number;
  sshOptions: string;
  olderAlgorithms: boolean | null;
  /**
   * Show the server's CPU, memory, disk and network in the status bar while one of its sessions' tabs is in front
   * (src-tauri/src/server_load.rs): for Linux servers, never a Cisco. null: Default Settings', else off. A Mac only
   */
  serverLoad: boolean | null;
  /** Its own SSH algorithm lists ("kex=a,b;mac=c"; "" for Default Settings' or ssh's own) */
  algorithms: string;
  sshConfigHost: string;
  cursor: string;
  cursorBlink: boolean | null;
  bell: string;
  /**
   * Which colours its tabs have: "" or "default" (Default Settings'), "theme" (Settings > Terminal's theme), "dark", "light" or
   * "custom" (the colours below); a profile with colours and no choice is custom
   */
  colours: string;
  /** The terminal's colours when custom, "#rrggbb"; "" for Default Settings', else Settings > Terminal's or the theme's */
  background: string;
  foreground: string;
  selection: string;
  /** The cursor's colour; a session's or folder's environment colour wins over it */
  cursorColor: string;
  /**
   * The colour of its sessions' tabs, "#rrggbb", whatever the Colours choice; "" for Default Settings', else none. A
   * session's or folder's environment colour wins over it
   */
  tabColor: string;
  /**
   * The button bar (a bar's name) shown while one of its sessions' tabs is active; "" for Default Settings', else the
   * bar the tab's icon picks
   */
  buttonBar: string;
  /**
   * An SPSL script its sessions run as they open, when a session has no script of its own: a file (absolute,
   * file://, or relative to the settings folder) or an http(s) address. "" for Default Settings', else none
   */
  script: string;
  /**
   * A jump server: its sessions' tabs log in to this profile's Host, as this profile, and leave the session's host,
   * username and port to the SPSL script. This profile's alone, not taken from Default Settings
   */
  logInHere: boolean;
  hasPassword: boolean;
}

/** A new, empty profile */
export function emptyProfile(name: string): Profile {
  return {
    name, username: "", port: 0, keyFile: "", keyFetch: "", keyLifetime: 0, offerKeys: null, askUsername: null, jumpHost: "", host: "", keepalive: 0, addressFamily: "", antiIdle: "", antiIdleSeconds: 0, sshOptions: "", olderAlgorithms: null, serverLoad: null, algorithms: "",
    sshConfigHost: "", cursor: "", cursorBlink: null, bell: "", colours: "", background: "", foreground: "", selection: "", cursorColor: "", tabColor: "", buttonBar: "", script: "", logInHere: false, hasPassword: false,
  };
}

/** The window event sent when profiles change, so lists of them (Quick connect's) update at once */
export const PROFILES_CHANGED = "superxterminal:profiles-changed";

/** Every profile, Default Settings first */
export function listProfiles(): Promise<Profile[]> {
  return invoke<Profile[]>("profiles_list");
}

/**
 * Add or change a profile
 * @param previousName its name before this change; "" for a new one
 * @param password undefined keeps the stored password, "" forgets it, anything else replaces it
 */
export async function saveProfile(profile: Profile, previousName: string, password?: string): Promise<Profile[]> {
  const profiles = await invoke<Profile[]>("profiles_save", { profile, previousName, password: password ?? null });
  window.dispatchEvent(new Event(PROFILES_CHANGED));
  return profiles;
}

/** Remove a profile and its stored password */
export async function deleteProfile(name: string): Promise<Profile[]> {
  const profiles = await invoke<Profile[]>("profiles_delete", { name });
  window.dispatchEvent(new Event(PROFILES_CHANGED));
  return profiles;
}

/** A profile's own Colours choice: what it says, else "custom" when it has colours, else "" (Default Settings') */
export function ownColoursChoice(profile: Profile | undefined): string {
  if (!profile) return "";
  const choice = profile.colours?.trim() ?? "";
  // chosen in the editor over colours it still has
  if (choice === "default") return "";
  if (choice) return choice;
  return [profile.background, profile.foreground, profile.selection, profile.cursorColor].some((c) => c?.trim()) ? "custom" : "";
}

/** The Colours choice a profile's tabs go by: its own, else Default Settings', else "theme" (`colours_choice` in launch.rs) */
export function coloursChoice(profile: Profile | undefined, fallback: Profile | undefined): string {
  return ownColoursChoice(profile) || ownColoursChoice(fallback) || "theme";
}

/**
 * A profile's terminal look as a session's tab takes it (`look` in src-tauri/src/launch.rs): the theme its Colours
 * choice names ("Dark", "Light" or "" for Settings > Terminal's), and when it's custom, each colour the profile's,
 * else Default Settings', when it's "#rrggbb"; "" for Settings > Terminal's or the theme's
 */
export function profileColours(profiles: Profile[], name: string): { theme: string; background: string; foreground: string; selection: string; cursor: string; tab: string } {
  const find = (n: string) => profiles.find((p) => p.name.toLowerCase() === n.trim().toLowerCase());
  const fallback = find(DEFAULT_PROFILE);
  const profile = find(name) ?? fallback;
  const choice = coloursChoice(profile, fallback);
  const pick = (field: "background" | "foreground" | "selection" | "cursorColor") =>
    choice === "custom" ? hexColour(profile?.[field]) || hexColour(fallback?.[field]) : "";
  return {
    theme: choice === "dark" ? "Dark" : choice === "light" ? "Light" : "",
    background: pick("background"), foreground: pick("foreground"), selection: pick("selection"), cursor: pick("cursorColor"),
    // not a terminal colour, so whatever the Colours choice
    tab: hexColour(profile?.tabColor) || hexColour(fallback?.tabColor),
  };
}
