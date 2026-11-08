<!--
  File > Edit Sessions as Table…: every session, Saved, Private and Database, in one table, to change many at once
  without exporting and importing them. Each column has a filter under its heading (the text anywhere in the cell,
  ignoring case; Store's a choice), and every cell can be edited in place: Store moves the session to another store,
  Folder shows its path until clicked, then takes a new one (moving it within its store), Icon opens Choose Icon,
  Colour the palette, Protocol and Highlight rules are lists, Profile suggests the profiles, the rest are text.

  Each column can be resized by dragging its heading's right edge (a double click puts it back), the widths kept for
  next time. New Session adds a row at the top, in the store the Store filter shows (else the first in use), which
  shows whatever the filters are and saves as a new session; × takes a new row away before it's saved.

  Export CSV… writes the rows shown, as they are in the table, saved or not, in Export Sessions to CSV's form, so a
  change can be tried and exported without saving it. Import CSV… brings a CSV's sessions in as edits: a row with a
  session's Source, Folder and Name fills in its cells, any other becomes a new row (sessionsTable.ts's applyCsv),
  all to look over before Save.

  Changed cells are marked, new rows as a whole, and nothing is saved until Save, which saves them all (sessionsTable.ts says how, store by
  store); Cancel or Escape drops them, asking first when there are some. A Database row the user can't change (Read
  only, no rights, or locked in the table) or a column locked there can't be edited, as in Edit Session.

  The window keeps one size whatever the filters show, so typing a filter doesn't resize it, and only the rows in view
  are drawn: a table of thousands of sessions scrolls as one of ten.
-->
<script lang="ts" module>
  let shown = $state(false);

  /** Whether Edit Sessions as Table is open, so the tree doesn't read the Database table again under it */
  export function sessionsTableShown(): boolean {
    return shown;
  }
</script>

