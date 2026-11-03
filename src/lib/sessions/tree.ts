/**
 * Turns the flat session list into the Sessions tree: one root per store, folders from the session ids ("Lab/Core/sw1"
 * is in folder "Lab/Core"), session counts on folders, folders first, then sorted by name, as in SuperXPuTTY.
 */
import { CUSTOM_PREFIX, parseKey } from "../icons/library";
import { templates } from "../icons/templates";
import { logos } from "../icons/brands";
import type { SessionView, Store } from "./api";

/** A folder in the tree (a root is a folder with an empty path) */
export interface FolderNode {
  kind: "folder";
  key: string;
  store: Store;
  name: string;
  path: string;
  folders: FolderNode[];
  sessions: SessionView[];
  /** Sessions in this folder and every folder under it */
  count: number;
  /** Favorites or Recent: a group of shortcuts to sessions from any root, in its own order */
  group?: Group;
}

/** The groups at the top of the tree, as SuperXPuTTY's */
export type Group = "favorites" | "recent";

/**
 * Build one store's tree from its sessions; `query` keeps only sessions matching it (and the folders holding them).
 * `extraFolders` are folders made in the tree that have no sessions yet, which exist only there until one is
 * added, as in SuperXPuTTY.
 */
export function buildTree(
  store: Store,
  title: string,
  sessions: SessionView[],
  query: string,
  extraFolders: string[] = [],
  descending = false,
  options: SearchOptions = PLAIN,
): FolderNode {
  const root = folder(store, title, "");
  const test = searchTest(query, options);
  if (!test) for (const path of extraFolders) place(root, store, path);
  for (const session of sessions) {
    if (session.store !== store || (test && !test(session))) continue;
    place(root, store, session.folder).sessions.push(session);
  }
  finish(root, descending);
  return root;
}

/**
 * Favorites or Recent: the sessions with these ids, in the list's order (ids of sessions that are gone are skipped),
 * those matching the search only; null when none is left to show, as SuperXPuTTY hides an empty group
 */
export function buildGroup(
  group: Group,
  title: string,
  ids: string[],
  sessions: SessionView[],
  query: string,
  options: SearchOptions = PLAIN,
): FolderNode | null {
  const test = searchTest(query, options);
  const byId = new Map<string, SessionView>();
  // ids are unique across the roots; the first root wins if one isn't
  for (const s of sessions) if (!byId.has(s.id)) byId.set(s.id, s);
  const shown = ids.map((id) => byId.get(id)).filter((s): s is SessionView => !!s && (!test || test(s)));
  if (shown.length === 0) return null;
  return { kind: "folder", key: `${group}:`, store: "saved", name: title, path: "", folders: [], sessions: shown, count: shown.length, group };
}

/** Each folder's subfolders by name in lower case, while a tree is built: a lookup instead of a search per session */
const childrenByName = new WeakMap<FolderNode, Map<string, FolderNode>>();

/** The node for a folder path, made (with the folders above it) if it isn't there yet */
function place(root: FolderNode, store: Store, folderPath: string): FolderNode {
  let node = root;
  let path = "";
  for (const part of folderPath.split("/")) {
    if (!part) continue;
    path = path ? `${path}/${part}` : part;
    // "test" and "Test" are one folder, as SuperXPuTTY's tree (the first spelling is shown)
    let byName = childrenByName.get(node);
    if (!byName) {
      byName = new Map(node.folders.map((f) => [f.name.toLowerCase(), f]));
      childrenByName.set(node, byName);
    }
    const key = part.toLowerCase();
    let child = byName.get(key);
    if (!child) {
      child = folder(store, part, path);
      node.folders.push(child);
      byName.set(key, child);
    }
    node = child;
    path = child.path;
  }
  return node;
}

/** Every session in a folder and the folders below it */
export function sessionsIn(node: FolderNode): SessionView[] {
  return [...node.sessions, ...node.folders.flatMap(sessionsIn)];
}

/** Every folder key below a node, for Expand all */
export function folderKeys(node: FolderNode): string[] {
  return [node.key, ...node.folders.flatMap(folderKeys)];
}

/** A new, empty folder node */
function folder(store: Store, name: string, path: string): FolderNode {
  return { kind: "folder", key: `${store}:${path}`, store, name, path, folders: [], sessions: [], count: 0 };
}

/**
 * Names compared as the tree sorts them: ignoring case and accents, numbers by value ("sw2" before "sw10"). One
 * collator, made once: localeCompare with options makes one per call, which was most of building a 10,000-session tree
 */
const NAME_ORDER = new Intl.Collator(undefined, { sensitivity: "base", numeric: true });

/** Sort a folder's contents (folders first; Z to A reverses names only) and count its sessions, all the way down */
function finish(node: FolderNode, descending: boolean): number {
  const byName = (a: { name: string }, b: { name: string }) => NAME_ORDER.compare(a.name, b.name) * (descending ? -1 : 1);
  node.folders.sort(byName);
  node.sessions.sort(byName);
  node.count = node.sessions.length + node.folders.reduce((sum, f) => sum + finish(f, descending), 0);
  return node.count;
}

