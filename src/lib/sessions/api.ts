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
  /** Database sessions: a change not in the table yet (waiting while it can't be reached, or refused) */
  pending: boolean;
  /** What became of it, for the tooltip */
  pendingNote: string;
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
      return null;
    case "RDP":
    case "VNC":
    case "PS":
      return null;
    case "RLOGIN":
      return "Not supported on Mac";
    default:
      return "Windows only";
  }
}

/** Whether a session opens in another app (RDP in Windows App, VNC in Screen Sharing) rather than a tab */
export function opensExternally(protocol: string): boolean {
  return ["RDP", "VNC"].includes(protocol.toUpperCase());
}

/** Open a session in another app: "open" (RDP, VNC), "terminal" (Connect external) or "filezilla"; what to say */
export function openExternally(store: Store, id: string, how: "open" | "terminal" | "filezilla"): Promise<string> {
  return invoke<string>("session_external", { store, id, how });
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
  /** How many sessions the last read (or the cache) had */
  count: number;
  /** When the sessions shown were read, "HH:MM" */
  syncedAt: string;
  /** Changes waiting to be sent, and changes the table refused */
  pending: number;
  failed: number;
}

/** A change to Database sessions not in the table yet, for Pending changes */
export interface PendingChange {
  id: string;
  /** e.g. "Change Lab/sw1", "Rename folder Lab to Labs" */
  what: string;
  queuedAt: string;
  /** Why the table refused it; null while it's waiting */
  error: string | null;
  /** Will save over someone else's change */
  force: boolean;
  /** A refused edit, which Overwrite theirs can save */
  canOverwrite: boolean;
}

/** The queued and refused changes, oldest first */
export function pendingChanges(): Promise<PendingChange[]> {
  return invoke<PendingChange[]>("db_pending_list");
}

/** Send changes again (overwrite: edits over someone else's change); they go now if the table can be reached */
export function retryPending(ids: string[], overwrite: boolean): Promise<DbState> {
  return invoke<DbState>("db_pending_retry", { ids, overwrite });
}

/** Drop changes; the sessions go back to what the table has */
export function discardPending(ids: string[]): Promise<DbState> {
  return invoke<DbState>("db_pending_discard", { ids });
}

/** How Database sessions stand now, without reading the table */
export function databaseStatus(): Promise<DbState> {
  return invoke<DbState>("db_status");
}

/** The user's Favorites and Recent sessions (SuperXPuTTY's SessionShortcuts.json), as session ids */
export interface Shortcuts {
  Favorites: string[];
  /** Most recent first */
  Recent: string[];
  FavoritesExpanded: boolean;
  RecentExpanded: boolean;
}

export function loadShortcuts(): Promise<Shortcuts> {
  return invoke<Shortcuts>("shortcuts_get");
}

/** Add a session to Favorites (on) or take it out */
export function setFavorite(id: string, on: boolean): Promise<Shortcuts> {
  return invoke<Shortcuts>("shortcuts_favorite", { id, on });
}

/** Put a session just opened from the tree first in Recent */
export function recordOpened(id: string): Promise<Shortcuts> {
  return invoke<Shortcuts>("shortcuts_opened", { id });
}

/** Take a session out of Recent, or clear Recent (null) */
export function forgetRecent(id: string | null): Promise<Shortcuts> {
  return invoke<Shortcuts>("shortcuts_forget_recent", { id });
}

/** Remember the user opening or closing Favorites or Recent */
export function setGroupExpanded(group: "favorites" | "recent", open: boolean): Promise<Shortcuts> {
  return invoke<Shortcuts>("shortcuts_expanded", { group, open });
}

/** Show the cached Database sessions at once, before the table is read */
export function cachedDatabase(): Promise<DbState> {
  return invoke<DbState>("db_cached");
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
