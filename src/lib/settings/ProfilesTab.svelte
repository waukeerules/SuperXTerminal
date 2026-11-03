<!--
  Settings > Profiles: the user's own profiles, matched by name to the PuTTY profile a session names ("root",
  "Default Settings"), as their PuTTY profiles are on Windows. The shared session says what to connect to; the
  profile says how this user logs in. Empty fields fall back to Default Settings.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { DEFAULT_PROFILE, deleteProfile, emptyProfile, listProfiles, saveProfile, type Profile } from "./profiles";
  import AlgorithmsDialog from "../sessions/AlgorithmsDialog.svelte";

  let profiles = $state<Profile[]>([]);
  let selected = $state(DEFAULT_PROFILE);
  /** The profile being edited, a copy until Save */
  let draft = $state<Profile>(emptyProfile(DEFAULT_PROFILE));
  /** The draft's name when it was loaded; "" for a new profile */
  let originalName = $state(DEFAULT_PROFILE);
  /** A new password typed in; undefined keeps the stored one */
  let password = $state<string | undefined>(undefined);
  let message = $state("");
  /** SSH algorithms… open over the tab */
  let algorithming = $state(false);

  const isDefault = $derived(originalName.toLowerCase() === DEFAULT_PROFILE.toLowerCase());

  onMount(async () => {
    profiles = await listProfiles();
    select(DEFAULT_PROFILE);
  });

  /** Edit a profile from the list */
  function select(name: string): void {
    const profile = profiles.find((p) => p.name === name);
    if (!profile) return;
    selected = name;
    draft = { ...profile };
    originalName = profile.name;
    password = undefined;
    message = "";
  }

  /** Start a new profile */
  function add(): void {
    selected = "";
    draft = emptyProfile("");
    originalName = "";
    password = undefined;
    message = "Name it after the PuTTY profile your sessions use, e.g. root.";
  }

  /** Save the draft */
  async function save(): Promise<void> {
    try {
      profiles = await saveProfile({ ...draft, port: Number(draft.port) || 0, keepalive: Number(draft.keepalive) || 0 }, originalName, password);
      select(draft.name.trim());
      message = "Saved.";
    } catch (e) {
      message = String(e);
    }
  }

  /** Remove the selected profile */
  async function remove(): Promise<void> {
    try {
      profiles = await deleteProfile(originalName);
      select(DEFAULT_PROFILE);
      message = "Removed.";
    } catch (e) {
      message = String(e);
    }
  }

  /** Choose a private key file */
  async function chooseKey(): Promise<void> {
    const path = await open({ title: "Choose a private key (OpenSSH format)", defaultPath: "~/.ssh" });
    if (typeof path === "string") draft.keyFile = path;
  }

  /** A three-way choice (default / on / off) as a select's value, and back */
  const triToValue = (v: boolean | null) => (v === null ? "" : v ? "on" : "off");
  const valueToTri = (v: string) => (v === "" ? null : v === "on");
</script>

