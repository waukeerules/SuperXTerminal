<!--
  The Sessions tree's rows, drawn from the flat list the tree makes of them (tree.ts's treeRows): the roots (Saved
  sessions, Private sessions, Database sessions, and Favorites and Recent, whose entries are shortcuts to sessions in
  the others), the folders under them, and the sessions of every folder that's open. Double-click a session to open it.

  Only the rows in view are in the page, with a few either side: every row is the same height, so which those are
  follows from how far the tree is scrolled, and the rest is empty space of the right height above and below them.
  With every row in the page, a folder of 10,000 sessions took seconds to open (150,000 elements), and as long again
  each time the sessions were read or a search cleared. Where the tree has no size to go by (the unit tests' jsdom,
  which lays nothing out), every row is drawn.
-->
<script lang="ts">
  import { tick } from "svelte";
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";
  import type { SessionStates } from "../layout/TabArea";
  import type { EnvironmentColors } from "../icons/environment";
  import { DRAG_TYPE, type Dragged, type FolderNode, type Group, type TreeRow } from "./tree";
  import { overriddenFields, unavailableReason, type Override, type SessionView } from "./api";
  import { settings } from "../settings/store.svelte";

  interface Props {
    /** The rows, top to bottom as drawn */
    rows: TreeRow[];
    /** The element the tree scrolls in, which says what's in view; undefined until it's in the page */
    scroller: HTMLElement | undefined;
    /** The environment colours, for each session's tab colour and the folders' colours */
    colors: EnvironmentColors;
    /** The size of the icons, as Settings > General has it */
    iconSize: number;
    /** The user's custom icons */
    customs: ReadonlyMap<string, CustomIcon>;
    /** Sessions with an open tab, as "store:id": connecting (a yellow dot), connected (green) or dropped (a red cross) */
    openSessions: SessionStates;
    /** The selected session or folder, as "store:id" or "store:folder:path" */
    selected: string;
    /** Sessions selected together (Cmd+click, Shift+click), as "store:id" */
    multi: ReadonlySet<string>;
    /** The user's local overrides, by session id, for the tooltip */
    overrides: Record<string, Override>;
    /** The tooltip on the Database sessions' folders: where they're from, or why the last read is shown */
    databaseTitle?: string;
    /** A folder's arrow or double-click: open or close it */
    onToggle: (key: string) => void;
    /** A session's double-click, or Enter: open it */
    onOpen: (session: SessionView) => void;
    /** A click (with its modifiers) or a right-click (no event) on a session or folder */
    onSelect: (key: string, event?: MouseEvent) => void;
    /** A right-click on a session or folder */
    onMenu: (event: MouseEvent, target: { session: SessionView; group?: Group } | { folder: FolderNode }) => void;
    /** Something dragged in the tree was dropped on a folder */
    onDrop: (folder: FolderNode, dragged: Dragged) => void;
  }

  let { rows, scroller, colors, iconSize, customs, openSessions, selected, multi, overrides, databaseTitle, onToggle, onOpen, onSelect, onMenu, onDrop }: Props = $props();

  /** Each level's line runs down the middle of its arrow's column */
  const STEP = 18;
  /** Where a level's tree line runs, from the row's left edge */
  const lineX = (level: number) => level * STEP + 9;
  /** Whether tree lines are drawn (Settings > General) */
  const lines = $derived(settings.treeLines);
  /** Every row's height: the icon's, and a little */
  const rowHeight = $derived(Math.round(iconSize * 1.36) + 4);
  /** Rows drawn above and below those in view, so a scroll shows rows at once rather than empty space */
  const SPARE = 12;

  /** The rows' own element, whose place in the scroller says where the first row is */
  let list = $state<HTMLElement | undefined>();
  /**
   * What's in view: how far down the rows the scroller's top edge is, and the scroller's height; null where there's
   * no knowing (no ResizeObserver: jsdom), and every row is drawn
   */
  let view = $state<{ top: number; height: number } | null>(null);

  /** Take what's in view from the scroller: called when it scrolls, changes size, or the rows above the list change */
  function look(): void {
    if (!scroller || !list) return;
    // the list isn't at the scroller's top: messages can come before it
    const top = scroller.getBoundingClientRect().top - list.getBoundingClientRect().top;
    const height = scroller.clientHeight;
    if (!view || view.top !== top || view.height !== height) view = { top, height };
  }

  $effect(() => {
    if (!scroller || typeof ResizeObserver === "undefined") return;
    const element = scroller;
    const observer = new ResizeObserver(look);
    observer.observe(element);
    element.addEventListener("scroll", look, { passive: true });
    look();
    return () => {
      observer.disconnect();
      element.removeEventListener("scroll", look);
    };
  });
  // the rows changed (a folder opened, the sessions read again): what's in view may have too
  $effect(() => {
    void rows.length;
    void rowHeight;
    if (view) look();
  });

  /** The first row drawn and the one after the last: those in view and SPARE either side; all of them without a view */
  const drawn = $derived.by(() => {
    if (!view) return { from: 0, to: rows.length };
    const from = Math.max(0, Math.min(rows.length, Math.floor(view.top / rowHeight) - SPARE));
    const to = Math.max(from, Math.min(rows.length, Math.ceil((view.top + view.height) / rowHeight) + SPARE));
    return { from, to };
  });

  /**
   * Bring the row a session is selected by into view (Show in Sessions tree): the tree is scrolled to where the row
   * is, so it's drawn, and then the row itself is shown, as before every row was in the page
   * @param select What the session is selected by ("store:id")
   */
  export async function reveal(select: string): Promise<void> {
    const index = rows.findIndex((row) => row.kind === "session" && row.select === select);
    if (index < 0) return;
    if (view && scroller) {
      const top = index * rowHeight;
      if (top < view.top) scroller.scrollTop += top - view.top;
      else if (top + rowHeight > view.top + view.height) scroller.scrollTop += top + rowHeight - (view.top + view.height);
      look();
      await tick();
    }
    // compared rather than put in a selector: a session's id can hold quotes
    [...(list?.querySelectorAll<HTMLElement>(".row.session") ?? [])].find((r) => r.dataset.key === select)?.scrollIntoView({ block: "nearest" });
  }

  /** The folder something is being dragged over (its key), or over a session of; "" for none */
  let over = $state("");

  /** What dragging `key` drags: the whole selection when it's one of several selected, else just it */
  function dragOf(key: string, node: FolderNode, single: Dragged): Dragged {
    return multi.size > 1 && multi.has(key) ? { kind: "many", store: node.store, keys: [...multi] } : single;
  }

  /** Start dragging a session or folder, for moving it to another folder or copying it to another store */
  function drag(event: DragEvent, dragged: Dragged): void {
    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify(dragged));
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "copyMove";
  }

  /** Let tree items be dropped on a folder, or on a session in it */
  function dragOver(event: DragEvent, node: FolderNode): void {
    if (!event.dataTransfer?.types.includes(DRAG_TYPE) || node.group) return;
    event.preventDefault();
    over = node.key;
  }

  /** Something was dropped on a folder, or on a session in it (it goes in the session's folder, as SuperXPuTTY's
   * DropTarget: no scrolling up to the folder among many sessions or in a search) */
  function drop(event: DragEvent, node: FolderNode): void {
    over = "";
    const data = event.dataTransfer?.getData(DRAG_TYPE);
    if (!data) return;
    event.preventDefault();
    onDrop(node, JSON.parse(data) as Dragged);
  }

  /** The roots' icons, as SuperXPuTTY's tree: a folder for Saved sessions, a padlock for Private ones, a database */
  const ROOT_ICONS: Record<string, string> = { saved: "folder", private: "lock", database: "database", favorites: "star", recent: "clock" };

  /** A folder's key when it's selected; Favorites and Recent have their own */
  const folderKey = (node: FolderNode) => (node.group ? `${node.group}:folder:` : `${node.store}:folder:${node.path}`);

  /** The tooltip for a session: protocol, address and note, then why it can't open, if it can't */
  function tooltip(session: SessionView): string {
    const address = `${session.username ? session.username + "@" : ""}${session.host}${session.port ? ":" + session.port : ""}`;
    const lines = [`${session.protocol || "SSH"}: ${address}`];
    if (session.note) lines.push(session.note);
    if (session.updatedBy) lines.push(`Changed by ${session.updatedBy}`);
    const mine = overriddenFields(overrides[session.id]);
    if (mine.length) {
      const off = overrides[session.id]?.Disabled ? " (off)" : !settings.useLocalOverrides ? " (off: Tools > Use Local Overrides)" : "";
      lines.push(`Your local overrides${off}: ${mine.join(", ")}`);
    }
    if (session.locked) lines.push("Locked in the database");
    if (session.pending) lines.unshift(session.pendingNote);
    const reason = unavailableReason(session.protocol);
    if (reason) lines.push(reason);
    return lines.join("\n");
  }
