<!--
  Tools > Environment Variables, as SuperXPuTTY's: the user's values for {key} placeholders in sessions, so one
  shared Sessions.xml works for everyone ("{user}@{domain}", "-pw {admin_password}"). Secret values are kept in
  the Keychain, never shown again: leave one empty to keep it, type to replace it. A secret copied over from
  Windows can't be read on a Mac (it was encrypted for that PC), so it asks to be typed again.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  interface Variable {
    key: string;
    value: string;
    secret: boolean;
    hasValue: boolean;
    fromWindows: boolean;
  }

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let rows = $state<Variable[]>([]);
  let message = $state("");

  onMount(async () => {
    rows = await invoke<Variable[]>("variables_list");
  });

  /** Add an empty row */
  function add(): void {
    rows = [...rows, { key: "", value: "", secret: false, hasValue: false, fromWindows: false }];
  }

  /** Save and close */
  async function save(): Promise<void> {
    try {
      rows = await invoke<Variable[]>("variables_save", { variables: rows });
      onClose();
    } catch (e) {
      message = String(e);
    }
  }

  /** What an empty secret field says */
  function secretHint(row: Variable): string {
    if (row.hasValue) return "Stored in the Keychain (type to replace)";
    if (row.fromWindows) return "Set on Windows: type it again here";
    return "Type the value";
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Environment Variables">
    <header>Environment Variables</header>
    <p class="hint">
      Sessions can use these as <code>{"{key}"}</code> in the host, username, extra arguments and jump host, e.g.
      <code>{"{user}@{domain}"}</code> or <code>-pw {"{admin_password}"}</code>. They're yours only, kept beside your settings.
    </p>
    <div class="table">
      <div class="head">Key</div>
      <div class="head">Value</div>
      <div class="head">Secret</div>
      <div class="head"></div>
      {#each rows as row, i}
        <input bind:value={row.key} placeholder="e.g. user" aria-label="Key" />
        {#if row.secret}
          <input type="password" bind:value={row.value} placeholder={secretHint(row)} aria-label="Secret value" />
        {:else}
          <input bind:value={row.value} aria-label="Value" />
        {/if}
        <input type="checkbox" bind:checked={row.secret} aria-label="Secret" />
        <button title="Remove" aria-label="Remove {row.key}" onclick={() => (rows = rows.filter((_, j) => j !== i))}>✕</button>
      {/each}
    </div>
    <footer>
      <button onclick={add}>Add</button>
      <span class="message">{message}</span>
      <button class="primary" onclick={save}>OK</button>
      <button onclick={onClose}>Cancel</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 110;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(720px, 94vw);
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    font-weight: 600;
  }

  .hint {
    margin: 0;
    color: var(--text-muted);
  }

  code {
    white-space: nowrap;
  }

  .table {
    display: grid;
    grid-template-columns: 200px 1fr 60px 32px;
    gap: 6px;
    align-items: center;
    overflow: auto;
  }

  .head {
    color: var(--text-muted);
    font-size: 12px;
  }

  input:not([type="checkbox"]),
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .message {
    flex: 1;
    color: #e5534b;
  }
</style>
