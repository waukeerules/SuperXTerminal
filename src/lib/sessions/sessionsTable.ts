/**
 * File > Edit Sessions as Table…'s workings, apart from its window (SessionsTable.svelte): the columns, each row's
 * draft (its store and fields as edited), new sessions added as rows, the per-column filters, which cells changed,
 * what stops a save, and the save itself.
 *
 * Saving goes in three steps, as each store saves differently:
 *   1. Saved and Private rows whose fields changed, and new ones: one write per file (sessions_table_save), all of a
 *      file's or none.
 *   2. Database rows whose fields changed, and new ones: one by one (db_session_save), each checked against the
 *      table's version and locks, and queued as pending while the table can't be reached, as Edit Session's are. One
 *      refused doesn't stop the others.
 *   3. Rows whose Store changed: moved there afterwards, as the tree's Move to does (sessions_transfer), once the
 *      sessions have been read again so the move finds them under their new ids.
 */
import {
  loadSessions, saveSession, saveTable, transferSessions, type SessionFields, type Store, type TableRow,
} from "./api";

/** A column's field: one of SessionFields', or the session's store */
export type ColumnKey = "store" | "folder" | "name" | "icon" | "color" | "protocol" | "host" | "port" | "username" | "profile"
  | "note" | "extraArgs" | "jumpHost" | "script" | "highlightRules";

/** One column: its field, heading, width in pixels, and the Database table's column a lock names it by */
export interface Column {
  key: ColumnKey;
  label: string;
  width: number;
  /** db.rs's column_of name, for DatabaseLockedFields; none for Store, which a lock on the row covers */
  dbColumn?: string;
}

/** The table's columns, in order: where it is, how it looks, how it connects, then the rest */
export const COLUMNS: Column[] = [
  { key: "store", label: "Store", width: 96 },
  { key: "folder", label: "Folder", width: 170, dbColumn: "folder" },
  { key: "name", label: "Name", width: 160, dbColumn: "name" },
  { key: "icon", label: "Icon", width: 52, dbColumn: "icon" },
  { key: "color", label: "Colour", width: 58, dbColumn: "color" },
  { key: "protocol", label: "Protocol", width: 96, dbColumn: "protocol" },
  { key: "host", label: "Host", width: 160, dbColumn: "host" },
  { key: "port", label: "Port", width: 66, dbColumn: "port" },
  { key: "username", label: "Username", width: 120, dbColumn: "username" },
  { key: "profile", label: "Profile", width: 140, dbColumn: "putty_profile" },
  { key: "note", label: "Note", width: 180, dbColumn: "notes" },
  { key: "extraArgs", label: "Extra args", width: 160, dbColumn: "extra_args" },
  { key: "jumpHost", label: "Jump host", width: 150, dbColumn: "jump_host" },
  { key: "script", label: "Script", width: 160, dbColumn: "spsl_script" },
  { key: "highlightRules", label: "Highlight rules", width: 140, dbColumn: "highlight_rules" },
];

/** What each store is called in the table */
export const STORE_LABELS: Record<Store, string> = { saved: "Saved", private: "Private", database: "Database" };

/**
 * One row as it's being edited: the session as read (for a new one, its store and empty fields, with no id), its
 * store and fields now, and a key that tells rows apart while new ones have no id yet
 */
export interface Draft {
  row: TableRow;
  store: Store;
  fields: SessionFields;
  key: string;
}

/** A row's draft, its fields copied so editing them leaves the row as read */
export function draftOf(row: TableRow): Draft {
  return { row, store: row.store, fields: { ...row.fields }, key: `${row.store}:${row.id}` };
}

/** How many new rows have been added, for their keys */
let added = 0;

/**
 * A new session's row (New Session), in a store, with a new session's defaults as the core gives them
 * (StoredSession::new): SSH on port 22, Default Settings, the computer icon. It has no id until it's saved
 */
