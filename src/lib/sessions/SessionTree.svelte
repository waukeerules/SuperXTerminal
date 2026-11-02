<!--
  The Sessions pane: the search box and the tree (Saved sessions from Sessions.xml, Private sessions from
  PrivateSessions.xml), as in SuperXPuTTY. Favorites, Recent and Database sessions join it later (M5, M2).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import TreeFolder from "./TreeFolder.svelte";
  import {
    chooseSettingsFolder,
    deleteSession,
    getSession,
    loadSessions,
    saveSession,
    moveFolder,
    deleteFolder,
    moveSession,
    saveFolderSettings,
    unavailableReason,
    type FolderSetting,
    type SessionFields,
    type SessionsLoaded,
    type SessionView,
    type Store,
  } from "./api";
  import EditSession from "./EditSession.svelte";
  import FolderSettings from "./FolderSettings.svelte";
  import Prompt from "../ui/Prompt.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { PALETTE } from "../icons/palette";
  import { buildTree, folderKeys, sessionsIn, type Dragged, type FolderNode } from "./tree";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { loadCustomIcons, type CustomIcon } from "../icons/custom";
  import { EnvironmentColors, parseRules } from "../icons/environment";
  import { invoke } from "@tauri-apps/api/core";

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
  let editing = $state<{ store: Store; id: string | null; fields: SessionFields } | null>(null);
  /** A folder whose settings are being edited */
  let folderSettings = $state<{ store: Store; setting: FolderSetting } | null>(null);
  /** A name being asked for (New folder, Rename) */
  let prompt = $state<{ title: string; value: string; validate: (name: string) => string; onOk: (name: string) => void } | null>(null);
  /** Folders made in the tree with no sessions yet, by store: they exist only here until a session goes in */
  let newFolders = $state<Record<Store, string[]>>({ saved: [], private: [] });

  let iconSize = $state(24);
  let rules = $state("");
  let customs = $state<ReadonlyMap<string, CustomIcon>>(new Map());

  let loaded = $state<SessionsLoaded | null>(null);
  let error = $state("");
  let query = $state("");
  // roots start open, as with SuperXPuTTY's "Expand tree on startup"
  let expanded = $state(new Set<string>(["saved:", "private:"]));

  const colors = $derived(new EnvironmentColors(loaded?.folders ?? [], parseRules(rules)));
  const saved = $derived(loaded ? buildTree("saved", "Saved sessions", loaded.sessions, query, newFolders.saved) : null);
  const privateRoot = $derived(loaded ? buildTree("private", "Private sessions", loaded.sessions, query, newFolders.private) : null);

  onMount(() => void reload());

  /** Read the settings, custom icons and session files again */
  async function reload(): Promise<void> {
    try {
      const settings = await invoke<{ iconSize: number; environmentColorRules: string }>("settings_get");
      iconSize = settings.iconSize;
      rules = settings.environmentColorRules;
      customs = await loadCustomIcons();
      onLook(iconSize, customs);
      loaded = await loadSessions();
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  /** Choose the settings folder, then read it */
  async function choose(): Promise<void> {
    if (await chooseSettingsFolder()) await reload();
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
      editing = { store: session.store, id: session.id, fields: await getSession(session.store, session.id) };
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
    const file = session.store === "saved" ? "Sessions.xml" : "PrivateSessions.xml";
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
  function showMenu(event: MouseEvent, target: { session: SessionView } | { folder: FolderNode }): void {
    let items: MenuItem[];
    if ("session" in target) {
      const session = target.session;
      items = [
        { label: "Connect", bold: true, action: () => open(session) },
        { label: "Connect external", disabled: "Open in Terminal.app or iTerm2: comes in M5" },
        { label: "Connect in new window", disabled: "Windows and splits come in M5" },
        { label: "Add to Favorites", disabled: "Favorites come in M5" },
        {
          label: "Colour",
          items: [
            { label: "None", swatch: "", action: () => void setColour(session, "") },
            ...PALETTE.map((p) => ({ label: p.name, swatch: p.color, action: () => void setColour(session, p.color) })),
          ],
        },
        { separator: true },
        { label: "Copy host address", action: () => void navigator.clipboard.writeText(session.host) },
        { label: "Duplicate as…", disabled: "Comes in M5" },
        { label: "Edit", action: () => void edit(session) },
        { label: "Local overrides…", disabled: "Local overrides come in M5" },
        { label: "Delete", action: () => void remove(session) },
        { label: "Move to", disabled: "Comes with Database sessions (M2)" },
        { label: "Copy to", disabled: "Comes with Database sessions (M2)" },
        { separator: true },
        { label: "File transfer (SFTP)", disabled: "The SFTP file browser comes later" },
        { label: "FileZilla (external)", disabled: "Comes in M5" },
      ];
    } else {
      const folder = target.folder;
      const isRoot = folder.path === "";
      const runnable = sessionsIn(folder).filter((s) => unavailableReason(s.protocol) === null);
      items = [
        { label: "New session…", action: () => (editing = { store: folder.store, id: null, fields: blank(folder.path) }) },
        { label: "New folder…", action: () => newFolder(folder) },
        ...(isRoot
          ? []
          : ([
              { label: "Rename…", action: () => rename(folder) },
              {
                label: "Colour",
                items: [
                  { label: "None", swatch: "", action: () => void setFolderColour(folder, "") },
                  ...PALETTE.map((p) => ({ label: p.name, swatch: p.color, action: () => void setFolderColour(folder, p.color) })),
                ],
              },
              { label: "Folder settings…", action: () => (folderSettings = { store: folder.store, setting: settingOf(folder) }) },
              { label: "Delete folder", action: () => void removeFolder(folder) },
            ] as MenuItem[])),
        { separator: true },
        { label: `Connect all (${runnable.length})`, disabled: runnable.length === 0, action: () => runnable.forEach(open) },
        { label: "Expand all", action: () => (expanded = new Set([...expanded, ...folderKeys(folder)])) },
        { label: "Collapse all", action: () => (expanded = new Set([...expanded].filter((k) => !folderKeys(folder).slice(1).includes(k)))) },
      ];
    }
    menu = { x: event.clientX, y: event.clientY, items };
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
  }

  /** Open or close a folder */
  function toggle(key: string): void {
    const next = new Set(expanded);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    expanded = next;
  }

</script>

<input class="search" placeholder="Type to search..." bind:value={query} />

<div class="tree" role="tree">
  {#if error}
    <p class="message">Couldn't read the sessions: {error}</p>
  {/if}
  {#if loaded}
    {#each loaded.problems as problem}
      <p class="message">Skipped: {problem}</p>
    {/each}
    {#if loaded.savedFile === null}
      <p class="message">Choose the settings folder: the folder that holds the team's Sessions.xml.</p>
      <button onclick={choose}>Choose Settings Folder...</button>
    {:else if saved}
      <TreeFolder node={saved} depth={0} {expanded} {colors} {iconSize} {customs} {openSessions} {selected}
        onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} onDrop={dropped} />
    {/if}
    {#if privateRoot}
      <TreeFolder node={privateRoot} depth={0} {expanded} {colors} {iconSize} {customs} {openSessions} {selected}
        onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} onDrop={dropped} />
    {/if}
  {/if}
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

{#if editing}
  <EditSession store={editing.store} id={editing.id} fields={editing.fields} folders={foldersAlong(editing.store, editing.fields.folder)}
    {iconSize} {customs} {onOpenProfiles} onCancel={() => (editing = null)}
    onSaved={async (id) => { const store = editing?.store; editing = null; await reload(); selected = `${store}:${id}`; }} />
{/if}

{#if folderSettings}
  <FolderSettings store={folderSettings.store} setting={folderSettings.setting} onCancel={() => (folderSettings = null)}
    onSaved={async () => { folderSettings = null; await reload(); }} />
{/if}

{#if prompt}
  <Prompt title={prompt.title} value={prompt.value} validate={prompt.validate} onOk={prompt.onOk} onCancel={() => (prompt = null)} />
{/if}

<svelte:window onclick={() => (menu = null)} />

<style>
  .search {
    margin: 0 8px 6px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 2px;
    padding: 2px 4px;
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
