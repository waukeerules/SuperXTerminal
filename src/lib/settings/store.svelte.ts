/**
 * The app's settings (`src-tauri/src/settings.rs`), shared by every part of the UI. Reading `settings.x` in a
 * component or $effect re-runs it when the setting changes, so the tree, tabs and theme follow the Settings window
 * at once.
 */
import { invoke } from "@tauri-apps/api/core";
import { applyUiTheme, loadUiThemes, themeColours, uiThemeNamed } from "./uiThemes.svelte";
import { ask, open } from "@tauri-apps/plugin-dialog";
import type { Dock } from "../layout/dock";

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
  /** The Sessions pane's width in points, as dragged at its edge, while it's docked at the left or right */
  sessionsPaneWidth: number;
  /** Where the Sessions pane is docked: its title dragged to the window's left, right or bottom (layout/dock.ts) */
  sessionsDock: Dock;
  /** The Sessions pane's height in points, as dragged at its edge, while it's docked at the bottom */
  sessionsPaneHeight: number;
  /** The Sessions pane is auto-hidden: a tab at the edge where it's docked slides it out */
  sessionsAutoHide: boolean;
  /** The layout opened at start ("" for none, "<Auto Restore>" for the last), as SuperXPuTTY's DefaultLayoutName */
  defaultLayoutName: string;
  /** View > Session Detail: the pane under the Sessions tree */
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
  /** View > Grid View: the tabs tiled in a grid, one split each, as tabs open and close (TabArea.setGridView) */
  gridView: boolean;
  /**
   * Settings > General > Tabs > Grid View off: "restore" puts back the splits from before it went on, "join" joins
   * every tab into one group
   */
  gridViewOff: "restore" | "join";
  /**
   * Settings > General > Tabs > Remote Desktop in Grid View: "window" opens an RDP session in a window of its own while
   * Grid View is on, "tile" tiles it with the others
   */
  gridViewRdp: "window" | "tile";
  /** RDP sessions open in a tab with the built-in client; off, in Windows App */
  rdpBuiltIn: boolean;
  /** Settings > Remote Desktop: the defaults for RDP sessions, as SuperXPuTTY's RdpOptions */
  rdpDefaults: string;
  /** FileZilla (external) gets the session's password on its address (off: it shows in the process list) */
  filezillaPasswords: boolean;
  /**
   * 1Password references (op://...) in passwords, Environment Variables and key files are read from 1Password, and
   * Settings and Edit Session offer them (Settings > General > 1Password). Off by default, as most people don't use
   * 1Password: off, none is read and its hints and examples aren't shown
   */
  onePassword: boolean;
  /** Tabs' close button: on each tab, or one beside the tab list's ▾ that closes the active tab */
  tabClose: "tab" | "right";
  /** The tab list's ▾ opens Switch Session (its tabs, with a filter) rather than a plain list */
  tabListSwitchSession: boolean;
  /** The longest a tab's name is shown, in characters, before it's cut short with …; 0 shows the whole name */
  tabNameMax: number;
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
  /**
   * Settings > General > Command bar: how many days Send Commands' history is kept between runs, as SuperPuTTY's
   * "Save history for N days"; 0, the default, keeps it only until the app closes
   */
  commandHistoryDays: number;
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
  sessionsDock: "left",
  sessionsPaneHeight: 260,
  sessionsAutoHide: false,
  defaultLayoutName: "",
  showSessionDetail: false,
  shortcuts: {},
  showInMenuBar: false,
  showLogViewer: false,
  autoUpdateCheck: true,
  updateRepository: "waukeerules/SuperXTerminal",
  showSessionsPane: true,
  showStatusBar: true,
  alwaysOnTop: false,
  gridView: false,
  gridViewOff: "restore",
  gridViewRdp: "window",
  rdpBuiltIn: true,
  rdpDefaults: "",
  filezillaPasswords: false,
  onePassword: false,
  tabClose: "tab",
  tabListSwitchSession: false,
  tabNameMax: 30,
  treeLines: true,
  searchWidensPane: true,
  treeSearchCase: false,
  treeSearchRegex: false,
  treeSearchFields: [],
  scriptLanguage: "rhai",
  commandHistoryDays: 0,
});

