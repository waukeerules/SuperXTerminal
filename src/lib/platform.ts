/**
 * Words that differ between the Mac, Windows and Linux, for what the interface says: the system's name, the computer,
 * where secrets are kept (the keyring crate's store: the Keychain, Windows Credential Manager, or the Secret Service),
 * the file manager, ssh's own keys and settings file, the shell a profile's Fetch command runs in, the modifier key of
 * the app's shortcuts, and the monospaced fonts that come with the system.
 *
 * The system is told from the web view's user agent (WKWebView says Mac, WebView2 Windows, WebKitGTK Linux). Each
 * system's words are [`wordsFor`]'s, so tests can ask for any of the three; the constants below are this system's.
 * Linux shares the Mac's Unix ways (~/.ssh, a shell that reads its rc file) and Windows' Ctrl.
 */
import { isMac } from "./shortcuts";

/** Which system the app is running on */
export type System = "mac" | "windows" | "linux";

/**
 * The system a web view's user agent names. Linux's WebKitGTK says "X11; Linux" (or Wayland's "Linux"); anything
 * else not the Mac is taken as Windows, which is where the app ships besides
 * @param userAgent navigator.userAgent
 */
export function systemOf(userAgent: string): System {
  if (/Mac/.test(userAgent)) return "mac";
  if (/Linux|X11/.test(userAgent) && !/Windows/.test(userAgent)) return "linux";
  return "windows";
}

/** The system the app is running on now */
export const SYSTEM_KIND: System = isMac ? "mac" : systemOf(navigator.userAgent);
/** Whether it's Linux: for what differs from both the Mac and Windows */
export const isLinux = SYSTEM_KIND === "linux";

/** The words that differ by system, as one system has them */
export interface SystemWords {
  /** The system's name: macOS, Windows or Linux */
  system: string;
  /** The computer, as the interface says it: this Mac, this PC, or this computer */
  thisComputer: string;
  /** Where passwords and secrets are kept */
  secretStore: string;
  /** The system's file manager, for Show in Finder (Show in File Explorer, Show in Files) */
  fileManager: string;
  /** The shortcuts' modifier as words: Cmd+V, or Ctrl+V */
  mod: string;
  /** The monospaced fonts that come with the system, offered for the terminal */
  monospaceFonts: string[];
  /** The keys ssh offers by itself */
  sshKeys: string;
  /** ssh's own settings file */
  sshConfig: string;
  /**
   * What a profile's Fetch command runs in (launch.rs's key_fetch): the user's shell, its rc file read (`$SHELL -i`)
   * so a function of theirs works, on the Mac and Linux; PowerShell, its profile read, on Windows
   */
  fetchShell: string;
  /** Where a function of the user's own for the Fetch command goes */
  fetchFunctionHome: string;
}

/**
 * One system's words
 * @param system which system
 */
export function wordsFor(system: System): SystemWords {
  switch (system) {
    case "mac":
      return {
        system: "macOS", thisComputer: "this Mac", secretStore: "the Keychain", fileManager: "Finder", mod: "Cmd",
        monospaceFonts: ["Menlo", "Monaco", "SF Mono", "Courier New", "Andale Mono", "PT Mono"],
        sshKeys: "~/.ssh keys", sshConfig: "~/.ssh/config",
        fetchShell: "your shell (its ~/.zshrc read, so a function works)", fetchFunctionHome: "your ~/.zshrc",
      };
    case "linux":
      return {
        system: "Linux", thisComputer: "this computer", secretStore: "the Secret Service (GNOME Keyring or KWallet)",
        fileManager: "the file manager", mod: "Ctrl",
        // what most distributions install: DejaVu and Liberation nearly everywhere, Ubuntu's own, Noto on Fedora
        monospaceFonts: ["DejaVu Sans Mono", "Liberation Mono", "Ubuntu Mono", "Noto Sans Mono", "Courier New"],
        sshKeys: "~/.ssh keys", sshConfig: "~/.ssh/config",
        fetchShell: "your shell (its ~/.bashrc read, or your own shell's, so a function works)",
        fetchFunctionHome: "your ~/.bashrc (or your own shell's rc file)",
      };
    default:
      return {
        system: "Windows", thisComputer: "this PC", secretStore: "Windows Credential Manager", fileManager: "File Explorer",
        mod: "Ctrl", monospaceFonts: ["Cascadia Mono", "Cascadia Code", "Consolas", "Courier New", "Lucida Console"],
        sshKeys: "the keys in your .ssh folder (C:\\Users\\<you>\\.ssh)", sshConfig: "C:\\Users\\<you>\\.ssh\\config",
        fetchShell: "PowerShell", fetchFunctionHome: "your PowerShell profile",
      };
  }
}

/** This system's words */
const WORDS = wordsFor(SYSTEM_KIND);

/** The system's name: macOS, Windows or Linux */
export const SYSTEM = WORDS.system;
/** The computer, as the interface says it: this Mac, this PC, or this computer */
export const THIS_COMPUTER = WORDS.thisComputer;
/** Where passwords and secrets are kept, as the interface says it */
export const SECRET_STORE = WORDS.secretStore;
/** The system's file manager, for Show in Finder (Show in File Explorer) */
export const FILE_MANAGER = WORDS.fileManager;
/** The shortcuts' modifier as words: Cmd+V, or Ctrl+V */
export const MOD = WORDS.mod;
/**
 * The monospaced fonts that come with the system, offered for the terminal (Settings > Terminal > Font, a profile's
 * Font, the tab's menu); any other installed one can still be named
 */
export const MONOSPACE_FONTS = WORDS.monospaceFonts;
/** The keys ssh offers by itself: ~/.ssh's, which on Windows is the .ssh folder in the user's folder */
export const SSH_KEYS = WORDS.sshKeys;
/** ssh's own settings file: ~/.ssh/config, which on Windows is config in the .ssh folder in the user's folder */
export const SSH_CONFIG = WORDS.sshConfig;
/** What a profile's Fetch command runs in, as Settings > Profiles says it */
export const FETCH_SHELL = WORDS.fetchShell;
