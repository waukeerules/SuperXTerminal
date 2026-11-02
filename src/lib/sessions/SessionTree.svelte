<!--
  The Sessions pane, as in SuperXPuTTY: the search box, the toolbar (new session, new folder, expand all, collapse
  all, sort), and the tree: Favorites and Recent (the user's own shortcuts), Saved sessions from Sessions.xml,
  Private sessions from PrivateSessions.xml and Database sessions from the team's table. A search opens every
  folder holding a match; clearing it puts the tree back as it was.
-->
<script lang="ts">
  import { untrack } from "svelte";
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
    loadShortcuts,
    setFavorite,
    recordOpened,
    forgetRecent,
    setGroupExpanded,
    type Shortcuts,
    type DbState,
    type FolderSetting,
    type SessionFields,
    type SessionsLoaded,
    type SessionView,
    type Store,
  } from "./api";
  import EditSession from "./EditSession.svelte";
  import FolderSettings from "./FolderSettings.svelte";
  import PendingChanges from "./PendingChanges.svelte";
  import Prompt from "../ui/Prompt.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { PALETTE } from "../icons/palette";
  import { buildGroup, buildTree, folderKeys, sessionsIn, type Dragged, type FolderNode, type Group } from "./tree";
  import Icon from "../icons/Icon.svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { loadCustomIcons, type CustomIcon } from "../icons/custom";
  import { EnvironmentColors, parseRules } from "../icons/environment";
  import { chooseSettingsFolder, databaseCheckSeconds, settings } from "../settings/store.svelte";
  import { DATABASE_SAVED } from "./api";

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
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** The session being edited: its store, its id (null for new) and its fields */
  let editing = $state<{ store: Store; id: string | null; fields: SessionFields; lockedFields?: string } | null>(null);
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
  const saved = $derived(loaded ? buildTree("saved", "Saved sessions", loaded.sessions, query, newFolders.saved, descending) : null);
  const privateRoot = $derived(
    loaded ? buildTree("private", "Private sessions", loaded.sessions, query, newFolders.private, descending) : null,
  );
  const databaseRoot = $derived(
    loaded && db?.enabled ? buildTree("database", dbLabel(db, dbLoading), loaded.sessions, query, newFolders.database, descending) : null,
  );
  const favorites = $derived(loaded ? buildGroup("favorites", "Favorites", shortcuts.Favorites, loaded.sessions, query) : null);
  const recent = $derived(loaded ? buildGroup("recent", "Recent", shortcuts.Recent, loaded.sessions, query) : null);
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
    if (editing || folderSettings || prompt || menu || showPending || dbLoading || db?.error) return;
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
      const setting = settings.find((f) => f.path === path);
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
        { label: `Connect all (${runnable.length})`, disabled: runnable.length === 0, action: () => runnable.forEach(open) },
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
    if ("session" in target) {
      const session = target.session;
      const block = dbBlock(session.store, session.locked);
      items = [
        { label: "Connect", bold: true, action: () => open(session) },
        { label: "Connect external", disabled: "Open in Terminal.app or iTerm2: comes in M5" },
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
        { label: "Duplicate as…", disabled: "Comes in M5" },
        { label: "Edit", disabled: block, action: () => void edit(session) },
        { label: "Local overrides…", disabled: "Local overrides come in M5" },
        { label: "Delete", disabled: block, action: () => void remove(session) },
        ...transferItems([session], session.store, session.locked),
        { separator: true },
        { label: "File transfer (SFTP)", disabled: "The SFTP file browser comes later" },
        { label: "FileZilla (external)", disabled: "Comes in M5" },
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
              { label: "Delete folder", disabled: block, action: () => void removeFolder(folder) },
              ...transferItems(sessionsIn(folder), folder.store, sessionsIn(folder).some((s) => s.locked)),
            ] as MenuItem[])),
        { separator: true },
        { label: `Connect all (${runnable.length})`, disabled: runnable.length === 0, action: () => runnable.forEach(open) },
        { label: "Expand all", action: () => (expanded = new Set([...expanded, ...folderKeys(folder)])) },
        { label: "Collapse all", action: () => (expanded = new Set([...expanded].filter((k) => !folderKeys(folder).slice(1).includes(k)))) },
      ];
    }
    menu = { x: event.clientX, y: event.clientY, items };
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
      (t) => t !== from && (t !== "database" || db?.enabled) && (t !== "saved" || settings.settingsFolder),
    );
    const item = (move: boolean): MenuItem => ({
      label: move ? "Move to" : "Copy to",
      disabled: sessions.length === 0 ? "No sessions here" : move && from === "database" && locked ? "Locked in the database" : false,
      items: targets.map((to) => ({
        label: ROOT_NAMES[to],
        disabled: to === "database" && !db?.writable ? "Database sessions can't be written now" : false,
        action: () => void transfer(sessions, from, to, move),
      })),
    });
    return [item(true), item(false)];
  }

  /** Copy or move sessions to another root, asking first when moving them out of the shared table */
  async function transfer(sessions: SessionView[], from: Store, to: Store, move: boolean): Promise<void> {
    const n = sessions.length;
    if (move && from === "database") {
      const sure = await ask(`Move ${n} session${n === 1 ? "" : "s"} out of Database sessions into ${ROOT_NAMES[to]}?\n\nThey're deleted from the table, for everyone who uses it.`,
        { title: "Move", kind: "warning", okLabel: "Move" });
      if (!sure) return;
    }
    try {
      const report = await transferSessions(from, sessions.map((s) => s.id), to, move);
      await reload();
      error = report.problems.length ? `${report.done} of ${n} ${move ? "moved" : "copied"}. ${report.problems.join(" ")}` : "";
      expanded = new Set([...expanded, `${to}:`]);
    } catch (e) {
      error = String(e);
    }
  }

  /** A folder's stored settings, or empty ones */
  function settingOf(folder: FolderNode): FolderSetting {
    const found = (loaded?.folders ?? []).find(([store, f]) => store === folder.store && f.path === folder.path)?.[1];
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
      error = "Moving between Saved and Private sessions comes with Move to / Copy to (M2).";
      return;
    }
    try {
      if (dragged.kind === "session") {
        const folder = dragged.id.includes("/") ? dragged.id.slice(0, dragged.id.lastIndexOf("/")) : "";
        if (folder === target.path) return;
        const id = await moveSession(target.store, dragged.id, target.path);
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

  /** Open a session, passing on its environment colour for its tab */
  function open(session: SessionView): void {
    onOpen(session, colors.forSession(session));
    // only sessions opened from the tree go in Recent, as in SuperXPuTTY
    void changeShortcuts(recordOpened(session.id));
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
    const fallback = loaded?.savedFile === null ? privateRoot : saved;
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
      <TreeFolder node={favorites} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected}
        onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if recent}
      <TreeFolder node={recent} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected}
        onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if loaded.savedFile === null}
      <p class="message">Choose the settings folder: the folder that holds the team's Sessions.xml.</p>
      <button onclick={choose}>Choose Settings Folder...</button>
    {:else if saved}
      <TreeFolder node={saved} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected}
        onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if privateRoot}
      <TreeFolder node={privateRoot} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected}
        onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if databaseRoot}
      <div class="db-root" title={db?.error ? `Showing the last read: ${db.error}` : `${db?.source}${db?.writable ? "" : " (read only)"}`}>
        <TreeFolder node={databaseRoot} depth={0} expanded={shownExpanded} {colors} {iconSize} {customs} {openSessions} {selected}
          onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} onDrop={dropped} />
      </div>
    {/if}
  {/if}
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

{#if editing}
  <EditSession store={editing.store} id={editing.id} fields={editing.fields} folders={foldersAlong(editing.store, editing.fields.folder)}
    {iconSize} {customs} {onOpenProfiles} lockedFields={editing.lockedFields ?? ""} onCancel={() => (editing = null)}
    onSaved={async (id) => { const store = editing?.store; editing = null; await reload(); selected = `${store}:${id}`; }} />
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
