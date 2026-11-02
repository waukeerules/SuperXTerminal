/**
 * The sessions as the Rust core gives them (`src-tauri/src/sessions.rs`): what the Sessions tree shows. Secrets
 * (a `-pw` in the extra arguments) never come here; the core starts sessions itself.
 */
import { invoke } from "@tauri-apps/api/core";

/** Which file a session is from: Sessions.xml (shared) or PrivateSessions.xml (the user's own) */
export type Store = "saved" | "private" | "database";

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
  /** Database sessions: who changed it last (their SuperXPuTTY nickname) */
  updatedBy: string;
  /** Database sessions: locked against changes in the table */
  locked: boolean;
  /** Database sessions: the columns locked in the table, comma-separated ("host,port") */
  lockedFields: string;
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

/** Save a session (`originalId` null for a new one) to its file or the Database table; returns its id */
export function saveSession(store: Store, originalId: string | null, fields: SessionFields): Promise<string> {
  if (store === "database") return invoke<string>("db_session_save", { originalId, fields });
  return invoke<string>("session_save", { store, originalId, fields });
}

/** Delete a session from its file or the Database table */
export function deleteSession(store: Store, id: string): Promise<void> {
  if (store === "database") return invoke("db_session_delete", { id });
  return invoke("session_delete", { store, id });
}

/** SuperXPuTTY's protocols, as Edit Session's Connection type lists them */
export const PROTOCOLS = ["SSH", "Telnet", "Rlogin", "Raw", "Serial", "VNC", "RDP", "Mintty", "Cygterm", "WSL", "WINCMD", "PS"];

/** Rename or move a folder: every session below goes with it, and its settings */
export function moveFolder(store: Store, from: string, to: string): Promise<void> {
  if (store === "database") return invoke("db_folder_move", { from, to });
  return invoke("folder_move", { store, from, to });
}

/** Delete a folder and every session in it */
export function deleteFolder(store: Store, path: string): Promise<void> {
  if (store === "database") return invoke("db_folder_delete", { path });
  return invoke("folder_delete", { store, path });
}

/** Save a folder's settings; all empty removes them */
export function saveFolderSettings(store: Store, fields: FolderSetting): Promise<void> {
  if (store === "database") return invoke("db_folder_settings_save", { fields });
  return invoke("folder_settings_save", { store, fields });
}

/** Move a session into another folder of its store; returns its new id */
export function moveSession(store: Store, id: string, folder: string): Promise<string> {
  if (store === "database") return invoke<string>("db_session_move", { id, folder });
  return invoke<string>("session_move", { store, id, folder });
}

/** How Database sessions stand: on or off, whether the user may change them, and why the last read failed */
export interface DbState {
  enabled: boolean;
  writable: boolean;
  error: string;
  source: string;
}

/** Read the Database sessions table again (or keep the cache when it can't be reached) */
export function refreshDatabase(): Promise<DbState> {
  return invoke<DbState>("db_refresh");
}

/** Whether the table changed since the last read */
export function databaseChanged(): Promise<boolean> {
  return invoke<boolean>("db_check");
}

/** The window event sent when Database sessions' settings are saved, so the tree reads the table again */
export const DATABASE_SAVED = "superxterminal:database-saved";

/**
 * Copy (or move) sessions to another root, each to the same folder path there, as SuperXPuTTY's Move to / Copy to.
 * Returns how many arrived and what went wrong with the others.
 */
export function transferSessions(from: Store, ids: string[], to: Store, move: boolean): Promise<{ done: number; problems: string[] }> {
  return invoke("sessions_transfer", { from, ids, to, deleteSource: move });
}
