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
  sshOptions: string;
  olderAlgorithms: boolean | null;
  /** Its own SSH algorithm lists ("kex=a,b;mac=c"; "" for Default Settings' or ssh's own) */
  algorithms: string;
  sshConfigHost: string;
  cursor: string;
  cursorBlink: boolean | null;
  bell: string;
  /** The terminal's colours, "#rrggbb"; "" for Default Settings', else Settings > Terminal's or the theme's */
  background: string;
  foreground: string;
  selection: string;
  /** The cursor's colour; a session's or folder's environment colour wins over it */
  cursorColor: string;
  hasPassword: boolean;
}

/** A new, empty profile */
export function emptyProfile(name: string): Profile {
  return {
    name, username: "", port: 0, keyFile: "", keyFetch: "", keyLifetime: 0, offerKeys: null, askUsername: null, jumpHost: "", host: "", keepalive: 0, sshOptions: "", olderAlgorithms: null, algorithms: "",
    sshConfigHost: "", cursor: "", cursorBlink: null, bell: "", background: "", foreground: "", selection: "", cursorColor: "", hasPassword: false,
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

/**
 * A profile's terminal colours as a session's tab takes them (`look` in src-tauri/src/launch.rs): each one the
 * profile's, else Default Settings', when it's "#rrggbb"; "" for Settings > Terminal's or the theme's
 */
export function profileColours(profiles: Profile[], name: string): { background: string; foreground: string; selection: string; cursor: string } {
  const find = (n: string) => profiles.find((p) => p.name.toLowerCase() === n.trim().toLowerCase());
  const fallback = find(DEFAULT_PROFILE);
  const profile = find(name) ?? fallback;
  const pick = (field: "background" | "foreground" | "selection" | "cursorColor") => hexColour(profile?.[field]) || hexColour(fallback?.[field]);
  return { background: pick("background"), foreground: pick("foreground"), selection: pick("selection"), cursor: pick("cursorColor") };
}
