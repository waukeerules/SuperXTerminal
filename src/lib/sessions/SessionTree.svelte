<!--
  The Sessions pane, as in SuperXPuTTY: the search box, the toolbar (new session, new folder, expand all, collapse
  all, sort), and the tree: Favorites and Recent (the user's own shortcuts), Saved sessions from Sessions.xml,
  Private sessions from PrivateSessions.xml and Database sessions from the team's table. A search opens every
  folder holding a match; clearing it puts the tree back as it was.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import TreeFolder from "./TreeFolder.svelte";
  import {
    deleteSession,
    getSession,
    loadSessions,
    saveSession,
    moveFolder,
    deleteFolder,
    moveSession,
    saveFolderSettings,
    transferSessions,
    unavailableReason,
    refreshDatabase,
    cachedDatabase,
    databaseStatus,
    databaseChanged,
    openExternally,
    loadShortcuts,
    setFavorite,
    recordOpened,
    forgetRecent,
    setGroupExpanded,
    loadOverrides,
    type Override,
    type Shortcuts,
    type DbState,
    type FolderSetting,
    type SessionFields,
    type SessionsLoaded,
    type SessionView,
    type Store,
  } from "./api";
  import EditSession from "./EditSession.svelte";
  import BulkEdit from "./BulkEdit.svelte";
  import Overrides from "./Overrides.svelte";
  import ImportSessions from "./ImportSessions.svelte";
  import ConnectSome from "./ConnectSome.svelte";
  import FolderSettings from "./FolderSettings.svelte";
  import PendingChanges from "./PendingChanges.svelte";
  import Prompt from "../ui/Prompt.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { PALETTE } from "../icons/palette";
  import { buildGroup, buildTree, folderKeys, sessionsIn, type Dragged, type FolderNode, type Group } from "./tree";
  import Icon from "../icons/Icon.svelte";
  import { context } from "../commands/sending.svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { loadCustomIcons, type CustomIcon } from "../icons/custom";
  import { EnvironmentColors, parseRules } from "../icons/environment";
  import { chooseSettingsFolder, databaseCheckSeconds, settings } from "../settings/store.svelte";
  import { DATABASE_SAVED, exportCsv, type ImportKind } from "./api";

  interface Props {
    /** Sessions with an open tab, as "store:id" */
    openSessions: ReadonlySet<string>;
    /** Open a session; given its icon key and environment colour for its tab */
    onOpen: (session: SessionView, environment: string | null) => void;
    /** The icon size and custom icons, once read, so tabs can draw icons the same way */
    onLook: (iconSize: number, customs: ReadonlyMap<string, CustomIcon>) => void;
    /** Open Settings at Profiles (from Edit Session) */
    onOpenProfiles: () => void;
  }

  let { openSessions, onOpen, onLook, onOpenProfiles }: Props = $props();

  let selected = $state("");
  /** Sessions selected together with Cmd+click and Shift+click, as "store:id" (SuperXPuTTY's multiple selection) */
  let multi = $state<ReadonlySet<string>>(new Set());
  /** Where a Shift+click range starts: the session last clicked without Shift */
  let anchor = "";
  /** Edit sessions… open: its title and sessions */
  let bulk = $state<{ title: string; sessions: SessionView[] } | null>(null);
  /** Local overrides… open, for these sessions */
  let overriding = $state<SessionView[] | null>(null);
  /** Connect all with more than ten sessions: the picker's title and sessions */
  let connecting = $state<{ title: string; sessions: SessionView[] } | null>(null);
  /** File > Import Sessions open: which kind of file */
  let importing = $state<ImportKind | null>(null);
  /** The user's local overrides, by session id */
  let overrides = $state<Record<string, Override>>({});
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** The session being edited: its store, its id (null for new) and its fields */
  /** Edit Session open; `copyOf` is the session a new one was filled in from, whose password it keeps */
  let editing = $state<{ store: Store; id: string | null; fields: SessionFields; lockedFields?: string; copyOf?: [Store, string] } | null>(null);
  /** A folder whose settings are being edited */
  /** Whether Pending changes is open */
  let showPending = $state(false);
  let folderSettings = $state<{ store: Store; setting: FolderSetting } | null>(null);
  /** A name being asked for (New folder, Rename) */
  let prompt = $state<{ title: string; value: string; validate: (name: string) => string; onOk: (name: string) => void } | null>(null);
  /** Folders made in the tree with no sessions yet, by store: they exist only here until a session goes in */
  let newFolders = $state<Record<Store, string[]>>({ saved: [], private: [], database: [] });
  /** How Database sessions stand; null until the first read */
  let db = $state<DbState | null>(null);
  /** Whether the table is being read */
  let dbLoading = $state(false);

  const iconSize = $derived(settings.iconSize);
  /** The toolbar's and search buttons' icons scale with the icon size, as SuperXPuTTY's toolbars do */
  const toolSize = $derived(Math.round(settings.iconSize * 0.85));
  let customs = $state<ReadonlyMap<string, CustomIcon>>(new Map());

  let loaded = $state<SessionsLoaded | null>(null);
  let error = $state("");
  let query = $state("");
  // roots start open, as with SuperXPuTTY's "Expand tree on startup"
  let expanded = $state(new Set<string>(["saved:", "private:"]));
  /** Folders the user closed during the current search (a search opens everything that holds a match) */
  let closedInSearch = $state(new Set<string>());
  /** Names Z to A (the toolbar's sort button); not saved, so A to Z returns on restart, as in SuperXPuTTY */
  let descending = $state(false);
  /** Favorites and Recent */
  let shortcuts = $state<Shortcuts>({ Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false });

  const colors = $derived(new EnvironmentColors(loaded?.folders ?? [], parseRules(settings.environmentColorRules)));
  /** The sessions as shown: a local override's note in place of the stored one, in the tooltip and the search */
  const shown = $derived(
    (loaded?.sessions ?? []).map((s) => (overrides[s.id]?.Note !== undefined ? { ...s, note: overrides[s.id].Note ?? "" } : s)),
  );
  const saved = $derived(loaded ? buildTree("saved", "Saved sessions", shown, query, newFolders.saved, descending) : null);
  const privateRoot = $derived(
    loaded ? buildTree("private", "Private sessions", shown, query, newFolders.private, descending) : null,
  );
  const databaseRoot = $derived(
    loaded && db?.enabled ? buildTree("database", dbLabel(db, dbLoading), shown, query, newFolders.database, descending) : null,
  );
  const favorites = $derived(loaded ? buildGroup("favorites", "Favorites", shortcuts.Favorites, shown, query) : null);
  const recent = $derived(loaded ? buildGroup("recent", "Recent", shortcuts.Recent, shown, query) : null);
  const roots = $derived([favorites, recent, saved, privateRoot, databaseRoot].filter((r): r is FolderNode => r !== null));
  const searching = $derived(query.trim() !== "");
  /** What's open: while searching every folder holding a match (but those closed since), else as the user left it */
  const shownExpanded = $derived(
    searching ? new Set(roots.flatMap(folderKeys).filter((k) => !closedInSearch.has(k))) : expanded,
  );

  // a new search starts with everything open again
  $effect(() => {
    void query;
    closedInSearch = new Set();
  });

  // Favorites and Recent open as the user last left them (closed at first)
  $effect(() => {
    const wanted = [["favorites:", shortcuts.FavoritesExpanded], ["recent:", shortcuts.RecentExpanded]] as const;
    untrack(() => {
      const next = new Set(expanded);
      for (const [key, open] of wanted) {
        if (open) next.add(key);
        else next.delete(key);
      }
      if (next.size !== expanded.size || [...next].some((k) => !expanded.has(k))) expanded = next;
    });
  });

  // read again whenever the settings folder changes (Settings > General); the first run reads them at start
  $effect(() => {
    if (!settings.loaded) return;
    void settings.settingsFolder;
    void settings.useSessionsXml;
    void reload();
  });

  // Database sessions, as SuperXPuTTY: show the cache at once, read the table, then check for changes every "Check
  // every" seconds (a one-row query), reading it again only when it changed; while it can't be reached, try again
  // every 2 minutes
  $effect(() => {
    if (!settings.loaded) return;
    void startDatabase();
    const saved = () => void startDatabase();
    window.addEventListener(DATABASE_SAVED, saved);
    return () => window.removeEventListener(DATABASE_SAVED, saved);
  });
  $effect(() => {
    const seconds = databaseCheckSeconds();
    if (!db?.enabled || !seconds) return;
    const timer = setInterval(() => void checkDatabase(), seconds * 1000);
    return () => clearInterval(timer);
  });
  $effect(() => {
    if (!db?.enabled || !db.error) return;
    const timer = setInterval(() => void readDatabase(), RETRY_MS);
    return () => clearInterval(timer);
  });

  /** SuperXPuTTY's RetryInterval */
  const RETRY_MS = 2 * 60 * 1000;

  /** Show this connection's cache, then read the table */
  async function startDatabase(): Promise<void> {
    try {
      db = await cachedDatabase();
      loaded = await loadSessions();
    } catch (e) {
      error = String(e);
    }
    await readDatabase();
  }

  /** Read the table (keeping what's shown when it can't be reached) and show it */
  async function readDatabase(): Promise<void> {
    if (dbLoading) return;
    dbLoading = true;
    try {
      db = await refreshDatabase();
      loaded = await loadSessions();
    } catch (e) {
      error = String(e);
    } finally {
      dbLoading = false;
    }
  }

  /** Read the table again if it changed; not while a window is open, so a session being edited isn't replaced, and
   * not while offline (the retry takes over) */
  async function checkDatabase(): Promise<void> {
    if (editing || bulk || overriding || folderSettings || prompt || menu || showPending || dbLoading || db?.error) return;
    try {
      if (await databaseChanged()) await readDatabase();
    } catch {
      // as SuperXPuTTY: a failed check reads the table, which reports offline and starts retrying
      await readDatabase();
    }
  }

  /** The root's label: the tree adds the count, so only SuperXPuTTY's other states are added, e.g. "(offline, cached
   * 09:41)", "(loading...)", "(read-only)" */
  function dbLabel(state: DbState, loading: boolean): string {
    const parts = loading ? ["loading..."] : state.error ? [state.syncedAt ? `offline, cached ${state.syncedAt}` : "offline"] : [];
    if (!state.writable && !loading) parts.push("read-only");
    if (state.pending) parts.push(`${state.pending} pending`);
    if (state.failed) parts.push(`${state.failed} not saved`);
    return parts.length ? `Database sessions (${parts.join(", ")})` : "Database sessions";
  }

  /** Read the settings, custom icons and session files again */
  async function reload(): Promise<void> {
    try {
      customs = await loadCustomIcons();
      onLook(settings.iconSize, customs);
      loaded = await loadSessions();
      shortcuts = await loadShortcuts();
      overrides = await loadOverrides();
      if (db?.enabled) db = await databaseStatus();
      error = "";
    } catch (e) {
      error = `Couldn't read the sessions: ${e}`;
    }
  }

  /** Choose the settings folder; the tree reads it when the setting changes */
  async function choose(): Promise<void> {
    await chooseSettingsFolder();
  }

  /** A new session's fields, in a folder */
  function blank(folder: string): SessionFields {
    return {
      name: "", folder, protocol: "SSH", host: "", port: 22, username: "", profile: "Default Settings", color: "",
      icon: "computer", highlightRules: "", extraArgs: "", jumpHost: "", script: "", note: "", localPath: "", remotePath: "",
    };
  }

  /** The folder settings along a folder's path in a store, nearest first, for Edit Session's inherited hints */
  function foldersAlong(store: Store, folder: string) {
    const settings = (loaded?.folders ?? []).filter(([s]) => s === store).map(([, f]) => f);
    const found = [];
    let path = folder;
    while (path) {
      const setting = settings.find((f) => f.path.toLowerCase() === path.toLowerCase());
      if (setting) found.push(setting);
      path = path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
    }
    return found;
  }

  /** Open Edit Session for a session */
  async function edit(session: SessionView): Promise<void> {
    try {
      editing = { store: session.store, id: session.id, fields: await getSession(session.store, session.id), lockedFields: session.lockedFields };
    } catch (e) {
      error = String(e);
    }
  }

  /** Set a session's colour straight from the menu */
  async function setColour(session: SessionView, color: string): Promise<void> {
    try {
      const fields = await getSession(session.store, session.id);
      await saveSession(session.store, session.id, { ...fields, color });
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  /** Delete a session, after asking */
  async function remove(session: SessionView): Promise<void> {
    const file = session.store === "saved" ? "Sessions.xml" : session.store === "private" ? "PrivateSessions.xml" : "the database";
    const sure = await ask(`Delete the session "${session.id}" from ${file}?`, { title: "Delete session", kind: "warning", okLabel: "Delete" });
    if (!sure) return;
    try {
      await deleteSession(session.store, session.id);
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  /** The right-click menu for a session or folder, as SuperXPuTTY's; what the Mac can't do yet is greyed with why */
  function showMenu(event: MouseEvent, target: { session: SessionView; group?: Group } | { folder: FolderNode }): void {
    let items: MenuItem[];
    if ("session" in target && target.group) {
      // a Favorites or Recent entry: a shortcut, so only what makes sense for one (SuperXPuTTY's shortcut menu)
      const { session, group } = target;
      items = [
        { label: "Connect", bold: true, action: () => open(session) },
        { label: "Show in Sessions tree", action: () => showInTree(session) },
        { separator: true },
        ...(group === "recent" ? [favoriteItem(session)] : []),
        group === "favorites"
          ? { label: "Remove from Favorites", action: () => void changeShortcuts(setFavorite(session.id, false)) }
          : { label: "Remove from Recent", action: () => void changeShortcuts(forgetRecent(session.id)) },
      ];
      menu = { x: event.clientX, y: event.clientY, items };
      return;
    }
    if ("folder" in target && target.folder.group) {
      const runnable = target.folder.sessions.filter((s) => unavailableReason(s.protocol) === null);
      items = [
        { label: `Connect all (${runnable.length})`, disabled: runnable.length === 0, action: () => connectMany(`in ${target.folder.name}`, runnable) },
        ...(target.folder.group === "recent"
          ? [{ separator: true } as MenuItem, { label: "Clear Recent", action: () => void changeShortcuts(forgetRecent(null)) }]
          : []),
      ];
      menu = { x: event.clientX, y: event.clientY, items };
      return;
    }
    // Database sessions change only when the login may write and Read only is off; a locked row not at all
    const dbBlock = (store: Store, locked = false): string | false =>
      store !== "database" ? false
      : !db?.writable ? "Read only: this login can't change the table, or Read only is ticked in Database Sessions"
      : locked ? "Locked in the database"
      : false;
    // folder settings aren't queued while offline, as in SuperXPuTTY: they need the table
    const folderBlock = (store: Store): string | false =>
      dbBlock(store) || (store === "database" && db?.error ? "Folder settings are saved only while the database can be reached" : false);
    if ("session" in target && multi.size > 1 && multi.has(`${target.session.store}:${target.session.id}`)) {
      const list = selectedSessions();
      const runnable = list.filter((s) => unavailableReason(s.protocol) === null);
      const stores = new Set(list.map((s) => s.store));
      const [only] = stores;
      items = [
        { label: `Connect ${runnable.length} sessions`, bold: true, disabled: runnable.length === 0, action: () => connectMany("from the selection", runnable) },
        { separator: true },
        { label: `Edit ${list.length} sessions…`, action: () => void editSessions("for the selection", list) },
        { label: `Local overrides for ${list.length} sessions…`, action: () => (overriding = list) },
        { label: `Delete ${list.length} sessions`, action: () => void removeSessions(list) },
        { label: `Export ${list.length} sessions to CSV…`, action: () => void exportSessions(list, "Sessions.csv") },
        ...(stores.size === 1
          ? transferItems(list, only, list.some((s) => s.locked))
          : [{ label: "Move to / Copy to", disabled: "The selection is in more than one root" } as MenuItem]),
      ];
      menu = { x: event.clientX, y: event.clientY, items };
      return;
    }
    if ("session" in target) {
      const session = target.session;
      const block = dbBlock(session.store, session.locked);
      items = [
        { label: "Connect", bold: true, action: () => open(session) },
        {
          label: "Connect external",
          disabled: (session.protocol || "SSH").toUpperCase() !== "SSH" && "Only SSH sessions open in Terminal",
          action: () => void external(session, "terminal"),
        },
        { label: "Connect in new window", disabled: "Windows and splits come in M5" },
        favoriteItem(session),
        {
          label: "Colour",
          disabled: block || (session.lockedFields.split(",").includes("color") && "The colour is locked in the database"),
          items: [
            { label: "None", swatch: "", action: () => void setColour(session, "") },
            ...PALETTE.map((p) => ({ label: p.name, swatch: p.color, action: () => void setColour(session, p.color) })),
          ],
        },
        { separator: true },
        { label: "Copy host address", action: () => void navigator.clipboard.writeText(session.host) },
        { label: "Duplicate as…", disabled: session.store === "database" && !db?.writable && "Database sessions can't be written now", action: () => void duplicateAs(session) },
        { label: "Edit", disabled: block, action: () => void edit(session) },
        { label: "Local overrides…", action: () => (overriding = [session]) },
        { label: "Delete", disabled: block, action: () => void remove(session) },
        ...transferItems([session], session.store, session.locked),
        { separator: true },
        { label: "File transfer (SFTP)", disabled: "The SFTP file browser comes later" },
        { label: "FileZilla (external)", action: () => void external(session, "filezilla") },
      ];
    } else {
      const folder = target.folder;
      const isRoot = folder.path === "";
      const runnable = sessionsIn(folder).filter((s) => unavailableReason(s.protocol) === null);
      const block = dbBlock(folder.store, sessionsIn(folder).some((s) => s.locked));
      const canAdd = dbBlock(folder.store);
      items = [
        ...(folder.store === "database" && isRoot
          ? [
              { label: "Refresh", action: () => void readDatabase() },
              ...(db && db.pending + db.failed > 0
                ? [{ label: `Pending changes (${db.pending + db.failed})…`, action: () => (showPending = true) }]
                : []),
              { separator: true } as MenuItem,
            ]
          : []),
        { label: "New session…", disabled: canAdd, action: () => (editing = { store: folder.store, id: null, fields: blank(folder.path) }) },
        { label: "New folder…", disabled: canAdd, action: () => newFolder(folder) },
        ...(isRoot
          ? []
          : ([
              { label: "Rename…", disabled: block, action: () => rename(folder) },
              {
                label: "Colour",
                disabled: folderBlock(folder.store),
                items: [
                  { label: "None", swatch: "", action: () => void setFolderColour(folder, "") },
                  ...PALETTE.map((p) => ({ label: p.name, swatch: p.color, action: () => void setFolderColour(folder, p.color) })),
                ],
              },
              { label: "Folder settings…", disabled: folderBlock(folder.store), action: () => (folderSettings = { store: folder.store, setting: settingOf(folder) }) },
              {
                label: "Edit sessions…",
                disabled: sessionsIn(folder).length === 0 ? "No sessions here" : dbBlock(folder.store),
                action: () => void editSessions(`in ${folder.name}`, sessionsIn(folder)),
              },
              { label: "Delete folder", disabled: block, action: () => void removeFolder(folder) },
              ...transferItems(sessionsIn(folder), folder.store, sessionsIn(folder).some((s) => s.locked)),
            ] as MenuItem[])),
        {
          label: "Export to CSV…",
          disabled: sessionsIn(folder).length === 0 && "No sessions here",
          action: () => void exportSessions(sessionsIn(folder), `${folder.name}.csv`),
        },
        { separator: true },
        { label: `Connect all (${runnable.length})`, disabled: runnable.length === 0, action: () => connectMany(`in ${folder.name}`, runnable) },
        { label: "Expand all", action: () => (expanded = new Set([...expanded, ...folderKeys(folder)])) },
        { label: "Collapse all", action: () => (expanded = new Set([...expanded].filter((k) => !folderKeys(folder).slice(1).includes(k)))) },
      ];
    }
    menu = { x: event.clientX, y: event.clientY, items };
  }

  /** A click on a session or folder: Cmd+click adds or removes a session from the selection, Shift+click selects the
   * sessions shown between the last click and this one, anything else selects just it (SuperXPuTTY's
   * BeforeSelectMulti, with Cmd for Ctrl). A right-click (no event) keeps the selection when it's on a selected one. */
  function select(key: string, event?: MouseEvent): void {
    const isSession = isSessionKey(key);
    if (!event) {
      if (!multi.has(key)) multi = new Set();
      selected = key;
      return;
    }
    if (isSession && (event.metaKey || event.ctrlKey)) {
      const next = new Set(multi.size ? multi : isSessionKey(selected) ? [selected] : []);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      multi = next;
      selected = key;
      anchor = key;
      return;
    }
    if (isSession && event.shiftKey && anchor) {
      const order = visibleSessionKeys();
      const [a, b] = [order.indexOf(anchor), order.indexOf(key)];
      if (a >= 0 && b >= 0) {
        multi = new Set(order.slice(Math.min(a, b), Math.max(a, b) + 1));
        selected = key;
        return;
      }
    }
    multi = new Set();
    selected = key;
    if (isSession) anchor = key;
  }

  /** Whether a selection key is a session in its own root (not a folder, nor a Favorites or Recent entry) */
  function isSessionKey(key: string): boolean {
    const [store, second] = key.split(":");
    return ["saved", "private", "database"].includes(store) && second !== "folder" && key.length > store.length + 1;
  }

  /** The sessions shown in the roots, top to bottom, for Shift+click */
  function visibleSessionKeys(): string[] {
    const out: string[] = [];
    const walk = (node: FolderNode) => {
      if (!shownExpanded.has(node.key)) return;
      node.folders.forEach(walk);
      for (const s of node.sessions) out.push(`${s.store}:${s.id}`);
    };
    [saved, privateRoot, databaseRoot].forEach((root) => root && walk(root));
    return out;
  }

  /** The sessions selected together */
  function selectedSessions(): SessionView[] {
    return (loaded?.sessions ?? []).filter((s) => multi.has(`${s.store}:${s.id}`));
  }

  /** Edit sessions…: the ones that can be changed here, after saying which can't (SuperXPuTTY's BulkEditSessions) */
  async function editSessions(title: string, list: SessionView[]): Promise<void> {
    const editable = list.filter((s) => s.store !== "database" || (db?.writable && !s.locked));
    const skipped = list.filter((s) => !editable.includes(s));
    if (editable.length === 0) {
      error = "None of these sessions can be edited here: they're locked in the database, or it can't be written.";
      return;
    }
    if (skipped.length) {
      const names = skipped.slice(0, 10).map((s) => s.name).join("\n") + (skipped.length > 10 ? `\n... and ${skipped.length - 10} more` : "");
      const sure = await ask(`These can't be edited here and will be left out:\n\n${names}\n\nEdit the other ${editable.length}?`, { title: "Edit sessions", okLabel: "Edit" });
      if (!sure) return;
    }
    bulk = { title, sessions: editable };
  }

  /** Delete several sessions after one question; locked Database ones are left (SuperXPuTTY's DeleteSessions) */
  async function removeSessions(list: SessionView[]): Promise<void> {
    const deletable = list.filter((s) => s.store !== "database" || (db?.writable && !s.locked));
    const left = list.length - deletable.length;
    if (deletable.length === 0) {
      error = "None of these sessions can be deleted here: they're locked in the database, or it can't be written.";
      return;
    }
    const names = deletable.slice(0, 10).map((s) => s.name).join("\n") + (deletable.length > 10 ? `\n... and ${deletable.length - 10} more` : "");
    const sure = await ask(`Delete these ${deletable.length} sessions?\n\n${names}${left ? `\n\n${left} locked in the database will be left.` : ""}`,
      { title: "Delete sessions", kind: "warning", okLabel: "Delete" });
    if (!sure) return;
    multi = new Set();
    const problems: string[] = [];
    for (const s of deletable) {
      try {
        await deleteSession(s.store, s.id);
      } catch (e) {
        problems.push(`${s.name}: ${e}`);
      }
    }
    await reload();
    error = problems.join(" ");
  }

  /** Add to Favorites, or Remove from Favorites when it's there */
  function favoriteItem(session: SessionView): MenuItem {
    const on = shortcuts.Favorites.includes(session.id);
    return {
      label: on ? "Remove from Favorites" : "Add to Favorites",
      action: () => void changeShortcuts(setFavorite(session.id, !on)),
    };
  }

  /** Show the lists a change returns */
  async function changeShortcuts(change: Promise<Shortcuts>): Promise<void> {
    try {
      shortcuts = await change;
    } catch (e) {
      error = String(e);
    }
  }

  /** Select a session in its own root, opening the folders above it (SuperXPuTTY's Show in Sessions tree) */
  function showInTree(session: SessionView): void {
    query = "";
    const keys = [`${session.store}:`];
    let path = "";
    for (const part of session.folder.split("/").filter(Boolean)) {
      path = path ? `${path}/${part}` : part;
      keys.push(`${session.store}:${path}`);
    }
    expanded = new Set([...expanded, ...keys]);
    selected = `${session.store}:${session.id}`;
  }

  /** The roots' names, for Move to / Copy to */
  const ROOT_NAMES: Record<Store, string> = { saved: "Saved sessions", private: "Private sessions", database: "Database sessions" };

  /** Move to and Copy to, each listing the other roots; the table only when it can be written, and no move of locked rows */
  function transferItems(sessions: SessionView[], from: Store, locked: boolean): MenuItem[] {
    const targets: Store[] = (["saved", "private", "database"] as Store[]).filter(
      (t) => t !== from && (t !== "database" || db?.enabled) && (t !== "saved" || (settings.settingsFolder && settings.useSessionsXml)),
    );
    const item = (move: boolean): MenuItem => ({
      label: move ? "Move to" : "Copy to",
      disabled: sessions.length === 0 ? "No sessions here" : move && from === "database" && locked ? "Locked in the database" : false,
      items: targets.map((to) =>
        // into Private sessions is only ever a copy: moving would take a shared session away from everyone else
        move && to === "private"
          ? { label: `${ROOT_NAMES[to]} (use Copy to)`, disabled: "Sessions only go into Private sessions as copies" }
          : {
              label: ROOT_NAMES[to],
              disabled: to === "database" && !db?.writable ? "Database sessions can't be written now" : false,
              action: () => void transfer(sessions, from, to, move),
            },
      ),
    });
    return [item(true), item(false)];
  }

  /**
   * Copy or move sessions to another root: each to the folder `folders` gives (a drop), else the same path there.
   * Asks first when moving them out of the shared table, and once for all whose names are taken there whether to
   * add -1, -2... to them, as SuperXPuTTY's Transfer.
   */
  async function transfer(sessions: SessionView[], from: Store, to: Store, move: boolean, folders: string[] | null = null): Promise<void> {
    const n = sessions.length;
    const verb = move ? "Move" : "Copy";
    if (move && from === "database") {
      const sure = await ask(`Move ${n} session${n === 1 ? "" : "s"} out of Database sessions into ${ROOT_NAMES[to]}?\n\nThey're deleted from the table, for everyone who uses it.`,
        { title: "Move", kind: "warning", okLabel: "Move" });
      if (!sure) return;
    }
    const folderOf = (i: number) => folders?.[i] ?? sessions[i].folder;
    const taken = sessions.filter((s, i) => takenId(to, folderOf(i), s.name));
    let renameTaken = false;
    if (taken.length) {
      const names = taken.slice(0, 10).map((s) => s.name).join("\n") + (taken.length > 10 ? "\n…" : "");
      renameTaken = await ask(
        `${taken.length === 1 ? "A session with this name is already" : `${taken.length} sessions with these names are already`} there` +
          `${to === "database" ? " in Database sessions" : " (Saved and Private sessions can't share a folder and name)"}:\n\n${names}\n\n` +
          `${verb} ${taken.length === 1 ? "it" : "them"} anyway, with -1, -2… added to the name? No leaves ${taken.length === 1 ? "it" : "them"} out.`,
        { title: verb, kind: "info", okLabel: `${verb} as -1`, cancelLabel: "Leave out" },
      );
      if (!renameTaken && taken.length === n) return;
    }
    try {
      const report = await transferSessions(from, sessions.map((s) => s.id), to, move, folders, renameTaken);
      await reload();
      const left = renameTaken ? report.problems : report.problems.filter((p) => !p.includes("already a session with that name"));
      error = left.length ? `${report.done} of ${n} ${move ? "moved" : "copied"}. ${left.join(" ")}` : "";
      context.status(`${move ? "Moved" : "Copied"} ${report.done} session${report.done === 1 ? "" : "s"} to ${ROOT_NAMES[to]}`);
      expanded = new Set([...expanded, `${to}:`, ...(folders ?? []).map((f) => `${to}:${f}`)]);
    } catch (e) {
      error = String(e);
    }
  }

  /**
   * Whether a session name is taken in a folder of a store: Saved and Private sessions share ids, Database sessions
   * have their own; ignoring case, as the files and table do. `except` is the session being renamed or moved.
   */
  function takenId(store: Store, folder: string, name: string, except = ""): boolean {
    const path = folder.replace(/^\/+|\/+$/g, "");
    const id = (path ? `${path}/${name}` : name).toLowerCase();
    return (loaded?.sessions ?? []).some((s) =>
      (store === "database") === (s.store === "database") && s.id !== except && s.id.replace(/^@db\//, "").toLowerCase() === id,
    );
  }

  /** The part of a folder path after `folder` ("" for the folder itself), ignoring case; null when it isn't in it */
  function folderUnder(path: string, folder: string): string | null {
    const head = path.slice(0, folder.length);
    const rest = path.slice(folder.length);
    return head.toLowerCase() === folder.toLowerCase() && (rest === "" || rest.startsWith("/")) ? rest : null;
  }

  /** A free name in a folder: the name itself, else with -1, -2… added, as SuperXPuTTY's UniqueName */
  function freeName(store: Store, folder: string, name: string, except = ""): string {
    let candidate = name;
    for (let i = 1; takenId(store, folder, candidate, except); i++) candidate = `${name}-${i}`;
    return candidate;
  }

  /** A folder's stored settings, or empty ones */
  function settingOf(folder: FolderNode): FolderSetting {
    const found = (loaded?.folders ?? []).find(([store, f]) => store === folder.store && f.path.toLowerCase() === folder.path.toLowerCase())?.[1];
    return found
      ? { ...found }
      : { path: folder.path, color: "", username: "", puttyProfile: "", port: 0, jumpHost: "", highlightRules: "" };
  }

  /** Why a new folder name can't be used in a folder, or "" */
  function folderNameProblem(parent: FolderNode, name: string, except = ""): string {
    if (!name) return "Empty folder name";
    if (name.includes("/")) return 'A name can\'t contain "/"';
    if (name !== except && parent.folders.some((f) => f.name.toLowerCase() === name.toLowerCase())) return "A folder with that name is already here";
    return "";
  }

  /** The folder node a path is in (its parent), searching from a root */
  function parentOf(folder: FolderNode): FolderNode | null {
    const root = folder.store === "saved" ? saved : privateRoot;
    const parentPath = folder.path.includes("/") ? folder.path.slice(0, folder.path.lastIndexOf("/")) : "";
    const find = (node: FolderNode | null): FolderNode | null =>
      !node ? null : node.path === parentPath ? node : node.folders.map(find).find(Boolean) ?? null;
    return find(root);
  }

  /** Ask for a new folder's name and show it in the tree (it's stored once a session goes in it) */
  function newFolder(parent: FolderNode): void {
    prompt = {
      title: "New Folder",
      value: "New Folder",
      validate: (name) => folderNameProblem(parent, name),
      onOk: (name) => {
        const path = parent.path ? `${parent.path}/${name}` : name;
        newFolders = { ...newFolders, [parent.store]: [...newFolders[parent.store], path] };
        expanded = new Set([...expanded, parent.key]);
        selected = `${parent.store}:folder:${path}`;
        prompt = null;
      },
    };
  }

  /** Ask for a folder's new name and rename it, with every session below */
  function rename(folder: FolderNode): void {
    const parent = parentOf(folder);
    prompt = {
      title: "Rename Folder",
      value: folder.name,
      validate: (name) => (parent ? folderNameProblem(parent, name, folder.name) : ""),
      onOk: async (name) => {
        prompt = null;
        const to = folder.path.includes("/") ? `${folder.path.slice(0, folder.path.lastIndexOf("/"))}/${name}` : name;
        await moveFolderTo(folder.store, folder.path, to);
      },
    };
  }

  /** Move or rename a folder, keeping tree-only folders and open folders in step */
  async function moveFolderTo(store: Store, from: string, to: string): Promise<void> {
    if (from === to) return;
    try {
      await moveFolder(store, from, to);
      const shift = (p: string) => (p === from || p.startsWith(from + "/") ? to + p.slice(from.length) : p);
      newFolders = { ...newFolders, [store]: newFolders[store].map(shift) };
      expanded = new Set([...expanded].map((k) => (k.startsWith(`${store}:`) ? `${store}:${shift(k.slice(store.length + 1))}` : k)));
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  /** Set a folder's colour straight from the menu */
  async function setFolderColour(folder: FolderNode, color: string): Promise<void> {
    try {
      await saveFolderSettings(folder.store, { ...settingOf(folder), color });
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  /** Delete a folder and its sessions, after asking */
  async function removeFolder(folder: FolderNode): Promise<void> {
    const count = sessionsIn(folder).length;
    const sure = count === 0 || (await ask(`Are you sure you want to remove folder "${folder.name}" and its ${count} session${count === 1 ? "" : "s"}?`,
      { title: "Remove folder", kind: "warning", okLabel: "Remove" }));
    if (!sure) return;
    try {
      if (count > 0) await deleteFolder(folder.store, folder.path);
      newFolders = { ...newFolders, [folder.store]: newFolders[folder.store].filter((p) => p !== folder.path && !p.startsWith(folder.path + "/")) };
      await reload();
    } catch (e) {
      error = String(e);
    }
  }

  /** Something dragged in the tree was dropped on a folder: move it there (within its own store) */
  async function dropped(target: FolderNode, dragged: Dragged): Promise<void> {
    if (dragged.store !== target.store) {
      // into another root (Saved, Private or Database sessions): always a copy, so the one it came from (perhaps
      // shared) keeps it; moving is on the menu
      if (target.store === "database" && !db?.writable) {
        error = "Database sessions can't be written now.";
        return;
      }
      const all = loaded?.sessions ?? [];
      let list: SessionView[];
      let folders: string[];
      const join = (a: string, b: string) => (a && b ? `${a}/${b}` : a || b);
      if (dragged.kind === "session") {
        list = all.filter((s) => s.store === dragged.store && s.id === dragged.id);
        folders = list.map(() => target.path);
      } else {
        // the folder and everything in it, under the folder dropped on
        const name = dragged.path.slice(dragged.path.lastIndexOf("/") + 1);
        list = all.filter((s) => s.store === dragged.store && folderUnder(s.folder, dragged.path) !== null);
        folders = list.map((s) => join(join(target.path, name), (folderUnder(s.folder, dragged.path) ?? "").replace(/^\//, "")));
      }
      if (list.length === 0) return;
      await transfer(list, dragged.store, target.store, false, folders);
      return;
    }
    try {
      if (dragged.kind === "session") {
        const folder = dragged.id.replace(/^@db\//, "").includes("/") ? dragged.id.replace(/^@db\//, "").replace(/\/[^/]*$/, "") : "";
        if (folder.toLowerCase() === target.path.toLowerCase()) return;
        const session = loaded?.sessions.find((s) => s.store === dragged.store && s.id === dragged.id);
        const name = session?.name ?? dragged.id.slice(dragged.id.lastIndexOf("/") + 1);
        let newName: string | null = null;
        if (takenId(target.store, target.path, name, dragged.id)) {
          // a session there has its name: offer it a free one, as SuperXPuTTY's MoveNode
          newName = freeName(target.store, target.path, name, dragged.id);
          const sure = await ask(`There is already a session ${target.path ? `${target.path}/` : ""}${name}.\n\nMove this one there anyway, as "${newName}"?`,
            { title: "Move", kind: "info", okLabel: "Move", cancelLabel: "Cancel" });
          if (!sure) return;
        }
        const id = await moveSession(target.store, dragged.id, target.path, newName);
        newFolders = { ...newFolders, [target.store]: newFolders[target.store].filter((p) => p !== target.path) };
        await reload();
        selected = `${target.store}:${id}`;
      } else {
        const name = dragged.path.slice(dragged.path.lastIndexOf("/") + 1);
        const to = target.path ? `${target.path}/${name}` : name;
        if (to === dragged.path || target.path === dragged.path || target.path.startsWith(dragged.path + "/")) return;
        await moveFolderTo(target.store, dragged.path, to);
      }
      expanded = new Set([...expanded, target.key]);
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  /** Duplicate as…: Edit Session for a new session in the same folder, filled in from this one, as SuperXPuTTY's */
  async function duplicateAs(session: SessionView): Promise<void> {
    try {
      editing = { store: session.store, id: null, fields: await getSession(session.store, session.id), copyOf: [session.store, session.id] };
    } catch (e) {
      error = String(e);
    }
  }

  // a session file changed elsewhere (a teammate saving to the shared folder, SuperXPuTTY, an editor): read it again,
  // but not while a window is open, so nothing being edited is lost
  $effect(() => {
    if (!settings.loaded) return;
    const timer = setInterval(async () => {
      if (editing || bulk || overriding || folderSettings || prompt || menu || showPending || connecting || importing) return;
      if (await invoke<boolean>("sessions_files_changed")) {
        await reload();
        context.status("The sessions changed on disk; read them again");
      }
    }, 3000);
    return () => clearInterval(timer);
  });

  /** Open a session in another app (Connect external, FileZilla), saying how it went in the status bar */
  async function external(session: SessionView, how: "terminal" | "filezilla"): Promise<void> {
    try {
      context.status(await openExternally(session.store, session.id, how));
    } catch (e) {
      context.status(String(e));
    }
  }

  /** Open a session, passing on its environment colour for its tab */
  function open(session: SessionView): void {
    onOpen(session, colors.forSession(session));
    // only sessions opened from the tree go in Recent, as in SuperXPuTTY
    void changeShortcuts(recordOpened(session.id));
  }

  /** Connect all: up to ten open at once; more, and the user picks up to ten of them (ConnectSome) */
  function connectMany(title: string, sessions: SessionView[]): void {
    if (sessions.length > 10) connecting = { title, sessions };
    else sessions.forEach(open);
  }

  /** Whether Saved sessions are shown: a settings folder is chosen and Use Sessions.xml is on */
  function savedInUse(): boolean {
    return !!loaded && loaded.savedFile !== null && !loaded.savedOff;
  }

  /** File > Import Sessions: the import window for that kind of file */
  export function importSessions(kind: ImportKind): void {
    importing = kind;
  }

  /** Export sessions to CSV (File > Export Sessions to CSV… for all of them, a folder's, or the selection) */
  export async function exportSessions(list: SessionView[] | null, fileName: string): Promise<void> {
    try {
      const text = await exportCsv(list && list.map((s) => [s.store, s.id] as [Store, string]), fileName);
      if (text) context.status(text);
    } catch (e) {
      error = `Couldn't export the sessions: ${e}`;
    }
  }

  /** Every session, for the tab menu's New session and Copy host address */
  export function allSessions(): SessionView[] {
    return loaded?.sessions ?? [];
  }

  /** The tab menu's Save session as…: Edit Session for a new saved session, from a tab's session, named as the tab */
  export async function saveSessionAs(store: Store, id: string, title: string): Promise<void> {
    try {
      const fields = await getSession(store, id);
      editing = { store: savedInUse() ? "saved" : "private", id: null, fields: { ...fields, name: title }, copyOf: [store, id] };
    } catch (e) {
      error = String(e);
    }
  }

  /** Open a session by its id (SPSL's OPENSESSION), looking in Saved, Private then Database sessions; false if none */
  export function openById(id: string): boolean {
    const session = ["saved", "private", "database"]
      .map((store) => loaded?.sessions.find((s) => s.store === store && s.id.toLowerCase() === id.toLowerCase()))
      .find(Boolean);
    if (!session) return false;
    open(session);
    return true;
  }

  /** Open or close a folder */
  function toggle(key: string): void {
    if (searching) {
      // during a search only that search's view changes; clearing it puts the tree back as it was
      const next = new Set(closedInSearch);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      closedInSearch = next;
      return;
    }
    const next = new Set(expanded);
    const open = !next.has(key);
    if (open) next.add(key);
    else next.delete(key);
    expanded = next;
    if (key === "favorites:" || key === "recent:") void changeShortcuts(setGroupExpanded(key === "favorites:" ? "favorites" : "recent", open));
  }

  /** The folder the toolbar's New session and New folder go in: the selected folder, or the selected session's, else
   * Saved sessions (Private sessions without a settings folder), as SuperXPuTTY's */
  function toolbarFolder(): FolderNode | null {
    const find = (node: FolderNode | null, path: string): FolderNode | null =>
      !node ? null : node.path === path ? node : node.folders.map((f) => find(f, path)).find(Boolean) ?? null;
    const rootOf = (store: string) => (store === "saved" ? saved : store === "private" ? privateRoot : store === "database" ? databaseRoot : null);
    const [store, ...rest] = selected.split(":");
    const fallback = savedInUse() ? saved : privateRoot;
    if (!rootOf(store)) return fallback;
    if (rest[0] === "folder") return find(rootOf(store), rest.slice(1).join(":")) ?? fallback;
    const id = rest.join(":");
    const session = loaded?.sessions.find((s) => s.store === store && s.id === id);
    return find(rootOf(store), session?.folder ?? "") ?? fallback;
  }

  /** The toolbar's New session */
  function newSessionHere(): void {
    const folder = toolbarFolder();
    if (!folder) return;
    if (folder.store === "database" && !db?.writable) {
      error = "Database sessions can't be written now.";
      return;
    }
    editing = { store: folder.store, id: null, fields: blank(folder.path) };
  }

  /** The toolbar's New folder */
  function newFolderHere(): void {
    const folder = toolbarFolder();
    if (folder) newFolder(folder);
  }

  /** Expand all: every folder of every root */
  function expandAll(): void {
    query = "";
    expanded = new Set(roots.flatMap(folderKeys));
  }

  /** Collapse all, leaving Saved sessions and Private sessions open so their top-level folders show */
  function collapseAll(): void {
    query = "";
    expanded = new Set(["saved:", "private:"]);
  }

</script>

<div class="search">
  <input placeholder="Type to search..." bind:value={query}
    onkeydown={(e) => { if (e.key === "Escape") query = ""; }} />
  <button class="icon" title="Search (the tree filters as you type)" onclick={() => (query = query.trim())}>
    <Icon key="search" size={toolSize} glyph />
  </button>
  <button class="icon" title="Clear the search" onclick={() => (query = "")}><Icon key="clear" size={toolSize} glyph /></button>
</div>
<div class="toolbar" role="toolbar" aria-label="Sessions">
  <button class="icon" title="New session (in the selected folder)" onclick={newSessionHere}><Icon key="newsession" size={toolSize} glyph /></button>
  <button class="icon" title="New folder (in the selected folder)" onclick={newFolderHere}><Icon key="newfolder" size={toolSize} glyph /></button>
  <span class="gap"></span>
  <button class="icon" title="Expand all" onclick={expandAll}><Icon key="expandall" size={toolSize} glyph /></button>
  <button class="icon" title="Collapse all" onclick={collapseAll}><Icon key="collapseall" size={toolSize} glyph /></button>
  <span class="gap"></span>
  <button class="icon" title={descending ? "Sorted Z to A: click for A to Z" : "Sorted A to Z: click for Z to A (until SuperXTerminal restarts)"}
    onclick={() => (descending = !descending)}><Icon key={descending ? "sortza" : "sortaz"} size={toolSize} glyph /></button>
</div>

<div class="tree" role="tree" style:font-size="{settings.treeFontSize}px">
  {#if error}
    <p class="message">{error}</p>
  {/if}
  {#if loaded}
    {#each loaded.problems as problem}
      <p class="message">Skipped: {problem}</p>
    {/each}
    {#if favorites}
      <TreeFolder node={favorites} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
        onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if recent}
      <TreeFolder node={recent} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
        onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if loaded.savedOff}
      <!-- Saved sessions turned off (Settings > General): only Private and Database sessions -->
    {:else if loaded.savedFile === null}
      <p class="message">Choose the settings folder: the folder that holds the team's Sessions.xml.</p>
      <button onclick={choose}>Choose Settings Folder...</button>
    {:else if saved}
      <TreeFolder node={saved} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
        onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if privateRoot}
      <TreeFolder node={privateRoot} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
        onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if databaseRoot}
      <div class="db-root" title={db?.error ? `Showing the last read: ${db.error}` : `${db?.source}${db?.writable ? "" : " (read only)"}`}>
        <TreeFolder node={databaseRoot} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
          onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
      </div>
    {/if}
  {/if}
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

{#if editing}
  <EditSession store={editing.store} id={editing.id} fields={editing.fields} folders={foldersAlong(editing.store, editing.fields.folder)}
    freeName={(store, folder, name, except) => freeName(store, folder, name, except)} copyOf={editing.copyOf ?? null}
    {iconSize} {customs} {onOpenProfiles} lockedFields={editing.lockedFields ?? ""} onCancel={() => (editing = null)}
    onSaved={async (id) => { const store = editing?.store; editing = null; await reload(); selected = `${store}:${id}`; }} />
{/if}

{#if bulk}
  <BulkEdit title={bulk.title} sessions={bulk.sessions} {iconSize} {customs} onCancel={() => (bulk = null)}
    onDone={async (done, problems) => {
      bulk = null;
      await reload();
      context.status(`Edited ${done} session${done === 1 ? "" : "s"}`);
      error = problems.length ? `Left alone: ${problems.join(" ")}` : "";
    }} />
{/if}

{#if connecting}
  <ConnectSome title={connecting.title} sessions={connecting.sessions} {iconSize} {customs} environment={(s) => colors.forSession(s)}
    onCancel={() => (connecting = null)} onConnect={(list) => { connecting = null; list.forEach(open); }} />
{/if}

{#if importing}
  <ImportSessions kind={importing} savedAvailable={savedInUse()} onCancel={() => (importing = null)}
    onDone={async (text) => { importing = null; await reload(); context.status(text); }} />
{/if}

{#if overriding}
  <Overrides sessions={overriding} onCancel={() => (overriding = null)}
    onSaved={async () => { overriding = null; overrides = await loadOverrides(); }} />
{/if}

{#if folderSettings}
  <FolderSettings store={folderSettings.store} setting={folderSettings.setting} onCancel={() => (folderSettings = null)}
    onSaved={async () => { folderSettings = null; await reload(); }} />
{/if}

{#if showPending}
  <PendingChanges onChanged={async (state) => { db = state; loaded = await loadSessions(); }} onClose={() => (showPending = false)} />
{/if}

{#if prompt}
  <Prompt title={prompt.title} value={prompt.value} validate={prompt.validate} onOk={prompt.onOk} onCancel={() => (prompt = null)} />
{/if}

<svelte:window onclick={() => (menu = null)} />

<style>
  .search {
    display: flex;
    gap: 2px;
    margin: 0 8px 4px;
  }

  .search input {
    flex: 1;
    min-width: 0;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 2px;
    padding: 2px 4px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 8px 2px;
  }

  .toolbar .gap {
    width: 6px;
  }

  button.icon {
    display: flex;
    margin: 0;
    padding: 1px;
    background: none;
    border: 1px solid transparent;
    border-radius: 3px;
  }

  button.icon:hover {
    border-color: var(--border);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .tree {
    flex: 1;
    overflow: auto;
    padding: 4px;
  }

  .message {
    color: var(--text-muted);
    margin: 4px;
    -webkit-user-select: text;
    user-select: text;
  }

  button {
    margin: 4px;
  }
</style>
