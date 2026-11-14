<!--
  The Quick connect toolbar, as SuperXPuTTY's: Protocol, Host, Login, Password and the profile to use ("Session" in
  SuperXPuTTY, a PuTTY profile; here one of the user's profiles), then Connect and Clear. Enter in any field
  connects. The host may be "host", "host:port", "user@host" or "ssh://host:port", or several of them with commas,
  spaces or new lines between, which open a tab each (quickHosts.ts); the button beside Host opens them as a list,
  one on each line, to type or paste a column of them (HostList.svelte). Nothing is saved: the password is used for
  these connections alone. A tab's menu can save its session afterwards (Save session as..., without the password).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../icons/Icon.svelte";
  import { settings } from "../settings/store.svelte";
  import { PROTOCOLS, unavailableReason, opensAsLocalShell } from "../sessions/api";
  import { DEFAULT_PROFILE, PROFILES_CHANGED, listProfiles } from "../settings/profiles";
  import type { QuickConnect } from "../terminal/PageHost";
  import { MANY_HOSTS, pastedHosts, splitHosts } from "./quickHosts";
  import HostList from "./HostList.svelte";

  interface Props {
    /**
     * Connect: the session typed, its tab's title, and whether it opens as a local shell (a protocol with no host).
     * Called once for each host typed.
     */
    onConnect: (quick: QuickConnect, title: string, local: boolean) => void;
    /** Shown in the status bar when something can't be opened */
    onProblem: (text: string) => void;
    /** Edit… beside Profile: open Settings > Profiles on the profile chosen */
    onEditProfile: (name: string) => void;
  }

  let { onConnect, onProblem, onEditProfile }: Props = $props();

  let protocol = $state("SSH");
  let host = $state("");
  let username = $state("");
  let password = $state("");
  let profile = $state(DEFAULT_PROFILE);
  let profiles = $state<string[]>([DEFAULT_PROFILE]);
  /** Whether the list of hosts is open under the Host box (HostList.svelte) */
  let listOpen = $state(false);
  /** The Host box, and the box with its list button around it, which the list goes under */
  let hostBox = $state<HTMLInputElement>();
  let hostArea = $state<HTMLElement>();

  onMount(() => {
    void refreshProfiles();
    const changed = () => void refreshProfiles();
    window.addEventListener(PROFILES_CHANGED, changed);
    return () => window.removeEventListener(PROFILES_CHANGED, changed);
  });

  /** The profile list, read again whenever profiles change in Settings */
  async function refreshProfiles(): Promise<void> {
    profiles = (await listProfiles()).map((p) => p.name);
    if (!profiles.includes(profile)) profile = DEFAULT_PROFILE;
  }

  /**
   * Connect with what's in the fields: a tab for each host typed (quickHosts.ts), all with the same Login, Password
   * and Profile. Nothing opens when one of them can't be, so a list isn't left half opened; and more than
   * MANY_HOSTS are asked about first, as a stray paste could hold hundreds.
   */
  async function connect(): Promise<void> {
    const typed = splitHosts(host);
    // no host at all is still one session: a protocol that opens as a local shell needs none
    const entries = (typed.length ? typed : [""]).map((entry) => {
      const scheme = entry.includes("://") ? entry.slice(0, entry.indexOf("://")).toUpperCase() : protocol;
      return { entry, scheme, local: opensAsLocalShell(scheme), title: entry.replace(/^\w+:\/\//, "") || "Local Shell" };
    });
    for (const { entry, scheme, local } of entries) {
      const reason = unavailableReason(scheme);
      if (reason) {
        onProblem(`${scheme}: ${reason}`);
        return;
      }
      if (!entry && !local) {
        onProblem("Type a host to connect to.");
        return;
      }
    }
    if (entries.length > MANY_HOSTS && !(await ask(`Open ${entries.length} sessions, one for each host typed?`, { title: "Quick connect", kind: "warning" }))) return;
    for (const { entry, title, local } of entries) onConnect({ protocol, host: entry, username, password, profile }, title, local);
  }

  /** A column of hosts pasted into Host goes in with commas between: the box alone would run them together */
  function paste(event: ClipboardEvent): void {
    const list = pastedHosts(event.clipboardData?.getData("text") ?? "");
    if (list === null) return;
    event.preventDefault();
    const box = event.currentTarget as HTMLInputElement;
    const start = box.selectionStart ?? host.length;
    const end = box.selectionEnd ?? host.length;
    // a comma before and after too, where it joins hosts already typed
    const before = host.slice(0, start);
    const after = host.slice(end);
    host = `${before}${before.trim() && !/[,;]\s*$/.test(before) ? ", " : ""}${list}${after.trim() && !/^\s*[,;]/.test(after) ? ", " : ""}${after}`;
  }

  /** Close the list of hosts; `refocus` puts the keyboard back in the Host box, which holds them with commas between */
  function closeList(refocus: boolean): void {
    listOpen = false;
    if (refocus) hostBox?.focus();
  }

  /** Empty the fields, as SuperXPuTTY's Clear button */
  function clear(): void {
    protocol = "SSH";
    host = username = password = "";
    profile = DEFAULT_PROFILE;
  }

  /** Enter in a field connects */
  function enter(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void connect();
    }
  }
</script>

<div class="toolbar" aria-label="Quick connect">
  <label><Icon key="computer" size={Math.round(settings.iconSize * 0.85)} />Protocol
    <select bind:value={protocol}>
      {#each PROTOCOLS as p}<option value={p} disabled={!!unavailableReason(p)} title={unavailableReason(p) ?? ""}>{p}</option>{/each}
    </select>
  </label>
  <span class="grow host" bind:this={hostArea}>
    <label>Host <input id="quick-connect-host" bind:this={hostBox} bind:value={host} placeholder="e.g. 192.168.0.5" onkeydown={enter} onpaste={paste} /></label>
    <!-- the press is kept from taking the keyboard, so pressing it with the list open closes it once, not closes and opens it -->
    <button class="list" class:on={listOpen} title="Several hosts: type or paste them, one on each line" aria-label="List of hosts" aria-expanded={listOpen}
      onmousedown={(e) => e.preventDefault()} onclick={() => (listOpen ? closeList(true) : (listOpen = true))}>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <path d="M2 4h1.5M6 4h8M2 8h1.5M6 8h8M2 12h1.5M6 12h8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </button>
    {#if listOpen && hostArea}
      <HostList hosts={host} anchor={hostArea} onChange={(list) => (host = list)} onClose={closeList} onConnect={() => void connect()} />
    {/if}
  </span>
  <label class="grow">Login <input bind:value={username} placeholder="e.g. admin" onkeydown={enter} /></label>
  <label class="grow">Password <input type="password" bind:value={password} onkeydown={enter} /></label>
  <label>Profile
    <select bind:value={profile} onkeydown={enter}>
      {#each profiles as p}<option value={p}>{p}</option>{/each}
    </select>
    <button title="Edit this profile in Settings > Profiles" onclick={() => onEditProfile(profile)}>Edit…</button>
  </label>
  <button class="glyph" title="Connect" aria-label="Connect" onclick={() => void connect()}><Icon key="check" size={22} glyph /></button>
  <button class="glyph" title="Clear the fields" aria-label="Clear" onclick={clear}><Icon key="clear" size={22} glyph /></button>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 8px;
    background: var(--bg-bar);
    border-bottom: 1px solid var(--border);
  }

  label {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .grow {
    flex: 1;
  }

  .grow input {
    flex: 1;
  }

  /* Host: its box and the list button as one, the box taking the room */
  .host {
    display: flex;
    align-items: center;
    gap: 3px;
  }

  /* no narrower than Login and Password get in a small window: all three give way alike */
  .host label {
    flex: 1;
  }

  .list {
    display: flex;
    padding: 3px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--text-muted);
  }

  .list:hover,
  .list.on {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }

  input,
  select {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 2px;
    padding: 2px 4px;
    min-width: 0;
  }

  .glyph {
    border: 0;
    background: none;
    padding: 0;
    line-height: 0;
  }
</style>
