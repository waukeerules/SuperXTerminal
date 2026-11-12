/**
 * The highlighting rule sets and themes every terminal tab gets, as SuperXPuTTY's OutputHighlighting.Load: the
 * defaults bundled with the app (`public/web/output`, listed in its `defaults.json`), then the team's
 * `<SettingsFolder>/highlighting`, then the user's own `highlighting` folder (both read by the Rust core). A rule
 * set with the same `id`, or a theme with the same `name`, replaces an earlier one. Read once and shared by tabs.
 *
 * The bundled rule sets can be turned off (Settings > Terminal > Built-in highlighting rules, the `builtInRules`
 * preference), for someone whose own rule sets they get in the way of: Automatic picks the first set whose `detect`
 * matches the output, and the bundled ones come first. Off, they're offered nowhere and never picked. The bundled
 * themes stay either way: they're the terminal's colours, not rules, and it can't draw without one.
 */
import { invoke } from "@tauri-apps/api/core";
import { pref } from "./prefs";

/** A rule set or theme, as its JSON file has it */
type Item = Record<string, unknown>;

/** Rule sets and themes, ready for the page's "config" */
export interface HighlightingFiles {
  /** The rule sets offered and picked from: every one, or with the built-in ones turned off, only the team's and the user's */
  ruleSets: Item[];
  themes: Item[];
  /**
   * The built-in rule sets while they're turned off, each marked `hidden`, and none otherwise. A page gets them after
   * `ruleSets` ({@link pageRuleSets}), so that a rule set of the user's that says `"include": ["generic"]` still
   * gets Generic's rules: that names it on purpose. Hidden sets are never listed or picked (rules.js's compile)
   */
  included: Item[];
}

/** Everything read, before the built-in rule sets are left out or kept */
interface Loaded {
  ruleSets: Item[];
  themes: Item[];
  /** The rule sets that are the app's own: a bundled one the team or the user replaced, by its id, isn't among them */
  builtIn: Set<Item>;
}

/** The read under way or done, shared by every tab; null until the first tab asks */
let loading: Promise<Loaded> | null = null;

/** Whether the built-in rule sets are in use (Settings > Terminal > Built-in highlighting rules; on unless turned off) */
export function builtInRulesOn(): boolean {
  return pref("builtInRules") !== "false";
}

/** The rule sets and themes, read on first use; the built-in rule sets are in or out as the preference is now */
export async function highlighting(): Promise<HighlightingFiles> {
  loading ??= load();
  const all = await loading;
  if (builtInRulesOn()) return { ruleSets: all.ruleSets, themes: all.themes, included: [] };
  return {
    ruleSets: all.ruleSets.filter((set) => !all.builtIn.has(set)),
    themes: all.themes,
    included: all.ruleSets.filter((set) => all.builtIn.has(set)).map((set) => ({ ...set, hidden: true })),
  };
}

/**
 * The rule sets a page is sent: those offered, then the built-in ones turned off, hidden, for the user's own rule
 * sets to include
 */
export function pageRuleSets(files: HighlightingFiles): Item[] {
  return [...files.ruleSets, ...files.included];
}

/** Read the defaults, then the folders, merging by id and name */
async function load(): Promise<Loaded> {
  const defaults = await bundled();
  const folders = await invoke<{ ruleSets: Item[]; themes: Item[]; problems: string[] }>("highlighting_load");
  for (const problem of folders.problems) console.warn("Highlighting file skipped:", problem);
  const ruleSets = merge(merge([], defaults.ruleSets, "id"), folders.ruleSets, "id");
  return {
    ruleSets,
    themes: merge(merge([], defaults.themes, "name"), folders.themes, "name"),
    // the bundled ones still in the list: one replaced from a folder is the folder's now
    builtIn: new Set(defaults.ruleSets.filter((set) => ruleSets.includes(set))),
  };
}

/** The defaults bundled with the app */
async function bundled(): Promise<{ ruleSets: Item[]; themes: Item[] }> {
  const base = "/web/output/";
  try {
    const list = (await (await fetch(base + "defaults.json")).json()) as { rules: string[]; themes: string[] };
    const read = (path: string) => fetch(base + path).then(async (r) => JSON.parse(stripComments(await r.text())) as Item);
    return {
      ruleSets: await Promise.all(list.rules.map((f) => read("rules/" + f))),
      themes: await Promise.all(list.themes.map((f) => read("themes/" + f))),
    };
  } catch (error) {
    console.warn("Default highlighting files couldn't be read:", error);
    return { ruleSets: [], themes: [] };
  }
}

/** Add items to a list, each replacing one with the same key (ignoring case) */
function merge(list: Item[], items: Item[], key: string): Item[] {
  for (const item of items) {
    const name = String(item[key] ?? "").toLowerCase();
    const index = name ? list.findIndex((other) => String(other[key] ?? "").toLowerCase() === name) : -1;
    if (index >= 0) list[index] = item;
    else list.push(item);
  }
  return list;
}

/** JSON with `//` comments made plain JSON; "//" inside strings (regexes, URLs) is kept, as the Rust core does */
function stripComments(text: string): string {
  let out = "";
  let inString = false;
  let escaped = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inString) {
      out += c;
      if (escaped) escaped = false;
      else if (c === "\\") escaped = true;
      else if (c === '"') inString = false;
    } else if (c === '"') {
      inString = true;
      out += c;
    } else if (c === "/" && text[i + 1] === "/") {
      while (i < text.length && text[i] !== "\n") i++;
      out += "\n";
    } else {
      out += c;
    }
  }
  return out;
}
