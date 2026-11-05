<!--
  One folder of the Sessions tree and everything under it (it shows itself for its subfolders). A root (Saved
  sessions, Private sessions) is a folder with an empty path, shown in bold; so are Favorites and Recent, whose
  entries are shortcuts to sessions in the roots. Double-click a session to open it.
-->
<script lang="ts">
  import TreeFolder from "./TreeFolder.svelte";
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";
  import type { EnvironmentColors } from "../icons/environment";
  import { DRAG_TYPE, type Dragged, type FolderNode, type Group } from "./tree";
  import { overriddenFields, unavailableReason, type Override, type SessionView } from "./api";
  import { settings } from "../settings/store.svelte";

  interface Props {
    /** The folder shown, with its folders and sessions */
    node: FolderNode;
    /** How deep it is (0 for a root), for its indent */
    depth: number;
    /** The keys of the folders open */
    expanded: Set<string>;
    /** The environment colours, for each session's tab colour and the folders' colours */
    colors: EnvironmentColors;
    /** The size of the icons, as Settings > General has it */
    iconSize: number;
    /** The user's custom icons */
    customs: ReadonlyMap<string, CustomIcon>;
    /** Sessions with an open tab, as "store:id" */
    openSessions: ReadonlySet<string>;
    /** The selected session or folder, as "store:id" or "store:folder:path" */
    selected: string;
    /** Sessions selected together (Cmd+click, Shift+click), as "store:id" */
    multi: ReadonlySet<string>;
    /** The user's local overrides, by session id, for the tooltip */
    overrides: Record<string, Override>;
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
    /** Tree lines (SuperXPuTTY's ShowLines): for each level above, whether its line carries on down past this folder */
    guides?: boolean[];
    /** The last item in its folder (or the last root): its line stops at it */
    last?: boolean;
    /** The first root: its line starts at it */
    first?: boolean;
  }

  let { node, depth, expanded, colors, iconSize, customs, openSessions, selected, multi, overrides, onToggle, onOpen, onSelect, onMenu, onDrop, guides = [], last = true, first = false }: Props = $props();

  /** Each level's line runs down the middle of its arrow's column */
  const STEP = 18;
  /** Where a level's tree line runs, from the row's left edge */
  const lineX = (level: number) => level * STEP + 9;
  /** Whether tree lines are drawn (Settings > General) */
  const lines = $derived(settings.treeLines);
  /** This folder's line carries on for its children's rows when it isn't the last in its own folder */
  const childGuides = $derived([...guides, !last]);

  let over = $state(false);

  /** Start dragging a session or folder */
  /** What dragging `key` drags: the whole selection when it's one of several selected, else just it */
  function dragOf(key: string, single: Dragged): Dragged {
    return multi.size > 1 && multi.has(key) ? { kind: "many", store: node.store, keys: [...multi] } : single;
  }

  /** Start dragging a session or folder, for moving it to another folder or copying it to another store */
  function drag(event: DragEvent, dragged: Dragged): void {
    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify(dragged));
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "copyMove";
  }

  /** Let tree items be dropped on this folder */
  function dragOver(event: DragEvent): void {
    if (!event.dataTransfer?.types.includes(DRAG_TYPE) || node.group) return;
    event.preventDefault();
    over = true;
  }

  /** Something was dropped on this folder, or on a session in it (it goes in the session's folder, as SuperXPuTTY's
   * DropTarget: no scrolling up to the folder among many sessions or in a search) */
  function drop(event: DragEvent): void {
    over = false;
    const data = event.dataTransfer?.getData(DRAG_TYPE);
    if (!data) return;
    event.preventDefault();
    onDrop(node, JSON.parse(data) as Dragged);
  }

  /** The roots' icons, as SuperXPuTTY's tree: a folder for Saved sessions, a padlock for Private ones, a database */
  const ROOT_ICONS: Record<string, string> = { saved: "folder", private: "lock", database: "database", favorites: "star", recent: "clock" };

  /** Whether this folder is open */
  const open = $derived(expanded.has(node.key));
  /** Whether it's a root (Saved sessions, Private sessions...) */
  const isRoot = $derived(node.path === "");
  /** The folder's key when it's selected; Favorites and Recent have their own */
  const folderKey = $derived(node.group ? `${node.group}:folder:` : `${node.store}:folder:${node.path}`);

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

