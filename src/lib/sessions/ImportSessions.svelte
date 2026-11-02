<!--
  File > Import Sessions, as SuperXPuTTY's: from a CSV file (its SessionCsv: rows matched by Source, Folder and Name,
  existing Saved and Private sessions updated, the rest added), a MobaXterm sessions file, Remote Desktop (.rdp)
  files, or ~/.ssh/config, the last three into a folder of their own ("ImportedFromMobaXterm", ...). Asks for the file,
  then shows what importing it will do before anything changes, as SuperXPuTTY's confirmation.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { applyImport, planImport, sshConfigPath, type ImportKind, type ImportSummary, type Store } from "./api";

  interface Props {
    kind: ImportKind;
    /** Whether Saved sessions are in use, so they can be chosen */
    savedAvailable: boolean;
    /** Imported: the status bar's text */
    onDone: (status: string) => void;
    onCancel: () => void;
  }

  let { kind, savedAvailable, onDone, onCancel }: Props = $props();

  const TITLES: Record<ImportKind, string> = {
    csv: "Import sessions from a CSV file",
    mobaxterm: "Import sessions from MobaXterm",
    rdp: "Import Remote Desktop (.rdp) files",
    ssh: "Import hosts from ssh config",
  };
  const FOLDERS: Record<ImportKind, string> = { csv: "", mobaxterm: "ImportedFromMobaXterm", rdp: "ImportedFromRDP", ssh: "ImportedFromSshConfig" };

  let paths = $state<string[]>([]);
  // sessions from another app go to Private sessions unless chosen otherwise: Sessions.xml is usually the team's
  let store = $state<Store>("private");
  let summary = $state<ImportSummary | null>(null);
  let error = $state("");
  let busy = $state(false);

  const fileName = (p: string) => p.split("/").pop() ?? p;
  const what = $derived(paths.length === 1 ? fileName(paths[0]) : `${paths.length} files`);

  onMount(async () => {
    if (untrack(() => kind) === "ssh") {
      paths = [await sshConfigPath()];
      await plan();
    } else if (!(await choose())) {
      onCancel();
    }
  });

  /** Ask for the file (or .rdp files); false if cancelled */
  async function choose(): Promise<boolean> {
    const filters = {
      csv: [{ name: "CSV files", extensions: ["csv", "txt"] }],
      mobaxterm: [{ name: "MobaXterm sessions", extensions: ["mxtsessions"] }],
      rdp: [{ name: "Remote Desktop files", extensions: ["rdp"] }],
      ssh: [],
    }[kind];
    const chosen = await open({ title: TITLES[kind], multiple: kind === "rdp", filters: filters.length ? [...filters, { name: "All files", extensions: ["*"] }] : undefined });
    if (!chosen) return false;
    paths = Array.isArray(chosen) ? chosen : [chosen];
    await plan();
    return true;
  }

  /** Work out what importing would do */
  async function plan(): Promise<void> {
    error = "";
    summary = null;
    try {
      summary = await planImport(kind, paths, store);
    } catch (e) {
      error = String(e);
    }
  }

  async function run(): Promise<void> {
    busy = true;
    try {
      const [added, updated] = await applyImport(kind, paths, store);
      onDone(`Imported ${added} new${kind === "csv" ? ` and ${updated} updated` : ""} session${added + updated === 1 ? "" : "s"} from ${what}`);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  const ready = $derived(!!summary && !summary.error && summary.adds + summary.updates > 0);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={TITLES[kind]}>
    <header>{TITLES[kind]}</header>

    <div class="file">
      <span class="path" title={paths.join("\n")}>{paths.length ? (paths.length === 1 ? paths[0] : `${paths.length} files: ${paths.map(fileName).join(", ")}`) : "Choosing…"}</span>
      <button onclick={() => void choose()}>Choose…</button>
    </div>

    {#if kind === "csv"}
      <p class="note">Sessions are matched by Source, Folder and Name: existing Saved and Private sessions get the file's
        values, others are added{savedAvailable ? "" : " to Private sessions (Saved sessions are off)"}. Database rows are
        skipped; a -pw written as ******** keeps the session's password.</p>
    {:else}
      <label class="store">Add them to
        <select bind:value={store} onchange={() => void plan()} disabled={!savedAvailable}>
          <option value="private">Private sessions (only you)</option>
          {#if savedAvailable}<option value="saved">Saved sessions (Sessions.xml, the team's)</option>{/if}
        </select>
        in the folder {FOLDERS[kind]}
      </label>
      {#if kind === "ssh"}
        <p class="note">One session per Host name without wildcards: its HostName, Port, User and ProxyJump, with
          IdentityFile and forwards as extra arguments. Host * and Match settings still apply, as ssh reads the file when
          a session opens.</p>
      {/if}
    {/if}

    {#if summary}
      {#if summary.error}
        <p class="error">{summary.error}</p>
      {:else}
        <p class="counts"><b>{summary.adds}</b> new session{summary.adds === 1 ? "" : "s"}{kind === "csv" ? `, ${summary.updates} updated` : ""}, {summary.skips} skipped.</p>
        {#if summary.examples.length}
          <ul class="list">
            {#each summary.examples as line}<li>{line}</li>{/each}
            {#if summary.adds + summary.updates > summary.examples.length}<li class="more">… and {summary.adds + summary.updates - summary.examples.length} more</li>{/if}
          </ul>
        {/if}
        {#if summary.skipped.length}
          <ul class="list skipped">
            {#each summary.skipped as line}<li>{line}</li>{/each}
            {#if summary.skips > summary.skipped.length}<li class="more">… and {summary.skips - summary.skipped.length} more skipped</li>{/if}
          </ul>
        {/if}
        {#if summary.ignored.length}<p class="note">Columns not used: {summary.ignored.join(", ")}</p>{/if}
      {/if}
    {/if}

    <footer>
      <span class="error">{error}</span>
      <button class="primary" disabled={!ready || busy} onclick={run}>Import</button>
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
    max-height: 94vh;
    overflow: auto;
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

  .file {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }

  .note,
  .counts {
    margin: 0;
  }

  .note {
    color: var(--text-muted);
  }

  .store {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }

  .list {
    margin: 0;
    padding: 4px 8px 4px 24px;
    max-height: 140px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-input);
  }

  .skipped {
    color: #c8811b;
  }

  .more {
    list-style: none;
    color: var(--text-muted);
  }

  select,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
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
    margin: 0;
    color: #e5534b;
  }
</style>
