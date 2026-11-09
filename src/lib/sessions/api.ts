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
  /** The extra arguments with any -pw value masked, and the SPSL script file, for the search */
  extraArgs: string;
  script: string;
  /** The button bar shown while its tab is active, SuperXTerminal's own; "" for its profile's, else the tab icon's */
  buttonBar: string;
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
  /** Saved sessions are turned off (Settings > General > Use Sessions.xml) */
  savedOff: boolean;
  /** Private sessions are turned off (Settings > General > Use PrivateSessions.xml) */
  privateOff?: boolean;
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
  /** Its own RDP options, as SuperXPuTTY's RdpOptions ("" for the defaults) */
  rdpOptions: string;
  /** Its own SSH algorithm lists ("kex=a,b;mac=c"; "" for its profile's), SuperXTerminal's own */
  sshAlgorithms: string;
  /** The button bar shown while its tab is active, SuperXTerminal's own; "" for its profile's, else the tab icon's */
  buttonBar: string;
}

/** A session's fields for editing */
export function getSession(store: Store, id: string): Promise<SessionFields> {
  return invoke<SessionFields>("session_get", { store, id });
}

/**
 * Save a session (`originalId` null for a new one) to its file or the Database table; returns its id. `copyOf` is the
 * session a new one was filled in from (Duplicate as…), whose -pw password it keeps.
 */
export function saveSession(store: Store, originalId: string | null, fields: SessionFields, copyOf: [Store, string] | null = null): Promise<string> {
  if (store === "database") return invoke<string>("db_session_save", { originalId, fields, copyOf });
  return invoke<string>("session_save", { store, originalId, fields, copyOf });
}

/** Delete a session from its file or the Database table */
export function deleteSession(store: Store, id: string): Promise<void> {
  if (store === "database") return invoke("db_session_delete", { id });
  return invoke("session_delete", { store, id });
}

/**
 * What a jump server needs for the Jump host field (ssh -J), shown under it in Edit Session and Settings > Profiles.
 * Forwarding is what usually stops it, as some servers have it off; a tab that hits it says so too (PageHost's advice
 * for "jumpForwarding"). Generic: sshd's own setting, or the server's SSH settings for one that manages sshd itself
 */
export const JUMP_HOST_NEEDS =
  "The jump server must allow TCP forwarding (AllowTcpForwarding in its sshd_config, or its SSH settings) and reach " +
  "the host itself. Where it can't, log in through it with a profile that has Log in here.";

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

