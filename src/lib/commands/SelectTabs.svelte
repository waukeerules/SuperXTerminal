<!--
  Select sessions to send to, as SuperXPuTTY's frmDocumentSelector: the open tabs (# with * for the active one, Tab,
  Session ID), ticked when they accept commands; OK sets each tab's Accept commands. "Send to visible tabs only"
  sends to every tab showing instead, and is remembered.
-->
<script lang="ts">
  import { context } from "./sending.svelte";
  import { saveSettings, settings } from "../settings/store.svelte";

  let { onClose }: { onClose: () => void } = $props();

  const tabs = context.tabs?.tabList() ?? [];
  let ticked = $state(new Set(tabs.filter((t) => t.acceptCommands).map((t) => t.id)));
  let visibleOnly = $state(settings.sendToVisibleOnly);

  function toggle(id: string): void {
    const next = new Set(ticked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    ticked = next;
  }

  async function ok(): Promise<void> {
    context.tabs?.setAccepting(ticked);
    if (visibleOnly !== settings.sendToVisibleOnly) await saveSettings({ sendToVisibleOnly: visibleOnly });
    onClose();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Select sessions to send to">
    <header>Select sessions to send to</header>
    <table class:dimmed={visibleOnly}>
      <thead><tr><th></th><th>#</th><th>Tab</th><th>Session ID</th></tr></thead>
      <tbody>
        {#each tabs as tab, i (tab.id)}
          <tr onclick={() => toggle(tab.id)}>
            <td><input type="checkbox" checked={ticked.has(tab.id)} disabled={visibleOnly} onclick={(e) => e.stopPropagation()}
              onchange={() => toggle(tab.id)} aria-label={tab.title} /></td>
            <td>{i + 1}{tab.active ? "*" : ""}</td>
            <td>{tab.title}</td>
            <td>{tab.session.replace(/^(saved|private|database):/, "")}</td>
          </tr>
        {:else}
          <tr><td colspan="4" class="empty">No tabs are open.</td></tr>
        {/each}
      </tbody>
    </table>
    <label><input type="checkbox" bind:checked={visibleOnly} /> Send to visible tabs only</label>
    <footer>
      <button class="primary" onclick={ok}>OK</button>
      <button onclick={onClose}>Cancel</button>
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
    width: min(560px, 94vw);
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header,
  th,
  .empty {
    color: var(--text-muted);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    background: var(--bg-input);
    border: 1px solid var(--border);
  }

  table.dimmed {
    opacity: 0.5;
  }

  th,
  td {
    text-align: left;
    padding: 3px 6px;
  }

  tbody tr:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  label {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 10px;
  }
</style>
