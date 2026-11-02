<!--
  The Sessions pane: the search box and the tree (Saved sessions from Sessions.xml, Private sessions from
  PrivateSessions.xml), as in SuperXPuTTY. Favorites, Recent and Database sessions join it later (M5, M2).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import TreeFolder from "./TreeFolder.svelte";
  import { buildTree } from "./tree";
  import { chooseSettingsFolder, loadSessions, type SessionsLoaded, type SessionView } from "./api";
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
  }

  let { openSessions, onOpen, onLook }: Props = $props();

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
      <TreeFolder node={saved} depth={0} {expanded} {colors} {iconSize} {customs} {openSessions} onToggle={toggle} onOpen={open} />
    {/if}
    {#if privateRoot}
      <TreeFolder node={privateRoot} depth={0} {expanded} {colors} {iconSize} {customs} {openSessions} onToggle={toggle} onOpen={open} />
    {/if}
  {/if}
</div>

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
