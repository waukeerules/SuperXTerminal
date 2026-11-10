<!--
  The Sessions pane, as in SuperXPuTTY: the search box, the toolbar (new session, new folder, expand all, collapse
  all, sort), and the tree: Favorites and Recent (the user's own shortcuts), Saved sessions from Sessions.xml,
  Private sessions from PrivateSessions.xml and Database sessions from the team's table. A search opens every
  folder holding a match; clearing it puts the tree back as it was. A folder made during a search shows among the
  matches, so what's found can be dragged into it.
-->
<script lang="ts">
  import { MOD } from "../platform";
  import type { SessionStates } from "../layout/TabArea";
  import { tick, untrack } from "svelte";
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
    folderOverridesSwitch,
    overridesInUse,
    saveOverrides,
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
  import SessionsTable from "./SessionsTable.svelte";
  import Overrides from "./Overrides.svelte";
  import ImportSessions from "./ImportSessions.svelte";
  import ConnectSome from "./ConnectSome.svelte";
  import FolderSettings from "./FolderSettings.svelte";
  import PendingChanges from "./PendingChanges.svelte";
  import Prompt from "../ui/Prompt.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { PALETTE } from "../icons/palette";
  import {
    ALL_SEARCH_FIELDS, DEFAULT_SEARCH_FIELDS, SEARCH_FIELDS, buildGroup, buildTree, folderKeys, searchFieldsOf, searchProblem,
    sessionsIn, toggledSearchFields, type Dragged, type FolderNode, type Group, type SearchField,
  } from "./tree";
  import Icon from "../icons/Icon.svelte";
  import { context } from "../commands/sending.svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { loadCustomIcons, type CustomIcon } from "../icons/custom";
  import { EnvironmentColors, parseRules } from "../icons/environment";
  import { chooseSettingsFolder, databaseCheckSeconds, saveSettings, settings } from "../settings/store.svelte";
  import { DATABASE_SAVED, editSessionsExternally, exportCsv, exportXml as exportSessionsXml, type ImportKind } from "./api";
  import { commandLabel } from "../shortcuts";

  interface Props {
    /** Sessions with an open tab, as "store:id": connecting (a yellow dot), connected (green) or dropped (a red cross) */
    openSessions: SessionStates;
    /** Open a session; given its icon key and environment colour for its tab */
    onOpen: (session: SessionView, environment: string | null) => void;
    /** The icon size and custom icons, once read, so tabs can draw icons the same way */
    onLook: (iconSize: number, customs: ReadonlyMap<string, CustomIcon>) => void;
    /** Open Settings at Profiles (from Edit Session) */
    /** Open Settings > Profiles, on the named profile when there is one */
    onOpenProfiles: (name?: string, field?: string) => void;
    /** Open the Database sessions settings (Tools > Database Sessions…), from the Database root's menu */
    onDatabaseSettings?: () => void;
    /** The session selected now (a shortcut in Favorites or Recent counts), or null, for Session Detail */
    onSelected?: (session: SessionView | null) => void;
    /** While searching, the width the matches' names need (null: none, back to the user's width) */
    onWantWidth?: (width: number | null) => void;
    /** Connect … in a Grid: turn View > Grid View on, so the sessions opened are tiled */
    onGridView?: () => void;
  }

  let { openSessions, onOpen, onLook, onOpenProfiles, onDatabaseSettings = () => {}, onSelected = () => {}, onWantWidth = () => {}, onGridView = () => {} }: Props = $props();

  let selected = $state("");

  // Session Detail follows the selection: a session's key is "store:id", or "favorites:store:id" for a shortcut
  $effect(() => {
    const parts = selected.split(":");
    const [store, ...rest] = ["saved", "private", "database"].includes(parts[0]) ? parts : parts.slice(1);
    const id = rest.join(":");
    onSelected(rest[0] === "folder" ? null : (loaded?.sessions.find((s) => s.store === store && s.id === id) ?? null));
  });
  /** Sessions selected together with Cmd+click and Shift+click, as "store:id" (SuperXPuTTY's multiple selection) */
  let multi = $state<ReadonlySet<string>>(new Set());
  /** Where a Shift+click range starts: the session last clicked without Shift */
  let anchor = "";
  /** Edit sessions… open: its title and sessions */
  let bulk = $state<{ title: string; sessions: SessionView[] } | null>(null);
  /** Local overrides… open, for these sessions */
  let overriding = $state<SessionView[] | null>(null);
  /**
   * The tree's own clipboard (Cmd+C, Cmd+V, Copy and Paste in its menus): sessions, or a folder with what's in it.
   * Kept here, never on the Mac's clipboard, so text copied in a terminal or another app isn't replaced.
   */
  let clip = $state<{ store: Store; sessions: SessionView[]; folder: { name: string; path: string } | null } | null>(null);
  /** Connect all with more than ten sessions: the picker's title and sessions */
  let connecting = $state<{ title: string; sessions: SessionView[]; grid: boolean } | null>(null);
  /** File > Import Sessions open: which kind of file */
  let importing = $state<ImportKind | null>(null);
  /** The user's local overrides, by session id */
  let overrides = $state<Record<string, Override>>({});

  /** Turn a session's own overrides on or off, keeping their values */
  async function toggleOverrides(id: string): Promise<void> {
    const current = overrides[id];
    if (!current) return;
    overrides = await saveOverrides([[id, { ...current, Disabled: !current.Disabled }]]);
  }
  /**
   * A folder's Use Local Overrides ✓, as a session's: ticked while every session in it (and the folders below) that
   * has overrides uses them; a click turns them all off, or all on, keeping their values. None when no session there
   * has any.
   */
  function folderOverridesItem(folder: FolderNode): MenuItem[] {
    const toggle = folderOverridesSwitch(sessionsIn(folder).map((s) => s.id), overrides);
    if (!toggle) return [];
    return [{
      label: "Use Local Overrides",
      checked: toggle.checked,
      disabled: !settings.useLocalOverrides && "Tools > Use Local Overrides is off",
      action: () => void saveOverrides(toggle.changes).then((all) => (overrides = all)),
    }];
  }
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** The session being edited: its store, its id (null for new) and its fields */
  /** Edit Session open; `copyOf` is the session a new one was filled in from, whose password it keeps */
  let editing = $state<{ store: Store; id: string | null; fields: SessionFields; lockedFields?: string; copyOf?: [Store, string] } | null>(null);
  /** Whether File > Edit Sessions as Table… is open */
  let tableOpen = $state(false);
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

  /** The size of the icons in the tree (Settings > General) */
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
  /** Folders opened during a search that matches too many sessions to open them all (SEARCH_OPEN_LIMIT) */
  let openedInSearch = $state(new Set<string>());
  /** Names Z to A (the toolbar's sort button); not saved, so A to Z returns on restart, as in SuperXPuTTY */
  let descending = $state(false);
  /** Favorites and Recent */
  let shortcuts = $state<Shortcuts>({ Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false });

  /** Each session's environment colour, from its folders' and the rules' colours */
  const colors = $derived(new EnvironmentColors(loaded?.folders ?? [], parseRules(settings.environmentColorRules)));
  /** The sessions as shown: a local override's note in place of the stored one, in the tooltip and the search */
  const shown = $derived(
    (loaded?.sessions ?? []).map((s) =>
      overridesInUse(overrides[s.id], settings.useLocalOverrides) && overrides[s.id].Note !== undefined
        ? { ...s, note: overrides[s.id].Note ?? "" }
        : s,
    ),
  );
  /** The search's Aa and .* (kept in the settings), and why a regular expression can't be used, if it can't */
  /** Where the search looks (Search in…): the user's fields, else the defaults */
  const searchFields = $derived<SearchField[]>(searchFieldsOf(settings.treeSearchFields));
  /** The search as set: Match case, Regex and Search in… */
  const searchOptions = $derived({ caseSensitive: settings.treeSearchCase, regex: settings.treeSearchRegex, fields: searchFields });
  /** Search in…'s list is open */
  let searchingIn = $state(false);
  /** How Search in… differs from the defaults, for its button: "+2", "-1", or "" when it doesn't */
  const searchInBadge = $derived.by(() => {
    const added = searchFields.filter((f) => !DEFAULT_SEARCH_FIELDS.includes(f)).length;
    const dropped = DEFAULT_SEARCH_FIELDS.filter((f) => !searchFields.includes(f)).length;
    return [added ? `+${added}` : "", dropped ? `-${dropped}` : ""].filter(Boolean).join(" ");
  });

  /** Tick or untick a field Search in… looks in (or all of them); the last one stays */
  function toggleSearchField(field: SearchField | "all"): void {
    void saveSettings({ treeSearchFields: toggledSearchFields(searchFields, field) });
  }
  /** Why the search can't be run (a bad regex), shown under it */
  const searchError = $derived(searchProblem(query, searchOptions));
  /**
   * A root as the tree shows it: none while a search leaves it no matches, rather than "Saved sessions (0)", which read
   * as though the root itself were empty; kept, though, while it holds a folder just made (New Folder during a search)
   */
  function unlessEmptySearch(root: FolderNode | null): FolderNode | null {
    return root && query.trim() !== "" && root.count === 0 && root.folders.length === 0 ? null : root;
  }
  /** The Saved sessions root, as the search leaves it */
  const saved = $derived(unlessEmptySearch(loaded ? buildTree("saved", "Saved sessions", shown, query, newFolders.saved, descending, searchOptions) : null));
  /** The Private sessions root, as the search leaves it */
  const privateRoot = $derived(
    unlessEmptySearch(loaded && !loaded.privateOff ? buildTree("private", "Private sessions", shown, query, newFolders.private, descending, searchOptions) : null),
  );
  /** The Database sessions root, when they're on */
  const databaseRoot = $derived(
    unlessEmptySearch(
      loaded && db?.enabled ? buildTree("database", dbLabel(db, dbLoading), shown, query, newFolders.database, descending, searchOptions) : null,
    ),
  );
  /** Favorites, as the search leaves it */
  const favorites = $derived(unlessEmptySearch(loaded ? buildGroup("favorites", "Favorites", shortcuts.Favorites, shown, query, searchOptions) : null));
  /** Recent, as the search leaves it */
  const recent = $derived(unlessEmptySearch(loaded ? buildGroup("recent", "Recent", shortcuts.Recent, shown, query, searchOptions) : null));
  /** Every root, in the tree's order */
  const roots = $derived([favorites, recent, saved, privateRoot, databaseRoot].filter((r): r is FolderNode => r !== null));
  /** Whether there's a search */
  const searching = $derived(query.trim() !== "");
  /** The roots as shown, for the tree lines (the first's line starts at it, the last's stops at it) */
  const shownRoots = $derived(
    [favorites, recent, loaded && !loaded.savedOff && loaded.savedFile !== null ? saved : null, privateRoot, databaseRoot].filter((r) => r !== null),
  );
  /** Where a root is among those shown: first or last, for its tree line */
  const rootPlace = (node: FolderNode) => ({ first: shownRoots[0] === node, last: shownRoots.at(-1) === node });

  let treeElement = $state<HTMLElement | undefined>();
  // while searching, the width the matches' names need, for App to widen the pane (Settings > General)
  $effect(() => {
    void shownExpanded;
    void roots;
    if (!searching || !settings.searchWidensPane) {
      untrack(() => onWantWidth(null));
      return;
    }
    requestAnimationFrame(() => {
      if (!treeElement) return;
      let need = 0;
      for (const label of treeElement.querySelectorAll<HTMLElement>(".row .label")) {
        need = Math.max(need, label.offsetLeft + label.scrollWidth);
      }
      // the tree's padding, the scroll bar and a little room
      onWantWidth(need ? need + 34 : null);
    });
  });
  /**
   * A search matching more sessions than this opens only the roots: drawing every match (10,000 rows, each with its
   * icon) made typing lag. Folders the user opens then stay open; a narrower search opens all again.
   */
  const SEARCH_OPEN_LIMIT = 1000;
  /** How many sessions the search matches (Favorites and Recent hold the same ones again, so they don't count) */
  const matchCount = $derived([saved, privateRoot, databaseRoot].reduce((n, r) => n + (r?.count ?? 0), 0));
  /** A search matching so many sessions that only the roots are opened, as SuperXPuTTY's */
  const broadSearch = $derived(searching && matchCount > SEARCH_OPEN_LIMIT);
  /**
   * What's open: while searching every folder holding a match (but those closed since), or for a broad search only the
   * roots and what's been opened; else as the user left it
   */
  const shownExpanded = $derived(
    !searching
      ? expanded
      : broadSearch
        ? new Set([...roots.map((r) => r.key), ...openedInSearch])
        : new Set(roots.flatMap(folderKeys).filter((k) => !closedInSearch.has(k))),
  );

  // a new search starts with everything open again
  $effect(() => {
    void query;
    closedInSearch = new Set();
    openedInSearch = new Set();
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

  // read again whenever the settings folder changes, or Saved or Private sessions are turned on or off (Settings >
  // General), so it shows at once; the first run reads them at start
  $effect(() => {
    if (!settings.loaded) return;
    void settings.settingsFolder;
    void settings.useSessionsXml;
    void settings.usePrivateSessions;
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
    if (editing || tableOpen || bulk || overriding || folderSettings || prompt || menu || showPending || dbLoading || db?.error) return;
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

  /** The reload running, and whether another was asked for meanwhile */
  let reloading: Promise<void> | null = null;
  let reloadAgain = false;

  /**
   * Read the settings, custom icons and session files again. Asked for again while one runs (a settings save, a file
   * changing and the Database at once), it waits for that one and one more runs after it, rather than one each:
   * with 10,000 sessions each read sends them all and builds the tree again
   */
  function reload(): Promise<void> {
    if (reloading) {
      reloadAgain = true;
      return reloading;
    }
    reloading = (async () => {
      do {
        reloadAgain = false;
        await reloadOnce();
      } while (reloadAgain);
    })().finally(() => (reloading = null));
    return reloading;
  }

  /** Read the custom icons, sessions, shortcuts and overrides again, once */
  async function reloadOnce(): Promise<void> {
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
      icon: "computer", highlightRules: "", extraArgs: "", jumpHost: "", script: "", note: "", localPath: "", remotePath: "", rdpOptions: "", sshAlgorithms: "", buttonBar: "",
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
        { label: "Show in Sessions tree", action: () => void showInTree(session) },
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
    // a selection of several with folders in it: what works on folders and sessions together
    const targetKey = "session" in target ? `${target.session.store}:${target.session.id}` : `${target.folder.store}:folder:${target.folder.path}`;
    if (multi.size > 1 && multi.has(targetKey) && [...multi].some(isFolderKey)) {
      const { store, folders, sessions } = selectionItems(multi);
      const all = [...sessions, ...(loaded?.sessions ?? []).filter((s) => s.store === store && folders.some((f) => folderUnder(s.folder, f) !== null))];
      const runnable = all.filter((s) => unavailableReason(s.protocol) === null);
      const n = folders.length + sessions.length;
      const block = dbBlock(store, all.some((s) => s.locked));
      items = [
        { label: `Connect all (${runnable.length})`, bold: true, disabled: runnable.length === 0, action: () => connectMany("from the selection", runnable) },
        { label: "Connect all in a Grid", disabled: runnable.length === 0, action: () => connectMany("from the selection", runnable, true) },
        { separator: true },
        { label: `Move ${n} items to folder…`, disabled: block, action: () => askMoveItems(store, folders, sessions) },
        { label: `Delete ${n} items…`, disabled: block, action: () => void removeItems(store, folders, sessions) },
        { label: `Export ${all.length} sessions to CSV…`, disabled: all.length === 0 && "No sessions here", action: () => void exportSessions(all, "Sessions.csv") },
      ];
      menu = { x: event.clientX, y: event.clientY, items };
      return;
    }
    if ("session" in target && multi.size > 1 && multi.has(`${target.session.store}:${target.session.id}`)) {
      const list = selectedSessions();
      const runnable = list.filter((s) => unavailableReason(s.protocol) === null);
      const stores = new Set(list.map((s) => s.store));
      const [only] = stores;
      items = [
        { label: `Connect ${runnable.length} sessions`, bold: true, disabled: runnable.length === 0, action: () => connectMany("from the selection", runnable) },
        { label: `Connect ${runnable.length} sessions in a Grid`, disabled: runnable.length === 0, action: () => connectMany("from the selection", runnable, true) },
        { separator: true },
        { label: `Edit ${list.length} sessions…`, action: () => void editSessions("for the selection", list) },
        { label: `Local overrides for ${list.length} sessions…`, action: () => (overriding = list) },
        { label: `Delete ${list.length} sessions`, action: () => void removeSessions(list) },
        { label: `Export ${list.length} sessions to CSV…`, action: () => void exportSessions(list, "Sessions.csv") },
        { label: `Copy ${list.length} sessions  ${commandLabel("C")}`, disabled: stores.size > 1 && "The selection is in more than one root", action: () => copySelection() },
        ...(stores.size === 1
          ? [
              { label: `Move ${list.length} sessions to folder…`, disabled: dbBlock(only, list.some((s) => s.locked)), action: () => askMoveItems(only, [], list) },
              ...transferItems(list, only, list.some((s) => s.locked)),
            ]
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
          disabled: !["SSH", "RDP", "VNC"].includes((session.protocol || "SSH").toUpperCase()) && "Only SSH (in Terminal), RDP and VNC sessions open in another app",
          // SSH in Terminal; RDP in Windows App, VNC in Screen Sharing
          action: () => void external(session, (session.protocol || "SSH").toUpperCase() === "SSH" ? "terminal" : "open"),
        },
        {
          label: "Connect in new window",
          disabled: unavailableReason(session.protocol) ?? false,
          action: () => void invoke("window_new", { open: `${session.store}:${session.id}` }).catch((e) => (error = String(e))),
        },
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
        { label: `Copy  ${commandLabel("C")}`, action: () => copySelection() },
        { label: "Duplicate as…", disabled: session.store === "database" && !db?.writable && "Database sessions can't be written now", action: () => void duplicateAs(session) },
        { label: "Edit", disabled: block, action: () => void edit(session) },
        { label: "Local overrides…", action: () => (overriding = [session]) },
        // switch between the session as stored (CyberArk, say) and the user's own values, keeping them either way
        ...(overrides[session.id]
          ? [{
              label: "Use Local Overrides",
              checked: !overrides[session.id].Disabled,
              disabled: !settings.useLocalOverrides && "Tools > Use Local Overrides is off",
              action: () => void toggleOverrides(session.id),
            }]
          : []),
        { label: "Delete", disabled: block, action: () => void remove(session) },
        ...transferItems([session], session.store, session.locked),
        { separator: true },
        {
          label: "File transfer (SFTP)",
          disabled: (session.protocol || "SSH").toUpperCase() !== "SSH" && "File transfer is for SSH sessions",
          action: () => context.tabs?.openSftp(session.store, session.id, session.name, session.icon, colors.forSession(session)),
        },
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
              // as SuperXPuTTY's root menu: the connection, table and read-only, as Tools > Database Sessions…
              { label: "Database settings…", action: () => onDatabaseSettings() },
              ...(db && db.pending + db.failed > 0
                ? [{ label: `Pending changes (${db.pending + db.failed})…`, action: () => (showPending = true) }]
                : []),
              { separator: true } as MenuItem,
            ]
          : []),
        { label: "New session…", disabled: canAdd, action: () => (editing = { store: folder.store, id: null, fields: blank(folder.path) }) },
        { label: "New folder…", disabled: canAdd, action: () => newFolder(folder) },
        ...(isRoot ? [] : [{ label: `Copy  ${commandLabel("C")}`, action: () => copySelection() } as MenuItem]),
        {
          label: clip ? `Paste ${clip.folder ? `folder ${clip.folder.name}` : clip.sessions.length === 1 ? clip.sessions[0].name : `${clip.sessions.length} sessions`}  ${commandLabel("V")}` : `Paste  ${commandLabel("V")}`,
          disabled: !clip ? `Copy a session or folder first (${commandLabel("C")})` : canAdd,
          action: () => void pasteHere(),
        },
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
              // every session in it and the folders below, as Local overrides for N sessions…
              { label: "Local overrides…", disabled: sessionsIn(folder).length === 0 && "No sessions here", action: () => (overriding = sessionsIn(folder)) },
              ...folderOverridesItem(folder),
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
        { label: "Connect all in a Grid", disabled: runnable.length === 0, action: () => connectMany(`in ${folder.name}`, runnable, true) },
        { label: "Expand all", action: () => (expanded = new Set([...expanded, ...folderKeys(folder)])) },
        { label: "Collapse all", action: () => (expanded = new Set([...expanded].filter((k) => !folderKeys(folder).slice(1).includes(k)))) },
      ];
    }
    menu = { x: event.clientX, y: event.clientY, items };
  }

  /** A click on a session or folder: Cmd+click adds or removes it from the selection, Shift+click selects the
   * sessions and folders shown between the last click and this one, anything else selects just it (SuperXPuTTY's
   * BeforeSelectMulti, with Cmd for Ctrl). A selection is in one root: a click in another starts a new one. A
   * right-click (no event) keeps the selection when it's on a selected one. */
  function select(key: string, event?: MouseEvent): void {
    const pickable = isSessionKey(key) || isFolderKey(key);
    if (!event) {
      if (!multi.has(key)) multi = new Set();
      selected = key;
      return;
    }
    const sameRoot = (a: string) => a.split(":")[0] === key.split(":")[0];
    if (pickable && (event.metaKey || event.ctrlKey)) {
      const start = multi.size ? [...multi] : isSessionKey(selected) || isFolderKey(selected) ? [selected] : [];
      const next = new Set(start.filter(sameRoot));
      if (next.has(key)) next.delete(key);
      else next.add(key);
      multi = next;
      selected = key;
      anchor = key;
      return;
    }
    if (pickable && event.shiftKey && anchor && sameRoot(anchor)) {
      const order = visibleRowKeys();
      const [a, b] = [order.indexOf(anchor), order.indexOf(key)];
      if (a >= 0 && b >= 0) {
        multi = new Set(order.slice(Math.min(a, b), Math.max(a, b) + 1));
        selected = key;
        return;
      }
    }
    multi = new Set();
    selected = key;
    if (pickable) anchor = key;
  }

  /** Whether a selection key is a session in its own root (not a folder, nor a Favorites or Recent entry) */
  function isSessionKey(key: string): boolean {
    const [store, second] = key.split(":");
    return ["saved", "private", "database"].includes(store) && second !== "folder" && key.length > store.length + 1;
  }

  /** Whether a selection key is a folder in a root (not a root itself, nor Favorites or Recent) */
  function isFolderKey(key: string): boolean {
    const match = /^(saved|private|database):folder:(.+)$/.exec(key);
    return !!match && match[2] !== "";
  }

  /** The folders and sessions shown in the roots, top to bottom as drawn, for Shift+click */
  function visibleRowKeys(): string[] {
    const out: string[] = [];
    const walk = (node: FolderNode) => {
      if (node.path) out.push(`${node.store}:folder:${node.path}`);
      if (!shownExpanded.has(node.key)) return;
      node.folders.forEach(walk);
      for (const s of node.sessions) out.push(`${s.store}:${s.id}`);
    };
    [saved, privateRoot, databaseRoot].forEach((root) => root && walk(root));
    return out;
  }

  /**
   * What a selection holds, in its root: the folders (only the outermost: one inside another goes with it) and the
   * sessions not inside any of them, as the tree has them now
   */
  function selectionItems(keys: Iterable<string>): { store: Store; folders: string[]; sessions: SessionView[] } {
    const list = [...keys];
    const store = (list[0]?.split(":")[0] ?? "saved") as Store;
    const paths = list.filter(isFolderKey).map((k) => k.slice(`${store}:folder:`.length));
    const folders = paths.filter((p) => !paths.some((o) => o !== p && folderUnder(p, o) !== null));
    const inside = (folder: string) => folders.some((f) => folderUnder(folder, f) !== null);
    const sessions = (loaded?.sessions ?? []).filter((s) => s.store === store && list.includes(`${s.store}:${s.id}`) && !inside(s.folder));
    return { store, folders, sessions };
  }

  /**
   * Move a selection's folders and sessions into a folder of the same root: each folder with all below it, each
   * session on its own; a session whose name is taken there gets a free one ("sw1-1"), as a single move offers. A
   * folder isn't moved into itself. One read of the sessions after.
   */
  async function moveItems(store: Store, folders: string[], sessions: SessionView[], to: string): Promise<void> {
    const target = to.replace(/^\/+|\/+$/g, "");
    const join = (a: string, b: string) => (a ? `${a}/${b}` : b);
    const problems: string[] = [];
    let moved = 0;
    for (const from of folders) {
      const dest = join(target, from.slice(from.lastIndexOf("/") + 1));
      if (folderUnder(target, from) !== null || dest.toLowerCase() === from.toLowerCase()) continue;
      try {
        await moveFolder(store, from, dest);
        const shift = (p: string) => (p === from || p.startsWith(from + "/") ? dest + p.slice(from.length) : p);
        newFolders = { ...newFolders, [store]: newFolders[store].map(shift) };
        expanded = new Set([...expanded].map((k) => (k.startsWith(`${store}:`) ? `${store}:${shift(k.slice(store.length + 1))}` : k)));
        moved += 1;
      } catch (e) {
        problems.push(`${from}: ${e}`);
      }
    }
    for (const s of sessions) {
      if (s.folder.toLowerCase() === target.toLowerCase()) continue;
      try {
        const name = takenId(store, target, s.name, s.id) ? freeName(store, target, s.name, s.id) : null;
        await moveSession(store, s.id, target, name);
        moved += 1;
      } catch (e) {
        problems.push(`${s.name}: ${e}`);
      }
    }
    newFolders = { ...newFolders, [store]: newFolders[store].filter((p) => p !== target) };
    multi = new Set();
    await reload();
    expanded = new Set([...expanded, `${store}:${target}`]);
    error = problems.length ? `Not moved: ${problems.join("; ")}` : "";
    context.status(`Moved ${moved} item${moved === 1 ? "" : "s"} to ${target || "the top"}`);
  }

  /** Delete a selection's folders (with all in them) and sessions, after one question */
  async function removeItems(store: Store, folders: string[], sessions: SessionView[]): Promise<void> {
    const inFolders = (loaded?.sessions ?? []).filter((s) => s.store === store && folders.some((f) => folderUnder(s.folder, f) !== null)).length;
    const what = [
      folders.length ? `${folders.length} folder${folders.length === 1 ? "" : "s"} (${inFolders} session${inFolders === 1 ? "" : "s"} in them)` : "",
      sessions.length ? `${sessions.length} session${sessions.length === 1 ? "" : "s"}` : "",
    ].filter(Boolean).join(" and ");
    if (!(await ask(`Delete ${what}?`, { title: "Delete", kind: "warning", okLabel: "Delete" }))) return;
    const problems: string[] = [];
    for (const f of folders) {
      try {
        await deleteFolder(store, f);
        newFolders = { ...newFolders, [store]: newFolders[store].filter((p) => folderUnder(p, f) === null) };
      } catch (e) {
        problems.push(`${f}: ${e}`);
      }
    }
    for (const s of sessions) {
      try {
        await deleteSession(s.store, s.id);
      } catch (e) {
        problems.push(`${s.name}: ${e}`);
      }
    }
    multi = new Set();
    await reload();
    error = problems.length ? `Not deleted: ${problems.join("; ")}` : "";
  }

  /** Move to folder…: ask for a folder path in the selection's root, then move it all there */
  function askMoveItems(store: Store, folders: string[], sessions: SessionView[]): void {
    const first = folders[0]?.includes("/") ? folders[0].slice(0, folders[0].lastIndexOf("/")) : folders.length ? "" : (sessions[0]?.folder ?? "");
    prompt = {
      title: `Move ${folders.length + sessions.length} items to folder (e.g. Lab/Core; empty for the top)`,
      value: first,
      validate: (path) => (/[<>|\\]/.test(path) ? "A folder path can't have < > | or \\" : ""),
      onOk: (path) => {
        prompt = null;
        void moveItems(store, folders, sessions, path);
      },
    };
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

  /**
   * Select a session in its own root, opening the folders above it, and scroll it into view (SuperXPuTTY's Show in
   * Sessions tree): it's often far below Recent or Favorites, where it was right-clicked
   */
  async function showInTree(session: SessionView): Promise<void> {
    query = "";
    const keys = [`${session.store}:`];
    let path = "";
    for (const part of session.folder.split("/").filter(Boolean)) {
      path = path ? `${path}/${part}` : part;
      keys.push(`${session.store}:${path}`);
    }
    expanded = new Set([...expanded, ...keys]);
    selected = `${session.store}:${session.id}`;
    // once the opened folders have drawn its row
    await tick();
    const key = selected;
    // compared rather than put in a selector: a session's id can hold quotes
    [...(treeElement?.querySelectorAll<HTMLElement>(".row.session") ?? [])].find((r) => r.dataset.key === key)?.scrollIntoView({ block: "nearest" });
  }

  /** The roots' names, for Move to / Copy to */
  const ROOT_NAMES: Record<Store, string> = { saved: "Saved sessions", private: "Private sessions", database: "Database sessions" };

  /** Move to and Copy to, each listing the other roots; the table only when it can be written, and no move of locked rows */
  function transferItems(sessions: SessionView[], from: Store, locked: boolean): MenuItem[] {
    const targets: Store[] = (["saved", "private", "database"] as Store[]).filter(
      (t) => t !== from && (t !== "database" || db?.enabled) && (t !== "saved" || (settings.settingsFolder && settings.useSessionsXml))
        && (t !== "private" || !loaded?.privateOff),
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

  /**
   * Why a new folder name can't be used in a folder, or "". During a search the folder shows only its folders holding
   * a match, so the name is checked against all of them, from the tree as it is unsearched
   */
  function folderNameProblem(parent: FolderNode, name: string, except = ""): string {
    if (!name) return "Empty folder name";
    if (name.includes("/")) return 'A name can\'t contain "/"';
    const all = searching ? folderAt(buildTree(parent.store, "", shown, "", newFolders[parent.store]), parent.path) ?? parent : parent;
    if (name !== except && all.folders.some((f) => f.name.toLowerCase() === name.toLowerCase())) return "A folder with that name is already here";
    return "";
  }

  /** The folder at a path in a tree, or null */
  function folderAt(node: FolderNode, path: string): FolderNode | null {
    return node.path === path ? node : node.folders.map((f) => folderAt(f, path)).find(Boolean) ?? null;
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
        // and open where it is in a search's own view, so it shows among the matches to drag into it
        if (searching) {
          closedInSearch = new Set([...closedInSearch].filter((k) => k !== parent.key));
          openedInSearch = new Set([...openedInSearch, parent.key]);
        }
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
    if (dragged.kind === "many") {
      const { store, folders, sessions } = selectionItems(dragged.keys);
      if (store === target.store) {
        if (target.store === "database" && !db?.writable) {
          error = "Database sessions can't be written now.";
          return;
        }
        await moveItems(store, folders, sessions, target.path);
        return;
      }
      // into another root: a copy, as one dragged there is, each folder with all in it under the folder dropped on
      if (target.store === "database" && !db?.writable) {
        error = "Database sessions can't be written now.";
        return;
      }
      const join = (a: string, b: string) => (a && b ? `${a}/${b}` : a || b);
      const all = loaded?.sessions ?? [];
      const list: SessionView[] = [...sessions];
      const into: string[] = sessions.map(() => target.path);
      for (const f of folders) {
        const name = f.slice(f.lastIndexOf("/") + 1);
        for (const s of all.filter((s) => s.store === store && folderUnder(s.folder, f) !== null)) {
          list.push(s);
          into.push(join(join(target.path, name), (folderUnder(s.folder, f) ?? "").replace(/^\//, "")));
        }
      }
      if (list.length) await transfer(list, store, target.store, false, into);
      return;
    }
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
  async function external(session: SessionView, how: "terminal" | "filezilla" | "open"): Promise<void> {
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

  /** The folder node with this path in a root, or null */
  function folderNode(store: Store, path: string): FolderNode | null {
    const root = store === "saved" ? saved : store === "private" ? privateRoot : databaseRoot;
    const find = (node: FolderNode | null): FolderNode | null =>
      !node ? null : node.path.toLowerCase() === path.toLowerCase() ? node : node.folders.map(find).find(Boolean) ?? null;
    return find(root);
  }

  /** What Cmd+C and Copy act on: the selected sessions, or the selected session or folder */
  function copyTarget(): { store: Store; sessions: SessionView[]; folder: FolderNode | null } | null {
    if (multi.size > 1) {
      const list = selectedSessions();
      const stores = new Set(list.map((s) => s.store));
      return stores.size === 1 ? { store: list[0].store, sessions: list, folder: null } : null;
    }
    const [store, ...rest] = selected.split(":");
    if (!["saved", "private", "database"].includes(store)) {
      // a shortcut in Favorites or Recent: the session it stands for
      const id = rest.slice(1).join(":");
      const session = (loaded?.sessions ?? []).find((s) => s.store === rest[0] && s.id === id);
      return session ? { store: session.store, sessions: [session], folder: null } : null;
    }
    if (rest[0] === "folder") {
      const node = folderNode(store as Store, rest.slice(1).join(":"));
      return node && node.path ? { store: node.store, sessions: sessionsIn(node), folder: node } : null;
    }
    const id = rest.join(":");
    const session = (loaded?.sessions ?? []).find((s) => s.store === store && s.id === id);
    return session ? { store: session.store, sessions: [session], folder: null } : null;
  }

  /** Cmd+C or Copy: the selection onto the tree's own clipboard */
  function copySelection(): void {
    const target = copyTarget();
    if (!target) return;
    clip = { store: target.store, sessions: target.sessions, folder: target.folder && { name: target.folder.name, path: target.folder.path } };
    const n = target.sessions.length;
    context.status(target.folder ? `Copied folder ${target.folder.name} (${n} session${n === 1 ? "" : "s"}); ${MOD}+V pastes it in a folder` : `Copied ${n === 1 ? target.sessions[0].name : `${n} sessions`}; ${MOD}+V pastes in a folder`);
  }

  /**
   * Cmd+V or Paste: copies of what was copied, in the selected folder (or the selected session's). Taken names get -1,
   * -2…, as Finder's copies; a folder pasted where one has its name becomes "name-1". Into Database sessions only while
   * they can be written.
   */
  async function pasteHere(): Promise<void> {
    const target = toolbarFolder();
    if (!clip || !target || target.group) return;
    if (target.store === "database" && !db?.writable) {
      error = "Database sessions can't be written now.";
      return;
    }
    const all = loaded?.sessions ?? [];
    // the sessions as they are now (one may have been renamed or deleted since)
    const list = clip.sessions.map((c) => all.find((s) => s.store === c.store && s.id === c.id)).filter((s): s is SessionView => !!s);
    if (list.length === 0) {
      error = "What was copied is no longer there.";
      return;
    }
    const join = (a: string, b: string) => (a && b ? `${a}/${b}` : a || b);
    let folders: string[];
    if (clip.folder) {
      const copied = clip.folder;
      let name = copied.name;
      for (let i = 1; target.folders.some((f) => f.name.toLowerCase() === name.toLowerCase()); i++) name = `${copied.name}-${i}`;
      const base = join(target.path, name);
      folders = list.map((s) => join(base, (folderUnder(s.folder, copied.path) ?? "").replace(/^\//, "")));
    } else {
      folders = list.map(() => target.path);
    }
    try {
      const report = await transferSessions(clip.store, list.map((s) => s.id), target.store, false, folders, true);
      await reload();
      expanded = new Set([...expanded, target.key, ...folders.map((f) => `${target.store}:${f}`)]);
      error = report.problems.join(" ");
      context.status(`Pasted ${report.done} session${report.done === 1 ? "" : "s"} in ${target.path || ROOT_NAMES[target.store]}`);
    } catch (e) {
      error = String(e);
    }
  }

  /** Cmd+Delete (or Delete): delete the selection, asking first, as SuperXPuTTY's Delete key */
  function deleteSelection(): void {
    const target = copyTarget();
    if (!target || !["saved", "private", "database"].includes(selected.split(":")[0])) return;
    if (target.folder) void removeFolder(target.folder);
    else if (target.sessions.length > 1) void removeSessions(target.sessions);
    else void remove(target.sessions[0]);
  }

  /** The tree's keys, while it has the focus; the search box keeps its own */
  function treeKey(event: KeyboardEvent): void {
    const command = event.metaKey || event.ctrlKey;
    if (command && event.key.toLowerCase() === "c") copySelection();
    else if (command && event.key.toLowerCase() === "v") void pasteHere();
    else if ((command && event.key === "Backspace") || event.key === "Delete") deleteSelection();
    else return;
    event.preventDefault();
    event.stopPropagation();
  }

  /** Connect all: up to ten open at once; more, and the user picks up to ten of them (ConnectSome) */
  function connectMany(title: string, sessions: SessionView[], grid = false): void {
    if (sessions.length > 10) connecting = { title, sessions, grid };
    else connectAll(sessions, grid);
  }

  /**
   * Open these sessions, each in its tab; in a grid (Connect … in a Grid), Grid View goes on first, so they're tiled
   * once they're all open
   */
  function connectAll(sessions: SessionView[], grid: boolean): void {
    if (grid) onGridView();
    sessions.forEach(open);
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

  /** File > Export Sessions…: every session to one Sessions.xml, as SuperXPuTTY's */
  export async function exportXml(): Promise<void> {
    try {
      const text = await exportSessionsXml();
      if (text) context.status(text);
    } catch (e) {
      error = `Couldn't export the sessions: ${e}`;
    }
  }

  /** File > Edit Sessions Externally…: the sessions file in an editor; the tree reads it again once it's saved */
  export async function editExternally(): Promise<void> {
    try {
      context.status(await editSessionsExternally());
    } catch (e) {
      error = `Couldn't open the sessions file: ${e}`;
    }
  }

  /** File > Edit Sessions as Table…: every session in one table, to change many at once (SessionsTable.svelte) */
  export function editAsTable(): void {
    if (loaded) tableOpen = true;
  }

  /** File > Reload Sessions File: read the sessions files again now, as SuperXPuTTY's */
  export async function reloadSessions(): Promise<void> {
    await reload();
    if (!error) context.status("Read the sessions again");
  }

  /** Edit Session for a session (Session Detail's double-click) */
  export function editSession(session: SessionView): void {
    void edit(session);
  }

  /** Open a session as the tree does (its colour on the tab, Recent), for the Quick Selector */
  export function openSession(session: SessionView): void {
    open(session);
  }

  /** A session's environment colour, for the Quick Selector's icons */
  export function environmentOf(session: SessionView): string | null {
    return colors.forSession(session);
  }

  /** A session by its "store:id", for its own button bar; undefined when there's none (gone, or a Quick connect) */
  export function sessionByKey(key: string): SessionView | undefined {
    return loaded?.sessions.find((s) => `${s.store}:${s.id}` === key);
  }

  /** Every session, for the tab menu's New session and Copy host address */
  export function allSessions(): SessionView[] {
    return loaded?.sessions ?? [];
  }

  /** The tab menu's Save session as…: Edit Session for a new saved session, from a tab's session, named as the tab */
  export async function saveSessionAs(store: Store, id: string, title: string): Promise<void> {
    try {
      const fields = await getSession(store, id);
      editing = { store: savedInUse() || loaded?.privateOff ? "saved" : "private", id: null, fields: { ...fields, name: title }, copyOf: [store, id] };
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
      const next = new Set(broadSearch ? openedInSearch : closedInSearch);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      if (broadSearch) openedInSearch = next;
      else closedInSearch = next;
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
    const fallback = savedInUse() || !privateRoot ? saved : privateRoot;
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
  /**
   * Expand all: every folder. During a search it opens every folder holding a match, the search kept, past the limit
   * for a broad one too (asked for, so drawn); clearing the search puts the tree back as it was
   */
  function expandAll(): void {
    if (searching) {
      closedInSearch = new Set();
      openedInSearch = new Set(roots.flatMap(folderKeys));
      // a broad search only opens what's in openedInSearch; a narrow one everything not in closedInSearch
      return;
    }
    expanded = new Set(roots.flatMap(folderKeys));
  }

  /**
   * Collapse all, leaving Saved sessions and Private sessions open so their top-level folders show. During a search,
   * every folder below the roots closes in the search's own view, the search kept
   */
  function collapseAll(): void {
    if (searching) {
      const rootKeys = new Set(roots.map((r) => r.key));
      openedInSearch = new Set();
      closedInSearch = new Set(roots.flatMap(folderKeys).filter((k) => !rootKeys.has(k)));
      return;
    }
    expanded = new Set(["saved:", "private:"]);
  }

</script>

<div class="search">
  <span class="field" class:bad={!!searchError}>
    <input placeholder="Type to search..." bind:value={query} aria-label="Search sessions" title={searchError}
      onkeydown={(e) => { if (e.key === "Escape") query = ""; }} />
    <!-- as the terminal's find bar: match case, and a regular expression -->
    <button class="toggle" aria-pressed={settings.treeSearchCase} title="Match case"
      onclick={() => void saveSettings({ treeSearchCase: !settings.treeSearchCase })}>Aa</button>
    <button class="toggle" aria-pressed={settings.treeSearchRegex} title={searchError ? `Not a regular expression: ${searchError}` : "Regular expression"}
      onclick={() => void saveSettings({ treeSearchRegex: !settings.treeSearchRegex })}>.*</button>
    <button class="toggle" aria-haspopup="true" aria-expanded={searchingIn} aria-pressed={!!searchInBadge}
      title={`Search in: ${searchFields.map((f) => SEARCH_FIELDS.find(([k]) => k === f)?.[1]).join(", ")}`}
      onclick={(e) => { e.stopPropagation(); searchingIn = !searchingIn; }}>⋯{searchInBadge ? ` ${searchInBadge}` : ""}</button>
  </span>
  {#if searchingIn}
    <!-- a click inside it mustn't reach the window, which closes it -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <div class="search-in" role="group" aria-label="Search in" tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <div class="search-in-title">Search in</div>
      {#each SEARCH_FIELDS as [field, label] (field)}
        <label><input type="checkbox" checked={searchFields.includes(field)}
          disabled={searchFields.includes(field) && searchFields.length === 1}
          onchange={() => toggleSearchField(field)} /> {label}</label>
      {/each}
      <span class="search-in-buttons">
        <button onclick={() => toggleSearchField("all")} disabled={searchFields.length === ALL_SEARCH_FIELDS.length}>All fields</button>
        <button onclick={() => void saveSettings({ treeSearchFields: [] })} disabled={!searchInBadge}>Defaults</button>
      </span>
    </div>
  {/if}
  <button class="icon" title="Search (the tree filters as you type)" onclick={() => (query = query.trim())}>
    <Icon key="search" size={toolSize} glyph />
  </button>
  <button class="icon" title="Clear the search" onclick={() => (query = "")}><Icon key="clear" size={toolSize} glyph /></button>
</div>
<div class="toolbar" role="toolbar" aria-label="Sessions">
  <button class="icon" title="New session (in the selected folder)" onclick={newSessionHere}><Icon key="newsession" size={toolSize} glyph /></button>
  <button class="icon" title="New folder (in the selected folder)" onclick={newFolderHere}><Icon key="newfolder" size={toolSize} glyph /></button>
  <span class="gap" role="separator" aria-orientation="vertical"></span>
  <button class="icon" title="Expand all" onclick={expandAll}><Icon key="expandall" size={toolSize} glyph /></button>
  <button class="icon" title="Collapse all" onclick={collapseAll}><Icon key="collapseall" size={toolSize} glyph /></button>
  <span class="gap" role="separator" aria-orientation="vertical"></span>
  <button class="icon" title={descending ? "Sorted Z to A: click for A to Z" : "Sorted A to Z: click for Z to A (until SuperXTerminal restarts)"}
    onclick={() => (descending = !descending)}><Icon key={descending ? "sortza" : "sortaz"} size={toolSize} glyph /></button>
</div>

<!-- the tree's own copy and paste (Cmd+C, Cmd+V), not the Mac's clipboard; the menu bar's Copy and Paste reach it as
     copy and paste events, which are stopped here so nothing is written to the clipboard -->
<div class="tree" role="tree" tabindex="-1" bind:this={treeElement} style:font-size="{settings.treeFontSize}px" onkeydown={treeKey}
  oncopy={(e) => { e.preventDefault(); copySelection(); }} onpaste={(e) => { e.preventDefault(); void pasteHere(); }}>
  {#if error}
    <p class="message">{error}</p>
  {/if}
  {#if broadSearch && openedInSearch.size === 0}
    <p class="message">{matchCount.toLocaleString()} matches: open a folder (or Expand all) to see them, or narrow the search.</p>
  {/if}
  {#if loaded}
    {#each loaded.problems as problem}
      <p class="message">Skipped: {problem}</p>
    {/each}
    {#if favorites}
      <TreeFolder node={favorites} depth={0} {...rootPlace(favorites)} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
        onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if recent}
      <TreeFolder node={recent} depth={0} {...rootPlace(recent)} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
        onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if loaded.savedOff}
      <!-- Saved sessions turned off (Settings > General): only Private and Database sessions -->
    {:else if loaded.savedFile === null}
      <p class="message">Choose the settings folder: the folder that holds the team's Sessions.xml.</p>
      <button onclick={choose}>Choose Settings Folder...</button>
    {:else if saved}
      <TreeFolder node={saved} depth={0} {...rootPlace(saved)} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
        onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if privateRoot}
      <TreeFolder node={privateRoot} depth={0} {...rootPlace(privateRoot)} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
        onToggle={toggle} onOpen={open} onSelect={select} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if databaseRoot}
      <div class="db-root" title={db?.error ? `Showing the last read: ${db.error}` : `${db?.source}${db?.writable ? "" : " (read only)"}`}>
        <TreeFolder node={databaseRoot} depth={0} {...rootPlace(databaseRoot)} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
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

{#if tableOpen && loaded}
  <SessionsTable {loaded} {db} {colors} {customs} onClose={() => (tableOpen = false)}
    onSaved={async (report) => { tableOpen = false; await reload(); context.status(report); }} />
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
    onCancel={() => (connecting = null)} onConnect={(list) => { const grid = connecting?.grid ?? false; connecting = null; connectAll(list, grid); }} />
{/if}

{#if importing}
  <ImportSessions kind={importing} savedAvailable={savedInUse()} privateAvailable={!loaded?.privateOff} onCancel={() => (importing = null)}
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

<!-- a click elsewhere closes the menu and Search in…'s list -->
<svelte:window onclick={() => { menu = null; searchingIn = false; }} />

<style>
  .search {
    display: flex;
    gap: 2px;
    margin: 0 8px 4px;
  }

  /* the search box: the text and its Aa and .* buttons in one border */
  .search .field {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 2px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 2px;
  }

  .search .field.bad {
    border-color: #e5534b;
  }

  /* the search text only: Search in…'s checkboxes are inside .search too, and stretched they each sat in the middle of
     what their label left */
  .search .field input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    padding: 2px 4px;
    outline: none;
  }

  .search .toggle {
    flex: none;
    min-width: 22px;
    padding: 0 4px;
    border: 1px solid transparent;
    border-radius: 3px;
    background: transparent;
    color: var(--text-muted);
    font: 12px/18px Menlo, Consolas, monospace;
  }

  .search .toggle:hover {
    border-color: var(--border);
  }

  .search .toggle[aria-pressed="true"] {
    background: var(--accent);
    color: #fff;
  }

  /* Search in…'s fields, under the search box */
  .search {
    position: relative;
  }

  .search-in {
    position: absolute;
    top: calc(100% + 2px);
    right: 52px;
    z-index: 20;
    display: grid;
    gap: 4px;
    padding: 8px 10px;
    min-width: 150px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.25);
  }

  .search-in-title {
    color: var(--text-muted);
    font-size: 12px;
  }

  .search-in label {
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }

  .search-in-buttons {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 8px 2px;
  }

  /* a line between the toolbar's groups (new; expand and collapse; sort), as SuperXPuTTY's ToolStripSeparator */
  .toolbar .gap {
    align-self: stretch;
    width: 1px;
    margin: 4px 7px;
    /* the muted text colour, faded: plain in both themes, where the border colour is faint on light */
    background: var(--text-muted);
    opacity: 0.45;
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
