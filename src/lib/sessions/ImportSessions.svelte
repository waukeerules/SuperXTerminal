<!--
  File > Import Sessions, as SuperXPuTTY's: from a CSV file (its SessionCsv: rows matched by Source, Folder and Name,
  existing Saved and Private sessions updated, the rest added), another Sessions.xml, a PuTTY CM export, a folder of
  PuTTY portable sessions, SecureCRT's Sessions folder, PuTTY's and KiTTY's saved sessions, Remote Desktop's recent
  computers (Windows), a MobaXterm sessions file, Remote Desktop (.rdp) files, or ~/.ssh/config, all but CSV into a
  folder of their own
  ("Imported", "ImportedFromPuTTY", ...). The file is dropped on the dialog's drop zone from Finder or File Explorer,
  or chosen by clicking it; then the dialog shows what importing will do before anything changes, as SuperXPuTTY's
  confirmation.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import DropZone from "../ui/DropZone.svelte";
  import { base64Of } from "../ui/fileDrops";
  import { applyImport, fileNameOf, importSource, planImport, type ImportKind, type ImportSummary, type Store } from "./api";

  interface Props {
    /** What's imported from: a CSV file, another app's sessions or files, or the system's lists */
    kind: ImportKind;
    /** Whether Saved sessions are in use, so they can be chosen */
    savedAvailable: boolean;
    /** Whether Private sessions are in use (Settings > General), so they can be chosen */
    privateAvailable?: boolean;
    /** Imported: the status bar's text */
    onDone: (status: string) => void;
    /** Cancel, or Escape: nothing is imported */
    onCancel: () => void;
  }

  let { kind, savedAvailable, privateAvailable = true, onDone, onCancel }: Props = $props();

  /** Each kind's window title */
  const TITLES: Record<ImportKind, string> = {
    csv: "Import sessions from a CSV file",
    mobaxterm: "Import sessions from MobaXterm",
    rdp: "Import Remote Desktop (.rdp) files",
    ssh: "Import hosts from ssh config",
    superputty: "Import sessions from a SuperPuTTY XML file",
    puttycm: "Import sessions from a PuTTY CM export",
    puttyFolder: "Import PuTTY portable sessions",
    securecrt: "Import sessions from SecureCRT",
    putty: "Import PuTTY's saved sessions",
    rdpRegistry: "Import Remote Desktop's recent computers",
  };
  /** The folder each kind's sessions go in, as SuperXPuTTY names them; a CSV file names its own */
  const FOLDERS: Record<ImportKind, string> = {
    csv: "", mobaxterm: "ImportedFromMobaXterm", rdp: "ImportedFromRDP", ssh: "ImportedFromSshConfig", superputty: "Imported",
    puttycm: "ImportedFromPuTTYCM", puttyFolder: "ImportedFromPortablePuTTY", securecrt: "ImportedFromSecureCRT", putty: "ImportedFromPuTTY",
    rdpRegistry: "ImportedFromRDPRecent",
  };
  /** Imports of a folder rather than a file: PuTTY's portable Sessions folder, and SecureCRT's */
  const FOLDER_KINDS: ImportKind[] = ["puttyFolder", "securecrt"];
  /** Imports with nothing to choose: they read where the system keeps them */
  const FIXED: ImportKind[] = ["putty", "rdpRegistry"];
  /** What each kind takes, for its drop zone and file picker: the file's extensions (none for any), in words */
  const TAKES: Record<ImportKind, { extensions: string[]; what: string; name: string }> = {
    csv: { extensions: ["csv", "txt"], what: "a CSV file", name: "CSV files" },
    mobaxterm: { extensions: ["mxtsessions"], what: "a MobaXterm .mxtsessions file", name: "MobaXterm sessions" },
    rdp: { extensions: ["rdp"], what: "Remote Desktop .rdp files", name: "Remote Desktop files" },
    superputty: { extensions: ["xml"], what: "a SuperPuTTY .xml file", name: "XML files" },
    puttycm: { extensions: ["xml"], what: "a PuTTY CM .xml export", name: "XML files" },
    ssh: { extensions: [], what: "an ssh config file", name: "" },
    puttyFolder: { extensions: [], what: "PuTTY's portable Sessions folder", name: "" },
    securecrt: { extensions: [], what: "SecureCRT's Sessions folder", name: "" },
    putty: { extensions: [], what: "", name: "" },
    rdpRegistry: { extensions: [], what: "", name: "" },
  };

  let paths = $state<string[]>([]);
  // sessions from another app go to Private sessions unless chosen otherwise: Sessions.xml is usually the team's
  // (to Saved sessions when Private sessions are turned off)
  let store = $state<Store>(untrack(() => (privateAvailable ? "private" : "saved")));
  let summary = $state<ImportSummary | null>(null);
  let error = $state("");
  let busy = $state(false);

  /** A path's file name, for the window's text */
  const fileName = fileNameOf;
  /** What's being imported, in the window's text: the file, or how many */
  const what = $derived(
    kind === "putty" ? "PuTTY's saved sessions" : kind === "rdpRegistry" ? "Remote Desktop's recent computers"
      : paths.length === 1 ? fileName(paths[0]) : `${paths.length} files`,
  );

  onMount(async () => {
    const start = untrack(() => kind);
    // ssh config, PuTTY's saved sessions and Remote Desktop's recent computers are read where they are
    const source = await importSource(start).catch(() => null);
    if (source !== null || FIXED.includes(start)) {
      paths = [source ?? ""];
      await plan();
    }
    // the rest wait for a file dropped on the zone, or chosen by clicking it
  });

  /**
   * Files dropped on the zone: the browser gives their contents, not their paths, so the core saves each where the
   * import can read it (dropped_file_save); then what importing them would do
   */
  async function dropped(files: File[]): Promise<void> {
    error = "";
    try {
      paths = await Promise.all(files.map(async (file) => invoke<string>("dropped_file_save", { name: file.name, data: await base64Of(file) })));
    } catch (e) {
      error = `Couldn't read the dropped file: ${e}`;
      return;
    }
    await plan();
  }

  /** Ask for the file (or .rdp files, or a folder of PuTTY's or SecureCRT's sessions); false if cancelled */
  async function choose(): Promise<boolean> {
    const takes = TAKES[kind];
    const filters = takes.extensions.length ? [{ name: takes.name, extensions: takes.extensions }] : [];
    const chosen = await open({
      title: kind === "puttyFolder" ? "Choose the portable Sessions folder" : kind === "securecrt" ? "Choose SecureCRT's Sessions folder" : TITLES[kind],
      directory: FOLDER_KINDS.includes(kind),
      multiple: kind === "rdp",
      filters: filters.length ? [...filters, { name: "All files", extensions: ["*"] }] : undefined,
    });
    if (!chosen) return false;
    paths = Array.isArray(chosen) ? chosen : [chosen];
    await plan();
    return true;
  }

  /** Where the CSV template was saved, to say so; "" until it is */
  let templateSaved = $state("");

  /**
   * Save a CSV template: the columns this import reads (the export's), with an example row for each common kind of
   * session, to fill in and drop here (importing.rs's template)
   */
  async function saveTemplate(): Promise<void> {
    error = "";
    const path = await save({ title: "Save a CSV template for sessions", defaultPath: "sessions-template.csv", filters: [{ name: "CSV files", extensions: ["csv"] }] });
    if (!path) return;
    try {
      await invoke("sessions_csv_template", { path });
      templateSaved = path;
    } catch (e) {
      error = `Couldn't save the template: ${e}`;
    }
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

  /** Import the sessions, then say how many were added and changed */
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

  /** Whether there's something to import and nothing's wrong, so Import can be pressed */
  const ready = $derived(!!summary && !summary.error && summary.adds + summary.updates > 0);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={TITLES[kind]}>
    <header>{TITLES[kind]}</header>

    {#if FIXED.includes(kind)}
      <div class="file"><span class="path">{what}</span></div>
    {:else}
      <DropZone {paths} extensions={TAKES[kind].extensions} multiple={kind === "rdp"} folder={FOLDER_KINDS.includes(kind)}
        what={TAKES[kind].what} onChoose={() => void choose()} onDrop={(chosen) => void dropped(chosen)} />
    {/if}

    {#if kind === "csv"}
      <p class="note">Sessions are matched by Source, Folder and Name: existing Saved and Private sessions get the file's
        values, others are added{savedAvailable ? "" : " to Private sessions (Saved sessions are off)"}. Database rows are
        skipped; a -pw written as ******** keeps the session's password.</p>
      <div class="template">
        <button onclick={() => void saveTemplate()}>Save a Template…</button>
        <span class="note">{templateSaved
          ? `Saved as ${fileName(templateSaved)}: replace its example rows with your sessions, then drop it here.`
          : "Every column, with an example SSH, serial and Remote Desktop session to copy. An export is the same shape."}</span>
      </div>
    {:else}
      <label class="store">Add them to
        <select bind:value={store} onchange={() => void plan()} disabled={!savedAvailable || !privateAvailable}>
          {#if privateAvailable}<option value="private">Private sessions (only you)</option>{/if}
          {#if savedAvailable}<option value="saved">Saved sessions (Sessions.xml, the team's)</option>{/if}
        </select>
        in the folder {FOLDERS[kind]}
      </label>
      {#if kind === "superputty"}
        <p class="note">Every session in the file, with all its settings, under Imported in its own folders. Names already
          taken get -1, -2... added.</p>
      {:else if kind === "putty" || kind === "puttyFolder" || kind === "puttycm"}
        <p class="note">Each session's host, port, protocol and username, with the PuTTY session it uses as its profile
          (PuttySession), as SuperXPuTTY imports them.{kind === "putty" ? " KiTTY's go in a KiTTY folder." : ""}</p>
      {:else if kind === "securecrt"}
        <p class="note">Each session's protocol, host, port and username in Session Manager's folders, with its key file
          (-i, SecureCRT's global one too), agent and X11 forwarding, port forwards and serial settings as extra
          arguments, and a jump through another session kept. SecureCRT's Default sessions are left out. Passwords
          aren't imported: SecureCRT encrypts them, so set them in the session or its profile. Its Config folder or
          Sessions folder can be chosen if SecureCRT's settings were moved (Global Options &gt; Configuration
          Paths).</p>
      {:else if kind === "rdpRegistry"}
        <p class="note">One RDP session for each computer in Remote Desktop Connection's recent list.</p>
      {/if}
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

  /* the CSV template's button, with what it gives beside it */
  .template {
    display: flex;
    gap: 10px;
    align-items: center;
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
