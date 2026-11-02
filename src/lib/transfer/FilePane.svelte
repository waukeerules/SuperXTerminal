<!--
  One side of a file transfer, as SuperXPuTTY's pscp browser: the folder's path (typed, Up, Home, Refresh), its files
  in columns (Name, Size, Last Mod, Perm, Owner, Group; a click on a heading sorts), and how many there are. A double
  click opens a folder or copies a file to the other side; files dragged to the other side (or onto one of its
  folders) are copied there. Its right-click menu has the side's own actions.
-->
<script lang="ts">
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { formatSize, formatTime, joinPath, parentPath, type Entry, type Listing } from "./api";

  interface Props {
    side: "local" | "remote";
    title: string;
    home: string;
    listing: Listing | null;
    loading: boolean;
    error: string;
    onNavigate: (path: string) => void;
    /** Copy these (full paths) to the other side's folder */
    onCopy: (paths: string[]) => void;
    /** Files dragged from the other side (full paths), dropped into this side's `folder` */
    onDrop: (paths: string[], folder: string) => void;
    onMkdir?: () => void;
    onRename?: (entry: Entry) => void;
    /** Remote only: delete these entries */
    onDelete?: (entries: Entry[]) => void;
    /** Local only: show an entry in Finder */
    onReveal?: (entry: Entry) => void;
  }

  let { side, title, home, listing, loading, error, onNavigate, onCopy, onDrop, onMkdir, onRename, onDelete, onReveal }: Props = $props();

  type Column = "name" | "size" | "modified" | "permissions" | "owner" | "group";
  let sort = $state<{ by: Column; up: boolean }>({ by: "name", up: true });
  let selected = $state<Set<string>>(new Set());
  /** Where a shift-click selects from */
  let anchor = $state("");
  let typed = $state("");
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** A folder (or the list) a drag is over */
  let dropTarget = $state<string | null>(null);

  const DRAG_TYPE = "application/x-sxt-transfer";

  // the path box follows the folder shown, and the selection is cleared on a new folder
  $effect(() => {
    typed = listing?.path ?? "";
    selected = new Set();
  });

  const entries = $derived.by(() => {
    const list = [...(listing?.entries ?? [])];
    const factor = sort.up ? 1 : -1;
    list.sort((a, b) => {
      // folders first, as both of SuperXPuTTY's panes do
      if (a.dir !== b.dir) return a.dir ? -1 : 1;
      const x = a[sort.by];
      const y = b[sort.by];
      const order = typeof x === "number" && typeof y === "number" ? x - y : String(x).localeCompare(String(y), undefined, { numeric: true, sensitivity: "base" });
      return order * factor || a.name.localeCompare(b.name);
    });
    return list;
  });
  const counts = $derived.by(() => {
    const all = listing?.entries ?? [];
    const dirs = all.filter((e) => e.dir).length;
    return `${all.length - dirs} file${all.length - dirs === 1 ? "" : "s"}, ${dirs} folder${dirs === 1 ? "" : "s"}`;
  });

  const full = (entry: Entry) => joinPath(listing?.path ?? "", entry.name);
  const chosen = () => entries.filter((e) => selected.has(e.name));

  function setSort(by: Column): void {
    sort = sort.by === by ? { by, up: !sort.up } : { by, up: true };
  }

  function click(event: MouseEvent, entry: Entry): void {
    const next = new Set(event.metaKey ? selected : []);
    if (event.shiftKey && anchor) {
      const names = entries.map((e) => e.name);
      const [a, b] = [names.indexOf(anchor), names.indexOf(entry.name)].sort((x, y) => x - y);
      for (const name of names.slice(a, b + 1)) next.add(name);
    } else if (event.metaKey && selected.has(entry.name)) {
      next.delete(entry.name);
    } else {
      next.add(entry.name);
      anchor = entry.name;
    }
    selected = next;
  }

  function open(entry: Entry): void {
    if (entry.dir) onNavigate(full(entry));
    else onCopy([full(entry)]);
  }

  function key(event: KeyboardEvent): void {
    if (event.key === "a" && event.metaKey) {
      event.preventDefault();
      selected = new Set(entries.map((e) => e.name));
    } else if (event.key === "Enter" && selected.size === 1) {
      open(chosen()[0]);
    } else if ((event.key === "Backspace" && event.metaKey) || event.key === "Delete") {
      if (onDelete && selected.size) onDelete(chosen());
    } else if (event.key === "ArrowUp" && event.metaKey) {
      event.preventDefault();
      if (listing) onNavigate(parentPath(listing.path));
    }
  }

  function dragStart(event: DragEvent, entry: Entry): void {
    if (!selected.has(entry.name)) selected = new Set([entry.name]);
    event.dataTransfer?.setData(DRAG_TYPE, JSON.stringify({ side, paths: chosen().map(full) }));
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "copy";
  }

  /** What's being dragged here from the other side, if anything */
  function incoming(event: DragEvent): boolean {
    return !!event.dataTransfer?.types.includes(DRAG_TYPE);
  }

  function over(event: DragEvent, folder: string): void {
    if (!incoming(event)) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    dropTarget = folder;
  }

  function drop(event: DragEvent, folder: string): void {
    dropTarget = null;
    const raw = event.dataTransfer?.getData(DRAG_TYPE);
    if (!raw) return;
    event.preventDefault();
    event.stopPropagation();
    const data = JSON.parse(raw) as { side: string; paths: string[] };
    if (data.side !== side && data.paths.length) onDrop(data.paths, folder);
  }

  /** Move an element to the page's body (the menu), back out when it goes */
  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function showMenu(event: MouseEvent, entry: Entry | null): void {
    event.preventDefault();
    if (entry && !selected.has(entry.name)) selected = new Set([entry.name]);
    const picked = entry ? chosen() : [];
    const other = side === "local" ? "Upload" : "Download";
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        ...(entry?.dir && picked.length === 1 ? [{ label: "Open", bold: true, action: () => onNavigate(full(entry)) } as MenuItem] : []),
        { label: picked.length > 1 ? `${other} ${picked.length} items` : other, disabled: !picked.length && "Select files or folders first", action: () => onCopy(picked.map(full)) },
        { separator: true },
        ...(onMkdir ? [{ label: "New Folder…", action: onMkdir } as MenuItem] : []),
        ...(onRename ? [{ label: "Rename…", disabled: picked.length !== 1 && "Select one file or folder", action: () => onRename(picked[0]) } as MenuItem] : []),
        ...(onDelete ? [{ label: "Delete…", disabled: !picked.length && "Select files or folders first", action: () => onDelete(picked) } as MenuItem] : []),
        ...(onReveal ? [{ label: "Show in Finder", action: () => onReveal(picked[0] ?? { name: "", dir: true } as Entry) } as MenuItem] : []),
        { separator: true },
        { label: "Refresh", action: () => listing && onNavigate(listing.path) },
      ],
    };
  }
