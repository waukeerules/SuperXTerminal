/**
 * Words that differ between the Mac and Windows, for what the interface says: the system's name, the computer, where
 * secrets are kept (the keyring crate's store: the Keychain, or Windows Credential Manager), the file manager, ssh's
 * own keys, and the modifier key of the app's shortcuts.
 */
import { isMac } from "./shortcuts";

/** The system's name: macOS or Windows */
export const SYSTEM = isMac ? "macOS" : "Windows";
/** The computer, as the interface says it: this Mac, or this PC */
export const THIS_COMPUTER = isMac ? "this Mac" : "this PC";
/** Where passwords and secrets are kept, as the interface says it */
export const SECRET_STORE = isMac ? "the Keychain" : "Windows Credential Manager";
/** The system's file manager, for Show in Finder (Show in File Explorer) */
export const FILE_MANAGER = isMac ? "Finder" : "File Explorer";
/** The shortcuts' modifier as words: Cmd+V, or Ctrl+V */
export const MOD = isMac ? "Cmd" : "Ctrl";
/** The keys ssh offers by itself: ~/.ssh's, which on Windows is the .ssh folder in the user's folder */
export const SSH_KEYS = isMac ? "~/.ssh keys" : "the keys in your .ssh folder (C:\\Users\\<you>\\.ssh)";
