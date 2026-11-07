/**
 * A login script's path as a session or profile keeps it. Sessions and profiles in a shared settings folder or
 * database are opened from the Mac and from Windows, so a script picked with Choose… is kept relative to the settings
 * folder when it's in there ("scripts/cisco-login.spsl"), which each system looks in. A full path ("C:\Scripts\...",
 * "/Users/...") is found by one system only, and Edit Session and Settings > Profiles say so under the field.
 */
import { invoke } from "@tauri-apps/api/core";
import { THIS_COMPUTER } from "../platform";

/**
 * A script file picked in the file dialog, as it's kept: relative to the settings folder when it's in there
 * @param path The full path the dialog gave
 */
export function chosenScript(path: string): Promise<string> {
  return invoke<string>("script_chosen", { path });
}

/**
 * Whether a login script is a full path, which only the system it was picked on finds: a Windows drive or share
 * (C:\..., \\server\...) or a Unix one (/Users/..., ~/...). An http(s) address and a relative path are found by both
 * @param spec The login script as typed or chosen
 */
export function isFullPath(spec: string): boolean {
  const s = spec.trim().replace(/^file:\/\//i, "");
  return /^[a-z]:[\\/]/i.test(s) || s.startsWith("\\\\") || s.startsWith("/") || s.startsWith("~");
}

/**
 * The note under a login script field that's a full path, else ""
 * @param spec The login script as typed or chosen
 */
export function fullPathNote(spec: string): string {
  return isFullPath(spec)
    ? `A full path, found on ${THIS_COMPUTER} only. To share it between the Mac and Windows, put the script in the ` +
        `settings folder's scripts folder and choose it from there.`
    : "";
}
