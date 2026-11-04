<!--
  Folder settings, as SuperXPuTTY's: the folder's environment colour, and defaults for the sessions below that have
  none of their own (username, profile, port, jump host, highlighting). Saved by path in Folders.xml (Saved
  sessions) or PrivateFolders.xml (Private ones), so a team sharing the settings folder shares them.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { PALETTE } from "../icons/palette";
  import { highlighting } from "../terminal/highlighting";
  import { listProfiles } from "../settings/profiles";
  import Combobox from "../ui/Combobox.svelte";
  import { saveFolderSettings, type FolderSetting, type Store } from "./api";

  interface Props {
    /** Which sessions the folder is in: Saved, Private or Database */
    store: Store;
    /** The folder's settings as stored, which the window starts from */
    setting: FolderSetting;
    /** Saved: the tree reads the settings again */
    onSaved: () => void;
    /** Cancel, or Escape: nothing changes */
    onCancel: () => void;
  }

  let { store, setting, onSaved, onCancel }: Props = $props();

  let draft = $state<FolderSetting>(untrack(() => ({ ...setting })));
  let profileNames = $state<string[]>([]);
  let ruleSets = $state<{ id: string; name: string }[]>([]);
  let error = $state("");

  onMount(async () => {
    profileNames = (await listProfiles()).map((p) => p.name);
    ruleSets = (await highlighting()).ruleSets.map((r) => ({ id: String(r.id ?? ""), name: String(r.name ?? r.id ?? "") }));
  });

  /** Save and close */
  async function save(): Promise<void> {
    try {
      await saveFolderSettings(store, { ...draft, port: Number(draft.port) || 0 });
      onSaved();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Folder settings">
    <header>Folder settings: {draft.path} — {store === "saved" ? "Folders.xml" : "PrivateFolders.xml"}</header>
    <p class="hint">Sessions in this folder, and in folders below it, use these when they don't set their own.</p>

    <div class="colours">
      <span class="label">Colour:</span>
      <button class:selected={!draft.color} onclick={() => (draft.color = "")}>None</button>
      {#each PALETTE as p}
        <button class="swatch" class:selected={draft.color.toUpperCase() === p.color} style:background={p.color}
          title={p.name} aria-label={p.name} onclick={() => (draft.color = p.color)}></button>
      {/each}
      <label class="more">More…
        <input type="color" value={draft.color || "#C62828"} oninput={(e) => (draft.color = e.currentTarget.value.toUpperCase())} />
      </label>
    </div>

    <label>Login username: <input bind:value={draft.username} placeholder="e.g. netops, or {'{user}'}" /></label>
    <label>Profile:
      <Combobox bind:value={draft.puttyProfile} options={profileNames} placeholder="none" />
    </label>
    <label>Port: <input class="port" type="number" min="0" max="65535" bind:value={draft.port} /> <span class="hint">0: none</span></label>
    <label>Jump host: <input bind:value={draft.jumpHost} placeholder="none; or a session, or admin@10.0.0.1:22" /></label>
    <label>Highlighting:
      <select bind:value={draft.highlightRules}>
        <option value="">None set (Default)</option>
        <option value="auto">Auto (picked from the prompt)</option>
        {#each ruleSets as r}<option value={r.id}>{r.name}</option>{/each}
        <option value="none">None</option>
      </select>
    </label>

    <footer>
      <span class="error">{error}</span>
      <button class="primary" onclick={save}>Save</button>
      <button onclick={onCancel}>Cancel</button>
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
    width: min(620px, 94vw);
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    color: var(--text-muted);
  }

  .hint {
    margin: 0;
    color: var(--text-muted);
  }

  label {
    display: grid;
    grid-template-columns: 130px 1fr;
    align-items: center;
    gap: 8px;
  }

  label:has(.port) {
    grid-template-columns: 130px 90px 1fr;
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

  .colours {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }

  .colours .label {
    width: 130px;
  }

  .swatch {
    width: 26px;
    height: 26px;
    padding: 0;
  }

  .selected {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .more {
    display: inline-block;
    position: relative;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
    overflow: hidden;
  }

  .more input {
    position: absolute;
    inset: 0;
    opacity: 0;
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
