<!--
  The Log Viewer (View > Log Viewer), as SuperXPuTTY's Log4netLogViewer: SuperXTerminal's own log (src-tauri/src/applog.rs)
  under the tabs, read again every half second as SuperXPuTTY's timer pulls it; the newest at the bottom, followed
  while the pane is scrolled to the end. The file itself is in ~/Library/Logs/SuperXTerminal on the Mac,
  %LOCALAPPDATA%\SuperXTerminal\logs on Windows.
-->
<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  interface Props {
    /** The pane's close button */
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  /** One entry of the app's log, as applog.rs keeps it */
  interface Entry {
    seq: number;
    time: string;
    level: "INFO" | "WARN" | "ERROR";
    text: string;
  }

  let entries = $state<Entry[]>([]);
  let level = $state<"INFO" | "WARN" | "ERROR">("INFO");
  let filter = $state("");
  let list: HTMLDivElement;
  let last = 0;

  /** The levels in order, for showing a level and those above it */
  const RANK = { INFO: 0, WARN: 1, ERROR: 2 };
  /** The entries at the level chosen or above, with the filter's text */
  const shown = $derived(
    entries.filter((e) => RANK[e.level] >= RANK[level] && (!filter.trim() || e.text.toLowerCase().includes(filter.trim().toLowerCase()))),
  );

  /** Add the entries logged since the last pull, keeping the newest 2000, and follow the end if it's showing */
  async function pull(): Promise<void> {
    const fresh = await invoke<Entry[]>("applog_since", { after: last }).catch(() => []);
    if (fresh.length === 0) return;
    last = fresh[fresh.length - 1].seq;
    // follow the end only when it's already there
    const atEnd = !list || list.scrollTop + list.clientHeight >= list.scrollHeight - 4;
    entries = [...entries, ...fresh].slice(-2000);
    if (atEnd) {
      await tick();
      list?.scrollTo({ top: list.scrollHeight });
    }
  }

  onMount(() => {
    void pull();
    const timer = setInterval(() => void pull(), 500);
    return () => clearInterval(timer);
  });

  /** Clear: empty the log kept in memory and the pane */
  async function clear(): Promise<void> {
    await invoke("applog_clear");
    entries = [];
  }

  /** Copy the entries shown, one per line */
  function copy(): void {
    void navigator.clipboard.writeText(shown.map((e) => `${e.time} ${e.level} ${e.text}`).join("\n"));
  }
</script>

<div class="pane">
  <div class="bar">
    <span class="title">Log Viewer</span>
    <select bind:value={level} aria-label="Show from this level">
      <option value="INFO">Everything</option>
      <option value="WARN">Warnings and errors</option>
      <option value="ERROR">Errors</option>
    </select>
    <input bind:value={filter} placeholder="Filter…" aria-label="Filter the log" />
    <span class="gap"></span>
    <button onclick={copy} title="Copy what's shown">Copy</button>
    <button onclick={() => void clear()} title="Empty the pane (the file keeps it)">Clear</button>
    <button onclick={() => void invoke("applog_reveal")} title="Show SuperXTerminal.log in its folder">Show Log File</button>
    <button class="close" onclick={onClose} title="Hide the Log Viewer (View > Log Viewer)" aria-label="Hide the Log Viewer">×</button>
  </div>
  <div class="list" bind:this={list} role="log">
    {#each shown as entry (entry.seq)}
      <div class="entry {entry.level.toLowerCase()}"><span class="time">{entry.time}</span> <span class="level">{entry.level}</span> {entry.text}</div>
    {:else}
      <p class="empty">Nothing logged{filter || level !== "INFO" ? " that matches" : " yet"}.</p>
    {/each}
  </div>
</div>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    height: 180px;
    border-top: 1px solid var(--border);
    background: var(--bg-pane);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px;
    font-size: 12px;
  }

  .title {
    color: var(--text-muted);
    font-size: 13px;
    margin-right: 6px;
  }

  .gap {
    flex: 1;
  }

  select,
  input,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 12px;
  }

  input {
    width: 180px;
  }

  .close {
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font-size: 15px;
    line-height: 1;
  }

  .list {
    flex: 1;
    overflow: auto;
    padding: 2px 8px 6px;
    font-family: ui-monospace, Menlo, monospace;
    font-size: 11.5px;
    user-select: text;
  }

  .entry {
    white-space: pre-wrap;
    word-break: break-word;
  }

  .time {
    color: var(--text-muted);
  }

  .level {
    display: inline-block;
    width: 3.6em;
  }

  .warn .level {
    color: #d29922;
  }

  .error {
    color: #e5534b;
  }

  .empty {
    margin: 6px 0;
    color: var(--text-muted);
    font-family: inherit;
  }
</style>
