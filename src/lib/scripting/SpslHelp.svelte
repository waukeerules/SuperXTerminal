<!--
  The SPSL panel beside a script, as SuperXPuTTY's SpslHelpPanel: the commands (double-click inserts one), the saved
  scripts (double-click inserts one), and Check script, which lists problems without running anything.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { checkScript, type Issue } from "./spsl";
  import { SEND_KEY_NAMES } from "./keys";
  import { loadSessions } from "../sessions/api";

  interface Props {
    /** The script being edited, for Check script */
    script: string;
    /** Put text in the script at the cursor */
    onInsert: (text: string) => void;
  }

  let { script, onInsert }: Props = $props();

  let tab = $state<"commands" | "saved" | "check">("commands");
  let saved = $state<{ name: string; kind: string }[]>([]);
  let issues = $state<Issue[] | null>(null);

  /** The commands: how each is written, and what it does */
  const COMMANDS: [string, string][] = [
    ["SENDLINE text", "Type the text, then Enter"],
    ["SENDCHAR text", "Type the text, without Enter"],
    ["SENDKEY key", "Press one key: ENTER, TAB, ESC, UP, F5..., or a character; ^ Ctrl, % Alt, + Shift (^C); {TAB 3} presses it 3 times"],
    ["WAITFOR text", "Wait until the text appears in the output (30 s), else stop the script"],
    ["WAITFOR /t:10 text", "The same, waiting up to 10 seconds"],
    ["SLEEP 500", "Wait 500 milliseconds"],
    ["PROMPT question", "Ask for a value and type it, then Enter"],
    ["PWDPROMPT", "Ask for a password (hidden) and type it, then Enter; Cancel stops the script"],
    ["OPENSESSION Folder/name", "Open a saved session"],
    ["SENDBREAK", "Send a Break in a Serial or Telnet tab, e.g. to stop a Cisco switch's boot at ROMMON (SuperXPuTTY skips it)"],
    ["# comment", "A comment; so is #!/bin/spsl on the first line"],
  ];

  async function showSaved(): Promise<void> {
    tab = "saved";
    saved = await invoke<{ name: string; kind: string }[]>("scripts_list");
  }

  async function insertSaved(entry: { name: string; kind: string }): Promise<void> {
    onInsert(await invoke<string>("script_load", { name: entry.name, kind: entry.kind }));
  }

  /** Check script, as SpslChecker: the sessions and Environment Variables are read for it */
  export async function check(): Promise<Issue[]> {
    const loaded = await loadSessions();
    const ids = new Set(loaded.sessions.map((s) => s.id.toLowerCase()));
    const missing = new Set((await invoke<{ missing: string[] }>("variables_fill", { text: script })).missing);
    issues = checkScript(script, (id) => ids.has(id.toLowerCase()), (text) => (text.match(/\{\w+\}/g) ?? []).filter((m) => missing.has(m)));
    tab = "check";
    return issues;
  }
</script>

<div class="panel">
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "commands"} class:on={tab === "commands"} onclick={() => (tab = "commands")}>Commands</button>
    <button role="tab" aria-selected={tab === "saved"} class:on={tab === "saved"} onclick={showSaved}>Saved scripts</button>
    <button role="tab" aria-selected={tab === "check"} class:on={tab === "check"} onclick={() => void check()}>Check script</button>
  </div>
  <div class="body">
    {#if tab === "commands"}
      {#each COMMANDS as [syntax, what]}
        <button class="entry" title="Double-click to insert" ondblclick={() => onInsert(syntax.split(" ")[0] + " ")}>
          <code>{syntax}</code><span>{what}</span>
        </button>
      {/each}
      <p class="keys"><b>Keys for SENDKEY:</b> {SEND_KEY_NAMES.filter((k) => k.length > 1).join(", ")}</p>
    {:else if tab === "saved"}
      {#each saved as entry}
        <button class="entry" title="Double-click to insert" ondblclick={() => void insertSaved(entry)}>
          <code>{entry.name}</code><span>{entry.kind === "spsl" ? "SPSL" : "lines"}</span>
        </button>
      {:else}
        <p class="muted">No saved scripts in the settings folder's scripts folder.</p>
      {/each}
    {:else if issues}
      {#each issues as issue}
        <p class:error={issue.error} class:warning={!issue.error}>Line {issue.line}: {issue.message}</p>
      {:else}
        <p class="ok">No problems found.</p>
      {/each}
    {/if}
  </div>
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-input);
  }

  .tabs {
    display: flex;
    border-bottom: 1px solid var(--border);
  }

  .tabs button {
    flex: 1;
    background: none;
    border: 0;
    padding: 4px;
    color: var(--text-muted);
  }

  .tabs button.on {
    color: inherit;
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .body {
    flex: 1;
    overflow: auto;
    padding: 4px;
  }

  .entry {
    display: grid;
    gap: 2px;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    padding: 3px 4px;
    border-radius: 3px;
  }

  .entry:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .entry span,
  .muted,
  .keys {
    color: var(--text-muted);
    font-size: 12px;
  }

  p {
    margin: 4px;
  }

  .error {
    color: #e5534b;
  }

  .warning {
    color: #e3b341;
  }

  .ok {
    color: #3fb950;
  }
</style>