<script lang="ts">
  import { onMount, tick } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../icons/Icon.svelte";
  import IconPicker from "../icons/IconPicker.svelte";
  import type { CustomIcon } from "../icons/custom";
  import type { EnvironmentColors } from "../icons/environment";
  import { PALETTE } from "../icons/palette";
  import { listProfiles } from "../settings/profiles";
  import { highlighting } from "../terminal/highlighting";
  import { ResizableColumns } from "../ui/columns.svelte";
  import { exportTableCsv, loadTable, PROTOCOLS, readTableCsv, type DbState, type SessionFields, type SessionsLoaded, type Store } from "./api";
  import {
    applyCsv, cellText, changed, COLUMNS, draftOf, isNew, locked, matches, newDraft, problems, rowChanged, saveDrafts,
    STORE_LABELS, type Column, type ColumnKey, type Draft,
  } from "./sessionsTable";

  interface Props {
    /** The sessions as the tree has them: which stores are in use */
    loaded: SessionsLoaded;
    /** How Database sessions stand, or null when they're off */
    db: DbState | null;
    /** The tree's environment colours, for the icons' circles and the folders' */
    colors: EnvironmentColors;
    /** The icons added to the library */
    customs: ReadonlyMap<string, CustomIcon>;
    /** Saved: the tree reads the sessions again; then the window closes */
    onSaved: (report: string) => void;
    /** Cancel or Escape, nothing saved */
    onClose: () => void;
  }

  let { loaded, db, colors, customs, onSaved, onClose }: Props = $props();

  /**
   * A row's height in pixels, as drawn: the CSS makes it 30, and the first row drawn is measured, so a font that
   * draws taller (another system's) can't put the rows in view out of step with the scroll position
   */
  let ROW = $state(30);
  /** Rows drawn above and below those in view, so a fast scroll doesn't show blanks */
  const SPARE = 10;

  let drafts = $state<Draft[]>([]);
  let loading = $state(true);
  let message = $state("");
  /** What Export CSV… or Import CSV… did, beside the count */
  let note = $state("");
  let saving = $state(false);
  let filters = $state<Partial<Record<ColumnKey, string>>>({});
  /** The row whose Folder is being typed in, its cell a field until it's left */
  let typingFolder = $state<Draft | null>(null);
  /** The row whose icon is being chosen (Choose Icon open) */
  let choosingIcon = $state<Draft | null>(null);
  /** The row whose colour is being chosen, and where its palette shows */
  let choosingColour = $state<{ draft: Draft; x: number; y: number } | null>(null);
  let profiles = $state<string[]>([]);
  let ruleSets = $state<{ id: string; name: string }[]>([]);
  let scrollTop = $state(0);
  /** The grid's height in view, measured as it opens, scrolls and the window resizes */
  let viewHeight = $state(0);
  let grid: HTMLDivElement | undefined = $state();

  /** Measure where the grid is scrolled to and how tall it is, for the rows in view */
  function measure(): void {
    if (!grid) return;
    scrollTop = grid.scrollTop;
    viewHeight = grid.clientHeight;
    const drawn = grid.querySelector("tbody tr[aria-label]")?.getBoundingClientRect().height;
    if (drawn) ROW = drawn;
  }

  // once rows are drawn, measure one
  $effect(() => {
    if (inView.length) queueMicrotask(measure);
  });

  /** Whether Database sessions can be changed at all: on, and neither Read only nor without rights */
  const writable = $derived(!!db?.enabled && db.writable);
  /** The stores a row can be moved to: those in use, and Database only when it can be written */
  const stores = $derived(
    (["saved", "private", "database"] as Store[]).filter((s) =>
      s === "saved" ? loaded.savedFile !== null && !loaded.savedOff : s === "private" ? !loaded.privateOff : writable),
  );
  const shownRows = $derived(drafts.filter((d) => matches(d, filters)));
  const changedRows = $derived(drafts.filter(rowChanged));
  const wrong = $derived(problems(drafts));
  /** The rows in view, and how far down the first is */
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW) - SPARE));
  const count = $derived(Math.ceil((viewHeight || 600) / ROW) + 2 * SPARE);
  const inView = $derived(shownRows.slice(first, first + count));
  /** Each store's folders, for Folder's suggestions */
  const folders = $derived(
    Object.fromEntries((["saved", "private", "database"] as Store[]).map((s) =>
      [s, [...new Set(drafts.filter((d) => d.store === s).map((d) => d.fields.folder).filter(Boolean))].sort()])),
  );
  /** The columns' widths, dragged at their headings' edges and kept for next time */
  const columns = new ResizableColumns("columns", COLUMNS.map((c) => c.width), 40, "sxt.sessionsTable.");

  onMount(() => {
    shown = true;
    measure();
    void (async () => {
      try {
        drafts = (await loadTable()).map(draftOf);
      } catch (e) {
        message = `Couldn't read the sessions: ${e}`;
      }
      loading = false;
    })();
    listProfiles().then((list) => (profiles = list.map((p) => p.name))).catch(() => {});
    highlighting()
      .then((h) => (ruleSets = h.ruleSets.map((r) => ({ id: String(r.id ?? ""), name: String(r.name ?? r.id ?? "") }))))
      .catch(() => {});
    return () => (shown = false);
  });

  /** New Session: a row at the top, in the store the Store filter shows, else the first in use; its Name to type in */
  async function addRow(): Promise<void> {
    const store = stores.includes(filters.store as Store) ? (filters.store as Store) : stores[0];
    if (!store) return;
    const draft = newDraft(store);
    drafts = [draft, ...drafts];
    if (grid) grid.scrollTop = 0;
    measure();
    await tick();
    grid?.querySelector<HTMLInputElement>(`tr[data-key="${draft.key}"] input[aria-label="Name"]`)?.focus();
  }

  /** Export CSV…: the rows shown, as they are now, saved or not */
  async function exportCsv(): Promise<void> {
    message = "";
    try {
      const rows: [Store, [Store, string] | null, SessionFields][] = shownRows.map((d) =>
        [d.store, isNew(d) ? null : [d.row.store, d.row.id], { ...d.fields, port: Number(d.fields.port) || 0 }]);
      const said = await exportTableCsv(rows, "Sessions.csv");
      if (said) note = `${said}. Nothing saved.`;
    } catch (e) {
      message = `Couldn't export: ${e}`;
    }
  }

  /** Import CSV…: a CSV's sessions brought in as edits, to look over before Save */
  async function importCsv(): Promise<void> {
    message = "";
    try {
      const read = await readTableCsv();
      if (!read) return;
      if (read.csv.error) {
        message = `${read.file}: ${read.csv.error}`;
        return;
      }
      const { drafts: next, report } = applyCsv(drafts, read.csv, stores, writable);
      drafts = next;
      if (grid) grid.scrollTop = 0;
      measure();
      const ignored = read.csv.ignored.length ? `; columns not used: ${read.csv.ignored.join(", ")}` : "";
      note = `From ${read.file}: ${report.changed} changed, ${report.added} new${ignored}. Not saved yet.`;
      if (report.problems.length) message = report.problems.join("\n");
    } catch (e) {
      message = `Couldn't import: ${e}`;
    }
  }

  /** Take a new row away before it's saved */
  function removeRow(draft: Draft): void {
    drafts = drafts.filter((d) => d !== draft);
  }

  /** A Database row that can't move at all: Read only, no rights, or locked in the table */
  const fixed = (d: Draft) => d.row.store === "database" && (!writable || d.row.locked);

  /** The tip on a cell that can't be changed */
  function lockTip(d: Draft, column: Column): string {
    if (!locked(d, column, writable)) return "";
    if (!writable) return "Database sessions are read-only here";
    return d.row.locked ? "This session is locked in the database" : "Locked in the database";
  }

  /** A text cell's new value; Port kept as typed until the save, which turns it into a number */
  function set(d: Draft, key: ColumnKey, value: string): void {
    // a new row's store is where it's made, not a move
    if (key === "store" && isNew(d)) d.store = d.row.store = value as Store;
    else if (key === "store") d.store = value as Store;
    else if (key === "port") (d.fields as unknown as Record<string, string>).port = value;
    else (d.fields as unknown as Record<string, string>)[key] = value;
  }

  /** A folder path for its cell: its parts with › between, or "(top)" for none */
  const folderShown = (path: string) => (path.trim() ? path.trim().replace(/^\/+|\/+$/g, "").split("/").join(" › ") : "(top)");

  /** Open the palette for a row, under its swatch */
  function openColour(d: Draft, button: HTMLElement): void {
    const box = button.getBoundingClientRect();
    choosingColour = { draft: d, x: box.left, y: box.bottom + 2 };
  }

  /** Save every change, then let the tree read the sessions again */
  async function save(): Promise<void> {
    if (!changedRows.length || wrong.size) return;
    saving = true;
    message = "";
    const report = await saveDrafts(drafts);
    saving = false;
    if (report.problems.length) {
      // what was saved stays saved: read the table again, with the problems shown
      message = report.problems.join("\n");
      try {
        drafts = (await loadTable()).map(draftOf);
      } catch {
        // the problems already say enough
      }
      return;
    }
    onSaved(`Saved ${report.saved} session${report.saved === 1 ? "" : "s"}`);
  }

  /** Cancel or Escape: the changes dropped, after asking when there are some */
  async function cancel(): Promise<void> {
    if (changedRows.length) {
      const sure = await ask(`Drop the changes to ${changedRows.length} session${changedRows.length === 1 ? "" : "s"}?`, {
        title: "Edit Sessions as Table", kind: "warning", okLabel: "Drop Changes",
      });
      if (!sure) return;
    }
    onClose();
  }

  /** Escape closes the palette, then the window; Choose Icon handles its own */
  function onKey(e: KeyboardEvent): void {
    if (e.key !== "Escape" || choosingIcon) return;
    if (choosingColour) choosingColour = null;
    else if (typingFolder) typingFolder = null;
    else void cancel();
  }
