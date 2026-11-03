/**
 * Words that differ between the Mac and Windows, for what the interface says: the system's name, the computer, where
 * secrets are kept (the keyring crate's store: the Keychain, or Windows Credential Manager), the file manager, and the
 * modifier key of the app's shortcuts.
 */
import { isMac } from "./shortcuts";

export const SYSTEM = isMac ? "macOS" : "Windows";
export const THIS_COMPUTER = isMac ? "this Mac" : "this PC";
export const SECRET_STORE = isMac ? "the Keychain" : "Windows Credential Manager";
export const FILE_MANAGER = isMac ? "Finder" : "File Explorer";
/** The shortcuts' modifier as words: Cmd+V, or Ctrl+V */
export const MOD = isMac ? "Cmd" : "Ctrl";
