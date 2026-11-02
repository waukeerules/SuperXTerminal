<!--
  The Sessions pane: the search box and the tree (Saved sessions from Sessions.xml, Private sessions from
  PrivateSessions.xml), as in SuperXPuTTY. Favorites, Recent and Database sessions join it later (M5, M2).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import TreeFolder from "./TreeFolder.svelte";
  import { buildTree } from "./tree";
  import {
    chooseSettingsFolder,
    deleteSession,
    getSession,
    loadSessions,
    saveSession,
    type SessionFields,
    type SessionsLoaded,
    type SessionView,
    type Store,
  } from "./api";
  import EditSession from "./EditSession.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { PALETTE } from "../icons/palette";
  import type { FolderNode } from "./tree";
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

  let iconSize = $state(24);
  let rules = $state("");
  let customs = $state<ReadonlyMap<string, CustomIcon>>(new Map());

  let loaded = $state<SessionsLoaded | null>(null);
  let error = $state("");
  let query = $state("");
  // roots start open, as with SuperXPuTTY's "Expand tree on startup"
  let expanded = $state(new Set<string>(["saved:", "private:"]));

  const colors = $derived(new EnvironmentColors(loaded?.folders ?? [], parseRules(rules)));
  const saved = $derived(loaded ? buildTree("saved", "Saved sessions", loaded.sessions, query) : null);
  const privateRoot = $derived(loaded ? buildTree("private", "Private sessions", loaded.sessions, query) : null);

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
      items = [
        { label: "New session…", action: () => (editing = { store: folder.store, id: null, fields: blank(folder.path) }) },
        { label: "New folder…", disabled: "Comes with the next M1 slice" },
        { separator: true },
        { label: "Folder settings…", disabled: "Comes with the next M1 slice" },
      ];
    }
    menu = { x: event.clientX, y: event.clientY, items };
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
        onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} />
    {/if}
    {#if privateRoot}
      <TreeFolder node={privateRoot} depth={0} {expanded} {colors} {iconSize} {customs} {openSessions} {selected}
        onToggle={toggle} onOpen={open} onSelect={(key) => (selected = key)} onMenu={showMenu} />
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