<div class="profiles">
  <div class="list">
    <ul role="listbox" aria-label="Profiles">
      {#each profiles as profile (profile.name)}
        <li role="option" aria-selected={profile.name === selected} class:selected={profile.name === selected}
          onclick={() => select(profile.name)} onkeydown={(e) => e.key === "Enter" && select(profile.name)} tabindex="0">
          {profile.name}
        </li>
      {/each}
    </ul>
    <div class="list-buttons">
      <button onclick={add}>New</button>
      <button onclick={remove} disabled={isDefault || originalName === ""}>Remove</button>
    </div>
  </div>

  <div class="form">
    <p class="hint">
      A session's PuTTY profile is looked up here by name. The session's own fields win, then its folder's
      defaults, then the profile, then Default Settings.
    </p>

    <fieldset>
      <legend>Profile</legend>
      <label>Name <input bind:value={draft.name} disabled={isDefault} placeholder="e.g. root" /></label>
    </fieldset>

    <fieldset>
      <legend>Login</legend>
      <label>Username <input bind:value={draft.username} placeholder="e.g. admin" /></label>
      <label>Password
        <input type="password" value={password ?? ""} oninput={(e) => (password = e.currentTarget.value)}
          placeholder={draft.hasPassword ? "Stored in the Keychain" : "none"} />
        {#if draft.hasPassword}<button onclick={() => (password = "")} title="Forget the stored password">Forget</button>{/if}
      </label>
      <label>Key file <input bind:value={draft.keyFile} placeholder="ssh's own keys and the agent" />
        <button onclick={chooseKey}>Choose…</button></label>
    </fieldset>

    <fieldset>
      <legend>Connection</legend>
      <label>Port <input class="short" type="number" min="0" max="65535" bind:value={draft.port} /> <span class="note">0: not set</span></label>
      <label>Jump host <input bind:value={draft.jumpHost} placeholder="a session, or admin@10.0.0.1:22" /></label>
      <label>Keep-alive <input class="short" type="number" min="0" bind:value={draft.keepalive} /> <span class="note">seconds (0: none)</span></label>
      <label>Older algorithms
        <select value={triToValue(draft.olderAlgorithms)} onchange={(e) => (draft.olderAlgorithms = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="on">Allow (older Cisco and similar devices)</option>
          <option value="off">Don't allow</option>
        </select></label>
      <label>SSH algorithms
        <button onclick={() => (algorithming = true)} title={draft.algorithms ? `Its own: ${draft.algorithms}` : isDefault ? "ssh's own lists" : "From Default Settings"}>
          Choose…{draft.algorithms ? " ●" : ""}</button>
        <span class="note">{draft.algorithms ? "its own lists" : isDefault ? "ssh's own lists" : "from Default Settings"}</span></label>
      <label>SSH config host <input bind:value={draft.sshConfigHost} placeholder="a Host in ~/.ssh/config" /></label>
      <label>More ssh options <input bind:value={draft.sshOptions} placeholder="e.g. -L 8080:localhost:80" /></label>
    </fieldset>

    <fieldset>
      <legend>Terminal</legend>
      <label>Cursor
        <select bind:value={draft.cursor}>
          <option value="">{isDefault ? "Block" : "From Default Settings"}</option>
          <option value="block">Block</option>
          <option value="underline">Underline</option>
          <option value="bar">Vertical line</option>
        </select></label>
      <label>Cursor blinks
        <select value={triToValue(draft.cursorBlink)} onchange={(e) => (draft.cursorBlink = valueToTri(e.currentTarget.value))}>
          <option value="">{isDefault ? "Yes" : "From Default Settings"}</option>
          <option value="on">Yes</option>
          <option value="off">No</option>
        </select></label>
      <label>Bell
        <select bind:value={draft.bell}>
          <option value="">{isDefault ? "Flash" : "From Default Settings"}</option>
          <option value="visual">Flash</option>
          <option value="sound">Sound</option>
          <option value="none">None</option>
        </select></label>
    </fieldset>

    <div class="actions">
      <span class="message">{message}</span>
      <button class="primary" onclick={save}>Save profile</button>
    </div>
  </div>
</div>

{#if algorithming}
  <AlgorithmsDialog title={`SSH algorithms: ${draft.name || "new profile"}`} text={draft.algorithms ?? ""}
    otherwise={isDefault ? "ssh's own list (with the older algorithms, if allowed)" : "Default Settings' list"}
    onCancel={() => (algorithming = false)} onDone={(text) => { draft.algorithms = text; algorithming = false; }} />
{/if}

<style>
  .profiles {
    display: flex;
    gap: 16px;
    min-height: 0;
    height: 100%;
  }

  .list {
    width: 200px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  ul {
    flex: 1;
    margin: 0;
    padding: 4px;
    list-style: none;
    border: 1px solid var(--border);
    background: var(--bg-input);
    overflow: auto;
  }

  li {
    padding: 4px 8px;
    border-radius: 3px;
  }

  li.selected {
    background: var(--accent);
    color: #fff;
  }

  .list-buttons,
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .form {
    flex: 1;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .hint,
  .note,
  .message {
    color: var(--text-muted);
    margin: 0;
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    display: grid;
    gap: 6px;
    padding: 8px 12px;
  }

  label {
    display: grid;
    grid-template-columns: 130px 1fr auto;
    align-items: center;
    gap: 8px;
  }

  label:has(.short) {
    grid-template-columns: 130px 90px 1fr;
  }

  input,
  select {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 3px 6px;
    min-width: 0;
  }

  .actions {
    justify-content: flex-end;
  }

  .message {
    flex: 1;
  }
</style>
