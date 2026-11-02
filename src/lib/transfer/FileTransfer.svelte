<!--
  A file transfer tab, as SuperXPuTTY's File transfer (pscp): this Mac's files (Local) beside the session's (Remote),
  and the transfers below (Id, Source, Target, Start, End, Progress, Status, Message), run one at a time. Files are
  copied by dragging them across (or onto a folder), a double click, the Upload/Download buttons, the right-click
  menu, or dropping them from Finder onto the remote side. The connection is the session's own ssh (sftp.rs).
-->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { ask as confirm } from "@tauri-apps/plugin-dialog";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import FilePane from "./FilePane.svelte";
  import { askName } from "../scripting/asking.svelte";
  import type { Store } from "../sessions/api";
  import {
    cancelTransfer, closeTransfer, formatSize, joinPath, listLocal, listRemote, localMkdir, localRename, openTransfer,
    remoteDelete, remoteMkdir, remoteRename, revealLocal, transfer, type Entry, type Listing, type Opened,
  } from "./api";

  let { store, id }: { store: Store; id: string } = $props();

  let conn = $state<Opened | null>(null);
  let failed = $state("");
  let localHome = $state("");

  /** Each side: what it shows, and whether it's loading or failed */
  const sides = $state({
    local: { listing: null as Listing | null, loading: false, error: "" },
    remote: { listing: null as Listing | null, loading: false, error: "" },
  });

  interface Job {
    id: number;
    upload: boolean;
    sources: string[];
    target: string;
    status: "Waiting" | "Running" | "Done" | "Failed" | "Cancelled";
    total: number;
    done: number;
    file: string;
    started: number;
    ended: number;
    message: string;
  }
  let jobs = $state<Job[]>([]);
  let nextJob = 1;
  let running = false;
  let unlistenDrop: (() => void) | null = null;
  let remoteElement = $state<HTMLElement | undefined>();

  /** Connect, and show both sides where they start */
  async function connect(): Promise<void> {
    failed = "";
    conn = null;
    try {
      conn = await openTransfer(store, id);
      localHome = conn.local;
      await Promise.all([load("local", conn.local), load("remote", conn.remote)]);
    } catch (e) {
      failed = String(e);
    }
  }

  onMount(async () => {
    void connect();
    // files dropped from Finder onto the remote side are uploaded to its folder
    unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop" || !remoteElement || !conn || !sides.remote.listing) return;
      const rect = remoteElement.getBoundingClientRect();
      const scale = window.devicePixelRatio || 1;
      const x = event.payload.position.x / scale;
      const y = event.payload.position.y / scale;
      if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) void queue(true, event.payload.paths, sides.remote.listing.path);
    });
  });

  onDestroy(() => {
    unlistenDrop?.();
    for (const job of jobs) if (job.status === "Running") void cancelTransfer(job.id);
    if (conn) void closeTransfer(conn.id);
  });

  async function load(side: "local" | "remote", path: string): Promise<void> {
    const state = sides[side];
    state.loading = true;
    state.error = "";
    try {
      state.listing = side === "local" ? await listLocal(path) : await listRemote(conn!.id, path);
    } catch (e) {
      state.error = String(e);
    } finally {
      state.loading = false;
    }
  }

  /** Copy into a folder on the other side, asking first about names already there */
  async function queue(upload: boolean, sources: string[], target: string): Promise<void> {
    const destination = sides[upload ? "remote" : "local"].listing;
    if (destination?.path === target) {
      const there = new Set(destination.entries.map((e) => e.name));
      const clashes = sources.filter((s) => there.has(s.replace(/\/+$/, "").split("/").pop() ?? ""));
      if (clashes.length) {
        const names = clashes.map((c) => c.split("/").pop()).slice(0, 5).join(", ") + (clashes.length > 5 ? ", …" : "");
        const replace = await confirm(`${clashes.length === 1 ? "This is" : "These are"} already in ${target}: ${names}.\n\nReplace ${clashes.length === 1 ? "it" : "them"}, or skip ${clashes.length === 1 ? "it" : "them"}?`,
          { title: "Replace files?", kind: "warning", okLabel: "Replace", cancelLabel: "Skip" });
        if (!replace) sources = sources.filter((s) => !clashes.includes(s));
      }
    }
    if (!sources.length) return;
    jobs = [...jobs, { id: nextJob++, upload, sources, target, status: "Waiting", total: 0, done: 0, file: "", started: 0, ended: 0, message: "" }];
    void run();
  }

  /** Run waiting transfers, one at a time */
  async function run(): Promise<void> {
    if (running || !conn) return;
    running = true;
    try {
      for (let job = jobs.find((j) => j.status === "Waiting"); job; job = jobs.find((j) => j.status === "Waiting")) {
        const current = job;
        current.status = "Running";
        current.started = Date.now();
        try {
          await transfer(conn.id, current.id, current.upload, current.sources, current.target, (p) => {
            if (p.type === "total") current.total = p.bytes;
            else {
              current.done = p.bytes;
              current.file = p.file;
            }
          });
          current.status = "Done";
          current.message = current.total ? `${formatSize(current.total)} in ${seconds(Date.now() - current.started)}` : "";
        } catch (e) {
          const text = String(e);
          current.status = text === "Cancelled" ? "Cancelled" : "Failed";
          current.message = text === "Cancelled" ? "" : text;
        }
        current.ended = Date.now();
        // the folder it went to shows what arrived
        const side = current.upload ? "remote" : "local";
        if (sides[side].listing?.path === current.target) void load(side, current.target);
      }
    } finally {
      running = false;
    }
  }

  function seconds(ms: number): string {
    return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
  }

  async function mkdir(side: "local" | "remote"): Promise<void> {
    const listing = sides[side].listing;
    if (!listing) return;
    const name = await askName("New Folder", "Name:", "", "Create");
    if (!name) return;
    try {
      const path = joinPath(listing.path, name);
      if (side === "local") await localMkdir(path);
      else await remoteMkdir(conn!.id, path);
      await load(side, listing.path);
    } catch (e) {
      sides[side].error = String(e);
    }
  }

  async function rename(side: "local" | "remote", entry: Entry): Promise<void> {
    const listing = sides[side].listing;
    if (!listing) return;
    const name = await askName(`Rename ${entry.name}`, "New name:", entry.name, "Rename");
    if (!name || name === entry.name) return;
    try {
      if (side === "local") await localRename(joinPath(listing.path, entry.name), joinPath(listing.path, name));
      else await remoteRename(conn!.id, joinPath(listing.path, entry.name), joinPath(listing.path, name));
      await load(side, listing.path);
    } catch (e) {
      sides[side].error = String(e);
    }
  }

  async function remove(entries: Entry[]): Promise<void> {
    const listing = sides.remote.listing;
    if (!listing || !entries.length) return;
    const folders = entries.some((e) => e.dir);
    const what = entries.length === 1 ? `"${entries[0].name}"` : `these ${entries.length} items`;
    const sure = await confirm(`Delete ${what} from ${conn?.address}?${folders ? " Folders are deleted with everything in them." : ""} This can't be undone.`,
      { title: "Delete", kind: "warning", okLabel: "Delete", cancelLabel: "Cancel" });
    if (!sure) return;
    try {
      for (const entry of entries) await remoteDelete(conn!.id, joinPath(listing.path, entry.name));
    } catch (e) {
      sides.remote.error = String(e);
    }
    await load("remote", listing.path);
  }

  function time(ms: number): string {
    return ms ? new Date(ms).toLocaleTimeString() : "";
  }

  const percent = (job: Job) => (job.total ? Math.min(100, Math.round((job.done / job.total) * 100)) : job.status === "Done" ? 100 : 0);
  const describe = (paths: string[]) => (paths.length === 1 ? paths[0] : `${paths[0]} and ${paths.length - 1} more`);
