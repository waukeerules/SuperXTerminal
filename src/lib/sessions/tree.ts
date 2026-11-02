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
}

/** Build one store's tree from its sessions; `query` keeps only sessions matching it (and the folders holding them) */
export function buildTree(store: Store, title: string, sessions: SessionView[], query: string): FolderNode {
  const root = folder(store, title, "");
  const needle = query.trim().toLowerCase();
  for (const session of sessions) {
    if (session.store !== store || (needle && !matches(session, needle))) continue;
    let node = root;
    let path = "";
    for (const part of session.folder.split("/").filter(Boolean)) {
      path = path ? `${path}/${part}` : part;
      let child = node.folders.find((f) => f.name === part);
      if (!child) {
        child = folder(store, part, path);
        node.folders.push(child);
      }
      node = child;
    }
    node.sessions.push(session);
  }
  finish(root);
  return root;
}

/** A new, empty folder node */
function folder(store: Store, name: string, path: string): FolderNode {
  return { kind: "folder", key: `${store}:${path}`, store, name, path, folders: [], sessions: [], count: 0 };
}

/** Sort a folder's contents and count its sessions, all the way down */
function finish(node: FolderNode): number {
  const byName = (a: { name: string }, b: { name: string }) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base", numeric: true });
  node.folders.sort(byName);
  node.sessions.sort(byName);
  node.count = node.sessions.length + node.folders.reduce((sum, f) => sum + finish(f), 0);
  return node.count;
}

/** Whether a session matches a search: its name, host, username, folder or note, ignoring case */
function matches(session: SessionView, needle: string): boolean {
  return [session.name, session.host, session.username, session.folder, session.note].some((field) =>
    field.toLowerCase().includes(needle),
  );
}
