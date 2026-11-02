/**
 * Environment colours (production in red, ...), as SuperXPuTTY's EnvironmentColors: a session's own colour, else
 * its nearest folder's, else the first name rule matching its name or one of its folders' names, nearest first.
 * Rules are "pattern=#RRGGBB" separated by ";" or new lines, with * and ? as wildcards, matched against whole
 * names, ignoring case: "Prod=#C62828;Staging=#EF6C00".
 */
import type { FolderSetting, Store } from "../sessions/api";

/** One name rule */
export interface ColorRule {
  pattern: string;
  color: string;
  regex: RegExp;
}

/** A colour as "#RRGGBB", or null if it isn't one */
export function normalizeColor(text: string | null | undefined): string | null {
  const match = /^\s*#?([0-9A-Fa-f]{6})\s*$/.exec(text ?? "");
  return match ? `#${match[1].toUpperCase()}` : null;
}

/** Parse rules in their stored form; malformed parts are skipped, as EnvironmentColors.ParseRules */
export function parseRules(text: string): ColorRule[] {
  const rules: ColorRule[] = [];
  for (const part of text.split(/[;\n]/)) {
    const eq = part.lastIndexOf("=");
    if (eq <= 0) continue;
    const pattern = part.slice(0, eq).trim();
    const color = normalizeColor(part.slice(eq + 1));
    if (!pattern || !color) continue;
    const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".");
    rules.push({ pattern, color, regex: new RegExp(`^${escaped}$`, "i") });
  }
  return rules;
}

/** Looks up colours for sessions and folders from the folders' settings and the name rules */
export class EnvironmentColors {
  private readonly folders = new Map<string, FolderSetting>();

  constructor(folders: [Store, FolderSetting][], private readonly rules: ColorRule[]) {
    // by path ignoring case: "test" and "Test" are one folder
    for (const [store, setting] of folders) this.folders.set(`${store}:${setting.path.toLowerCase()}`, setting);
  }

  /** A session's colour, or null for none */
  forSession(session: { store: Store; folder: string; name: string; color: string }): string | null {
    const own = normalizeColor(session.color);
    if (own) return own;
    for (const path of ancestors(session.folder)) {
      const color = normalizeColor(this.folders.get(`${session.store}:${path.toLowerCase()}`)?.color);
      if (color) return color;
    }
    const names = [session.name, ...ancestors(session.folder).map((p) => p.slice(p.lastIndexOf("/") + 1))];
    return this.firstRule(names);
  }

  /** A folder's colour (its own, else a rule matching its name), or null; for the circle behind folder icons */
  forFolder(store: Store, path: string): string | null {
    const own = normalizeColor(this.folders.get(`${store}:${path.toLowerCase()}`)?.color);
    if (own) return own;
    return path ? this.firstRule([path.slice(path.lastIndexOf("/") + 1)]) : null;
  }

  /** The colour of the first rule matching the first name that any rule matches */
  private firstRule(names: string[]): string | null {
    for (const name of names) {
      const rule = this.rules.find((r) => r.regex.test(name));
      if (rule) return rule.color;
    }
    return null;
  }
}

/** A folder path and each folder above it, nearest first: "a/b/c", "a/b", "a" */
function ancestors(path: string): string[] {
  const list: string[] = [];
  let current = path.replace(/^\/+|\/+$/g, "");
  while (current) {
    list.push(current);
    current = current.includes("/") ? current.slice(0, current.lastIndexOf("/")) : "";
  }
  return list;
}