</script>

<div class="transfer">
  {#if failed}
    <div class="failed">
      <p>{failed}</p>
      <button onclick={() => void connect()}>Try again</button>
    </div>
  {:else if !conn}
    <div class="failed"><p>Connecting…</p></div>
  {:else}
    <div class="panes">
      <FilePane side="local" title="Local (this Mac)" home={localHome} {...sides.local}
        onNavigate={(p) => void load("local", p)}
        onCopy={(paths) => sides.remote.listing && void queue(true, paths, sides.remote.listing.path)}
        onDrop={(paths, folder) => void queue(false, paths, folder)}
        onMkdir={() => void mkdir("local")}
        onRename={(e) => void rename("local", e)}
        onReveal={(e) => sides.local.listing && void revealLocal(e.name ? joinPath(sides.local.listing.path, e.name) : sides.local.listing.path)} />
      <div class="remote" bind:this={remoteElement}>
        <FilePane side="remote" title={`Remote (${conn.address})`} home={conn.home} {...sides.remote}
          onNavigate={(p) => void load("remote", p)}
          onCopy={(paths) => sides.local.listing && void queue(false, paths, sides.local.listing.path)}
          onDrop={(paths, folder) => void queue(true, paths, folder)}
          onMkdir={() => void mkdir("remote")}
          onRename={(e) => void rename("remote", e)}
          onDelete={(entries) => void remove(entries)} />
      </div>
    </div>

    <div class="queue">
      <table>
        <thead>
          <tr><th class="id">Id</th><th>Source</th><th>Target</th><th class="t">Start</th><th class="t">End</th><th class="progress">Progress</th><th class="status">Status</th><th>Message</th><th class="act"></th></tr>
        </thead>
        <tbody>
          {#each jobs as job (job.id)}
            <tr>
              <td class="id">{job.id}</td>
              <td title={job.sources.join("\n")}>{describe(job.sources)}</td>
              <td title={job.target}>{job.target}</td>
              <td class="t">{time(job.started)}</td>
              <td class="t">{time(job.ended)}</td>
              <td class="progress" title={job.file}>
                <div class="bar"><div style:width="{percent(job)}%"></div></div>
                <span>{percent(job)}%{job.total ? ` of ${formatSize(job.total)}` : ""}</span>
              </td>
              <td class="status" class:bad={job.status === "Failed"}>{job.status}</td>
              <td class="message" title={job.message}>{job.status === "Running" ? job.file : job.message}</td>
              <td class="act">
                {#if job.status === "Running"}
                  <button title="Stop this transfer" onclick={() => void cancelTransfer(job.id)}>Stop</button>
                {:else if job.status === "Waiting"}
                  <button title="Take it off the list" onclick={() => (jobs = jobs.filter((j) => j !== job))}>✕</button>
                {/if}
              </td>
            </tr>
          {:else}
            <tr><td colspan="9" class="empty">Drag files from one side to the other (or onto a folder), double-click a file, or drop files from Finder on the remote side.</td></tr>
          {/each}
        </tbody>
      </table>
      {#if jobs.some((j) => ["Done", "Failed", "Cancelled"].includes(j.status))}
        <button class="clear" onclick={() => (jobs = jobs.filter((j) => j.status === "Waiting" || j.status === "Running"))}>Clear finished</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .transfer {
    height: 100%;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px;
    box-sizing: border-box;
    background: var(--bg-window);
    color: var(--text);
    font-family: var(--font-ui);
    font-size: 13px;
  }

  .panes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .panes > :global(*) {
    min-height: 0;
  }

  .remote {
    display: flex;
    flex-direction: column;
  }

  .remote > :global(.pane) {
    flex: 1;
  }

  .queue {
    height: 170px;
    overflow: auto;
    background: var(--bg-pane);
    border: 1px solid var(--border);
    position: relative;
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
  }

  td {
    padding: 2px 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .id {
    width: 32px;
  }

  .t {
    width: 82px;
  }

  .progress {
    width: 150px;
  }

  .status {
    width: 70px;
  }

  .act {
    width: 54px;
  }

  .bad {
    color: #d9534f;
  }

  .bar {
    display: inline-block;
    width: 60px;
    height: 7px;
    margin-right: 6px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 3px;
    vertical-align: middle;
    overflow: hidden;
  }

  .bar div {
    height: 100%;
    background: var(--accent);
  }

  .empty {
    color: var(--text-muted);
    white-space: normal;
    padding: 10px;
  }

  td button,
  .clear,
  .failed button {
    padding: 0 6px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    color: var(--text);
  }

  .clear {
    position: absolute;
    right: 6px;
    bottom: 6px;
  }

  .failed {
    margin: auto;
    max-width: 640px;
    text-align: center;
    color: var(--text-muted);
    white-space: pre-wrap;
  }
</style>