</script>

<!-- SuperXPuTTY's tree lines (TreeView.ShowLines): the levels above that carry on, and this row's own elbow, a
     line down from above (and on down, unless it's the last) and across to its icon -->
{#snippet connectors(level: number, above: boolean[], isLast: boolean, isFirst: boolean, across: number)}
  {#each above as carries, k}
    {#if carries}<span class="guide down" style:left="{lineX(k)}px"></span>{/if}
  {/each}
  <span class="guide elbow" class:last={isLast} class:first={isFirst} style:left="{lineX(level)}px"></span>
  <span class="guide across" style:left="{lineX(level)}px" style:width="{across}px"></span>
{/snippet}

<!-- as tall as every row together, so the tree scrolls as though they were all here; those drawn start where the
     first of them belongs -->
<div class="rows" bind:this={list} style:height="{rows.length * rowHeight}px" style:padding-top="{drawn.from * rowHeight}px">
  {#each rows.slice(drawn.from, drawn.to) as row (row.key)}
    {#if row.kind === "folder"}
      {@const node = row.node}
      {@const isRoot = node.path === ""}
      {@const key = folderKey(node)}
      <!-- as SuperXPuTTY's tree: a click selects a folder, a double-click (or its arrow) opens or closes it -->
      <div class="row folder" class:root={isRoot} class:selected={selected === key || multi.has(key)} class:over={over === node.key}
        draggable={!isRoot} ondragstart={(e) => drag(e, dragOf(key, node, { kind: "folder", store: node.store, path: node.path }))}
        ondragover={(e) => dragOver(e, node)} ondragleave={() => (over = "")} ondrop={(e) => drop(e, node)}
        oncontextmenu={(e) => { e.preventDefault(); onSelect(key); onMenu(e, { folder: node }); }} style:padding-left="{row.depth * 18 + 2}px" style:height="{rowHeight}px"
        title={node.store === "database" && !node.group ? databaseTitle : undefined}
        role="treeitem" aria-expanded={row.open} aria-selected="false" tabindex="-1"
        onclick={(e) => onSelect(key, e)} ondblclick={() => onToggle(node.key)}
        onkeydown={(e) => (e.key === "Enter" || e.key === "ArrowRight" || e.key === "ArrowLeft") && onToggle(node.key)}>
        {#if lines}{@render connectors(row.depth, row.guides, row.last, row.first, 13)}{/if}
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="twisty" onclick={(e) => { e.stopPropagation(); onToggle(node.key); }} ondblclick={(e) => e.stopPropagation()}>{row.open ? "▾" : "▸"}</span>
        <Icon key={isRoot ? ROOT_ICONS[node.group ?? node.store] : "folder"} size={iconSize} environment={isRoot ? null : colors.forFolder(node.store, node.path)} {customs} />
        <span class="label">{node.name} ({node.count})</span>
      </div>
    {:else}
      {@const session = row.session}
      {@const node = row.parent}
      {@const reason = unavailableReason(session.protocol)}
      {@const key = row.select}
      {@const state = openSessions.get(session.store + ":" + session.id)}
      <!-- data-key: Show in Sessions tree finds the row by it, to scroll it into view -->
      <div class="row session" class:dimmed={reason !== null} class:pending={session.pending} class:selected={selected === key || multi.has(key)}
        data-key={key} onclick={(e) => onSelect(key, e)} draggable={!session.locked && !node.group}
        ondragstart={(e) => drag(e, dragOf(key, node, { kind: "session", store: session.store, id: session.id }))}
        ondragover={(e) => dragOver(e, node)} ondragleave={() => (over = "")} ondrop={(e) => drop(e, node)}
        oncontextmenu={(e) => { e.preventDefault(); onSelect(key); onMenu(e, { session, group: node.group }); }} style:padding-left="{(row.depth + 1) * 18 + (lines ? 22 : 14)}px"
        style:height="{rowHeight}px"
        title={tooltip(session)} role="treeitem" aria-selected="false" aria-disabled={reason !== null} tabindex="-1"
        ondblclick={() => reason === null && onOpen(session)}
        onkeydown={(e) => e.key === "Enter" && reason === null && onOpen(session)}>
        {#if lines}{@render connectors(row.depth + 1, row.guides, row.last, false, 13)}{/if}
        <Icon key={session.icon} size={iconSize} environment={colors.forSession(session)} {customs}
          open={state === "connected"} connecting={state === "connecting"} down={state === "disconnected"} />
        <span class="label">{session.name}</span>
      </div>
    {/if}
  {/each}
</div>

<style>
  /* the height of every row and the space above those drawn are both in its height */
  .rows {
    box-sizing: border-box;
  }

  /* not in the table yet: queued while offline, or refused (SuperXPuTTY draws these in italics) */
  .row.pending .label {
    font-style: italic;
  }

  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
    border-radius: 3px;
  }

  .row:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .row.over {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .row.selected {
    background: color-mix(in srgb, var(--accent) 28%, transparent);
  }

  .root .label {
    font-weight: 600;
  }

  /* tree lines: dotted, under the arrows and icons */
  .guide {
    position: absolute;
    pointer-events: none;
    border-color: var(--text-muted);
    border-style: dotted;
    border-width: 0;
  }

  .guide.down,
  .guide.elbow {
    top: 0;
    bottom: 0;
    border-left-width: 1px;
  }

  .guide.elbow.last {
    bottom: 50%;
  }

  .guide.elbow.first {
    top: 50%;
  }

  .guide.elbow.first.last {
    display: none;
  }

  .guide.across {
    top: 50%;
    border-top-width: 1px;
  }

  .twisty {
    position: relative;
    z-index: 1;
    background: var(--bg-pane);
    border-radius: 2px;
    width: 14px;
    text-align: center;
    color: var(--text-muted);
    font-size: 11px;
  }

  .dimmed {
    opacity: 0.45;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
