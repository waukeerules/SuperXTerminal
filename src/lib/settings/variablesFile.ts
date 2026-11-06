/**
 * Environment Variables' Export… and Import…: the variables as SuperXPuTTY's `EnvironmentVariables.json`
 * (`{"Variables":[{"Key","Value","Secret"}]}`), the file the core keeps them in (`src-tauri/src/variables.rs`), so a
 * team can pass a set around, and a file from SuperXPuTTY on Windows imports as it is.
 *
 * Secrets go by name only: their values stay in this computer's Keychain or Credential Manager, and the window never
 * has a stored one to give. An imported secret with no value keeps the one stored here, or waits to be typed.
 */

/** One row of the Environment Variables window, as `variables_list` gives it */
export interface Variable {
  key: string;
  value: string;
  secret: boolean;
  /** A secret with a value stored here */
  hasValue: boolean;
  /** A secret set on Windows (DPAPI) that has to be typed again here */
  fromWindows: boolean;
}

/** What Import… made of a file: the window's rows with it merged in */
export interface Imported {
  rows: Variable[];
  /** How many variables the file had */
  count: number;
  /** The secrets that still need their value typed, by key */
  toType: string[];
}

/**
 * The window's variables as an export file: every one with a name, a secret without its value (even one just typed)
 * @returns The file's text
 */
export function exportVariables(rows: Variable[]): string {
  const Variables = rows
    .filter((r) => r.key.trim())
    .map((r) => (r.secret ? { Key: r.key.trim(), Secret: true } : { Key: r.key.trim(), Value: r.value, Secret: false }));
  return JSON.stringify({ Variables }, null, 2);
}

/**
 * Merge an export file into the window's rows. A name already here (whatever its case) takes the file's value and
 * keeps its own spelling; a new one is added at the end. A secret the file has no value for keeps the one stored or
 * typed here, or waits to be typed; Windows' encrypted value (ProtectedValue) can't be read here, so it waits too.
 * Nothing is removed.
 * @param text The file's text
 * @param rows The window's rows now
 * @throws When the text isn't an Environment Variables file
 */
export function importVariables(text: string, rows: Variable[]): Imported {
  let entries: unknown;
  try {
    entries = (JSON.parse(text.replace(/^﻿/, "")) as { Variables?: unknown }).Variables;
  } catch {
    entries = undefined;
  }
  if (!Array.isArray(entries)) throw new Error("This file isn't an Environment Variables file: it has no \"Variables\" list.");
  const merged = rows.map((r) => ({ ...r }));
  let count = 0;
  for (const entry of entries as Record<string, unknown>[]) {
    const key = typeof entry?.Key === "string" ? entry.Key.trim() : "";
    if (!key) continue;
    count++;
    const secret = entry.Secret === true;
    const value = typeof entry.Value === "string" ? entry.Value : "";
    const here = merged.find((r) => r.key.trim().toLowerCase() === key.toLowerCase());
    if (!here) {
      merged.push({ key, value, secret, hasValue: false, fromWindows: false });
    } else if (secret) {
      // the value here stays unless the file brings one
      if (!here.secret) Object.assign(here, { value: "", hasValue: false });
      here.secret = true;
      if (value) here.value = value;
    } else {
      Object.assign(here, { value, secret: false, hasValue: false, fromWindows: false });
    }
  }
  const toType = merged.filter((r) => r.secret && !r.value && !r.hasValue).map((r) => r.key.trim());
  return { rows: merged, count, toType };
}
