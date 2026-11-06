/**
 * The app's settings (`src-tauri/src/settings.rs`), shared by every part of the UI. Reading `settings.x` in a
 * component or $effect re-runs it when the setting changes, so the tree, tabs and theme follow the Settings window
 * at once.
 */
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

/** The settings, as the Rust core keeps them */
export interface AppSettings {
  settingsFolder: string | null;
  iconSize: number;
  environmentColorRules: string;
  /** "system" (follow macOS), "light" or "dark" */
  theme: string;
  treeFontSize: number;
  /** Database sessions' connection, as the core keeps it (the password is in the Keychain) */
  database?: { checkSeconds: number; enabled: boolean };
  /** The button bar chosen by hand ("" for the first) */
  activeButtonBar: string;
  /** View > Button Bar and View > Send Commands */
  showButtonBar: boolean;
  showSendCommands: boolean;
  /** A button bar button asks before it runs (Settings > General > Button bar; on by default) */
  confirmButtons: boolean;
  /** Send Commands goes to every visible tab rather than the ticked ones */
  sendToVisibleOnly: boolean;
  /** The Script Editor stays above the tabs */
  scriptEditorOnTop: boolean;
  /** A dropped session reconnects by itself: how many times, how many seconds apart */
  autoReconnect: boolean;
  autoReconnectAttempts: number;
  autoReconnectDelaySeconds: number;
  /** SSH sessions run ssh -v, its messages in the tab, for working out why a connection fails */
  sshVerbose: boolean;
  /** Tools > Use Local Overrides: off, every session opens as stored, the overrides kept for later */
  useLocalOverrides: boolean;
  /** The terminal's preferences, by name; only those changed from their defaults */
  terminalPrefs: Record<string, string>;
  /** Keep a log of every session (in sessionLogFolder, "" for Documents/SuperXTerminal/logs); otherwise temporary */
  sessionLogEnabled: boolean;
  sessionLogFolder: string;
  /** Saved sessions (Sessions.xml and Folders.xml in the settings folder) are used, as SuperXPuTTY's UseSessionsXml */
  useSessionsXml: boolean;
  /** Private sessions (PrivateSessions.xml, only yours) are used; one of the two stays on */
  usePrivateSessions: boolean;
  /** The Sessions pane's width in points, as dragged at its edge */
  sessionsPaneWidth: number;
  /** The Sessions pane is auto-hidden: a tab at the left edge slides it out */
  sessionsAutoHide: boolean;
  /** The layout opened at start ("" for none, "<Auto Restore>" for the last), as SuperXPuTTY's DefaultLayoutName */
  defaultLayoutName: string;
  /** View > Layouts and View > Session Detail */
  showLayouts: boolean;
  showSessionDetail: boolean;
  /** Settings > Shortcuts: the actions' keys changed from the defaults ("" for none) */
  shortcuts: Record<string, string>;
  /** The menu bar icon (SuperXPuTTY's Minimize to tray), and SuperXTerminal kept running there with no window */
  showInMenuBar: boolean;
  /** View > Log Viewer */
  showLogViewer: boolean;
  /** Look for a newer release at start; the GitHub repository ("owner/name") looked at */
  autoUpdateCheck: boolean;
  updateRepository: string;
  /** View > Sessions, Status Bar and Always on Top */
  showSessionsPane: boolean;
  showStatusBar: boolean;
  alwaysOnTop: boolean;
  /** RDP sessions open in a tab with the built-in client; off, in Windows App */
  rdpBuiltIn: boolean;
  /** Settings > Remote Desktop: the defaults for RDP sessions, as SuperXPuTTY's RdpOptions */
  rdpDefaults: string;
  /** FileZilla (external) gets the session's password on its address (off: it shows in the process list) */
  filezillaPasswords: boolean;
  /** Tabs' close button: on each tab, or one beside the tab list's ▾ that closes the active tab */
  tabClose: "tab" | "right";
  /** The Sessions tree's dotted lines, as SuperXPuTTY's */
  treeLines: boolean;
  /** While searching, the Sessions pane widens (up to a limit) so the matches' names fit */
  searchWidensPane: boolean;
  /** The Sessions search: match case, and a regular expression (its Aa and .* buttons) */
  treeSearchCase: boolean;
  treeSearchRegex: boolean;
  /** Where the Sessions search looks (Search in…); empty for name, host, username, folder and note */
  treeSearchFields: string[];
  /**
   * What a new script is (Settings > General > Scripts): the Script Editor's Run as when it opens, and a new button's
   * Action. "rhai" by default; "spsl", or "lines" for lines to send (a button's Send string)
   */
  scriptLanguage: ScriptLanguage;
}

