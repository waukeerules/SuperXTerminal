/**
 * The highlighting rule sets and themes every terminal tab gets, as SuperXPuTTY's OutputHighlighting.Load: the
 * defaults bundled with the app (`public/web/output`, listed in its `defaults.json`), then the team's
 * `<SettingsFolder>/highlighting`, then the user's own `highlighting` folder (both read by the Rust core). A rule
 * set with the same `id`, or a theme with the same `name`, replaces an earlier one. Read once and shared by tabs.
 */
import { invoke } from "@tauri-apps/api/core";

type Item = Record<string, unknown>;

/** Rule sets and themes, ready for the page's "config" */
export interface HighlightingFiles {
  ruleSets: Item[];
  themes: Item[];
}

let loading: Promise<HighlightingFiles> | null = null;

/** The rule sets and themes, read on first use */
export function highlighting(): Promise<HighlightingFiles> {
  loading ??= load();
  return loading;
}

/** Read the defaults, then the folders, merging by id and name */
async function load(): Promise<HighlightingFiles> {
  const defaults = await bundled();
  const folders = await invoke<{ ruleSets: Item[]; themes: Item[]; problems: string[] }>("highlighting_load");
  for (const problem of folders.problems) console.warn("Highlighting file skipped:", problem);
  return {
    ruleSets: merge(merge([], defaults.ruleSets, "id"), folders.ruleSets, "id"),
    themes: merge(merge([], defaults.themes, "name"), folders.themes, "name"),
  };
}

/** The defaults bundled with the app */
async function bundled(): Promise<HighlightingFiles> {
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