export function newDraft(store: Store): Draft {
  const fields: SessionFields = {
    name: "", folder: "", protocol: "SSH", host: "", port: 22, username: "", profile: "Default Settings", color: "",
    icon: "computer", highlightRules: "", extraArgs: "", jumpHost: "", script: "", note: "", localPath: "", remotePath: "",
    rdpOptions: "", sshAlgorithms: "", buttonBar: "",
  };
  return { row: { store, id: "", fields: { ...fields }, locked: false, lockedFields: "" }, store, fields, key: `new:${++added}` };
}

/** Whether a row is a new session, not saved yet */
export const isNew = (draft: Draft): boolean => draft.row.id === "";

/** A cell's text, as the filters match it: the store's name, the port as a number, the rest as written */
export function cellText(draft: Draft, key: ColumnKey): string {
  if (key === "store") return STORE_LABELS[draft.store];
  return String(draft.fields[key] ?? "");
}

/**
 * Whether a row shows under the filters: each filter's text in its column's cell, ignoring case (Store's is a store,
 * or "" for all of them); empty filters match everything. A new row always shows, so it can be filled in whatever the
 * filters are
 */
export function matches(draft: Draft, filters: Partial<Record<ColumnKey, string>>): boolean {
  if (isNew(draft)) return true;
  return COLUMNS.every(({ key }) => {
    const filter = (filters[key] ?? "").trim().toLowerCase();
    if (!filter) return true;
    if (key === "store") return draft.store === filter;
    return cellText(draft, key).toLowerCase().includes(filter);
  });
}

/**
 * Whether a cell differs from the session as read, as text and trimmed: the save trims too, and a port typed is text
 * where the one read is a number
 */
export function changed(draft: Draft, key: ColumnKey): boolean {
  // a new row is marked as a whole, not cell by cell
  if (isNew(draft)) return false;
  if (key === "store") return draft.store !== draft.row.store;
  return String(draft.fields[key] ?? "").trim() !== String(draft.row.fields[key] ?? "").trim();
}

/** Whether any field changed (not counting the store); a new row's are all to save */
export function fieldsChanged(draft: Draft): boolean {
  return isNew(draft) || COLUMNS.some(({ key }) => key !== "store" && changed(draft, key));
}

/** Whether anything in the row changed */
export function rowChanged(draft: Draft): boolean {
  return draft.store !== draft.row.store || fieldsChanged(draft);
}

/**
 * Whether a cell can't be changed: a Database row the user can't write (Read only, or no rights), a row locked in the
 * table, or a column locked there
 * @param writable Whether Database sessions can be changed at all (DbState.writable)
 */
export function locked(draft: Draft, column: Column, writable: boolean): boolean {
  if (draft.row.store !== "database") return false;
  if (!writable || draft.row.locked) return true;
  const fields = draft.row.lockedFields.split(",").map((f) => f.trim());
  return !!column.dbColumn && fields.includes(column.dbColumn);
}

/** The session's id after the save: its folder and name, with Database's @db/ */
function idOf(store: Store, fields: SessionFields): string {
  const folder = fields.folder.trim().replace(/^\/+|\/+$/g, "");
  const name = fields.name.trim();
  const id = folder ? `${folder}/${name}` : name;
  return store === "database" ? `@db/${id}` : id;
}

/**
 * What stops each changed row from saving, by row: no name, a "/" in it, a port that isn't one, or a folder and name
 * another session has. Saved and Private sessions share their ids, as in SuperXPuTTY; Database's are its own.
 * Rows not changed are left alone, as they're not saved
 */