</script>

<section class="pane" aria-label={title}>
  <header>
    <span class="title">{title}</span>
    <button title="Up a folder (⌘↑)" aria-label="Up" disabled={!listing || parentPath(listing.path) === listing.path} onclick={() => listing && onNavigate(parentPath(listing.path))}>↑</button>
    <button title="Home folder" aria-label="Home" onclick={() => onNavigate(home)}>⌂</button>
    <button title="Refresh" aria-label="Refresh" onclick={() => listing && onNavigate(listing.path)}>⟳</button>
    {#if onMkdir}<button title="New folder" aria-label="New folder" onclick={onMkdir}>+</button>{/if}
    <button class="copy" title={side === "local" ? "Copy the selected files to the remote folder" : "Copy the selected files to this Mac's folder"}
      disabled={!selected.size} onclick={() => onCopy(chosen().map(full))}>{side === "local" ? "Upload →" : "← Download"}</button>
  </header>
  <form class="path" onsubmit={(e) => { e.preventDefault(); onNavigate(typed); }}>
    <input bind:value={typed} spellcheck="false" aria-label="{title} folder" />
  </form>

  <div class="list" class:drop={dropTarget === listing?.path} role="grid" tabindex="0" aria-label="{title} files"
    onkeydown={key}
    ondragover={(e) => listing && over(e, listing.path)}
    ondragleave={() => (dropTarget = null)}
    ondrop={(e) => listing && drop(e, listing.path)}
    oncontextmenu={(e) => showMenu(e, null)}>
    <table>
      <thead>
        <tr>
          {#each [["name", "Name"], ["size", "Size"], ["modified", "Last Mod"], ["permissions", "Perm"], ["owner", "Owner"], ["group", "Group"]] as [by, label]}
            <th class={by} onclick={() => setSort(by as Column)}>{label}{sort.by === by ? (sort.up ? " ▲" : " ▼") : ""}</th>
          {/each}
        </tr>
      </thead>
      <tbody>
        {#if listing && parentPath(listing.path) !== listing.path}
          <tr class="up" ondblclick={() => onNavigate(parentPath(listing.path))}
            ondragover={(e) => over(e, parentPath(listing.path))} ondrop={(e) => drop(e, parentPath(listing.path))}>
            <td class="name"><span class="icon folder"></span>..</td><td></td><td></td><td></td><td></td><td></td>
          </tr>
        {/if}
        {#each entries as entry (entry.name)}
          <tr class:selected={selected.has(entry.name)} class:drop={dropTarget === full(entry)} draggable="true"
            onclick={(e) => click(e, entry)}
            ondblclick={() => open(entry)}
            oncontextmenu={(e) => { e.stopPropagation(); showMenu(e, entry); }}
            ondragstart={(e) => dragStart(e, entry)}
            ondragover={(e) => entry.dir && over(e, full(entry))}
            ondrop={(e) => entry.dir && drop(e, full(entry))}>
            <td class="name" title={entry.link ? `${entry.name} (a link)` : entry.name}>
              <span class="icon" class:folder={entry.dir} class:link={entry.link}></span>{entry.name}
            </td>
            <td class="size">{entry.dir ? "" : formatSize(entry.size)}</td>
            <td class="modified" title={entry.date}>{formatTime(entry.modified) || entry.date}</td>
            <td class="permissions">{entry.permissions}</td>
            <td class="owner">{entry.owner}</td>
            <td class="group">{entry.group}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if loading}<p class="note">Loading…</p>{:else if error}<p class="note error">{error}</p>{/if}
  </div>
  <footer>{listing ? counts : ""}{selected.size ? ` · ${selected.size} selected` : ""}</footer>
</section>

{#if menu}
  <!-- at the page's top: inside the tab, a fixed menu would be placed relative to the tab -->
  <div use:portal><Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} /></div>
{/if}

<style>
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--bg-pane);
    border: 1px solid var(--border);
  }

  header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-bar);
  }

  .title {
    flex: 1;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  header button {
    min-width: 26px;
    padding: 1px 6px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text);
  }

  header button:disabled {
    opacity: 0.45;
  }

  .copy {
    margin-left: 6px;
  }

  .path {
    display: flex;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
  }

  .path input {
    flex: 1;
    font-family: ui-monospace, Menlo, monospace;
    font-size: 12px;
    padding: 3px 6px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text);
  }

  .list {
    flex: 1;
    overflow: auto;
    outline: none;
    position: relative;
  }

  .list.drop,
  tr.drop {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
    font-size: 12px;
  }

  th {
    position: sticky;
    top: 0;
    background: var(--bg-bar);
    text-align: left;
    font-weight: 500;
    padding: 3px 6px;
    border-bottom: 1px solid var(--border);
    cursor: default;
    white-space: nowrap;
    user-select: none;
  }

  td {
    padding: 2px 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    user-select: none;
  }

  th.size {
    width: 64px;
  }

  td.size {
    text-align: right;
  }

  th.modified {
    width: 124px;
  }

  th.permissions {
    width: 82px;
  }

  th.owner {
    width: 64px;
  }

  th.group {
    width: 56px;
  }

  td.permissions,
  td.modified {
    font-family: ui-monospace, Menlo, monospace;
    font-size: 11px;
  }

  tr.selected td {
    background: var(--accent);
    color: #fff;
  }

  .icon {
    display: inline-block;
    width: 12px;
    height: 10px;
    margin-right: 6px;
    border: 1px solid var(--text-muted);
    border-radius: 1px;
    vertical-align: -1px;
  }

  .icon.folder {
    background: #e8b84a;
    border-color: #c99628;
    border-radius: 1px 3px 1px 1px;
  }

  .icon.link {
    border-style: dashed;
  }

  .note {
    margin: 8px;
    color: var(--text-muted);
  }

  .error {
    color: #d9534f;
    white-space: pre-wrap;
  }

  footer {
    padding: 3px 8px;
    font-size: 11px;
    color: var(--text-muted);
    border-top: 1px solid var(--border);
    background: var(--bg-bar);
  }
</style>
