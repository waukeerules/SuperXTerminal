<!--
  The Quick connect toolbar, as SuperXPuTTY's: Protocol, Host, Login, Password and the profile to use ("Session" in
  SuperXPuTTY, a PuTTY profile; here one of the user's profiles), then Connect and Clear. Enter in any field
  connects. The host may be "host", "host:port", "user@host" or "ssh://host:port". Nothing is saved: the password
  is used for this one connection.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../icons/Icon.svelte";
  import { PROTOCOLS, unavailableReason, opensAsLocalShell } from "../sessions/api";
  import { DEFAULT_PROFILE, listProfiles } from "../settings/profiles";
  import type { QuickConnect } from "../terminal/PageHost";

  interface Props {
    onConnect: (quick: QuickConnect, title: string, local: boolean) => void;
    /** Shown in the status bar when something can't be opened */
    onProblem: (text: string) => void;
  }

  let { onConnect, onProblem }: Props = $props();

  let protocol = $state("SSH");
  let host = $state("");
  let username = $state("");
  let password = $state("");
  let profile = $state(DEFAULT_PROFILE);
  let profiles = $state<string[]>([DEFAULT_PROFILE]);

  onMount(() => void refreshProfiles());

  /** The profile list, read again when it's opened so new profiles show */
  async function refreshProfiles(): Promise<void> {
    profiles = (await listProfiles()).map((p) => p.name);
    if (!profiles.includes(profile)) profile = DEFAULT_PROFILE;
  }

  /** Connect with what's in the fields */
  function connect(): void {
    const scheme = host.includes("://") ? host.slice(0, host.indexOf("://")).toUpperCase() : protocol;
    const reason = unavailableReason(scheme);
    if (reason) {
      onProblem(`${scheme}: ${reason}`);
      return;
    }
    const local = opensAsLocalShell(scheme);
    if (!host.trim() && !local) {
      onProblem("Type a host to connect to.");
      return;
    }
    const title = host.trim().replace(/^\w+:\/\//, "") || "Local Shell";
    onConnect({ protocol, host, username, password, profile }, title, local);
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
      connect();
    }
  }
</script>

<div class="toolbar" aria-label="Quick connect">
  <label>Protocol
    <select bind:value={protocol}>
      {#each PROTOCOLS as p}<option value={p} disabled={!!unavailableReason(p)} title={unavailableReason(p) ?? ""}>{p}</option>{/each}
    </select>
  </label>
  <label class="grow">Host <input bind:value={host} placeholder="e.g. 192.168.0.5" onkeydown={enter} /></label>
  <label class="grow">Login <input bind:value={username} placeholder="e.g. admin" onkeydown={enter} /></label>
  <label class="grow">Password <input type="password" bind:value={password} onkeydown={enter} /></label>
  <label>Profile
    <select bind:value={profile} onfocus={refreshProfiles} onkeydown={enter}>
      {#each profiles as p}<option value={p}>{p}</option>{/each}
    </select>
  </label>
  <button class="glyph" title="Connect" aria-label="Connect" onclick={connect}><Icon key="check" size={22} glyph /></button>
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
