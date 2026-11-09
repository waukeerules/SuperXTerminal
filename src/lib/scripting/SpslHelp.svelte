<!--
  The script panel beside a script, as SuperXPuTTY's SpslHelpPanel: the commands (double-click inserts one; EXPECT a
  whole block to fill in) and the saved scripts in the settings folder's scripts folder, shared by the team (click
  one to choose it; double-click or Insert puts it in the script; Edit… opens it to change and save again; Delete;
  and Save this script as… keeps the script being written as a new saved script, or over one, after asking).
  SuperXTerminal's own commands and options
  (EXPECT, labels, WAITFOR's /i, /re, /else:) follow SuperXPuTTY's. For a Rhai script (`language="rhai"`) the same
  panel lists Rhai's functions and the saved Rhai scripts. SuperXPuTTY's Check script tab is no longer here: the
  script's problems show under its box as it's written (ScriptProblems.svelte).
-->
<script lang="ts">
  import { ask } from "@tauri-apps/plugin-dialog";
  import { SEND_KEY_NAMES } from "./keys";
  import { SCRIPT_OUTPUT_FOLDER } from "../platform";
  import Prompt from "../ui/Prompt.svelte";
  import SavedScriptEditor from "./SavedScriptEditor.svelte";
  import { deleteScript, kindName, listScripts, loadScript, saveScript, scriptNameProblem, type SavedScript } from "./library";

  interface Props {
    /** Its language: SPSL's commands, or Rhai's functions */
    language?: "spsl" | "rhai";
    /** The script being written, for Save this script as… */
    script?: string;
    /** Put text in the script at the cursor; for a saved script, with its kind ("spsl", "rhai" or "lines") */
    onInsert: (text: string, kind?: string) => void;
    /** The saved scripts changed here (saved, edited or deleted), for a list of them elsewhere (the Script Editor's) */
    onSaved?: (scripts: SavedScript[]) => void;
  }

  let { onInsert, language = "spsl", script = "", onSaved }: Props = $props();

  let tab = $state<"commands" | "saved">("commands");
  let saved = $state<SavedScript[]>([]);
  /** The saved script clicked, for Insert, Edit… and Delete */
  let chosen = $state<SavedScript | null>(null);
  /** The saved script open in Edit…, if one is */
  let editing = $state<SavedScript | null>(null);
  /** Whether Save this script as… is asking for a name */
  let naming = $state(false);
  let note = $state("");

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
    ["SAVE file.txt", `Save what the last command printed (without its echo and prompt) to ${SCRIPT_OUTPUT_FOLDER}, or a full path; /append adds to it`, "SAVE "],
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

  /**
   * Rhai's functions (rhai_scripts.rs), and the language's own words a device script needs, as COMMANDS: how each is
   * written, what it does, and what double-clicking inserts
   */
  const RHAI_COMMANDS: [string, string, string][] = [
    ["send_line(\"text\")", "Type the text, then Enter", 'send_line("");'],
    ["send(\"text\")", "Type the text, without Enter; \\r is Enter, \\t Tab, \\x03 Ctrl+C", 'send("");'],
    ["wait_for(\"text\")", "Wait until the text appears in the output (30 s): true if it came, false if not, and the script goes on, so check it", 'wait_for("");'],
    ["wait_for(\"text\", 10)", "The same, waiting up to 10 seconds; 0 waits for as long as it takes", 'wait_for("", 10);'],
    ["wait_for(`[>#]$`, #{ regex: true })", "Wait for a regular expression; ^ and $ are each line's start and end", "wait_for(``, #{ regex: true });"],
    ["wait_for(\"password:\", #{ any_case: true })", "In any case: password: matches Password:. Options: timeout (seconds), regex, any_case", 'wait_for("", #{ any_case: true, timeout: 10 });'],
    ["if !wait_for(\"#\") { throw \"…\" }", "Stop the script when the text doesn't come, as SPSL's WAITFOR does", 'if !wait_for("#") { throw "no prompt"; }\n'],
    ["wait_for_any([\"#\", \">\"])", "Wait for whichever comes first (30 s, or seconds after the list): its index, 0 for the first, or -1 when none came", 'let i = wait_for_any(["", ""]);\nswitch i {\n  0 => {  }\n  1 => {  }\n  _ => { throw "none came"; }\n}\n'],
    ["wait_prompt()", "Wait for the device's prompt (a last line ending in > # $ or %); wait_prompt(10) waits up to 10 seconds", "wait_prompt();"],
    ["matched()", "What the last wait matched: matched()[0] the whole match, matched()[1]… a regular expression's groups", "matched()[1]"],
    ["sleep(500)", "Wait 500 milliseconds", "sleep(500);"],
    ["prompt(\"question\")", "Ask for a value: the answer, or \"\" when cancelled", 'let answer = prompt("");'],
    ["password(\"Password\")", "Ask for a password (hidden): the answer; Cancel stops the script", 'send_line(password("Password"));'],
    ["status(\"text\")", "Show the text in the status bar (print does too)", 'status("");'],
    ["env.ad_user", "An Environment Variable (as SPSL's {ad_user}), secrets included; one with no value stops the script naming it. env.has(\"name\") says whether it has one", "env."],
    ["session.host_address", "The tab's session: host_address, host_port, session_name, session_id, session_folder, session_username, session_profile, session_protocol", "session.host_address"],
    ["let name = value;", "A variable; `text ${name}` puts it in text (backquotes). A variable must be set before it's used", "let  = ;"],
    ["if a == b { … } else { … }", "Run lines only when a test holds: == != < > <= >=, && and, || or, ! not", "if  {\n  \n} else {\n  \n}\n"],
    ["for item in [\"a\", \"b\"] { … }", "Run the lines once per item; for n in 1..=5 counts 1 to 5", "for item in [] {\n  \n}\n"],
    ["while test { … }", "Run the lines while the test holds; loop { … } for ever, break to leave", "while  {\n  \n}\n"],
    ["text.contains(\"x\")", "Text: contains, starts_with, ends_with, split(\",\"), to_upper(), len; parse_int(\"42\") makes a number", ".contains(\"\")"],
    ["fn name(a) { … }", "A function of your own: name(1) runs it; it sees only its arguments, not the script's variables", "fn (a) {\n  \n}\n"],
    ["throw \"message\"", "End the script here, with the message in the status bar", 'throw "";'],
    ["// comment", "A comment, to the end of the line; /* … */ for several lines", "// "],
  ];

  /** The commands the panel lists for its language, as [syntax, what, insert] */
  const commands = $derived(language === "rhai" ? RHAI_COMMANDS : COMMANDS);

  /** The saved scripts this panel shows: for a Rhai script the Rhai ones, else all (SPSL and lines) */
  function shown(all: SavedScript[]): SavedScript[] {
    return language === "rhai" ? all.filter((s) => s.kind === "rhai") : all;
  }

  /** The saved scripts changed: list them again, keep the one chosen if it's still there, and say so */
  function changed(all: SavedScript[], message: string): void {
    saved = shown(all);
    chosen = saved.find((s) => s.name === chosen?.name && s.kind === chosen?.kind) ?? null;
    note = message;
    onSaved?.(all);
  }

  /** Show the Saved tab, with the saved scripts listed again */
  async function showSaved(): Promise<void> {
    tab = "saved";
    note = "";
    saved = shown(await listScripts());
    chosen = saved.find((s) => s.name === chosen?.name && s.kind === chosen?.kind) ?? null;
  }

  /** Insert a saved script's text into the script being edited */
  async function insertSaved(entry: SavedScript | null): Promise<void> {
    if (entry) onInsert(await loadScript(entry), entry.kind);
  }

  /** Delete the saved script chosen, after asking: it's the team's, in the settings folder */
  async function remove(): Promise<void> {
    const entry = chosen;
    if (!entry) return;
    if (!(await ask(`Delete the saved script "${entry.name}"? It's in the settings folder, so it goes for everyone who shares it.`, { title: "Saved scripts", kind: "warning", okLabel: "Delete" }))) return;
    chosen = null;
    changed(await deleteScript(entry), `Deleted "${entry.name}"`);
  }

  /**
   * Save the script being written as a saved script of this language, under a name; over one of that name (of any
   * kind, as a name is one script) only after asking
   */
  async function saveAs(name: string): Promise<void> {
    naming = false;
    const all = await listScripts();
    const same = all.find((s) => s.name.toLowerCase() === name.toLowerCase());
    if (same && !(await ask(`Replace the saved ${kindName(same.kind)} script "${same.name}" with this one?`, { title: "Saved scripts", kind: "warning", okLabel: "Replace" }))) return;
    try {
      const entry = { name, kind: language };
      changed(await saveScript(entry, script), `Saved "${name}"`);
      chosen = saved.find((s) => s.name === name) ?? null;
    } catch (e) {
      note = String(e);
    }
  }

  // while the name is asked for, Escape cancels that alone, not the dialog this panel is in (Edit Button's)
  $effect(() => {
    if (!naming) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      naming = false;
    };
    window.addEventListener("keydown", escape, true);
    return () => window.removeEventListener("keydown", escape, true);
  });
