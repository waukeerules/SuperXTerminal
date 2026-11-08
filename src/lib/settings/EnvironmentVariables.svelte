<!--
  Tools > Environment Variables, as SuperXPuTTY's: the user's values for {key} placeholders in sessions, so one
  shared Sessions.xml works for everyone ("{user}@{domain}", "-pw {admin_password}"). Secret values are kept in
  the Keychain or Windows Credential Manager, never shown again: leave one empty to keep it, type to replace it. A
  secret copied over from Windows can't be read on a Mac (it was encrypted for that PC), so it asks to be typed again.
  A value can be a 1Password reference (op://vault/item/field), read as a session uses it (onepassword.rs).

  Laid out to stay calm as it grows: one list, each row a name, its value (a 1Password reference shown as its item,
  a secret as stored), how many places use it, and a ⋯ menu for Secret and Remove. Placeholders used but not set are
  rows in the same list, under a warning, a few shown and the rest folded; one is added by clicking its value. The
  longer help (?), the built-in placeholders and a template's preview are folded away. Filter narrows the list.
  Export… and Import… pass a set on as SuperXPuTTY's EnvironmentVariables.json, secrets by name only
  (variablesFile.ts); an import fills the window, and Save saves it.
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
  import { OP_REFERENCE_HINT, isReference, referencedItem } from "./onepassword";
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { exportVariables, importVariables, type Variable } from "./variablesFile";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";

  interface Props {
    /** Save, Cancel or Escape: the window closes */
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
  /** What Filter narrows the list to: a name's or a value's words */
  let filter = $state("");
  /** The longer help, behind ? */
  let helpOpen = $state(false);
  /** All the placeholders not set, rather than the first few */
  let allUnset = $state(false);
  /** The row whose 1Password reference is being edited, shown as its text rather than its item */
  let editing = $state<number | null>(null);
  /** A row's ⋯ menu, where it opened */
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** The list's element, to put the cursor in a row just added */
  let list = $state<HTMLElement>();

  /** How many placeholders not set show before the rest fold */
  const UNSET_SHOWN = 3;

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
  /** Whether any row has a problem, which keeps Save greyed */
  const blocked = $derived(problems.some(Boolean));

  /** Whether a name or value has Filter's words in it */
  const matches = (...texts: string[]) => !filter.trim() || texts.some((t) => t.toLowerCase().includes(filter.trim().toLowerCase()));

  /** The rows to show, with their place in `rows`: those Filter leaves, and a row just added whatever its name */
  const shownRows = $derived(rows.map((row, i) => ({ row, i })).filter(({ row }) => !row.key.trim() || matches(row.key, row.secret ? "" : row.value)));

  /** Placeholders used somewhere that are neither the user's nor built in: they'd stop a session opening */
  const unset = $derived(
    Object.entries(usage)
      .filter(([name]) => !builtInNames.has(name) && !rows.some((r) => r.key.trim().toLowerCase() === name))
      .sort(([a], [b]) => a.localeCompare(b)),
  );
  /** Those Filter leaves; the first few unless all are asked for */
  const unsetShown = $derived.by(() => {
    const left = unset.filter(([name]) => matches(name));
    return allUnset || filter.trim() ? left : left.slice(0, UNSET_SHOWN);
  });

  /** Where a variable is used, for its row */
  const usedBy = (key: string) => usage[key.trim().toLowerCase()] ?? [];
  /** How many places use a name, as its row says it */
  const usedText = (places: string[]) => (places.length ? `${places.length} place${places.length === 1 ? "" : "s"}` : "unused");

  /** The template with the user's values filled in: secrets as dots, built-ins and unknowns left as written */
  const preview = $derived.by(() =>
    template.replace(/\{([\p{L}\p{N}_]+)\}/gu, (whole, name: string) => {
      const row = rows.find((r) => r.key.trim().toLowerCase() === name.toLowerCase());
      if (!row) return whole;
      if (row.secret) return row.hasValue || row.value ? "••••" : whole;
      return row.value;
    }),
  );

  /**
   * Add a row, empty or for a placeholder that's used but not set, and put the cursor in it: in its value when it
   * has a name, else in its name
   */
  async function add(key = ""): Promise<void> {
    rows = [...rows, { key, value: "", secret: /pass|pw|secret|token/i.test(key), hasValue: false, fromWindows: false }];
    await tick();
    const fields = list?.querySelectorAll<HTMLInputElement>(key ? "[data-value]" : "[data-key]");
    fields?.[fields.length - 1]?.focus();
  }

  /** A row's ⋯ menu: Secret, and Remove */
  function showMenu(event: MouseEvent, index: number): void {
    event.stopPropagation();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const row = rows[index];
    menu = {
      x: rect.right - 180,
      y: rect.bottom + 2,
      items: [
        { label: "Secret", checked: row.secret, action: () => (rows[index].secret = !rows[index].secret) },
        { separator: true },
        { label: "Remove", action: () => (rows = rows.filter((_, j) => j !== index)) },
      ],
    };
  }

  /** Edit a 1Password reference's text, shown till then as its item */
  async function editReference(index: number): Promise<void> {
    editing = index;
    await tick();
    list?.querySelector<HTMLInputElement>(`[data-row="${index}"] [data-value]`)?.focus();
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

  /** Import…: a file's variables into the window, for Save to save; nothing here is removed */
  async function importFile(): Promise<void> {
    const path = await open({ filters: [{ name: "Environment Variables", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      const imported = importVariables(await invoke<string>("text_file_read", { path }), rows);
      rows = imported.rows;
      const { toType } = imported;
      message = `Imported ${imported.count} variables. `
        + (toType.length ? `Type the value${toType.length === 1 ? "" : "s"} of ${toType.join(", ")}, then Save.` : "Save keeps them.");
    } catch (e) {
      message = e instanceof Error ? e.message : `Couldn't import: ${e}`;
    }
  }

  /** What an empty secret field says */
  function secretHint(row: Variable): string {
    if (row.hasValue) return `Stored in ${SECRET_STORE}`;
    if (row.fromWindows) return isMac ? "Set on Windows: type it again here" : "Set for another Windows account: type it again here";
    return "Type the secret";
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !menu && onClose()} onclick={() => (menu = null)} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Environment Variables">
    <div class="top">
      <header>Environment Variables</header>
      <input class="filter" bind:value={filter} placeholder="Filter" aria-label="Filter" />
      <button class="help-button" class:on={helpOpen} onclick={() => (helpOpen = !helpOpen)} aria-label="How these work" aria-expanded={helpOpen}
        title="How these work">?</button>
    </div>
    <p class="hint">Your values for <code>{"{placeholders}"}</code> in sessions, profiles and scripts. Only on {isMac ? "this Mac" : "this PC"}.</p>
    {#if helpOpen}
      <div class="help" role="note" aria-label="How Environment Variables work">
        <p>A session can use them in its host, username, extra arguments and jump host, e.g.
          <code>{"{user}@{domain}"}</code> or <code>-pw {"{admin_password}"}</code>; so can profiles and scripts. They're
          yours only, kept beside your settings. A secret is kept in {SECRET_STORE}, never in a file, and isn't shown
          again: leave it empty to keep it, type to replace it.</p>
        <p>{OP_REFERENCE_HINT} A reference isn't a secret itself, so it needn't be marked Secret.</p>
      </div>
    {/if}

    <div class="body" bind:this={list}>
      <div class="row head" aria-hidden="true"><span>Name</span><span>Value</span><span class="used">Used</span><span></span></div>
      {#each shownRows as { row, i } (i)}
        <div class="row" data-row={i}>
          <input class="key" data-key bind:value={row.key} placeholder="name" aria-label="Key" class:bad={!!problems[i]} title={problems[i]} spellcheck="false" />
          {#if row.secret}
            <span class="value">
              <span class="badge">Secret</span>
              <input data-value type="password" bind:value={row.value} placeholder={secretHint(row)} aria-label="Secret value" />
            </span>
          {:else if isReference(row.value) && editing !== i}
            <!-- a 1Password reference as the item it names; a click edits its text -->
            <button class="value reference" onclick={() => void editReference(i)} title={`${row.value.trim()}: read from 1Password as a session uses it. Click to edit`}>
              <span class="item">{referencedItem(row.value)}</span><span class="badge op">1Password</span>
            </button>
          {:else}
            <span class="value">
              <input data-value bind:value={row.value} aria-label="Value" placeholder="value" onblur={() => editing === i && (editing = null)} />
              {#if isReference(row.value)}<span class="badge op">1Password</span>{/if}
            </span>
          {/if}
          <span class="used" title={usedBy(row.key).join("\n")}>{row.key.trim() ? usedText(usedBy(row.key)) : ""}</span>
          <button class="more" aria-label="More for {row.key || 'the new variable'}" title="Secret, Remove" onclick={(e) => showMenu(e, i)}>⋯</button>
          {#if problems[i]}<p class="problem">{problems[i]}</p>{/if}
        </div>
      {/each}

      {#if unset.length}
        <p class="warn" role="note" aria-label="Used but not set">Used but not set: sessions using these won't open until they have a value.</p>
        {#each unsetShown as [name, places] (name)}
          <div class="row unset">
            <code class="key" title={`{${name}}`}>{name}</code>
            <button class="value add" onclick={() => void add(name)} aria-label="Set {name}">Type a value</button>
            <span class="used" title={places.join("\n")}>{usedText(places)}</span>
            <span></span>
          </div>
        {/each}
        {#if !filter.trim() && unset.length > UNSET_SHOWN}
          <button class="fold" onclick={() => (allUnset = !allUnset)}>{allUnset ? "▾ Fewer" : `▸ ${unset.length - UNSET_SHOWN} more not set`}</button>
        {/if}
      {/if}

      <div class="extras">
        <details>
          <summary>Try a template</summary>
          <input class="template" bind:value={template} aria-label="Template to preview"
            placeholder="A template, e.g. {'{'}ad_username{'}'}@{'{'}domain{'}'}" />
          {#if template}
            <p class="filled" aria-label="Preview">{preview}</p>
            <p class="hint">Secrets show as dots; built-in placeholders and ones not set stay as written.</p>
          {/if}
        </details>
        <details>
          <summary>Built-in placeholders ({builtIn.length})</summary>
          <p class="hint">Filled by SuperXTerminal itself, so no variable can have these names.</p>
          <ul>
            {#each builtIn as [name, about] (name)}
              <li><code>{`{${name}}`}</code> <span class="hint">{about}</span></li>
            {/each}
          </ul>
        </details>
      </div>
    </div>

    <footer>
      <button onclick={() => void add()}>Add</button>
      <button onclick={importFile} title="Add or update variables from a file; nothing here is removed">Import…</button>
      <button onclick={exportFile} title="Save these to a file to pass on; secrets go by name only, without their values">Export…</button>
      <span class="message">{message || (blocked ? "Fix the names marked in red to save." : "")}</span>
      <button onclick={onClose}>Cancel</button>
      <button class="primary" onclick={save} disabled={blocked}>Save</button>
    </footer>
  </div>
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

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
    width: min(720px, 94vw);
    max-height: 90vh;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  .top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  header {
    flex: 1;
    font-weight: 600;
  }

  .filter {
    width: 180px;
  }

  .help-button {
    width: 26px;
    border-radius: 50%;
    padding: 2px 0;
  }

  .help-button.on {
    border-color: var(--accent);
  }

  .hint {
    margin: 0;
    color: var(--text-muted);
  }

  .help {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-muted);
  }

  .help p {
    margin: 0 0 6px;
  }

  .help p:last-child {
    margin: 0;
  }

  code {
    white-space: nowrap;
  }

  /* the list scrolls; the title and Save stay put */
  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
  }

  /* a variable: its name, its value, where it's used, and its ⋯ */
  .row {
    display: grid;
    grid-template-columns: 170px minmax(0, 1fr) 72px 28px;
    gap: 8px;
    align-items: center;
    padding: 5px 0;
    border-top: 1px solid var(--border);
  }

  .row.head {
    border-top: none;
    padding-top: 0;
    color: var(--text-muted);
    font-size: 12px;
  }

  /* the name, quiet until it's worked on */
  .key {
    font-family: Menlo, Consolas, monospace;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  input.key {
    border-color: transparent;
    background: transparent;
  }

  input.key:hover,
  input.key:focus {
    border-color: var(--border);
    background: var(--bg-input);
  }

  input.key.bad {
    border-color: #e5534b;
  }

  .value {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .value input {
    flex: 1;
  }

  /* a reference shown as its item: a field-like button to edit it */
  .value.reference {
    justify-content: space-between;
    text-align: left;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
  }

  .item {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    flex: none;
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    color: var(--text-muted);
    border: 1px solid var(--border);
  }

  .badge.op {
    color: #fff;
    background: #1a5fd0;
    border-color: transparent;
  }

  .used {
    color: var(--text-muted);
    font-size: 12px;
    text-align: right;
  }

  .more {
    border: none;
    background: none;
    padding: 0;
    color: var(--text-muted);
    font-size: 16px;
  }

  .more:hover {
    color: var(--text);
  }

  .problem {
    grid-column: 1 / -1;
    margin: -2px 0 2px;
    color: #e5534b;
    font-size: 12px;
  }

  .warn {
    margin: 12px 0 2px;
    color: #d29922;
    font-size: 12px;
  }

  .row.unset .key {
    color: #d29922;
  }

  /* a placeholder not set: a dashed field whose click adds it, ready to type */
  .value.add {
    justify-content: flex-start;
    color: var(--text-muted);
    background: none;
    border: 1px dashed var(--border);
    border-radius: 4px;
    padding: 4px 6px;
  }

  .fold {
    align-self: flex-start;
    border: none;
    background: none;
    padding: 4px 0;
    color: var(--text-muted);
  }

  .extras {
    margin-top: 10px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  summary {
    color: var(--text-muted);
    cursor: default;
    padding: 2px 0;
  }

  details[open] {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 6px;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
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

  input,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  /* Save: the one accented button */
  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .primary:disabled {
    opacity: 0.45;
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