</script>

<svelte:window onkeydown={onKey} onresize={measure} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Edit Sessions as Table">
    <header>
      <span class="title">Edit Sessions as Table</span>
      <span class="hint">Change any cell, then Save. Filter under each heading. Changed cells are marked.</span>
    </header>

    <div class="grid" bind:this={grid} onscroll={measure}>
      <table style:width="{columns.total}px">
        <colgroup>{#each COLUMNS as c, i (c.key)}<col style:width="{columns.widths[i]}px" />{/each}</colgroup>
        <thead>
          <tr>
            {#each COLUMNS as c, i (c.key)}
              <th>{c.label}
                <span class="grip" role="separator" aria-orientation="vertical" aria-label="Resize {c.label}"
                  title="Drag to resize; double-click for the default width"
                  ondblclick={() => columns.reset(i)} onpointerdown={(e) => columns.startResize(e, i)}></span>
              </th>
            {/each}
          </tr>
          <tr class="filters">
            {#each COLUMNS as c (c.key)}
              <th>
                {#if c.key === "store"}
                  <select aria-label="Filter Store" value={filters.store ?? ""} onchange={(e) => (filters.store = e.currentTarget.value)}>
                    <option value="">All</option>
                    {#each ["saved", "private", "database"] as Store[] as s (s)}<option value={s}>{STORE_LABELS[s]}</option>{/each}
                  </select>
                {:else}
                  <input aria-label="Filter {c.label}" placeholder="Filter" value={filters[c.key] ?? ""}
                    oninput={(e) => (filters[c.key] = e.currentTarget.value)} />
                {/if}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          <tr class="spacer" style:height="{first * ROW}px"></tr>
          {#each inView as d (d.key)}
            {@const problem = wrong.get(d)}
            <tr class:changed-row={rowChanged(d)} class:new-row={isNew(d)} class:wrong={!!problem} title={problem ?? ""}
              aria-label={isNew(d) ? "New session" : d.row.id} data-key={d.key}>
              {#each COLUMNS as c (c.key)}
                {@const off = locked(d, c, writable)}
                <td class:changed={changed(d, c.key)} title={lockTip(d, c)}>
                  {#if c.key === "store"}
                    <div class="store">
                      {#if isNew(d)}
                        <button class="remove" aria-label="Remove new session" title="Take this new session away" onclick={() => removeRow(d)}>×</button>
                      {/if}
                      <select aria-label="Store" value={d.store} disabled={fixed(d)} onchange={(e) => set(d, "store", e.currentTarget.value)}>
                        {#each ["saved", "private", "database"] as Store[] as s (s)}
                          {#if stores.includes(s) || d.row.store === s}<option value={s}>{STORE_LABELS[s]}</option>{/if}
                        {/each}
                      </select>
                    </div>
                  {:else if c.key === "folder"}
                    {#if typingFolder === d}
                      <!-- svelte-ignore a11y_autofocus -->
                      <input aria-label="Folder" list="table-folders-{d.store}" value={d.fields.folder} autofocus
                        oninput={(e) => set(d, "folder", e.currentTarget.value)} onblur={() => (typingFolder = null)}
                        onkeydown={(e) => e.key === "Enter" && (typingFolder = null)} />
                    {:else}
                      <button class="folder" aria-label="Folder" disabled={off} onclick={() => (typingFolder = d)} onfocus={() => !off && (typingFolder = d)}
                        title={off ? lockTip(d, c) : `${d.fields.folder || "(top)"}: click to change`}>
                        <Icon key="folder" size={16} environment={colors.forFolder(d.store, d.fields.folder)} {customs} />
                        <span>{folderShown(d.fields.folder)}</span>
                      </button>
                    {/if}
                  {:else if c.key === "icon"}
                    <button class="icon" aria-label="Icon" disabled={off} onclick={() => (choosingIcon = d)} title={off ? lockTip(d, c) : d.fields.icon}>
                      <Icon key={d.fields.icon} size={18} environment={colors.forSession({ store: d.store, folder: d.fields.folder, name: d.fields.name, color: d.fields.color })} {customs} />
                    </button>
                  {:else if c.key === "color"}
                    <button class="swatch" aria-label="Colour" disabled={off} style:background={d.fields.color || "transparent"}
                      title={off ? lockTip(d, c) : d.fields.color || "None"} onclick={(e) => openColour(d, e.currentTarget)}></button>
                  {:else if c.key === "protocol"}
                    <select aria-label="Protocol" value={d.fields.protocol || "SSH"} disabled={off} onchange={(e) => set(d, "protocol", e.currentTarget.value)}>
                      {#if d.fields.protocol && !PROTOCOLS.some((p) => p.toUpperCase() === d.fields.protocol.toUpperCase())}
                        <option value={d.fields.protocol}>{d.fields.protocol}</option>
                      {/if}
                      {#each PROTOCOLS as p (p)}<option value={p}>{p}</option>{/each}
                    </select>
                  {:else if c.key === "highlightRules"}
                    <select aria-label="Highlight rules" value={d.fields.highlightRules} disabled={off} onchange={(e) => set(d, "highlightRules", e.currentTarget.value)}>
                      <option value="">(profile's)</option>
                      {#if d.fields.highlightRules && !ruleSets.some((r) => r.id === d.fields.highlightRules)}
                        <option value={d.fields.highlightRules}>{d.fields.highlightRules}</option>
                      {/if}
                      {#each ruleSets as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
                    </select>
                  {:else}
                    <input aria-label={c.label} value={cellText(d, c.key)} disabled={off} spellcheck="false"
                      list={c.key === "profile" ? "table-profiles" : undefined}
                      inputmode={c.key === "port" ? "numeric" : undefined}
                      oninput={(e) => set(d, c.key, e.currentTarget.value)} />
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
          <tr class="spacer" style:height="{Math.max(0, shownRows.length - first - inView.length) * ROW}px"></tr>
        </tbody>
      </table>
      {#if loading}
        <p class="empty">Reading the sessions…</p>
      {:else if !shownRows.length}
        <p class="empty">{drafts.length ? "No session matches the filters." : "No sessions yet."}</p>
      {/if}
    </div>

    <datalist id="table-profiles">{#each profiles as p (p)}<option value={p}></option>{/each}</datalist>
    {#each ["saved", "private", "database"] as Store[] as s (s)}
      <datalist id="table-folders-{s}">{#each folders[s] ?? [] as f (f)}<option value={f}></option>{/each}</datalist>
    {/each}

    <footer>
      <button onclick={() => void addRow()} disabled={!stores.length || loading}
        title="A new session, at the top of the table, saved with the rest">New Session</button>
      <span class="count">
        {shownRows.length === drafts.length ? `${drafts.length} sessions` : `${shownRows.length} of ${drafts.length} sessions`}{changedRows.length ? `, ${changedRows.length} changed` : ""}
      </span>
      <button onclick={() => (filters = {})} disabled={!Object.values(filters).some(Boolean)}>Clear Filters</button>
      <button onclick={() => void importCsv()} disabled={loading || saving}
        title="Bring a CSV's sessions in as changes to look over: those it names are changed, the rest added">Import CSV…</button>
      <button onclick={() => void exportCsv()} disabled={loading || !shownRows.length}
        title="The rows shown, as they are now, saved or not">Export CSV…</button>
      {#if note}<span class="note">{note}</span>{/if}
      <span class="message" role="status">{wrong.size ? [...wrong.values()][0] : message}</span>
      <button onclick={() => void cancel()} disabled={saving}>Cancel</button>
      <button class="primary" onclick={() => void save()} disabled={saving || !changedRows.length || wrong.size > 0}>
        {saving ? "Saving…" : "Save"}
      </button>
    </footer>
  </div>
</div>

{#if choosingColour}
  {@const target = choosingColour.draft}
  <div class="popover" role="dialog" aria-label="Colour" style:left="{choosingColour.x}px" style:top="{choosingColour.y}px">
    <button onclick={() => { target.fields.color = ""; choosingColour = null; }}>None</button>
    {#each PALETTE as p (p.color)}
      <button class="swatch" style:background={p.color} title={p.name} aria-label={p.name}
        onclick={() => { target.fields.color = p.color; choosingColour = null; }}></button>
    {/each}
    <label class="more">More…
      <input type="color" value={target.fields.color || "#C62828"}
        onchange={(e) => { target.fields.color = e.currentTarget.value.toUpperCase(); choosingColour = null; }} />
    </label>
  </div>
{/if}

{#if choosingIcon}
  {@const target = choosingIcon}
  <IconPicker value={target.fields.icon} {customs} onCancel={() => (choosingIcon = null)}
    onChoose={(key) => { target.fields.icon = key; choosingIcon = null; }} />
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  /* one size whatever the filters show, so typing one doesn't resize the window */
  .dialog {
    width: min(1400px, 96vw);
    height: min(780px, 90vh);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }

  .title {
    font-weight: 600;
  }

  .hint,
  .count,
  .empty {
    color: var(--text-muted);
  }

  .grid {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-pane);
  }

  table {
    table-layout: fixed;
    border-collapse: separate;
    border-spacing: 0;
  }

  /* the headings and filters stay at the top as the rows scroll */
  thead th {
    position: sticky;
    background: var(--bg-bar);
    z-index: 2;
    text-align: left;
    font-weight: 600;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }

  /* the headings' row a fixed height, so the filters' row sticks right under it */
  /* the drag handle at a heading's right edge, as the file transfer tabs' */
  .grip {
    position: absolute;
    top: 0;
    right: 0;
    width: 7px;
    height: 100%;
    cursor: col-resize;
  }

  .grip:hover {
    background: color-mix(in srgb, var(--accent) 35%, transparent);
  }

  thead tr:first-child th {
    top: 0;
    height: 25px;
    box-sizing: border-box;
  }

  thead tr.filters th {
    top: 25px;
    padding: 3px 4px;
    font-weight: normal;
  }

  /* exactly ROW (30px) with its border, as the rows in view are worked out from it */
  td {
    height: 30px;
    box-sizing: border-box;
    padding: 0 3px;
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    white-space: nowrap;
  }

  tr.spacer {
    border: none;
    padding: 0;
  }

  /* a changed cell: the accent down its left and a tint, so a row's changes are seen at a glance */
  td.changed {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    box-shadow: inset 3px 0 0 var(--accent);
  }

  /* a new row, marked as a whole: the accent down its left and a tint */
  tr.new-row td {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }

  tr.new-row td:first-child {
    box-shadow: inset 3px 0 0 var(--accent);
  }

  .store {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  button.remove {
    flex: none;
    width: 20px;
    height: 20px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-muted);
    font-size: 15px;
    line-height: 1;
  }

  button.remove:hover {
    color: #e5534b;
  }

  tr.wrong td {
    background: color-mix(in srgb, #e5534b 14%, transparent);
  }

  td input,
  td select,
  th input,
  th select {
    width: 100%;
    height: 24px;
    box-sizing: border-box;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 3px;
    padding: 3px 4px;
    color: inherit;
    font: inherit;
  }

  th input,
  th select {
    background: var(--bg-input);
    border-color: var(--border);
  }

  td input:hover:not(:disabled),
  td select:hover:not(:disabled) {
    border-color: var(--border);
  }

  td input:focus,
  td select:focus {
    background: var(--bg-input);
    border-color: var(--accent);
    outline: none;
  }

  td :disabled {
    opacity: 0.55;
  }

  /* the buttons in cells as tall as the fields, so every row is ROW high */
  button.folder {
    display: flex;
    align-items: center;
    gap: 5px;
    width: 100%;
    height: 24px;
    box-sizing: border-box;
    background: none;
    border: 1px solid transparent;
    padding: 2px 4px;
    text-align: left;
    color: inherit;
    overflow: hidden;
  }

  button.folder span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  button.folder:hover:not(:disabled) {
    border-color: var(--border);
    border-radius: 3px;
  }

  button.icon {
    background: none;
    border: 1px solid transparent;
    padding: 0 6px;
    display: flex;
    align-items: center;
    height: 24px;
    box-sizing: border-box;
    overflow: hidden;
  }

  .swatch {
    width: 22px;
    height: 18px;
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 0;
  }

  /* None's diagonal, as Edit Session's swatch shows no colour */
  td .swatch[style*="transparent"] {
    background: linear-gradient(to top right, transparent 45%, #e5534b 47%, #e5534b 53%, transparent 55%) !important;
  }

  .empty {
    position: absolute;
    top: 70px;
    left: 0;
    right: 0;
    text-align: center;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .note {
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 30%;
  }

  .message {
    flex: 1;
    color: #e5534b;
    white-space: pre-line;
    max-height: 3.6em;
    overflow: auto;
  }

  footer button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 10px;
  }

  footer .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  footer button:disabled {
    opacity: 0.45;
  }

  .popover {
    position: fixed;
    z-index: 130;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    width: 180px;
    padding: 6px;
    background: var(--bg-pane);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.3);
  }

  .popover button:not(.swatch) {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 1px 6px;
  }

  .more {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .more input {
    width: 28px;
    height: 20px;
    padding: 0;
  }
</style>
