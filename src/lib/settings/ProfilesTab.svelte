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

  interface Props {
    /** The profile to show first (Quick connect's Edit…, Edit Session's Profiles…); Default Settings when it's none */
    initial?: string | null;
  }

  let { initial = null }: Props = $props();

  let profiles = $state<Profile[]>([]);
  let selected = $state(DEFAULT_PROFILE);
  /** The profile being edited, a copy until Save */
  let draft = $state<Profile>(emptyProfile(DEFAULT_PROFILE));
  /** The draft's name when it was loaded; "" for a new profile */
  let originalName = $state(DEFAULT_PROFILE);
  /** The profile as last loaded or saved (or as a new one starts), to tell which fields have changed */
  let saved = $state<Profile>(emptyProfile(DEFAULT_PROFILE));
  /** A new password typed in; undefined keeps the stored one */
  let password = $state<string | undefined>(undefined);
  let message = $state("");
  /** SSH algorithms… open over the tab */
  let algorithming = $state(false);

  const isDefault = $derived(originalName.toLowerCase() === DEFAULT_PROFILE.toLowerCase());

  type Field = keyof Profile | "password";

  /**
   * A field's value as it counts: numbers as numbers, and on Default Settings a blank three-way choice as what it
   * means there (keys offered, username not asked), so choosing that explicitly isn't a change
   */
  function meaning(profile: Profile, field: keyof Profile): unknown {
    const value = profile[field];
    if (field === "port" || field === "keepalive") return Number(value) || 0;
    if (isDefault && field === "offerKeys") return value ?? true;
    if (isDefault && field === "askUsername") return value ?? false;
    return value ?? null;
  }

  /** Whether a field differs from the saved profile; the password when one was typed in or forgotten */
  function changed(field: Field): boolean {
    if (field === "password") return password !== undefined;
    return meaning(draft, field) !== meaning(saved, field);
  }

  /** The fields the editor shows, for counting the changes */
  const FIELDS: Field[] = [
    "name", "username", "password", "keyFile", "offerKeys", "askUsername", "port", "jumpHost", "host", "keepalive",
    "olderAlgorithms", "algorithms", "sshConfigHost", "sshOptions", "cursor", "cursorBlink", "bell",
  ];
  /** How many fields have changed since the profile was loaded or saved */
  const changes = $derived(FIELDS.filter(changed).length);
  onMount(async () => {
    profiles = await listProfiles();
    // a session's profile is matched by name whatever its case, as the launch matches it
    const wanted = initial ? profiles.find((p) => p.name.toLowerCase() === initial.trim().toLowerCase()) : undefined;
    select(wanted?.name ?? DEFAULT_PROFILE);
  });

  /** Edit a profile from the list */
  function select(name: string): void {
    const profile = profiles.find((p) => p.name === name);
    if (!profile) return;
    selected = name;
    draft = { ...profile };
    saved = { ...profile };
    originalName = profile.name;
    password = undefined;
    message = "";
  }

  /** Start a new profile: it asks for the password rather than offering SSH keys, as most are for network devices */
  function add(): void {
    selected = "";
    draft = { ...emptyProfile(""), offerKeys: false };
    saved = { ...draft };
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

  /** Throw the changes away: the profile as it was loaded or saved (a new one, blank again) */
  function revert(): void {
    draft = { ...saved };
    password = undefined;
    message = "";
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
        <!-- another profile can't be opened over unsaved changes: Save or Revert first -->
        <li role="option" aria-selected={profile.name === selected} aria-disabled={changes > 0}
          class:selected={profile.name === selected} class:locked={changes > 0}
          title={changes > 0 && profile.name !== selected ? "Save or revert the changes first" : undefined}
          onclick={() => changes === 0 && select(profile.name)}
          onkeydown={(e) => e.key === "Enter" && changes === 0 && select(profile.name)} tabindex="0">
          {profile.name}
        </li>
      {/each}
    </ul>
    <div class="list-buttons">
      <button onclick={add} disabled={changes > 0}>New</button>
      <button onclick={remove} disabled={changes > 0 || isDefault || originalName === ""}>Remove</button>
    </div>
  </div>

  <div class="form">
    <!-- stays put while the fields scroll -->
    <div class="head">
      <strong class="title">{originalName || draft.name.trim() || "New profile"}</strong>
      <span class="message" class:pending={changes > 0}>
        {changes > 0 ? `Unsaved changes (${changes})` : message}
      </span>
      <button onclick={revert} disabled={changes === 0} title="Put the fields back as they were">Revert</button>
      <button class="primary" onclick={save} disabled={changes === 0}>Save</button>
    </div>

    <div class="fields">
    <p class="hint">
      A session's PuTTY profile is looked up here by name. The session's own fields win, then its folder's
      defaults, then the profile, then Default Settings.
    </p>

    <fieldset>
      <legend>Profile</legend>
      <label>{@render caption("Name", "name")}<input bind:value={draft.name} disabled={isDefault} placeholder="e.g. root" /></label>
    </fieldset>

    <fieldset>
      <legend>Login</legend>
      <label>{@render caption("Username", "username")}<input bind:value={draft.username} placeholder="e.g. admin" /></label>
      <label>{@render caption("Password", "password")}
        <input type="password" value={password ?? ""} oninput={(e) => (password = e.currentTarget.value)}
          placeholder={draft.hasPassword ? "Stored in the Keychain" : "none"} />
        {#if draft.hasPassword}<button onclick={() => (password = "")} title="Forget the stored password">Forget</button>{/if}
      </label>
      <label>{@render caption("Key file", "keyFile")}<input bind:value={draft.keyFile} placeholder="ssh's own keys and the agent" />
        <button onclick={chooseKey}>Choose…</button></label>
      <!-- Default Settings has no default of its own to point at: its blank is the built-in behaviour -->
      <label>{@render caption("SSH keys", "offerKeys")}
        <select value={triToValue(isDefault ? (draft.offerKeys ?? true) : draft.offerKeys)}
          onchange={(e) => (draft.offerKeys = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="on">Offer ~/.ssh keys and the agent's</option>
          <option value="off">Don't offer them: ask for the password (switches, routers)</option>
        </select></label>
      <p class="note under">A Cisco closes the connection after 3 failed attempts, and every key offered is one, so with
        no stored password its password prompt never comes. A key file above is always used.</p>
      <label>{@render caption("No username", "askUsername")}
        <select value={triToValue(isDefault ? (draft.askUsername ?? false) : draft.askUsername)}
          onchange={(e) => (draft.askUsername = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="off">Use this computer's user name (ssh's own)</option>
          <option value="on">Ask in the tab ("login as:")</option>
        </select></label>
    </fieldset>

    <fieldset>
      <legend>Connection</legend>
      <label>{@render caption("Port", "port")}<input class="short" type="number" min="0" max="65535" bind:value={draft.port} /> <span class="note">0: not set</span></label>
      <label>{@render caption("Jump host", "jumpHost")}<input bind:value={draft.jumpHost} placeholder="a session, or admin@10.0.0.1:22" /></label>
      <label>{@render caption("Host", "host")}<input bind:value={draft.host} placeholder="none; e.g. example.cyberark.cloud" /></label>
      <p class="note under">Used, at the Port above, by sessions with no host of their own, as a PuTTY saved session's
        Host Name; and by sessions whose username names their host as {"{host_address}"} (a CyberArk SSH gateway, which
        takes the target in the username). Otherwise a session's own host wins. See docs/CYBERARK.md.</p>
      <label>{@render caption("Keep-alive", "keepalive")}<input class="short" type="number" min="0" bind:value={draft.keepalive} /> <span class="note">seconds (0: none)</span></label>
      <label>{@render caption("Older algorithms", "olderAlgorithms")}
        <select value={triToValue(draft.olderAlgorithms)} onchange={(e) => (draft.olderAlgorithms = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="on">Allow (older Cisco and similar devices)</option>
          <option value="off">Don't allow</option>
        </select></label>
      <label>{@render caption("SSH algorithms", "algorithms")}
        <button onclick={() => (algorithming = true)} title={draft.algorithms ? `Its own: ${draft.algorithms}` : isDefault ? "ssh's own lists" : "From Default Settings"}>
          Choose…</button>
        <span class="note">{draft.algorithms ? "its own lists" : isDefault ? "ssh's own lists" : "from Default Settings"}</span></label>
      <label>{@render caption("SSH config host", "sshConfigHost")}<input bind:value={draft.sshConfigHost} placeholder="a Host in ~/.ssh/config" /></label>
      <label>{@render caption("More ssh options", "sshOptions")}<input bind:value={draft.sshOptions} placeholder="e.g. -L 8080:localhost:80" /></label>
    </fieldset>

    <fieldset>
      <legend>Terminal</legend>
      <label>{@render caption("Cursor", "cursor")}
        <select bind:value={draft.cursor}>
          <option value="">{isDefault ? "Block" : "From Default Settings"}</option>
          <option value="block">Block</option>
          <option value="underline">Underline</option>
          <option value="bar">Vertical line</option>
        </select></label>
      <label>{@render caption("Cursor blinks", "cursorBlink")}
        <select value={triToValue(draft.cursorBlink)} onchange={(e) => (draft.cursorBlink = valueToTri(e.currentTarget.value))}>
          <option value="">{isDefault ? "Yes" : "From Default Settings"}</option>
          <option value="on">Yes</option>
          <option value="off">No</option>
        </select></label>
      <label>{@render caption("Bell", "bell")}
        <select bind:value={draft.bell}>
          <option value="">{isDefault ? "Flash" : "From Default Settings"}</option>
          <option value="visual">Flash</option>
          <option value="sound">Sound</option>
          <option value="none">None</option>
        </select></label>
    </fieldset>

    </div>
  </div>
</div>

<!-- a field's caption, with a dot while it differs from the saved profile -->
{#snippet caption(text: string, field: Field)}
  <span class="caption">{text}{#if changed(field)}<span class="dot" title="Changed: not saved yet" aria-label="changed"></span>{/if}</span>
{/snippet}

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
  .head {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  /* the editor: its header fixed, the fields scrolling under it */
  .form {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .head {
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 40%;
  }

  .fields {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-right: 4px;
  }

  li.locked:not(.selected) {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .caption {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  /* a field changed since it was loaded or saved */
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
    flex: none;
  }

  .hint,
  .note,
  .message {
    color: var(--text-muted);
    margin: 0;
  }

  /* a note for the row above, lined up with its control (label's first column and gap) */
  .note.under {
    margin-left: 138px;
    font-size: 12px;
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

  .message {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .message.pending {
    color: var(--accent);
  }
</style>
