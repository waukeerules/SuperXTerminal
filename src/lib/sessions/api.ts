/**
 * The sessions as the Rust core gives them (`src-tauri/src/sessions.rs`): what the Sessions tree shows. Secrets
 * (a `-pw` in the extra arguments) never come here; the core starts sessions itself.
 */
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

/** Which file a session is from: Sessions.xml (shared) or PrivateSessions.xml (the user's own) */
export type Store = "saved" | "private";

/** One session, as the tree shows it */
export interface SessionView {
  id: string;
  store: Store;
  name: string;
  folder: string;
  protocol: string;
  host: string;
  port: number;
  username: string;
  profile: string;
  icon: string;
  color: string;
  note: string;
}

/** One folder's own settings: its colour and the defaults its sessions inherit */
export interface FolderSetting {
  path: string;
  color: string;
  username: string;
  puttyProfile: string;
  port: number;
  jumpHost: string;
  highlightRules: string;
}

/** What loading gives: the sessions, the folders' settings, and any file that couldn't be read */
export interface SessionsLoaded {
  sessions: SessionView[];
  folders: [Store, FolderSetting][];
  problems: string[];
  savedFile: string | null;
  privateFile: string;
}

/** Read the session files (again) */
export function loadSessions(): Promise<SessionsLoaded> {
  return invoke<SessionsLoaded>("sessions_load");
}

/** Ask for the settings folder (where Sessions.xml is) and remember it; false if the user cancelled */
export async function chooseSettingsFolder(): Promise<boolean> {
  const path = await open({ directory: true, title: "Choose the settings folder (the folder holding Sessions.xml)" });
  if (typeof path !== "string") return false;
  await invoke("settings_set_folder", { path });
  return true;
}

/** Protocols this Mac can open now, and why the others can't (shown dimmed, with the reason, in the tree) */
export function unavailableReason(protocol: string): string | null {
  switch (protocol.toUpperCase()) {
    case "SSH":
    case "":
    case "WINCMD":
    case "WSL":
      return null;
    case "TELNET":
    case "RAW":
    case "SERIAL":
      return "Not available yet: comes in M5";
    case "RDP":
      return "Opens in Windows App: comes in M5";
    case "VNC":
      return "Opens in Screen Sharing: comes in M5";
    case "PS":
      return "Opens in PowerShell (pwsh) if installed: comes in M5";
    case "RLOGIN":
      return "Not supported on Mac";
    default:
      return "Windows only";
  }
}

/** Whether a session opens as a local shell here (cmd and WSL sessions do, as docs/PLAN.md §2 says) */
export function opensAsLocalShell(protocol: string): boolean {
  return ["WINCMD", "WSL"].includes(protocol.toUpperCase());
}

/** One session's fields, as Edit Session shows them; a `-pw` password in `extraArgs` comes masked */
export interface SessionFields {
  name: string;
  folder: string;
  protocol: string;
  host: string;
  port: number;
  username: string;
  profile: string;
  color: string;
  icon: string;
  highlightRules: string;
  extraArgs: string;
  jumpHost: string;
  script: string;
  note: string;
  localPath: string;
  remotePath: string;
}

/** A session's fields for editing */
export function getSession(store: Store, id: string): Promise<SessionFields> {
  return invoke<SessionFields>("session_get", { store, id });
}

/** Save a session (`originalId` null for a new one) to its file; returns its id */
export function saveSession(store: Store, originalId: string | null, fields: SessionFields): Promise<string> {
  return invoke<string>("session_save", { store, originalId, fields });
}

/** Delete a session from its file */
export function deleteSession(store: Store, id: string): Promise<void> {
  return invoke("session_delete", { store, id });
}

/** SuperXPuTTY's protocols, as Edit Session's Connection type lists them */
export const PROTOCOLS = ["SSH", "Telnet", "Rlogin", "Raw", "Serial", "VNC", "RDP", "Mintty", "Cygterm", "WSL", "WINCMD", "PS"];