<!-- as SuperXPuTTY's tree: a click selects a folder, a double-click (or its arrow) opens or closes it -->
<div class="row folder" class:root={isRoot} class:selected={selected === folderKey || multi.has(folderKey)} class:over
  draggable={!isRoot} ondragstart={(e) => drag(e, dragOf(folderKey, { kind: "folder", store: node.store, path: node.path }))}
  ondragover={dragOver} ondragleave={() => (over = false)} ondrop={drop}
  oncontextmenu={(e) => { e.preventDefault(); onSelect(folderKey); onMenu(e, { folder: node }); }} style:padding-left="{depth * 18 + 2}px" style:height="{Math.round(iconSize * 1.36) + 4}px"
  role="treeitem" aria-expanded={open} aria-selected="false" tabindex="-1"
  onclick={(e) => onSelect(folderKey, e)} ondblclick={() => onToggle(node.key)}
  onkeydown={(e) => (e.key === "Enter" || e.key === "ArrowRight" || e.key === "ArrowLeft") && onToggle(node.key)}>
  {#if lines}{@render connectors(depth, guides, last, first, 13)}{/if}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <span class="twisty" onclick={(e) => { e.stopPropagation(); onToggle(node.key); }} ondblclick={(e) => e.stopPropagation()}>{open ? "▾" : "▸"}</span>
  <Icon key={isRoot ? ROOT_ICONS[node.group ?? node.store] : "folder"} size={iconSize} environment={isRoot ? null : colors.forFolder(node.store, node.path)} {customs} />
  <span class="label">{node.name} ({node.count})</span>
</div>

{#if open}
  {#each node.folders as child, i (child.key)}
    <TreeFolder node={child} depth={depth + 1} {expanded} {colors} {iconSize} {customs} {openSessions} {selected} {multi} {overrides}
      {onToggle} {onOpen} {onSelect} {onMenu} {onDrop} guides={childGuides} last={i === node.folders.length - 1 && node.sessions.length === 0} />
  {/each}
  {#each node.sessions as session, i (session.store + ":" + session.id)}
    {@const reason = unavailableReason(session.protocol)}
    {@const key = node.group ? `${node.group}:${session.store}:${session.id}` : `${session.store}:${session.id}`}
    <!-- data-key: Show in Sessions tree finds the row by it, to scroll it into view -->
    <div class="row session" class:dimmed={reason !== null} class:pending={session.pending} class:selected={selected === key || multi.has(key)}
      data-key={key} onclick={(e) => onSelect(key, e)} draggable={!session.locked && !node.group}
      ondragstart={(e) => drag(e, dragOf(key, { kind: "session", store: session.store, id: session.id }))}
      ondragover={dragOver} ondragleave={() => (over = false)} ondrop={drop}
      oncontextmenu={(e) => { e.preventDefault(); onSelect(key); onMenu(e, { session, group: node.group }); }} style:padding-left="{(depth + 1) * 18 + (lines ? 22 : 14)}px"
      style:height="{Math.round(iconSize * 1.36) + 4}px"
      title={tooltip(session)} role="treeitem" aria-selected="false" aria-disabled={reason !== null} tabindex="-1"
      ondblclick={() => reason === null && onOpen(session)}
      onkeydown={(e) => e.key === "Enter" && reason === null && onOpen(session)}>
      {#if lines}{@render connectors(depth + 1, childGuides, i === node.sessions.length - 1, false, 13)}{/if}
      <Icon key={session.icon} size={iconSize} environment={colors.forSession(session)} {customs}
        open={openSessions.has(session.store + ":" + session.id)} />
      <span class="label">{session.name}</span>
    </div>
  {/each}
{/if}

<style>
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
