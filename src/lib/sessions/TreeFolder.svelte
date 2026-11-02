<!--
  One folder of the Sessions tree and everything under it (it shows itself for its subfolders). A root (Saved
  sessions, Private sessions) is a folder with an empty path, shown in bold. Double-click a session to open it.
-->
<script lang="ts">
  import TreeFolder from "./TreeFolder.svelte";
  import type { FolderNode } from "./tree";
  import { unavailableReason, type SessionView } from "./api";

  interface Props {
    node: FolderNode;
    depth: number;
    expanded: Set<string>;
    colorOf: (session: SessionView) => string;
    onToggle: (key: string) => void;
    onOpen: (session: SessionView) => void;
  }

  let { node, depth, expanded, colorOf, onToggle, onOpen }: Props = $props();

  const open = $derived(expanded.has(node.key));
  const isRoot = $derived(node.path === "");

  /** The tooltip for a session: protocol, address and note, then why it can't open, if it can't */
  function tooltip(session: SessionView): string {
    const address = `${session.username ? session.username + "@" : ""}${session.host}${session.port ? ":" + session.port : ""}`;
    const lines = [`${session.protocol || "SSH"}: ${address}`];
    if (session.note) lines.push(session.note);
    const reason = unavailableReason(session.protocol);
    if (reason) lines.push(reason);
    return lines.join("\n");
  }
</script>

<div class="row folder" class:root={isRoot} style:padding-left="{depth * 16 + 4}px"
  role="treeitem" aria-expanded={open} aria-selected="false" tabindex="-1"
  onclick={() => onToggle(node.key)} onkeydown={(e) => e.key === "Enter" && onToggle(node.key)}>
  <span class="twisty">{open ? "▾" : "▸"}</span>
  <span class="folder-icon" aria-hidden="true">{isRoot ? (node.store === "private" ? "🔒" : "🗂") : "📁"}</span>
  <span class="label">{node.name} ({node.count})</span>
</div>

{#if open}
  {#each node.folders as child (child.key)}
    <TreeFolder node={child} depth={depth + 1} {expanded} {colorOf} {onToggle} {onOpen} />
  {/each}
  {#each node.sessions as session (session.store + ":" + session.id)}
    {@const reason = unavailableReason(session.protocol)}
    <div class="row session" class:dimmed={reason !== null} style:padding-left="{(depth + 1) * 16 + 20}px"
      title={tooltip(session)} role="treeitem" aria-selected="false" aria-disabled={reason !== null} tabindex="-1"
      ondblclick={() => reason === null && onOpen(session)}
      onkeydown={(e) => e.key === "Enter" && reason === null && onOpen(session)}>
      <span class="dot" style:background={colorOf(session) || "var(--text-muted)"}></span>
      <span class="label">{session.name}</span>
    </div>
  {/each}
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    white-space: nowrap;
    border-radius: 3px;
    font-size: 14px;
  }

  .row:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .root .label {
    font-weight: 600;
  }

  .twisty {
    width: 10px;
    color: var(--text-muted);
    font-size: 11px;
  }

  .folder-icon {
    font-size: 14px;
  }

  .dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    flex: none;
  }

  .dimmed {
    opacity: 0.45;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
