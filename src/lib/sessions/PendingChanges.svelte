<!--
  Database sessions > Pending changes, as SuperXPuTTY's dlgPendingChanges: the changes that haven't reached the
  table, waiting while it can't be reached or refused (someone else's change, a lock, a name clash). Send again
  retries the selected ones, Overwrite theirs saves a refused edit over someone else's change, Discard drops them.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { discardPending, pendingChanges, retryPending, type DbState, type PendingChange } from "./api";

  let { onChanged, onClose }: { onChanged: (state: DbState) => void; onClose: () => void } = $props();

  let changes = $state<PendingChange[]>([]);
  let chosen = $state(new Set<string>());
  let busy = $state("");
  let error = $state("");

  const picked = $derived(changes.filter((c) => chosen.has(c.id)));

  onMount(() => void fill());

  /** List the changes, keeping the selection */
  async function fill(): Promise<void> {
    try {
      changes = await pendingChanges();
      chosen = new Set([...chosen].filter((id) => changes.some((c) => c.id === id)));
    } catch (e) {
      error = String(e);
    }
  }

  function toggle(id: string): void {
    const next = new Set(chosen);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    chosen = next;
  }

  /** Send the selected changes again, optionally over someone else's edits */
  async function retry(overwrite: boolean): Promise<void> {
    busy = "Sending…";
    error = "";
    try {
      onChanged(await retryPending(picked.map((c) => c.id), overwrite));
    } catch (e) {
      error = String(e);
    }
    busy = "";
    await fill();
  }

  /** Drop the selected changes, after asking */
  async function discard(): Promise<void> {
    const n = picked.length;
    const sure = await ask(`Discard ${n} change${n === 1 ? "" : "s"}? The sessions go back to what the database has.`,
      { title: "Pending changes", kind: "warning", okLabel: "Discard" });
    if (!sure) return;
    try {
      onChanged(await discardPending(picked.map((c) => c.id)));
    } catch (e) {
      error = String(e);
    }
    await fill();
  }

  function status(c: PendingChange): string {
    return c.error === null ? (c.force ? "Waiting (will overwrite)" : "Waiting to be sent") : `Not saved: ${c.error}`;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Pending changes">
    <header>Pending changes</header>
    <p class="hint">Changes to Database sessions that aren't in the table yet. Waiting ones are sent when it can be reached.</p>
    <div class="list" role="listbox" aria-multiselectable="true">
      {#each changes as c (c.id)}
        <div class="item" class:chosen={chosen.has(c.id)} class:failed={c.error !== null} role="option" aria-selected={chosen.has(c.id)}
          tabindex="-1" onclick={() => toggle(c.id)} onkeydown={(e) => e.key === " " && toggle(c.id)} title={c.error ?? c.what}>
          <span class="what">{c.what}</span>
          <span class="when">{c.queuedAt}</span>
          <span class="status">{status(c)}</span>
        </div>
      {:else}
        <div class="empty">Nothing is waiting: every change is in the table.</div>
      {/each}
    </div>
    <footer>
      <span class="error">{error || busy}</span>
      <button disabled={picked.length === 0 || !!busy} title="Try the selected changes again; they're sent now if the database can be reached"
        onclick={() => retry(false)}>Send again</button>
      <button disabled={!picked.some((c) => c.canOverwrite) || !!busy} title="Save the selected edits over someone else's change"
        onclick={() => retry(true)}>Overwrite theirs</button>
      <button disabled={picked.length === 0 || !!busy} title="Drop the selected changes; the sessions go back to what the database has"
        onclick={discard}>Discard</button>
      <button class="primary" onclick={onClose}>Close</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 130;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(720px, 94vw);
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header,
  .hint,
  .when,
  .empty {
    color: var(--text-muted);
  }

  .hint {
    margin: 0;
  }

  .list {
    height: 240px;
    overflow: auto;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
  }

  .item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 50px minmax(0, 1.2fr);
    gap: 8px;
    padding: 4px 8px;
    cursor: default;
  }

  .item > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item.chosen {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .item.failed .status {
    color: #e5534b;
  }

  .empty {
    padding: 8px;
  }

  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
  }

  button:disabled {
    opacity: 0.5;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .error {
    flex: 1;
    color: #e5534b;
  }
</style>
