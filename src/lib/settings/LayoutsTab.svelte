<!--
  Settings > Layouts: the saved layouts, as SuperXPuTTY's Layouts pane had them, in one place to manage them.
  "<Auto Restore>" comes first, then the team's layouts by name, the default one (opened at start) and the one open
  in this window marked. Load and Load in New Window open the one chosen; the tabs open now are saved as a new layout
  or over the one chosen; Rename, Delete and Set as Default act on the one chosen. "<Auto Restore>" can't be renamed
  or deleted, nor the default deleted. It took the place of the Layouts pane under the Sessions tree, which was easy
  to miss; View > Layouts opens it.

  What needs this window's tabs is the App's (`actions`): loading, saving the tabs open now, and renaming (the open
  layout's name follows). Deleting and the default are done here.
-->
<script lang="ts" module>
  /** What the App does for the tab: the parts that need the window's own tabs */
  export interface LayoutActions {
    /** Open a layout in this window (the App asks first when sessions would close, and closes Settings) */
    load: (name: string) => Promise<void>;
    /** Open a layout in a window of its own */
    loadInNewWindow: (name: string) => void;
    /** Save the tabs open now as the layout of this name */
    save: (name: string) => Promise<void>;
    /** Rename a layout (and the default's name, and the open one's, with it) */
    rename: (from: string, to: string) => Promise<void>;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { AUTO_RESTORE, LAYOUTS_CHANGED, deleteLayout, listLayouts } from "../layout/layouts";
  import { saveSettings, settings } from "./store.svelte";

  interface Props {
    /** The layout open in this window, marked "open now"; null for none */
    current: string | null;
    /** The App's side */
    actions: LayoutActions;
  }

  let { current, actions }: Props = $props();

  /** The layouts' names, "<Auto Restore>" first */
  let names = $state<string[]>([]);
  /** The one chosen in the list */
  let selected = $state<string | null>(null);
  /** The new layout's name, typed */
  let newName = $state("");
  /** The name being typed for the one chosen, while renaming; null when not */
  let renaming = $state<string | null>(null);
  /** What the last action said: a problem, or what was done */
  let message = $state("");

  const automatic = $derived(selected === AUTO_RESTORE);
  const isDefault = $derived(!!selected && selected === settings.defaultLayoutName);

  /** Read the layouts again (after a save, rename or delete here or in another window) */
  async function refresh(): Promise<void> {
    names = await listLayouts().catch(() => []);
    if (selected && !names.includes(selected)) selected = null;
  }

  onMount(() => {
    void refresh();
    const changed = () => void refresh();
    window.addEventListener(LAYOUTS_CHANGED, changed);
    return () => window.removeEventListener(LAYOUTS_CHANGED, changed);
  });

  /** A layout's name as it's written already, matched whatever its case; undefined when there's none */
  function existing(name: string): string | undefined {
    return names.find((n) => n.toLowerCase() === name.trim().toLowerCase());
  }

  /** Save the tabs open now as a layout, asking before replacing one */
  async function saveAs(name: string): Promise<void> {
    const there = existing(name);
    if (there && !(await ask(`Replace the layout "${there}" with the tabs open now?`, { title: "Save layout", kind: "warning", okLabel: "Replace" }))) return;
    const as = there ?? name.trim();
    try {
      await actions.save(as);
      newName = "";
      selected = as;
      message = `Saved the tabs open now as "${as}".`;
      await refresh();
    } catch (e) {
      message = String(e);
    }
  }

  /** Rename the one chosen to what was typed */
  async function rename(): Promise<void> {
    const from = selected;
    const to = renaming?.trim() ?? "";
    renaming = null;
    if (!from || !to || to === from) return;
    try {
      await actions.rename(from, to);
      selected = to;
      message = `Renamed "${from}" to "${to}".`;
      await refresh();
    } catch (e) {
      message = String(e);
    }
  }

  /** Delete the one chosen, after asking */
  async function remove(): Promise<void> {
    const name = selected;
    if (!name || automatic || isDefault) return;
    if (!(await ask(`Delete the layout "${name}"?`, { title: "Delete layout", kind: "warning", okLabel: "Delete" }))) return;
    try {
      await deleteLayout(name);
      window.dispatchEvent(new Event(LAYOUTS_CHANGED));
      message = `Deleted "${name}".`;
      await refresh();
    } catch (e) {
      message = String(e);
    }
  }
</script>

<div class="layouts">
  <p class="hint">
    A layout is a set of tabs: which sessions are open, their names, and how they're split. Save the tabs open now as
    one, and open it again later, here or in a new window. The default layout opens at start.
  </p>
  <div class="body">
    <div class="list" role="listbox" aria-label="Layouts">
      {#each names as name (name)}
        <div class="item" class:selected={selected === name} role="option" aria-selected={selected === name} tabindex="0"
          title={name === AUTO_RESTORE ? "The tabs as they were last, saved as they change; set it as the default to have them back at start" : ""}
          onclick={() => { selected = name; renaming = null; }} ondblclick={() => void actions.load(name)}
          onkeydown={(e) => { if (e.key === "Enter") void actions.load(name); }}>
          <span class="name">{name}</span>
          {#if name === current}<span class="tag">open now</span>{/if}
          {#if name === settings.defaultLayoutName}<span class="tag">default</span>{/if}
        </div>
      {/each}
    </div>
    <div class="buttons">
      <button onclick={() => selected && void actions.load(selected)} disabled={!selected}>Load</button>
      <button onclick={() => selected && actions.loadInNewWindow(selected)} disabled={!selected}>Load in New Window</button>
      <button onclick={() => selected && void saveAs(selected)} disabled={!selected}
        title="Replace this layout with the tabs open now">Save Current Tabs Here</button>
      <button onclick={() => (renaming = selected)} disabled={!selected || automatic}
        title={automatic ? "The automatic layout keeps its name" : ""}>Rename…</button>
      <button onclick={() => void remove()} disabled={!selected || automatic || isDefault}
        title={automatic ? "The automatic layout can't be deleted" : isDefault ? "The default layout can't be deleted: set another as the default first" : ""}>Delete</button>
      {#if isDefault}
        <button onclick={() => void saveSettings({ defaultLayoutName: "" })} title="Start with a Local Shell instead">No Default</button>
      {:else}
        <button onclick={() => selected && void saveSettings({ defaultLayoutName: selected })} disabled={!selected}>Set as Default</button>
      {/if}
    </div>
  </div>
  {#if renaming !== null}
    <form class="row" onsubmit={(e) => { e.preventDefault(); void rename(); }}>
      <input aria-label="Layout's new name" bind:value={renaming} onkeydown={(e) => e.key === "Escape" && (renaming = null)} />
      <button type="submit">Rename</button>
      <button type="button" onclick={() => (renaming = null)}>Cancel</button>
    </form>
  {/if}
  <form class="row" onsubmit={(e) => { e.preventDefault(); if (newName.trim()) void saveAs(newName); }}>
    <input aria-label="New layout's name" placeholder="New layout's name, e.g. Core switches" bind:value={newName} />
    <button type="submit" disabled={!newName.trim()}>Save Current Tabs as New</button>
  </form>
  {#if message}<p class="message">{message}</p>{/if}
</div>

<style>
  .layouts {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
    min-height: 0;
  }

  .hint,
  .message {
    margin: 0;
    color: var(--text-muted);
  }

  .body {
    display: flex;
    gap: 12px;
    min-height: 0;
    flex: 1;
  }

  .list {
    flex: 1;
    min-width: 0;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px;
  }

  .item {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 4px 8px;
    border-radius: 3px;
    cursor: default;
  }

  .item .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item.selected {
    background: color-mix(in srgb, var(--accent) 25%, transparent);
  }

  .tag {
    font-size: 12px;
    color: var(--text-muted);
  }

  .buttons {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 190px;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .row input {
    flex: 1;
    min-width: 0;
  }
</style>
