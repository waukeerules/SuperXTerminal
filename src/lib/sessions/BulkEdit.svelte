<!--
  Edit sessions…, as SuperXPuTTY's dlgBulkEdit: change the same fields on many sessions at once, e.g. a new username
  for every device at a site. Lists the sessions (a folder's with its subfolders', or the selection), all ticked; only
  the fields ticked are changed. Database sessions go to the table in one batch, and those with a changed field
  locked there are left alone and listed.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import Icon from "../icons/Icon.svelte";
  import IconPicker from "../icons/IconPicker.svelte";
  import type { CustomIcon } from "../icons/custom";
  import { PALETTE } from "../icons/palette";
  import { highlighting } from "../terminal/highlighting";
  import { listProfiles, DEFAULT_PROFILE } from "../settings/profiles";
  import { PROTOCOLS, bulkEdit, type BulkChange, type SessionView } from "./api";

  interface Props {
    /** "in Lab" or "3 selected sessions", for the title */
    title: string;
    /** The sessions that can be changed, all ticked to start with */
    sessions: SessionView[];
    /** The size of the icons in the list, as the Sessions tree's */
    iconSize: number;
    /** The user's custom icons, for the list and Choose Icon */
    customs: ReadonlyMap<string, CustomIcon>;
    /** Done: how many changed, and what was left alone */
    onDone: (done: number, problems: string[]) => void;
    /** Cancel, or Escape: nothing changes */
    onCancel: () => void;
  }

  let { title, sessions, iconSize, customs, onDone, onCancel }: Props = $props();

  /** A session's key in the ticked set, "store:id", as its id alone may be in two stores */
  const keyOf = (s: SessionView) => `${s.store}:${s.id}`;
  /** The sessions in id order, as the list shows them */
  const sorted = $derived([...sessions].sort((a, b) => a.id.localeCompare(b.id, undefined, { sensitivity: "base", numeric: true })));
  let ticked = $state(new Set(untrack(() => sessions.map(keyOf))));

  /** Which fields are changed, and their new values */
  let use = $state({ username: false, profile: false, port: false, protocol: false, icon: false, color: false, extraArgs: false, jumpHost: false, highlightRules: false });
  let values = $state({ username: "", profile: DEFAULT_PROFILE, port: 22, protocol: "SSH", icon: "computer", color: "", extraArgs: "", jumpHost: "", highlightRules: "" });

  let profileNames = $state<string[]>([]);
  let ruleSets = $state<{ id: string; name: string }[]>([]);
  let choosingIcon = $state(false);
  let error = $state("");
  let busy = $state(false);

  onMount(async () => {
    profileNames = (await listProfiles()).map((p) => p.name);
    ruleSets = (await highlighting()).ruleSets.map((r) => ({ id: String(r.id ?? ""), name: String(r.name ?? r.id ?? "") }));
  });

  /** Tick or untick a session */
  function toggle(key: string, on: boolean): void {
    const next = new Set(ticked);
    if (on) next.add(key);
    else next.delete(key);
    ticked = next;
  }

  /** Change the ticked sessions' ticked fields, then say how many changed and what was left alone */
  async function apply(): Promise<void> {
    const change: BulkChange = {};
    if (use.username) change.username = values.username;
    if (use.profile) change.profile = values.profile;
    if (use.port) change.port = Number(values.port) || 0;
    if (use.protocol) change.protocol = values.protocol;
    if (use.icon) change.icon = values.icon;
    if (use.color) change.color = values.color;
    if (use.extraArgs) change.extraArgs = values.extraArgs;
    if (use.jumpHost) change.jumpHost = values.jumpHost;
    if (use.highlightRules) change.highlightRules = values.highlightRules;
    if (Object.keys(change).length === 0) {
      error = "Tick the fields to change.";
      return;
    }
    const items = sorted.filter((s) => ticked.has(keyOf(s))).map((s) => [s.store, s.id] as [SessionView["store"], string]);
    if (items.length === 0) {
      error = "Tick the sessions to change.";
      return;
    }
    busy = true;
    try {
      const report = await bulkEdit(items, change);
      onDone(report.done, report.problems);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !choosingIcon && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Edit sessions {title}">
    <header>Edit sessions {title}</header>
    <p class="note">Only the fields ticked are changed, on the sessions ticked.</p>

    <div class="list" role="list">
      {#each sorted as s (keyOf(s))}
        <label class="item" title={s.locked ? "Locked in the database" : ""}>
          <input type="checkbox" checked={ticked.has(keyOf(s))} onchange={(e) => toggle(keyOf(s), e.currentTarget.checked)} />
          <span class="where">{s.store === "database" ? `Database/${s.id.replace(/^@db\//, "")}` : s.id}</span>
          {#if s.host}<span class="host">({s.host})</span>{/if}
        </label>
      {/each}
    </div>
    <div class="all">
      <button onclick={() => (ticked = new Set(sessions.map(keyOf)))}>Tick all</button>
      <button onclick={() => (ticked = new Set())}>Untick all</button>
      <span class="count">{ticked.size} of {sessions.length}</span>
    </div>

    <div class="fields">
      <label class="use"><input type="checkbox" bind:checked={use.username} /> Login username:</label>
      <input bind:value={values.username} placeholder="e.g. admin, or {'{user}'}" disabled={!use.username} />

      <label class="use"><input type="checkbox" bind:checked={use.profile} /> Profile:</label>
      <span>
        <input list="bulk-profiles" bind:value={values.profile} disabled={!use.profile} />
        <datalist id="bulk-profiles">{#each profileNames as n}<option value={n}></option>{/each}</datalist>
      </span>

      <label class="use"><input type="checkbox" bind:checked={use.port} /> Port:</label>
      <input class="port" type="number" min="0" max="65535" bind:value={values.port} disabled={!use.port} title="0 for the protocol's usual port" />

      <label class="use"><input type="checkbox" bind:checked={use.protocol} /> Connection type:</label>
      <select bind:value={values.protocol} disabled={!use.protocol}>{#each PROTOCOLS as p}<option value={p}>{p}</option>{/each}</select>

      <label class="use"><input type="checkbox" bind:checked={use.icon} /> Icon:</label>
      <span><button class="icon" disabled={!use.icon} onclick={() => (choosingIcon = true)} aria-label="Choose Icon">
        <Icon key={values.icon} size={iconSize} {customs} /></button></span>

      <label class="use"><input type="checkbox" bind:checked={use.color} /> Colour:</label>
      <select bind:value={values.color} disabled={!use.color}>
        <option value="">None (use the folder's or a rule's)</option>
        {#each PALETTE as p}<option value={p.color}>{p.name}</option>{/each}
      </select>

      <label class="use"><input type="checkbox" bind:checked={use.extraArgs} /> Extra arguments:</label>
      <input bind:value={values.extraArgs} disabled={!use.extraArgs} placeholder="replaces each session's" />

      <label class="use"><input type="checkbox" bind:checked={use.jumpHost} /> Jump host:</label>
      <input bind:value={values.jumpHost} disabled={!use.jumpHost} placeholder="none; or a session, or admin@10.0.0.1:22" />

      <label class="use"><input type="checkbox" bind:checked={use.highlightRules} /> Highlight:</label>
      <select bind:value={values.highlightRules} disabled={!use.highlightRules}>
        <option value="">From the folder, or Settings</option>
        <option value="auto">Auto (picked from the prompt)</option>
        {#each ruleSets as r}<option value={r.id}>{r.name}</option>{/each}
        <option value="none">None</option>
      </select>
    </div>

    <footer>
      <span class="error">{error}</span>
      <button class="primary" disabled={busy} onclick={apply}>Change {ticked.size} session{ticked.size === 1 ? "" : "s"}</button>
      <button onclick={onCancel}>Cancel</button>
    </footer>
  </div>
</div>

{#if choosingIcon}
  <IconPicker value={values.icon} {customs} onCancel={() => (choosingIcon = false)}
    onChoose={(key) => { values.icon = key; choosingIcon = false; }} />
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

  .dialog {
    width: min(620px, 94vw);
    max-height: 94vh;
    overflow: auto;
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
    height: 160px;
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
  }

  .host {
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

  .fields {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 10px;
    align-items: center;
  }

  .use {
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }

  .fields span {
    display: flex;
  }

  .fields span input {
    flex: 1;
  }

  input,
  select,
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

  .port {
    width: 90px;
  }

  button.icon {
    display: flex;
    padding: 2px;
  }

  :disabled {
    opacity: 0.45;
  }

  footer {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
  }

  .error {
    flex: 1;
    color: #e5534b;
  }
</style>
