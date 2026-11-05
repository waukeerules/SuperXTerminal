<!--
  The SPSL panel beside a script, as SuperXPuTTY's SpslHelpPanel: the commands (double-click inserts one; EXPECT a
  whole block to fill in), the saved scripts (double-click inserts one), and Check script, which lists problems
  without running anything. SuperXTerminal's own commands and options (EXPECT, labels, WAITFOR's /i, /re, /else:)
  follow SuperXPuTTY's.
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

  /** The commands: how each is written, what it does, and what double-clicking inserts when it isn't the first word */
  const COMMANDS: [string, string, string?][] = [
    ["SENDLINE text", "Type the text, then Enter"],
    ["SENDCHAR text", "Type the text, without Enter"],
    ["SENDKEY key", "Press one key: ENTER, TAB, ESC, UP, F5..., or a character; ^ Ctrl, % Alt, + Shift (^C); {TAB 3} presses it 3 times"],
    ["WAITFOR text", "Wait until the text appears in the output (30 s), else stop the script"],
    ["WAITFOR /t:10 text", "The same, waiting up to 10 seconds"],
    ["WAITFOR /i text", "The same in any case: password: matches Password:", "WAITFOR /i "],
    ["WAITFOR /re [>#]$", "Wait for a regular expression; ^ and $ are each line's start and end", "WAITFOR /re "],
    ["WAITFOR /else:continue text", "If the text doesn't come in time, go on with the next line instead of stopping", "WAITFOR /else:continue "],
    ["WAITFOR /else:label text", "If it doesn't come, go to the line :label", "WAITFOR /else:"],
    ["SLEEP 500", "Wait 500 milliseconds"],
    ["PROMPT question", "Ask for a value and type it, then Enter"],
    ["PWDPROMPT", "Ask for a password (hidden) and type it, then Enter; Cancel stops the script"],
    ["OPENSESSION Folder/name", "Open a saved session"],
    ["SENDBREAK", "Send a Break in a Serial or Telnet tab, e.g. to stop a Cisco switch's boot at ROMMON (SuperXPuTTY skips it)"],
    [
      "EXPECT … CASE … END",
      "Wait for whichever CASE comes first and run the lines under it; AGAIN waits again, the end of a CASE goes on after END. Takes /t:, /i and /else: as WAITFOR. A CASE is \"text\", /regex/ (/regex/i) or text to the end of the line",
      'EXPECT /t:15\n  CASE "(yes/no"\n    SENDLINE yes\n    AGAIN\n  CASE assword:\n    PWDPROMPT\n    AGAIN\n  CASE /[>#]$/\nEND\n',
    ],
    ["CASE PROMPT", "In an EXPECT: the device's prompt (as WAITPROMPT), without a regular expression", "  CASE PROMPT\n"],
    ["WAITPROMPT", "Wait for the device's prompt (a last line ending in > # $ or %); /t: and /else: as WAITFOR. SET prompt to a regular expression for another"],
    ["WAITQUIET 1500", "Wait until the output has stopped changing for 1.5 s (a command's whole answer); /t: and /else: as WAITFOR", "WAITQUIET "],
    ["SET name value", "A variable: {name} in any later line is its value. SET name alone forgets it", "SET "],
    ["PROMPT /var:name question", "Ask, and keep the answer in {name} instead of typing it (PWDPROMPT /var:name too)", "PROMPT /var:"],
    ["{0} {1} {name}", "What the last wait matched ({0}), a regular expression's groups ({1}…{9}) and (?<name>…) groups", "{1}"],
    ["FAILON % Invalid input", "From here on, any wait that sees it stops the script, naming the device's line. /i and /re as WAITFOR; FAILON OFF forgets them", "FAILON "],
    ["ECHO text", "Show the text in the status bar, {variables} filled in"],
    ["SAVE file.txt", "Save what the last command printed (without its echo and prompt) to Documents/SuperXTerminal/output, or a full path; /append adds to it", "SAVE "],
    ["CALL name", "Run a saved script here (the Saved scripts tab), with the same variables, then go on"],
    ["REPEAT 5 … END", "Run the lines up to END 5 times; {repeat} is the round (1, 2…)", "REPEAT 5\n  \nEND\n"],
    ["LIMIT 10m", "The whole script may run that long (90s, 10m, 1h), its waits included; then it stops. A loop that goes round 10,000 times without waiting stops anyway", "LIMIT "],
    ["{host_address} {session_name}", "The tab's session: {host_address}, {host_port}, {session_name}, {session_id}, {session_folder}, {session_username}, {session_profile}, {session_protocol}", "{host_address}"],
    ["CHECK name text", "A prerequisite, watched in all the output from here on (/not: must NOT appear; /i, /re); once met, {name} is what it matched", "CHECK "],
    ["REQUIRE names", "Stop here unless these CHECKs (or all, with no names) are met, saying which isn't; /else:label goes there instead", "REQUIRE "],
    ["PAGER --More--", "From here on, a wait that sees the pager's prompt presses Space (/key:ENTER another key) and keeps waiting; PAGER OFF stops", "PAGER --More--\n"],
    ["WAITFOR /t:1800 /idle:60 text", "Wait up to 30 minutes, but stop if no new output comes for a minute (a stalled copy); EXPECT and WAITPROMPT take /idle: too", "WAITFOR /t:1800 /idle:60 "],
    ["COLLECT name /re pattern", "Every match in the last command's output, as a list (a regular expression's first group); {name_count} is how many", "COLLECT "],
    ["FOREACH item IN list … END", "Run the lines once per item of a COLLECT's list (or 10, 20, 30), {item} set to it", "FOREACH item IN \n  \nEND\n"],
    ["IF a == b … ELSE … END", "Run lines only when a test holds: == != < > <= >= (as numbers when both are), ~ /regex/ and !~, or IF {name} (not empty)", "IF  == \n  \nEND\n"],
    [":label", "A place GOTO and /else: can go to"],
    ["GOTO label", "Go on from the line :label"],
    ["STOP message", "End the script here, with the message in the status bar"],
    ["# comment", "A comment; so is #!/bin/spsl on the first line"],
  ];

  /** Show the Saved tab, with the saved scripts listed again */
  async function showSaved(): Promise<void> {
    tab = "saved";
    saved = await invoke<{ name: string; kind: string }[]>("scripts_list");
  }

  /** Insert a saved script's text into the script being edited */
  async function insertSaved(entry: { name: string; kind: string }): Promise<void> {
    onInsert(await invoke<string>("script_load", { name: entry.name, kind: entry.kind }));
  }

  /** Check script, as SpslChecker: the sessions and Environment Variables are read for it */
  export async function check(): Promise<Issue[]> {
    const loaded = await loadSessions();
    const ids = new Set(loaded.sessions.map((s) => s.id.toLowerCase()));
    const missing = new Set((await invoke<{ missing: string[] }>("variables_fill", { text: script })).missing);
    const scripts = new Set((await invoke<{ name: string; kind: string }[]>("scripts_list")).filter((s) => s.kind === "spsl").map((s) => s.name.toLowerCase()));
    issues = checkScript(script, (id) => ids.has(id.toLowerCase()), (text) => (text.match(/\{\w+\}/g) ?? []).filter((m) => missing.has(m)),
      (name) => scripts.has(name.toLowerCase()));
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
      {#each COMMANDS as [syntax, what, insert]}
        <button class="entry" title="Double-click to insert" ondblclick={() => onInsert(insert ?? syntax.split(" ")[0] + " ")}>
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
