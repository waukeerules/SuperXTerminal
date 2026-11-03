/**
 * Turns the flat session list into the Sessions tree: one root per store, folders from the session ids ("Lab/Core/sw1"
 * is in folder "Lab/Core"), session counts on folders, folders first, then sorted by name, as in SuperXPuTTY.
 */
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

/** The node for a folder path, made (with the folders above it) if it isn't there yet */
function place(root: FolderNode, store: Store, folderPath: string): FolderNode {
  let node = root;
  let path = "";
  for (const part of folderPath.split("/").filter(Boolean)) {
    path = path ? `${path}/${part}` : part;
    // "test" and "Test" are one folder, as SuperXPuTTY's tree (the first spelling is shown)
    let child = node.folders.find((f) => f.name.toLowerCase() === part.toLowerCase());
    if (!child) {
      child = folder(store, part, path);
      node.folders.push(child);
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

/** Sort a folder's contents (folders first; Z to A reverses names only) and count its sessions, all the way down */
function finish(node: FolderNode, descending: boolean): number {
  const byName = (a: { name: string }, b: { name: string }) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base", numeric: true }) * (descending ? -1 : 1);
  node.folders.sort(byName);
  node.sessions.sort(byName);
  node.count = node.sessions.length + node.folders.reduce((sum, f) => sum + finish(f, descending), 0);
  return node.count;
}

/** How the Sessions search matches, as SuperXPuTTY's search mode: case, and a regular expression or plain text */
export interface SearchOptions {
  caseSensitive: boolean;
  regex: boolean;
}

const PLAIN: SearchOptions = { caseSensitive: false, regex: false };

/** The fields a search looks in */
const searched = (s: SessionView) => [s.name, s.host, s.username, s.folder, s.note];

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
export type Dragged = { kind: "session"; store: Store; id: string } | { kind: "folder"; store: Store; path: string };