/** Database sessions' "Check every", in seconds; 0 when they're off or never checked */
export function databaseCheckSeconds(): number {
  return settings.database?.enabled ? Number(settings.database.checkSeconds) || 0 : 0;
}

/** Whether macOS is in dark mode, kept up to date */
const systemDark = $state({ value: matchMedia("(prefers-color-scheme: dark)").matches });
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (e) => (systemDark.value = e.matches));

/** Whether the app is drawn dark: the theme chosen (a custom one's base), or macOS's when it's "system" */
export function isDark(): boolean {
  const custom = uiThemeNamed(settings.theme);
  if (custom) return custom.base === "dark";
  return settings.theme === "dark" || (settings.theme !== "light" && systemDark.value);
}

/**
 * The colours of the theme in use when it's one of the user's own, for the pages in frames of their own that app.css
 * can't reach (the script box's Monaco): the window's, each one it leaves alone its base's (bgWindow, accent... as
 * uiThemes.svelte.ts names them), and the script colours it sets (scriptComment...), the rest left to Monaco's own.
 * Null for Follow the system, Light and Dark, which those pages have themes of their own for
 */
export function ownThemeColours(): Record<string, string> | null {
  const custom = uiThemeNamed(settings.theme);
  return custom ? themeColours(custom) : null;
}

/** Read the settings from the core */
export async function loadSettings(): Promise<void> {
  Object.assign(settings, await invoke<AppSettings>("settings_get"), { loaded: true });
  // the window's own themes too, for a theme chosen that's one of them
  await loadUiThemes();
  applyTheme();
}

/** Save changed settings (not the settings folder: see `chooseSettingsFolder`) */
export async function saveSettings(changes: Partial<AppSettings>): Promise<void> {
  const { loaded: _, ...current } = settings;
  Object.assign(settings, await invoke<AppSettings>("settings_save", { settings: { ...current, ...changes } }));
  applyTheme();
}

/** What's said before using a settings folder this account can't make files in */
export const READ_ONLY_FOLDER =
  "You can't save to this folder: changes to sessions, button bars and layouts won't be kept.\n\n" +
  "Use it anyway? A folder kept read-only on purpose, with the team's settings set up by someone else, is fine.";

/**
 * Ask for the settings folder (where Sessions.xml is) and remember it; false if the user cancelled. A folder this
 * account can't write to (Program Files, a read-only share) is used only once the user says so: everything changed in
 * the app would fail to save there, though a team's folder may be read-only on purpose.
 */
export async function chooseSettingsFolder(): Promise<boolean> {
  const path = await open({ directory: true, title: "Choose the settings folder (the folder holding Sessions.xml)" });
  if (typeof path !== "string") return false;
  if (!(await invoke<boolean>("folder_writable", { path }))) {
    const use = await ask(READ_ONLY_FOLDER, { title: "Settings Folder", kind: "warning", okLabel: "Use It", cancelLabel: "Cancel" });
    if (!use) return false;
  }
  await invoke("settings_set_folder", { path });
  settings.settingsFolder = path;
  return true;
}

/**
 * Mark the page with the chosen theme, which app.css's colours follow ("system" leaves it to macOS); a custom one
 * (uiThemes.svelte.ts) marks its base and sets its own colours over it. Also run when Edit Themes… saves
 */
export function applyTheme(): void {
  const custom = uiThemeNamed(settings.theme);
  const base = custom?.base ?? settings.theme;
  if (base === "light" || base === "dark") document.documentElement.dataset.theme = base;
  else delete document.documentElement.dataset.theme;
  applyUiTheme(custom);
}