/** A field the Sessions search can look in (Search in…, as SuperXPuTTY's) */
export type SearchField =
  | "name" | "folder" | "host" | "username" | "port" | "protocol" | "profile" | "extraArgs" | "note" | "script" | "icon";

/** Every field, in Search in…'s order and with its names, as SuperXPuTTY's, then the icon */
export const SEARCH_FIELDS: [SearchField, string][] = [
  ["name", "Session name"], ["folder", "Folder"], ["host", "Host address"], ["username", "Login username"],
  ["port", "TCP port"], ["protocol", "Connection type"], ["profile", "Profile"], ["extraArgs", "Extra arguments"],
  ["note", "Notes"], ["script", "SPSL script file"], ["icon", "Icon"],
];

/** Every field, for All fields */
export const ALL_SEARCH_FIELDS: SearchField[] = SEARCH_FIELDS.map(([f]) => f);

/** The fields the settings name (treeSearchFields), else the defaults; names it doesn't know are left out */
export function searchFieldsOf(saved: string[]): SearchField[] {
  const known = saved.filter((f): f is SearchField => ALL_SEARCH_FIELDS.includes(f as SearchField));
  return known.length ? known : DEFAULT_SEARCH_FIELDS;
}

/**
 * What to save after ticking or unticking a field (or "all"): in Search in…'s order, [] when it comes back to the
 * defaults, and the last field kept, so the search always looks somewhere
 */
export function toggledSearchFields(current: SearchField[], field: SearchField | "all"): SearchField[] {
  const next = field === "all"
    ? ALL_SEARCH_FIELDS
    : ALL_SEARCH_FIELDS.filter((f) => (f === field ? !current.includes(f) : current.includes(f)));
  if (next.length === 0) return current;
  const isDefault = next.length === DEFAULT_SEARCH_FIELDS.length && next.every((f) => DEFAULT_SEARCH_FIELDS.includes(f));
  return isDefault ? [] : next;
}

/** The fields searched unless the user picks others: those it always looked in */
export const DEFAULT_SEARCH_FIELDS: SearchField[] = ["name", "host", "username", "folder", "note"];

/** How the Sessions search matches, as SuperXPuTTY's search mode: case, a regular expression or plain text, and where */
export interface SearchOptions {
  caseSensitive: boolean;
  regex: boolean;
  /** Where to look; the defaults when missing or empty */
  fields?: SearchField[];
}

const PLAIN: SearchOptions = { caseSensitive: false, regex: false };

/** An icon as a search finds it: its key's name ("switch", without the colour) and its title ("Switch") */
function iconText(key: string): string {
  const name = (parseKey(key || "") ?? { name: key }).name.toLowerCase();
  const title = templates[name]?.title ?? logos[name]?.title ?? (name.startsWith(CUSTOM_PREFIX) ? name.slice(CUSTOM_PREFIX.length) : "");
  return title ? `${name} ${title}` : name;
}

/** One field of a session, as the search reads it */
function fieldText(s: SessionView, field: SearchField): string {
  switch (field) {
    case "icon": return iconText(s.icon);
    case "protocol": return s.protocol || "SSH";
    case "port": return s.port ? String(s.port) : "";
    default: return s[field] ?? "";
  }
}

/** Why a search can't be used as a regular expression, or "" when it can (or isn't one) */
export function searchProblem(query: string, options: SearchOptions): string {
  if (!options.regex || !query.trim()) return "";
  try {
    new RegExp(query.trim());
    return "";
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}

/**
 * The test a search makes of each session: its name, host, username, folder or note holding the text (or matching the
 * regular expression), ignoring case unless told not to; null for no search. A regular expression that isn't one
 * matches nothing (searchProblem says why).
 */
export function searchTest(query: string, options: SearchOptions = PLAIN): ((session: SessionView) => boolean) | null {
  const text = query.trim();
  if (!text) return null;
  const fields = options.fields?.length ? options.fields : DEFAULT_SEARCH_FIELDS;
  const searched = (s: SessionView) => fields.map((f) => fieldText(s, f));
  if (options.regex) {
    let pattern: RegExp;
    try {
      pattern = new RegExp(text, options.caseSensitive ? "" : "i");
    } catch {
      return () => false;
    }
    return (s) => searched(s).some((field) => pattern.test(field));
  }
  if (options.caseSensitive) return (s) => searched(s).some((field) => field.includes(text));
  const needle = text.toLowerCase();
  return (s) => searched(s).some((field) => field.toLowerCase().includes(needle));
}

/** Whether a session matches a plain search, ignoring case */
export function matches(session: SessionView, needle: string): boolean {
  return searchTest(needle)?.(session) ?? true;
}

/** The drag data type for sessions and folders dragged within the tree */
export const DRAG_TYPE = "application/x-superxterminal-tree";

/** What's being dragged in the tree */
export type Dragged =
  | { kind: "session"; store: Store; id: string }
  | { kind: "folder"; store: Store; path: string }
  /** A selection of several: its keys ("saved:Lab/sw1", "saved:folder:Lab/Core"), all in one root */
  | { kind: "many"; store: Store; keys: string[] };