</script>

<div class="panel">
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === "commands"} class:on={tab === "commands"} onclick={() => (tab = "commands")}>Commands</button>
    <button role="tab" aria-selected={tab === "saved"} class:on={tab === "saved"} onclick={showSaved}>Saved scripts</button>
  </div>
  <div class="body">
    {#if tab === "commands"}
      {#each commands as [syntax, what, insert]}
        <button class="entry" title="Double-click to insert" ondblclick={() => onInsert(insert ?? syntax.split(" ")[0] + " ")}>
          <code>{syntax}</code><span>{what}</span>
        </button>
      {/each}
      {#if language === "rhai"}
        <p class="keys">How Rhai scripts work here, <b>session</b> and <b>env</b> included: <b>docs/RHAI.md</b>. The whole
          language: <b>rhai.rs/book</b>.</p>
      {:else}
        <p class="keys"><b>Keys for SENDKEY:</b> {SEND_KEY_NAMES.filter((k) => k.length > 1).join(", ")}</p>
      {/if}
    {:else if tab === "saved"}
      <div class="saved" role="listbox" aria-label="Saved scripts">
        {#each saved as entry (entry.name + entry.kind)}
          <button class="entry" class:on={chosen === entry} role="option" aria-selected={chosen === entry}
            title="Click to choose, double-click to insert" onclick={() => (chosen = entry)} ondblclick={() => void insertSaved(entry)}>
            <code>{entry.name}</code><span>{kindName(entry.kind)}</span>
          </button>
        {:else}
          <p class="muted">No saved {language === "rhai" ? "Rhai " : ""}scripts in the settings folder's scripts folder yet.</p>
        {/each}
      </div>
    {/if}
  </div>
  {#if tab === "saved"}
    <div class="actions">
      <button disabled={!chosen} onclick={() => void insertSaved(chosen)} title="Put the chosen script in this one, at the cursor">Insert</button>
      <button disabled={!chosen} onclick={() => (editing = chosen)} title="Open the chosen script to change it and save it again">Edit…</button>
      <button disabled={!chosen} onclick={() => void remove()}>Delete</button>
      <span class="grow"></span>
      <button disabled={!script.trim()} onclick={() => { note = ""; naming = true; }} title="Keep the script you're writing as a saved script">Save this script as…</button>
    </div>
    {#if note}<p class="note" role="status">{note}</p>{/if}
  {/if}
</div>

{#if editing}
  <SavedScriptEditor entry={editing} onClose={(all) => { const name = editing?.name; editing = null; if (all) changed(all, `Saved "${name}"`); }} />
{/if}

{#if naming}
  <Prompt title={`Save this ${kindName(language)} script as`} value={chosen?.kind === language ? chosen.name : ""} validate={scriptNameProblem}
    onCancel={() => (naming = false)} onOk={(name) => void saveAs(name)} />
{/if}

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

  .entry.on {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
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

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    padding: 4px;
    border-top: 1px solid var(--border);
  }

  .actions button {
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 8px;
    font-size: 12px;
  }

  .actions button:disabled {
    opacity: 0.5;
  }

  .grow {
    flex: 1;
  }

  .note {
    padding: 0 4px 4px;
    font-size: 12px;
    color: var(--text-muted);
  }


</style>
