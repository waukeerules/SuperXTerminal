<!--
  Connect all on a folder (or Favorites, Recent, or a selection) with more than ten sessions: rather than opening a
  tab for every one, the sessions are listed and up to ten can be ticked to open.
-->
<script lang="ts">
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";
  import type { SessionView } from "./api";

  interface Props {
    /** "in Lab", for the title */
    title: string;
    sessions: SessionView[];
    iconSize: number;
    customs: ReadonlyMap<string, CustomIcon>;
    environment: (session: SessionView) => string | null;
    onConnect: (sessions: SessionView[]) => void;
    onCancel: () => void;
  }

  let { title, sessions, iconSize, customs, environment, onConnect, onCancel }: Props = $props();

  /** The most opened at once */
  const MAX = 10;
  const keyOf = (s: SessionView) => `${s.store}:${s.id}`;

  let ticked = $state<string[]>([]);
  let filter = $state("");

  const shown = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    return needle ? sessions.filter((s) => [s.name, s.host, s.folder].some((f) => f.toLowerCase().includes(needle))) : sessions;
  });
  const full = $derived(ticked.length >= MAX);

  function toggle(session: SessionView, on: boolean): void {
    const key = keyOf(session);
    ticked = on ? (ticked.includes(key) || full ? ticked : [...ticked, key]) : ticked.filter((k) => k !== key);
  }

  function connect(): void {
    const chosen = new Set(ticked);
    onConnect(sessions.filter((s) => chosen.has(keyOf(s))));
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Connect sessions {title}">
    <header>Connect sessions {title}</header>
    <p class="note">There are {sessions.length} sessions. Tick up to {MAX} to open, each in its own tab.</p>
    <input class="filter" bind:value={filter} placeholder="Type to filter…" aria-label="Filter the sessions" />

    <div class="list" role="list">
      {#each shown as s (keyOf(s))}
        {@const on = ticked.includes(keyOf(s))}
        <label class="item" class:off={full && !on} role="listitem">
          <input type="checkbox" checked={on} disabled={full && !on} onchange={(e) => toggle(s, e.currentTarget.checked)} />
          <Icon key={s.icon} size={Math.min(iconSize, 20)} environment={environment(s)} {customs} />
          <span class="name">{s.name}</span>
          <span class="where">{s.folder}{s.host ? ` · ${s.host}` : ""}</span>
        </label>
      {:else}
        <p class="empty">No session matches.</p>
      {/each}
    </div>

    <div class="all">
      <button onclick={() => (ticked = [...ticked, ...shown.map(keyOf).filter((k) => !ticked.includes(k))].slice(0, MAX))}>Tick the first {MAX}</button>
      <button disabled={ticked.length === 0} onclick={() => (ticked = [])}>Untick all</button>
      <span class="count" class:full>{ticked.length} of {MAX}</span>
    </div>

    <footer>
      <button class="primary" disabled={ticked.length === 0} onclick={connect}>Connect {ticked.length || ""}</button>
      <button onclick={onCancel}>Cancel</button>
    </footer>
  </div>
</div>

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

  .dialog {
    width: min(560px, 94vw);
    max-height: 94vh;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    font-weight: 600;
  }

  .note {
    margin: 0;
    color: var(--text-muted);
  }

  .list {
    height: 300px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px;
    background: var(--bg-input);
  }

  .item {
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
    padding: 1px 2px;
  }

  .item.off {
    opacity: 0.5;
  }

  .name {
    flex: none;
  }

  .where {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text-muted);
  }

  .empty {
    margin: 8px;
    color: var(--text-muted);
  }

  .all {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .count {
    color: var(--text-muted);
  }

  .count.full {
    color: var(--accent);
    font-weight: 600;
  }

  input,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  input[type="checkbox"] {
    padding: 0;
  }

  :disabled {
    opacity: 0.45;
  }

  footer {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
</style>
