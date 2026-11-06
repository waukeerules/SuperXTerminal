<!--
  Tools > Environment Variables, as SuperXPuTTY's: the user's values for {key} placeholders in sessions, so one
  shared Sessions.xml works for everyone ("{user}@{domain}", "-pw {admin_password}"). Secret values are kept in
  the Keychain, never shown again: leave one empty to keep it, type to replace it. A secret copied over from
  Windows can't be read on a Mac (it was encrypted for that PC), so it asks to be typed again.

  Beside the user's own: where each is used, the placeholders used that have no value yet, the ones SuperXTerminal
  fills itself (which no variable can be named), and a preview of a template filled in, secrets as dots. Export…
  and Import… pass a set on as SuperXPuTTY's EnvironmentVariables.json, secrets by name only (variablesFile.ts);
  an import fills the window, and OK saves it.
  It opens in front of Edit Session, whose Environment Variables… button opens it for a placeholder with no value.
-->
<script lang="ts" module>
  let shown = $state(false);

  /** Said when the variables are saved, so a dialog showing what's missing (Edit Session) looks again */
  export const VARIABLES_CHANGED = "superxterminal:variables-changed";

  /** Whether Environment Variables is open, so a dialog it was opened over (Edit Session) leaves Escape to it */
  export function variablesShown(): boolean {
    return shown;
  }
</script>