export function problems(drafts: Draft[]): Map<Draft, string> {
  const found = new Map<Draft, string>();
  const space = (store: Store) => (store === "database" ? "database" : "files");
  const owners = new Map<string, Draft[]>();
  for (const draft of drafts) {
    const key = `${space(draft.store)}:${idOf(draft.store, draft.fields).toLowerCase()}`;
    owners.set(key, [...(owners.get(key) ?? []), draft]);
  }
  for (const draft of drafts) {
    if (!rowChanged(draft)) continue;
    const name = draft.fields.name.trim();
    const port = Number(draft.fields.port);
    if (!name) found.set(draft, "A session needs a name.");
    else if (name.includes("/")) found.set(draft, "A name can't contain \"/\": that separates folders.");
    else if (!Number.isInteger(port) || port < 0 || port > 65535) found.set(draft, "The port is a number from 0 to 65535.");
    else {
      const key = `${space(draft.store)}:${idOf(draft.store, draft.fields).toLowerCase()}`;
      if ((owners.get(key)?.length ?? 0) > 1) found.set(draft, `Another session is called ${idOf(draft.store, draft.fields)}.`);
    }
  }
  return found;
}

/** What the save did: how many sessions it saved or moved, and what it couldn't, one line each */
export interface SaveReport {
  saved: number;
  problems: string[];
}

/**
 * Save every changed row, in the three steps the file's header says. A file whose rows can't all be saved saves none
 * of them, and its moves wait; a Database row refused leaves the others to go on
 * @param drafts every row, changed or not
 */
export async function saveDrafts(drafts: Draft[]): Promise<SaveReport> {
  const report: SaveReport = { saved: 0, problems: [] };
  const todo = drafts.filter(rowChanged);
  /** Each changed row's id after its fields are saved, in its store as read; none for one that couldn't be saved */
  const now = new Map<Draft, string>();
  const fields = (d: Draft): SessionFields => ({ ...d.fields, port: Number(d.fields.port) || 0 });

  // a new row is saved where it's put; a row read is saved where it was, and moved afterwards
  const home = (d: Draft): Store => (isNew(d) ? d.store : d.row.store);
  for (const store of ["saved", "private"] as const) {
    const edits = todo.filter((d) => home(d) === store);
    const changedFields = edits.filter(fieldsChanged);
    for (const d of edits) if (!fieldsChanged(d)) now.set(d, d.row.id);
    if (!changedFields.length) continue;
    try {
      const ids = await saveTable(store, changedFields.map((d) => [isNew(d) ? null : d.row.id, fields(d)]));
      changedFields.forEach((d, i) => now.set(d, ids[i]));
      report.saved += changedFields.length;
    } catch (e) {
      // none of the file's were saved: its moves wait too
      for (const d of edits) now.delete(d);
      report.problems.push(`${STORE_LABELS[store]}: ${e}`);
    }
  }

  for (const d of todo.filter((d) => home(d) === "database")) {
    if (!fieldsChanged(d)) {
      now.set(d, d.row.id);
      continue;
    }
    try {
      now.set(d, await saveSession("database", isNew(d) ? null : d.row.id, fields(d)));
      report.saved++;
    } catch (e) {
      report.problems.push(`${isNew(d) ? `New session ${d.fields.name.trim()}` : d.row.id.replace(/^@db\//, "")}: ${e}`);
    }
  }

  const moves = todo.filter((d) => !isNew(d) && d.store !== d.row.store && now.has(d));
  if (moves.length) {
    // the move finds sessions in what was last read: read them again, with the new ids
    await loadSessions();
    const groups = new Map<string, Draft[]>();
    for (const d of moves) groups.set(`${d.row.store}>${d.store}`, [...(groups.get(`${d.row.store}>${d.store}`) ?? []), d]);
    for (const group of groups.values()) {
      const { row, store } = group[0];
      try {
        const moved = await transferSessions(row.store, group.map((d) => now.get(d)!), store, true);
        // a move that saved fields too is one session, not two
        report.saved += group.filter((d) => !fieldsChanged(d)).length;
        report.problems.push(...moved.problems);
        if (moved.done < group.length && !moved.problems.length) {
          report.problems.push(`${group.length - moved.done} of them couldn't be moved to ${STORE_LABELS[store]}.`);
        }
      } catch (e) {
        report.problems.push(`Moving to ${STORE_LABELS[store]}: ${e}`);
      }
    }
  }
  return report;
}