/** Move a session into another folder of its store, as `name` when given (its own is taken there); returns its new id */
export function moveSession(store: Store, id: string, folder: string, name: string | null = null): Promise<string> {
  if (store === "database") return invoke<string>("db_session_move", { id, folder, name });
  return invoke<string>("session_move", { store, id, folder, name });
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

/** The user's Favorites and Recent */
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
 * Copy (or move) sessions to another root, as SuperXPuTTY's Move to / Copy to and drags: each to the folder given in
 * `folders` (same order as `ids`), else the same folder path there. A name taken there gets -1, -2... added when
 * `renameTaken`, else that session is left out. Returns how many arrived and what went wrong with the others.
 */
export function transferSessions(
  from: Store, ids: string[], to: Store, move: boolean, folders: string[] | null = null, renameTaken = false,
): Promise<{ done: number; problems: string[] }> {
  return invoke("sessions_transfer", { from, ids, to, deleteSource: move, folders, renameTaken });
}

/** The new values for Edit sessions…: a field left out stays as each session has it */
export interface BulkChange {
  username?: string;
  profile?: string;
  port?: number;
  protocol?: string;
  icon?: string;
  color?: string;
  extraArgs?: string;
  jumpHost?: string;
  highlightRules?: string;
}

/**
 * Change the same fields on many sessions at once, as SuperXPuTTY's Edit sessions…: Database sessions with a changed
 * field locked are left out, and listed in `problems`.
 */
export function bulkEdit(items: [Store, string][], change: BulkChange): Promise<{ done: number; problems: string[] }> {
  return invoke("sessions_bulk_edit", { items, change });
}

/**
 * One session's local overrides (SessionOverrides.json, as SuperXPuTTY's): the user's own values, used when it opens
 * instead of the stored ones. Absent (or Port 0) isn't overridden; JumpHost "" means none.
 */
export interface Override {
  Host?: string;
  Port: number;
  Username?: string;
  PuttyProfile?: string;
  ExtraArgs?: string;
  JumpHost?: string;
  Note?: string;
  /** Turned off: kept, but the session opens as stored (written only when off) */
  Disabled?: boolean;
}

/** Whether a session's overrides are used when it opens: it has some, they're on, and so is Tools > Use Local Overrides */
export function overridesInUse(o: Override | undefined, globallyOn: boolean): boolean {
  return globallyOn && !!o && !o.Disabled;
}

/**
 * A folder's Use Local Overrides, for the sessions in it that have overrides: ticked when they're all on, and the
 * changes a click makes (all off when they're all on, otherwise all on), their values kept. Sessions with none are
 * left without.
 * @param ids The sessions in the folder and the folders below it
 * @param overrides Everyone's overrides, by session id
 * @returns null when none of the sessions has overrides
 */
export function folderOverridesSwitch(ids: string[], overrides: Record<string, Override>): { checked: boolean; changes: [string, Override][] } | null {
  const having = ids.filter((id) => overrides[id]);
  if (having.length === 0) return null;
  const checked = having.every((id) => !overrides[id].Disabled);
  return { checked, changes: having.map((id) => [id, { ...overrides[id], Disabled: checked }]) };
}

/** The user's override presets, by name (OverridePresets.json in their own folder) */
export function loadOverridePresets(): Promise<Record<string, Override>> {
  return invoke("override_presets_get");
}

/** Save an override preset under a name, or remove it (null); returns them all */
export function saveOverridePreset(name: string, value: Override | null): Promise<Record<string, Override>> {
  return invoke("override_presets_set", { name, value });
}

/** Everyone's overrides here, by session id */
export function loadOverrides(): Promise<Record<string, Override>> {
  return invoke("overrides_get");
}

/** Set sessions' overrides (an empty one removes them); returns them all */
export function saveOverrides(changes: [string, Override][]): Promise<Record<string, Override>> {
  return invoke("overrides_set", { changes });
}

/** The names of the fields an override sets, for the tree's tooltip */
export function overriddenFields(o: Override | undefined): string[] {
  if (!o) return [];
  const names: [keyof Override, string][] = [
    ["Host", "Host"], ["Username", "Username"], ["PuttyProfile", "Profile"], ["ExtraArgs", "Extra args"], ["JumpHost", "Jump host"], ["Note", "Notes"],
  ];
  const out = names.filter(([k]) => o[k] !== undefined && o[k] !== null).map(([, n]) => n);
  if (o.Port > 0) out.splice(1, 0, "Port");
  return out;
}

/**
 * Where sessions come from: "csv" (a spreadsheet), "mobaxterm", "rdp" (Remote Desktop files), "ssh" (~/.ssh/config),
 * "superputty" (another Sessions.xml), "puttycm" (a PuTTY Connection Manager export), "puttyFolder" (a folder of
 * PuTTY session files), "securecrt" (SecureCRT's Sessions folder), "putty" (PuTTY's and KiTTY's saved sessions) or
 * "rdpRegistry" (Remote Desktop's recent computers, Windows only)
 */
export type ImportKind = "csv" | "mobaxterm" | "rdp" | "ssh" | "superputty" | "puttycm" | "puttyFolder" | "securecrt" | "putty" | "rdpRegistry";

/**
 * A file's name from its path, with Windows' or the Mac's separators
 * @param path A full path
 * @returns The last part of it
 */
export function fileNameOf(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** What an import will do, before anything changes */
export interface ImportSummary {
  adds: number;
  updates: number;
  skips: number;
  /** "Line 4: no name", the first eight */
  skipped: string[];
  /** Columns in the file that aren't used */
  ignored: string[];
  error: string | null;
  /** The first sessions it adds, then changes, "New Private: ImportedFromMobaXterm/sw1" */
  examples: string[];
}

/** What importing would do; `store` is where sessions from another app go (CSV rows say their own) */
export function planImport(kind: ImportKind, paths: string[], store: Store): Promise<ImportSummary> {
  return invoke("sessions_import_plan", { kind, paths, store });
}

/** Import them: returns how many were added and how many updated */
export function applyImport(kind: ImportKind, paths: string[], store: Store): Promise<[number, number]> {
  return invoke("sessions_import_apply", { kind, paths, store });
}

/** ~/.ssh/config */
export function sshConfigPath(): Promise<string> {
  return invoke("ssh_config_path");
}

/**
 * Where an import that asks for no file reads: ssh config, PuTTY's saved sessions, Remote Desktop's recent computers
 * @param kind The import
 * @returns Its source, to show; null for an import that asks for a file
 */
export function importSource(kind: ImportKind): Promise<string | null> {
  return invoke("sessions_import_source", { kind });
}

/**
 * Export every session to a Sessions.xml, as SuperXPuTTY's File > Export sessions: asks where, then says how it went
 * (for the status bar); null when cancelled
 */
export async function exportXml(): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ title: "Export sessions", defaultPath: "Sessions.xml", filters: [{ name: "XML files", extensions: ["xml"] }] });
  if (!path) return null;
  const count = await invoke<number>("sessions_export_xml", { path });
  return `Exported ${count} session${count === 1 ? "" : "s"} to ${fileNameOf(path)}`;
}

/**
 * Open the sessions file in an editor (Notepad++ on Windows when it's installed), as SuperXPuTTY's File > Edit
 * sessions externally; the tree reads it again when it's saved
 * @returns What to say in the status bar
 */
