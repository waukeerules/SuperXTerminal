<!--
  Settings > General's Restore from Backup…, beside Use Sessions.xml and Use PrivateSessions.xml: the copies kept
  before each save (Sessions.yyyyMMdd_hhmmss.xml beside the file, the newest 20, as SuperXPuTTY keeps), newest first,
  with when each was saved and how many sessions it holds. Restore puts the one chosen back as the file, after asking;
  the file as it was is backed up first, so it's in this list to go back to. The Sessions tree reads the file again
  by itself, as for any change on disk. Folders.xml (folder colours and logins) is left as it is. Database sessions
  have no file, so there's nothing here for them.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";

  interface Props {
    /** Which file: Sessions.xml ("saved") or PrivateSessions.xml ("private") */
    store: "saved" | "private";
    /** Restored, Close or Escape: the window closes */
    onClose: () => void;
  }

  let { store, onClose }: Props = $props();

  /** One backup, as sessions_backups lists it */
  interface Backup {
    /** Its file name, beside the sessions file */
    name: string;
    /** When the version it holds was saved, "2026-11-07 14:03:12" */
    saved: string;
    /** How many sessions it holds */
    sessions: number;
    /** Why it can't be restored; null when it can */
    problem: string | null;
  }

  /** The file's name, for the title and the question */
  const file = $derived(store === "saved" ? "Sessions.xml" : "PrivateSessions.xml");

  /** The backups, newest first; null while they're read */
  let backups = $state<Backup[] | null>(null);
  /** The backup chosen, by name */
  let chosen = $state<string | null>(null);
  let error = $state("");

  onMount(async () => {
    try {
      backups = await invoke<Backup[]>("sessions_backups", { store });
    } catch (e) {
      backups = [];
      error = String(e);
    }
  });

  /** The backup chosen, if it can be restored */
  const picked = $derived(backups?.find((b) => b.name === chosen && !b.problem));

  /** Put the chosen backup back as the file, once the user says so */
  async function restore(): Promise<void> {
    const backup = picked;
    if (!backup) return;
    const question = `Replace ${file} with the version saved ${backup.saved} (${backup.sessions} session${backup.sessions === 1 ? "" : "s"})?\n\n`
      + `${file} as it is now is backed up first, so it can be restored from this list.`;
    if (!(await ask(question, { title: "Restore from Backup", kind: "warning", okLabel: "Restore" }))) return;
    try {
      await invoke("sessions_restore", { store, name: backup.name });
      onClose();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={`Restore ${file} from Backup`}>
    <header>Restore {file} from Backup</header>
    <p class="note">A copy is kept before each save: the newest 20. Restoring puts the one chosen back as {file}; the
      Sessions tree shows it at once. Folder colours and logins (Folders.xml) stay as they are.</p>

    <div class="table" role="listbox" aria-label="Backups">
      {#if backups === null}
        <p class="empty">Reading the backups…</p>
      {:else}
        {#each backups as backup (backup.name)}
          <div class="backup" class:current={backup.name === chosen} class:bad={!!backup.problem} role="option"
            aria-selected={backup.name === chosen} tabindex="0" title={backup.problem ?? backup.name}
            onclick={() => (chosen = backup.name)}
            onkeydown={(e) => e.key === "Enter" && (chosen = backup.name)}
            ondblclick={() => { chosen = backup.name; void restore(); }}>
            <span>{backup.saved}</span>
            <span class="count">{backup.problem ? "Can't be read" : `${backup.sessions} session${backup.sessions === 1 ? "" : "s"}`}</span>
          </div>
        {:else}
          <p class="empty">No backups yet: one is kept each time {file} is saved.</p>
        {/each}
      {/if}
    </div>

    <footer>
      <span class="error">{error}</span>
      <button class="primary" disabled={!picked} onclick={restore}>Restore…</button>
      <button onclick={onClose}>Close</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 140;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(480px, 94vw);
    max-height: 94vh;
    display: flex;
    flex-direction: column;
    gap: 10px;
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

  .table {
    height: 260px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-input);
  }

  .backup {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 8px;
    cursor: default;
  }

  .backup.current {
    background: var(--selection, rgb(80 120 200 / 0.3));
  }

  .backup.bad {
    opacity: 0.55;
  }

  .count {
    color: var(--text-muted);
  }

  .empty {
    margin: 8px;
    color: var(--text-muted);
  }

  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
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
