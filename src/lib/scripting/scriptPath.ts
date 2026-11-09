/**
 * A login script's path as a session or profile keeps it. Sessions and profiles in a shared settings folder or
 * database are opened from the Mac and from Windows, so a script picked with Choose… is kept relative to the settings
 * folder when it's in there ("scripts/cisco-login.spsl"), which each system looks in. A full path ("C:\Scripts\...",
 * "/Users/...") is found by one system only, and Edit Session and Settings > Profiles say so under the field, with a
 * button that copies the script in (ScriptPathNote.svelte).
 */
import { invoke } from "@tauri-apps/api/core";
import { SYSTEM_KIND, THIS_COMPUTER, type System } from "../platform";

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
 * Whether a full path is another system's, which no file on this computer can have: a Windows drive or share
 * (C:\..., \\server\...) on the Mac and Linux, a Unix one (/Users/..., ~/...) on Windows. Linux's paths are Unix
 * ones, its own; it took them for another system's when only the Mac was told from Windows
 * @param spec The login script as typed or chosen
 * @param system Whose paths are this computer's: this system's, unless a test says
 */
export function isOtherSystemsPath(spec: string, system: System = SYSTEM_KIND): boolean {
  const s = spec.trim().replace(/^file:\/\//i, "");
  const windows = /^[a-z]:[\\/]/i.test(s) || s.startsWith("\\\\");
  const unix = s.startsWith("/") || s.startsWith("~");
  return system === "windows" ? unix : windows;
}

/**
 * Whose a full path is, as the warning names it: Windows for a drive or share, the Mac for /Users/..., Linux for
 * /home/..., and "Mac or Linux" for another Unix path (~/..., /opt/...), which could be either
 * @param spec A full path
 */
export function systemOfPath(spec: string): string {
  const s = spec.trim().replace(/^file:\/\//i, "");
  if (/^[a-z]:[\\/]/i.test(s) || s.startsWith("\\\\")) return "Windows";
  if (s.startsWith("/Users/")) return "Mac";
  if (s.startsWith("/home/")) return "Linux";
  return "Mac or Linux";
}

/**
 * The note under a login script field that's a full path, else ""
 * @param spec The login script as typed or chosen
 */
export function fullPathNote(spec: string): string {
  return isFullPath(spec)
    ? `A full path, found on ${THIS_COMPUTER} only. To share it between the Mac and Windows, copy the script to the ` +
        `settings folder's scripts folder.`
    : "";
}