export function editSessionsExternally(): Promise<string> {
  return invoke("sessions_edit_externally");
}

/**
 * Export sessions to a CSV file, as SuperXPuTTY's Export sessions to CSV: asks where, then says how it went (for the
 * status bar); null when cancelled. `items` null exports every session.
 */
export async function exportCsv(items: [Store, string][] | null, fileName: string): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ title: "Export sessions to CSV", defaultPath: fileName.replace(/[/:\\]/g, "_"), filters: [{ name: "CSV files", extensions: ["csv"] }] });
  if (!path) return null;
  const [count, masked] = await invoke<[number, number]>("sessions_export_csv", { items, path });
  const file = fileNameOf(path);
  return `Exported ${count} session${count === 1 ? "" : "s"} to ${file}${masked ? ` (${masked} -pw password${masked === 1 ? "" : "s"} written as ********)` : ""}`;
}

/** One session for File > Edit Sessions as Table…: where it is, all its fields, and what the Database table locks */
export interface TableRow {
  store: Store;
  id: string;
  fields: SessionFields;
  /** Database sessions: the whole row locked in the table */
  locked: boolean;
  /** Database sessions: the columns locked in the table, comma-separated ("host,port") */
  lockedFields: string;
  /** It has a password of its own in the Keychain (Edit Session's Password); the password itself never comes here */
  hasPassword: boolean;
}

/** Every session, Saved, Private and Database, with all its fields, for Edit Sessions as Table… */
export function loadTable(): Promise<TableRow[]> {
  return invoke<TableRow[]>("sessions_table_load");
}

/**
 * Save the rows changed in Edit Sessions as Table… that are in one file, Saved or Private, in one write: all or none.
 * Returns each session's id now, in the order given
 * @param edits each session's id before the change (null for a new session), and its fields now
 */
export function saveTable(store: Exclude<Store, "database">, edits: [string | null, SessionFields][]): Promise<string[]> {
  return invoke<string[]>("sessions_table_save", { store, edits });
}

/**
 * Edit Sessions as Table…'s Export CSV…: rows as they are in the table, saved or not, to a CSV file the user picks,
 * in Export Sessions to CSV's form. What to say, or null when no file was picked
 * @param rows each row's store now, where it was read (its store and id; null for a new row), and its fields now
 * @param fileName the file name offered
 */
export async function exportTableCsv(rows: [Store, [Store, string] | null, SessionFields][], fileName: string): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ title: "Export sessions to CSV", defaultPath: fileName, filters: [{ name: "CSV files", extensions: ["csv"] }] });
  if (!path) return null;
  const [count, masked] = await invoke<[number, number]>("sessions_table_export_csv", { rows, path });
  return `Exported ${count} session${count === 1 ? "" : "s"} to ${fileNameOf(path)}${masked ? ` (${masked} -pw password${masked === 1 ? "" : "s"} written as ********)` : ""}`;
}

/** One CSV row for Edit Sessions as Table…'s Import CSV…, as the core reads it (importing.rs's CsvSession) */
export interface CsvSession {
  line: number;
  /** Saved, Private or Database as written; "" for no Source column or an empty cell */
  source: string;
  folder: string;
  name: string;
  /** The session attributes the file has columns for (Proto, Host, ImageKey...), checked and in the form kept */
  values: [string, string][];
  /** Why the row can't be used */
  problem: string | null;
}

/** A CSV file read for Import CSV…: its rows, its columns that aren't used, or why none can be */
export interface CsvSessions {
  sessions: CsvSession[];
  ignored: string[];
  error: string | null;
}

/** Edit Sessions as Table…'s Import CSV…: a CSV file the user picks, read; null when none was picked */
export async function readTableCsv(): Promise<{ file: string; csv: CsvSessions } | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const path = await open({ title: "Import sessions from CSV", multiple: false, filters: [{ name: "CSV files", extensions: ["csv"] }] });
  if (!path || Array.isArray(path)) return null;
  return { file: fileNameOf(path), csv: await invoke<CsvSessions>("sessions_table_read_csv", { path }) };
}

/**
 * The advice beside setting many sessions' own passwords at once (Edit sessions…, Edit Sessions as Table…): sessions
 * that share a login are better served by their profile's password, one place to change it. Theirs is still offered
 */
export const PROFILE_PASSWORD_ADVICE =
  "For sessions that share a login, their profile's password is usually better (Settings > Profiles > Login): one place " +
  "to change it. A password here is each session's own, used before its profile's.";

/**
 * One password stored as each session's own (Edit Session's Password), in the Keychain or Credential Manager on this
 * computer only, never in the sessions' files or the Database table; "" forgets each one's. For Edit sessions… and
 * Edit Sessions as Table…. Returns how many
 * @param ids the sessions' ids, a Database session's with its @db/
 */
export function setSessionPasswords(ids: string[], password: string): Promise<number> {
  return invoke<number>("session_passwords_set", { ids, password });
}
