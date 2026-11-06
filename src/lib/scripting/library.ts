/**
 * The saved scripts (`<SettingsFolder>/scripts`, shared by the team: src-tauri/src/scripts.rs), as the Script Editor
 * and the script panel's Saved scripts tab use them: listing, reading, saving and deleting, and the name rule.
 */
import { invoke } from "@tauri-apps/api/core";

/** A saved script: its name, and its kind ("spsl", "rhai" or "lines": plain lines to send) */
export interface SavedScript {
  name: string;
  kind: string;
}

/** The saved scripts, by name ignoring case */
export function listScripts(): Promise<SavedScript[]> {
  return invoke<SavedScript[]>("scripts_list");
}

/** A saved script's text, with LF line breaks */
export function loadScript(entry: SavedScript): Promise<string> {
  return invoke<string>("script_load", { name: entry.name, kind: entry.kind });
}

/** Save a script, replacing one of the same name (of any kind); the saved scripts afterwards */
export function saveScript(entry: SavedScript, text: string): Promise<SavedScript[]> {
  return invoke<SavedScript[]>("script_save", { name: entry.name, kind: entry.kind, text });
}

/** Delete a saved script; the saved scripts afterwards */
export function deleteScript(entry: SavedScript): Promise<SavedScript[]> {
  return invoke<SavedScript[]>("script_delete", { name: entry.name, kind: entry.kind });
}

/** Why a name can't be a saved script's, or "" when it can: scripts.rs's name_problem, said as it's typed */
export function scriptNameProblem(name: string): string {
  return /^[A-Za-z0-9 ._-]{1,60}$/.test(name.trim()) ? "" : "Use 1 to 60 letters, digits, spaces, dots, dashes or underscores.";
}

/** How the panel and the Script Editor name a kind: SPSL, Rhai, or lines */
export function kindName(kind: string): string {
  return ({ spsl: "SPSL", rhai: "Rhai" } as Record<string, string>)[kind] ?? "lines";
}