/** A script's kind, as the Script Editor's Run as names it: Rhai, SPSL, or lines sent as they are */
export type ScriptLanguage = "rhai" | "spsl" | "lines";

/** What a new script is, from Settings > General > Scripts; Rhai for a value this version doesn't know */
export function newScriptLanguage(): ScriptLanguage {
  const chosen = settings.scriptLanguage;
  return chosen === "spsl" || chosen === "lines" ? chosen : "rhai";
}

/** The settings now, for every component; `loaded` once they've been read at the start */
export const settings = $state<AppSettings & { loaded: boolean }>({
  loaded: false,
  settingsFolder: null,
  iconSize: 24,
  environmentColorRules: "",
  theme: "system",
  treeFontSize: 14,
  database: { checkSeconds: 30, enabled: false },
  activeButtonBar: "",
  showButtonBar: true,
  confirmButtons: true,
  showSendCommands: true,
  sendToVisibleOnly: false,
  scriptEditorOnTop: false,
  autoReconnect: true,
  autoReconnectAttempts: 6,
  autoReconnectDelaySeconds: 10,
  sshVerbose: false,
  useLocalOverrides: true,
  terminalPrefs: {},
  sessionLogEnabled: false,
  sessionLogFolder: "",
  useSessionsXml: true,
  usePrivateSessions: true,
  sessionsPaneWidth: 280,
  sessionsAutoHide: false,
  defaultLayoutName: "",
  showLayouts: false,
  showSessionDetail: false,
  shortcuts: {},
  showInMenuBar: false,
  showLogViewer: false,
  autoUpdateCheck: true,
  updateRepository: "waukeerules/SuperXTerminal",
  showSessionsPane: true,
  showStatusBar: true,
  alwaysOnTop: false,
  rdpBuiltIn: true,
  rdpDefaults: "",
  filezillaPasswords: false,
  tabClose: "tab",
  treeLines: true,
  searchWidensPane: true,
  treeSearchCase: false,
  treeSearchRegex: false,
  treeSearchFields: [],
  scriptLanguage: "rhai",
});

/** Database sessions' "Check every", in seconds; 0 when they're off or never checked */
export function databaseCheckSeconds(): number {
  return settings.database?.enabled ? Number(settings.database.checkSeconds) || 0 : 0;
}

/** Whether macOS is in dark mode, kept up to date */
const systemDark = $state({ value: matchMedia("(prefers-color-scheme: dark)").matches });
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => (systemDark.value = e.matches));

/** Whether the app is drawn dark: the theme chosen, or macOS's when it's "system" */
export function isDark(): boolean {
  return settings.theme === "dark" || (settings.theme !== "light" && systemDark.value);
}

/** Read the settings from the core */
export async function loadSettings(): Promise<void> {
  Object.assign(settings, await invoke<AppSettings>("settings_get"), { loaded: true });
  applyTheme();
}

/** Save changed settings (not the settings folder: see `chooseSettingsFolder`) */
export async function saveSettings(changes: Partial<AppSettings>): Promise<void> {
  const { loaded: _, ...current } = settings;
  Object.assign(settings, await invoke<AppSettings>("settings_save", { settings: { ...current, ...changes } }));
  applyTheme();
}

/** Ask for the settings folder (where Sessions.xml is) and remember it; false if the user cancelled */
export async function chooseSettingsFolder(): Promise<boolean> {
  const path = await open({ directory: true, title: "Choose the settings folder (the folder holding Sessions.xml)" });
  if (typeof path !== "string") return false;
  await invoke("settings_set_folder", { path });
  settings.settingsFolder = path;
  return true;
}

/** Mark the page with the chosen theme, which app.css's colours follow ("system" leaves it to macOS) */
function applyTheme(): void {
  if (settings.theme === "light" || settings.theme === "dark") document.documentElement.dataset.theme = settings.theme;
  else delete document.documentElement.dataset.theme;
}
