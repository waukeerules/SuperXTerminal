/**
 * The user's profiles, as the Rust core keeps them (`src-tauri/src/profiles.rs`). Passwords never come here: a
 * profile only says whether one is stored, and the UI sends a new one (or "" to forget it) when it changes.
 */
import { invoke } from "@tauri-apps/api/core";

/** The profile every session without one uses, and every profile falls back to */
export const DEFAULT_PROFILE = "Default Settings";

/** One profile; empty fields aren't set and fall back to Default Settings */
export interface Profile {
  name: string;
  username: string;
  port: number;
  keyFile: string;
  jumpHost: string;
  keepalive: number;
  sshOptions: string;
  olderAlgorithms: boolean | null;
  sshConfigHost: string;
  cursor: string;
  cursorBlink: boolean | null;
  bell: string;
  hasPassword: boolean;
}

/** A new, empty profile */
export function emptyProfile(name: string): Profile {
  return {
    name, username: "", port: 0, keyFile: "", jumpHost: "", keepalive: 0, sshOptions: "", olderAlgorithms: null,
    sshConfigHost: "", cursor: "", cursorBlink: null, bell: "", hasPassword: false,
  };
}

/** Every profile, Default Settings first */
export function listProfiles(): Promise<Profile[]> {
  return invoke<Profile[]>("profiles_list");
}

/**
 * Add or change a profile
 * @param previousName its name before this change; "" for a new one
 * @param password undefined keeps the stored password, "" forgets it, anything else replaces it
 */
export function saveProfile(profile: Profile, previousName: string, password?: string): Promise<Profile[]> {
  return invoke<Profile[]>("profiles_save", { profile, previousName, password: password ?? null });
}

/** Remove a profile and its stored password */
export function deleteProfile(name: string): Promise<Profile[]> {
  return invoke<Profile[]>("profiles_delete", { name });
}
