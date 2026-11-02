<!--
  Tools > Database Sessions, as SuperXPuTTY's window: the PostgreSQL table both apps share. The connection is
  named SuperPuTTY so the table's lock trigger treats the Mac like SuperXPuTTY, and the nickname goes into
  updated_by. The password is kept in the Keychain (a {key} placeholder from an imported file works too).
  Export/Import use SuperXPuTTY's file, so settings move between Mac and Windows. Create / upgrade schema, enabled
  when Test connection finds something missing, makes the same table, triggers and folders table as SuperXPuTTY's.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";

  interface DbSettings {
    enabled: boolean;
    host: string;
    port: number;
    database: string;
    schema: string;
    table: string;
    user: string;
    sslMode: string;
    nickname: string;
    readOnly: boolean;
    checkSeconds: number;
    passwordPlaceholder: string;
  }

  interface Props {
    onSaved: () => void;
    onCancel: () => void;
  }

  let { onSaved, onCancel }: Props = $props();

  let s = $state<DbSettings | null>(null);
  let hasPassword = $state(false);
  /** A typed password; undefined keeps the stored one */
  let password = $state<string | undefined>(undefined);
  let result = $state("");
  let resultKind = $state<"ok" | "error" | "">("");

  onMount(async () => {
    [s, hasPassword] = await invoke<[DbSettings, boolean]>("db_settings_get");
  });

  /** Show a line under the buttons */
  function say(text: string, kind: "ok" | "error" | "" = ""): void {
    result = text;
    resultKind = kind;
  }

  /** Test connection with what's in the window */
  async function test(): Promise<void> {
    if (!s) return;
    say("Connecting…");
    try {
      const t = await invoke<TestResult>("db_test", { database: { ...s, port: Number(s.port) }, password: password ?? null });
      say(describe(t), t.missing.length && s.readOnly ? "error" : "ok");
      needsSchema = t.missing.length > 0 && !s.readOnly;
    } catch (e) {
      say(String(e), "error");
      needsSchema = false;
    }
  }

  interface TestResult {
    serverVersion: string;
    tableExists: boolean;
    rows: number;
    canWrite: boolean;
    hasVersionTable: boolean;
    missing: string[];
  }

  /** Whether Test connection found something for Create / upgrade schema to add */
  let needsSchema = $state(false);

  /** What Test connection found, as SuperXPuTTY's ProbeResult */
  function describe(t: TestResult): string {
    if (!s) return "";
    const table = `${s.schema}.${s.table}`;
    const found = t.tableExists
      ? `${t.rows.toLocaleString()} sessions, ${t.canWrite && !s.readOnly ? "can change them" : "read only"}`
      : `but ${table} isn't there yet`;
    const missing = t.tableExists && t.missing.length ? `; missing ${t.missing.join(", ")}: run Create / upgrade schema` : "";
    const readOnly = t.missing.length && s.readOnly ? " (Read only is on, so nothing is created.)" : "";
    return `Connected to PostgreSQL ${t.serverVersion}${t.tableExists ? ": " : ", "}${found}${missing}.${readOnly}`;
  }

  /** Create / upgrade schema, then test again */
  async function createSchema(): Promise<void> {
    if (!s) return;
    say("Creating…");
    needsSchema = false;
    try {
      const done = await invoke<string>("db_create_schema", { database: { ...s, port: Number(s.port) }, password: password ?? null });
      const t = await invoke<TestResult>("db_test", { database: { ...s, port: Number(s.port) }, password: null });
      say(`${done} ${describe(t)}`, "ok");
    } catch (e) {
      say(`Couldn't create the schema: ${e}`, "error");
    }
  }

  /** Save and close; the tree then reads the table */
  async function ok(): Promise<void> {
    if (!s) return;
    if (s.enabled && !s.nickname.trim()) {
      say("A nickname is needed: it's how others see who changed a session.", "error");
      return;
    }
    try {
      await invoke("db_settings_save", { database: { ...s, port: Number(s.port), checkSeconds: Number(s.checkSeconds) }, password: password ?? null });
      onSaved();
    } catch (e) {
      say(String(e), "error");
    }
  }

  /** Export the connection for colleagues */
  async function exportFile(): Promise<void> {
    if (!s) return;
    const path = await save({ title: "Export Database sessions settings", defaultPath: "superputty database settings.json",
      filters: [{ name: "JSON", extensions: ["json"] }] });
    if (!path) return;
    try {
      await invoke("db_export", { database: s, path });
      say("Exported. The password isn't in the file (unless it's a {key} placeholder), nor your nickname.", "ok");
    } catch (e) {
      say(String(e), "error");
    }
  }

  /** Fill the window from an exported file (SuperXPuTTY's too); OK saves it */
  async function importFile(): Promise<void> {
    const path = await open({ title: "Import Database sessions settings", filters: [{ name: "JSON", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      s = await invoke<DbSettings>("db_import", { path });
      say("Imported. Check the password and nickname, then press OK.", "ok");
    } catch (e) {
      say(String(e), "error");
    }
  }

  /** Empty the connection, as SuperXPuTTY's Clear */
  function clear(): void {
    if (!s) return;
    s = { ...s, enabled: false, host: "", user: "", passwordPlaceholder: "", port: 5432, database: "superputty", schema: "superputty",
      table: "sessions", sslMode: "Prefer", readOnly: false, checkSeconds: 30 };
    password = "";
    say("Cleared. OK saves it, and forgets the stored password.");
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Database sessions">
    <header>Database sessions</header>
    {#if s}
      <label class="check"><input type="checkbox" bind:checked={s.enabled} /> Show sessions from a PostgreSQL table</label>
      <div class="grid">
        <span>Host:</span><input bind:value={s.host} />
        <span>Port:</span><input class="short" type="number" min="1" max="65535" bind:value={s.port} />
        <span>Database:</span><input bind:value={s.database} />
        <span>Schema:</span><input bind:value={s.schema} />
        <span>Table:</span><input bind:value={s.table} />
        <span>Username:</span><input bind:value={s.user} />
        <span>Password:</span>
        <input type="password" value={password ?? ""} oninput={(e) => (password = e.currentTarget.value)}
          placeholder={hasPassword ? "saved in the Keychain (type to replace)" : s.passwordPlaceholder || ""} />
        <span>Nickname:</span><input bind:value={s.nickname} placeholder="how others see who changed a session" />
        <span>SSL:</span>
        <select class="short" bind:value={s.sslMode}>
          {#each ["Disable", "Allow", "Prefer", "Require", "VerifyCA", "VerifyFull"] as m}<option value={m}>{m}</option>{/each}
        </select>
        <span>Check every:</span>
        <span class="inline"><input class="short" type="number" min="0" bind:value={s.checkSeconds} /> seconds (0 = never)</span>
      </div>
      <label class="check"><input type="checkbox" bind:checked={s.readOnly} /> Read only: never create or change anything in the database</label>
      <div class="buttons">
        <button onclick={test}>Test connection</button>
        <button disabled={!needsSchema} title="Test connection first; it's enabled when something is missing" onclick={createSchema}>Create / upgrade schema</button>
        <button onclick={exportFile}>Export…</button>
        <button onclick={importFile}>Import…</button>
        <button onclick={clear}>Clear</button>
      </div>
      <p class="result {resultKind}">{result}</p>
      <footer>
        <button class="primary" onclick={ok}>OK</button>
        <button onclick={onCancel}>Cancel</button>
      </footer>
    {/if}
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

  .grid {
    display: grid;
    grid-template-columns: 100px 1fr;
    gap: 6px 10px;
    align-items: center;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .inline {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  input:not([type="checkbox"]),
  select,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  .short {
    width: 120px;
  }

  .buttons,
  footer {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  footer {
    justify-content: flex-end;
  }

  .result {
    margin: 0;
    min-height: 1.2em;
    color: var(--text-muted);
  }

  .result.ok {
    color: #3fb950;
  }

  .result.error {
    color: #e5534b;
  }

  button:disabled {
    opacity: 0.45;
  }
</style>
