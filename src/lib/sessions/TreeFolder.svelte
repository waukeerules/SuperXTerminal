<!--
  One folder of the Sessions tree and everything under it (it shows itself for its subfolders). A root (Saved
  sessions, Private sessions) is a folder with an empty path, shown in bold. Double-click a session to open it.
-->
<script lang="ts">
  import TreeFolder from "./TreeFolder.svelte";
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";
  import type { EnvironmentColors } from "../icons/environment";
  import { DRAG_TYPE, type Dragged, type FolderNode } from "./tree";
  import { unavailableReason, type SessionView } from "./api";

  interface Props {
    node: FolderNode;
    depth: number;
    expanded: Set<string>;
    colors: EnvironmentColors;
    iconSize: number;
    customs: ReadonlyMap<string, CustomIcon>;
    /** Sessions with an open tab, as "store:id" */
    openSessions: ReadonlySet<string>;
    /** The selected session or folder, as "store:id" or "store:folder:path" */
    selected: string;
    onToggle: (key: string) => void;
    onOpen: (session: SessionView) => void;
    onSelect: (key: string) => void;
    /** A right-click on a session or folder */
    onMenu: (event: MouseEvent, target: { session: SessionView } | { folder: FolderNode }) => void;
    /** Something dragged in the tree was dropped on a folder */
    onDrop: (folder: FolderNode, dragged: Dragged) => void;
  }

  let { node, depth, expanded, colors, iconSize, customs, openSessions, selected, onToggle, onOpen, onSelect, onMenu, onDrop }: Props = $props();

  let over = $state(false);

  /** Start dragging a session or folder */
  function drag(event: DragEvent, dragged: Dragged): void {
    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify(dragged));
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  }

  /** Let tree items be dropped on this folder */
  function dragOver(event: DragEvent): void {
    if (!event.dataTransfer?.types.includes(DRAG_TYPE)) return;
    event.preventDefault();
    over = true;
  }

  /** Something was dropped on this folder */
  function drop(event: DragEvent): void {
    over = false;
    const data = event.dataTransfer?.getData(DRAG_TYPE);
    if (!data) return;
    event.preventDefault();
    onDrop(node, JSON.parse(data) as Dragged);
  }

  /** The roots' icons, as SuperXPuTTY's tree: a folder for Saved sessions, a padlock for Private ones, a database */
  const ROOT_ICONS: Record<string, string> = { saved: "folder", private: "lock", database: "database" };

  const open = $derived(expanded.has(node.key));
  const isRoot = $derived(node.path === "");

  /** The tooltip for a session: protocol, address and note, then why it can't open, if it can't */
  function tooltip(session: SessionView): string {
    const address = `${session.username ? session.username + "@" : ""}${session.host}${session.port ? ":" + session.port : ""}`;
    const lines = [`${session.protocol || "SSH"}: ${address}`];
    if (session.note) lines.push(session.note);
    if (session.updatedBy) lines.push(`Changed by ${session.updatedBy}`);
    if (session.locked) lines.push("Locked in the database");
    if (session.pending) lines.unshift(session.pendingNote);
    const reason = unavailableReason(session.protocol);
    if (reason) lines.push(reason);
    return lines.join("\n");
  }
</script>

<div class="row folder" class:root={isRoot} class:selected={selected === `${node.store}:folder:${node.path}`} class:over
  draggable={!isRoot} ondragstart={(e) => drag(e, { kind: "folder", store: node.store, path: node.path })}
  ondragover={dragOver} ondragleave={() => (over = false)} ondrop={drop}
  oncontextmenu={(e) => { e.preventDefault(); onSelect(`${node.store}:folder:${node.path}`); onMenu(e, { folder: node }); }} style:padding-left="{depth * 18 + 2}px" style:height="{Math.round(iconSize * 1.36) + 4}px"
  role="treeitem" aria-expanded={open} aria-selected="false" tabindex="-1"
  onclick={() => onToggle(node.key)} onkeydown={(e) => e.key === "Enter" && onToggle(node.key)}>
  <span class="twisty">{open ? "▾" : "▸"}</span>
  <Icon key={isRoot ? ROOT_ICONS[node.store] : "folder"} size={iconSize} environment={isRoot ? null : colors.forFolder(node.store, node.path)} {customs} />
  <span class="label">{node.name} ({node.count})</span>
</div>

{#if open}
  {#each node.folders as child (child.key)}
    <TreeFolder node={child} depth={depth + 1} {expanded} {colors} {iconSize} {customs} {openSessions} {selected}
      {onToggle} {onOpen} {onSelect} {onMenu} {onDrop} />
  {/each}
  {#each node.sessions as session (session.store + ":" + session.id)}
    {@const reason = unavailableReason(session.protocol)}
    {@const key = `${session.store}:${session.id}`}
    <div class="row session" class:dimmed={reason !== null} class:pending={session.pending} class:selected={selected === key}
      onclick={() => onSelect(key)} draggable={!session.locked}
      ondragstart={(e) => drag(e, { kind: "session", store: session.store, id: session.id })}
      oncontextmenu={(e) => { e.preventDefault(); onSelect(key); onMenu(e, { session }); }} style:padding-left="{(depth + 1) * 18 + 14}px"
      style:height="{Math.round(iconSize * 1.36) + 4}px"
      title={tooltip(session)} role="treeitem" aria-selected="false" aria-disabled={reason !== null} tabindex="-1"
      ondblclick={() => reason === null && onOpen(session)}
      onkeydown={(e) => e.key === "Enter" && reason === null && onOpen(session)}>
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

  .twisty {
    width: 10px;
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