<script lang="ts">
  import { isMac } from "../shortcuts";
  import { SECRET_STORE } from "../platform";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { exportVariables, importVariables, type Variable } from "./variablesFile";

  interface Props {
    /** OK, Cancel or Escape: the window closes */
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  let rows = $state<Variable[]>([]);
  /** Where each placeholder is used, by name in lower case (sessions, folders, profiles, overrides, presets) */
  let usage = $state<Record<string, string[]>>({});
  /** The placeholders SuperXTerminal fills itself: name and what it is */
  let builtIn = $state<[string, string][]>([]);
  let template = $state("");
  let message = $state("");

  onMount(() => {
    shown = true;
    void load();
    return () => (shown = false);
  });

  /** The variables, where each is used, and the built-in names */
  async function load(): Promise<void> {
    [rows, usage, builtIn] = await Promise.all([
      invoke<Variable[]>("variables_list"),
      invoke<Record<string, string[]>>("variables_usage"),
      invoke<[string, string][]>("variables_built_in"),
    ]);
  }

  /** The built-in placeholders' names, lower-cased: a variable can't take one */
  const builtInNames = $derived(new Set(builtIn.map(([n]) => n.toLowerCase())));

  /** Why a row's key can't be saved, or "" when it can (an empty row is just left out) */
  function problem(row: Variable, index: number): string {
    const key = row.key.trim();
    if (!key) return "";
    if (!/^[\p{L}\p{N}_]+$/u.test(key)) return "Letters, digits and _ only";
    if (builtInNames.has(key.toLowerCase())) return `{${key}} is built in: give the variable another name`;
    if (rows.some((r, j) => j !== index && r.key.trim().toLowerCase() === key.toLowerCase())) return `{${key}} is there twice`;
    return "";
  }

  /** Each row's problem, or "" when it can be saved */
  const problems = $derived(rows.map((r, i) => problem(r, i)));
  /** Whether any row has a problem, which keeps OK greyed */
  const blocked = $derived(problems.some(Boolean));

  /** Placeholders used somewhere that are neither the user's nor built in: they'd stop a session opening */
  const undefinedUsed = $derived(
    Object.entries(usage)
      .filter(([name]) => !builtInNames.has(name) && !rows.some((r) => r.key.trim().toLowerCase() === name))
      .sort(([a], [b]) => a.localeCompare(b)),
  );

  /** Where a variable is used, for its row */
  const usedBy = (key: string) => usage[key.trim().toLowerCase()] ?? [];

  /** The template with the user's values filled in: secrets as dots, built-ins and unknowns left as written */
  const preview = $derived.by(() =>
    template.replace(/\{([\p{L}\p{N}_]+)\}/gu, (whole, name: string) => {
      const row = rows.find((r) => r.key.trim().toLowerCase() === name.toLowerCase());
      if (!row) return whole;
      if (row.secret) return row.hasValue || row.value ? "••••" : whole;
      return row.value;
    }),
  );

  /** Add an empty row, or one for a placeholder that's used but has no value */
  function add(key = ""): void {
    rows = [...rows, { key, value: "", secret: /pass|pw|secret|token/i.test(key), hasValue: false, fromWindows: false }];
  }

  /** Save and close */
  async function save(): Promise<void> {
    if (blocked) return;
    try {
      rows = await invoke<Variable[]>("variables_save", { variables: rows });
      window.dispatchEvent(new Event(VARIABLES_CHANGED));
      onClose();
    } catch (e) {
      message = String(e);
    }
  }

  /** Export…: the variables in the window to a file to pass on, secrets by name only */
  async function exportFile(): Promise<void> {
    const path = await saveDialog({ defaultPath: "EnvironmentVariables.json", filters: [{ name: "Environment Variables", extensions: ["json"] }] });
    if (!path) return;
    try {
      await invoke("text_file_write", { path, text: exportVariables(rows) });
      const secrets = rows.filter((r) => r.key.trim() && r.secret).length;
      message = `Exported ${rows.filter((r) => r.key.trim()).length} variables${secrets ? `, ${secrets} secret${secrets === 1 ? "" : "s"} by name only` : ""}.`;
    } catch (e) {
      message = `Couldn't export: ${e}`;
    }
  }

  /** Import…: a file's variables into the window, for OK to save; nothing here is removed */
  async function importFile(): Promise<void> {
    const path = await open({ filters: [{ name: "Environment Variables", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      const imported = importVariables(await invoke<string>("text_file_read", { path }), rows);
      rows = imported.rows;
      const { toType } = imported;
      message = `Imported ${imported.count} variables. `
        + (toType.length ? `Type the value${toType.length === 1 ? "" : "s"} of ${toType.join(", ")}, then OK to save.` : "OK saves them.");
    } catch (e) {
      message = e instanceof Error ? e.message : `Couldn't import: ${e}`;
    }
  }

  /** What an empty secret field says */
  function secretHint(row: Variable): string {
    if (row.hasValue) return `Stored in ${SECRET_STORE} (type to replace)`;
    if (row.fromWindows) return isMac ? "Set on Windows: type it again here" : "Set for another Windows account: type it again here";
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

    <div class="body">
      <div class="table">
        <div class="head">Key</div>
        <div class="head">Value</div>
        <div class="head">Secret</div>
        <div class="head">Used by</div>
        <div class="head"></div>
        {#each rows as row, i}
          <input bind:value={row.key} placeholder="e.g. user" aria-label="Key" class:bad={!!problems[i]} title={problems[i]} />
          {#if row.secret}
            <input type="password" bind:value={row.value} placeholder={secretHint(row)} aria-label="Secret value" />
          {:else}
            <input bind:value={row.value} aria-label="Value" />
          {/if}
          <input type="checkbox" bind:checked={row.secret} aria-label="Secret" />
          <span class="used" title={usedBy(row.key).join("\n")}>
            {usedBy(row.key).length ? `${usedBy(row.key).length} place${usedBy(row.key).length === 1 ? "" : "s"}` : row.key.trim() ? "none" : ""}
          </span>
          <button title="Remove" aria-label="Remove {row.key}" onclick={() => (rows = rows.filter((_, j) => j !== i))}>✕</button>
          {#if problems[i]}<p class="problem">{problems[i]}</p>{/if}
        {/each}
      </div>

      {#if undefinedUsed.length}
        <section>
          <h3>Used but not defined</h3>
          <p class="hint">A session using one of these says it has no value yet, and doesn't open.</p>
          <ul>
            {#each undefinedUsed as [name, places] (name)}
              <li><code>{`{${name}}`}</code> <span class="where" title={places.join("\n")}>{places.slice(0, 3).join(", ")}{places.length > 3 ? `, and ${places.length - 3} more` : ""}</span>
                <button onclick={() => add(name)}>Add</button></li>
            {/each}
          </ul>
        </section>
      {/if}

      <section>
        <h3>Built in</h3>
        <p class="hint">Filled by SuperXTerminal itself, so no variable can have these names.</p>
        <ul>
          {#each builtIn as [name, about] (name)}
            <li><code>{`{${name}}`}</code> <span class="where">{about}</span></li>
          {/each}
        </ul>
      </section>

      <section>
        <h3>Preview</h3>
        <input class="template" bind:value={template} aria-label="Template to preview"
          placeholder="Type a template, e.g. {'{'}ad_username{'}'}@{'{'}domain{'}'}#{'{'}cyberark_region{'}'}" />
        {#if template}
          <p class="filled" aria-label="Preview">{preview}</p>
          <p class="hint">Secrets show as dots; built-in and undefined placeholders stay as written.</p>
        {/if}
      </section>
    </div>

    <footer>
      <button onclick={() => add()}>Add</button>
      <button onclick={importFile} title="Add or update variables from a file; nothing here is removed">Import…</button>
      <button onclick={exportFile} title="Save these to a file to pass on; secrets go by name only, without their values">Export…</button>
      <span class="message">{message || (blocked ? "Fix the names marked in red to save." : "")}</span>
      <button class="primary" onclick={save} disabled={blocked}>OK</button>
      <button onclick={onClose}>Cancel</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    /* over Edit Session (120), as Settings is (125); below the dialogs Settings opens (140) */
    z-index: 126;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(760px, 94vw);
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

  /* the rows and the sections below scroll; the title and OK stay put */
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .table {
    display: grid;
    grid-template-columns: 200px 1fr 60px 70px 32px;
    gap: 6px;
    align-items: center;
  }

  .head {
    color: var(--text-muted);
    font-size: 12px;
  }

  .used,
  .where {
    color: var(--text-muted);
    font-size: 12px;
  }

  .problem {
    grid-column: 1 / -1;
    margin: -2px 0 2px;
    color: #e5534b;
    font-size: 12px;
  }

  input.bad {
    border-color: #e5534b;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  h3 {
    margin: 0;
    font-size: 13px;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  li {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  li button {
    margin-left: auto;
  }

  .template {
    width: 100%;
  }

  .filled {
    margin: 0;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    font-family: Menlo, Consolas, monospace;
    word-break: break-all;
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
