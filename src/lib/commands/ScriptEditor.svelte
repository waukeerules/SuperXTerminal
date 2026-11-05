<!--
  The Script Editor, as SuperXPuTTY's dlgScriptEditor: the saved scripts on the left (the settings folder's scripts
  folder: Insert, Save as, Delete; double-click inserts), the script in the middle (the script box: Monaco, with SPSL's
  colours when it runs as an SPSL script, and Wrap lines), the SPSL panel on the right, and
  Open file, Save to file, Run as (Send lines or SPSL script), Step and Send at the bottom. Send goes to the Send
  Commands selection and closes the editor; Step does the same, the script pausing before each line until Next line in
  the status bar. It floats over the window without blocking it; drag it by its title.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ask, open, save } from "@tauri-apps/plugin-dialog";
  import Prompt from "../ui/Prompt.svelte";
  import SpslHelp from "../scripting/SpslHelp.svelte";
  import ScriptBox from "../scripting/ScriptBox.svelte";
  import { context, fillScriptPlaceholders, runScriptIn, selectedTargets } from "./sending.svelte";
  import { resolveSnippet } from "../scripting/asking.svelte";
  import { lineFeedsAsEnter } from "../scripting/spsl";

  let { onClose }: { /** Close, or after Send */ onClose: () => void } = $props();

  /** A saved script: its name, and "spsl" or "lines" */
  type Entry = { name: string; kind: string };

  let text = $state("");
  let kind = $state<"spsl" | "lines">("spsl");
  let saved = $state<Entry[]>([]);
  let chosen = $state<Entry | null>(null);
  let naming = $state(false);
  let help: SpslHelp | undefined = $state();
  let box: ScriptBox | undefined = $state();
  let at = $state({ x: 120, y: 90 });

  void refresh();

  /** Read the saved scripts' list again */
  async function refresh(): Promise<void> {
    saved = await invoke<Entry[]>("scripts_list");
  }

  /** Put text in the script at the cursor, in place of what's selected */
  function insert(more: string): void {
    box?.insert(more);
  }

  /** Insert a saved script's text at the cursor */
  async function insertSaved(entry: Entry | null): Promise<void> {
    if (!entry) return;
    insert(await invoke<string>("script_load", { name: entry.name, kind: entry.kind }));
  }

  /** Delete the saved script chosen, after asking */
  async function remove(): Promise<void> {
    if (!chosen) return;
    if (!(await ask(`Delete the saved script "${chosen.name}"?`, { title: "Saved scripts", kind: "warning", okLabel: "Delete" }))) return;
    saved = await invoke<Entry[]>("script_delete", { name: chosen.name, kind: chosen.kind });
    chosen = null;
  }

  /** Save the script under a name, in the scripts folder */
  async function saveAs(name: string): Promise<void> {
    naming = false;
    try {
      saved = await invoke<Entry[]>("script_save", { name, kind, text });
      context.status(`Saved the script "${name}"`);
    } catch (e) {
      context.status(String(e));
    }
  }

  /** Open a script file into the editor */
  async function openFile(): Promise<void> {
    const path = await open({ filters: [{ name: "Scripts", extensions: ["spsl", "txt"] }, { name: "All files", extensions: ["*"] }] });
    if (typeof path !== "string") return;
    text = await invoke<string>("text_file_read", { path });
    // an .spsl file, or one starting #!/bin/spsl, is a script
    if (path.toLowerCase().endsWith(".spsl") || text.startsWith("#!/bin/spsl")) kind = "spsl";
  }

  /** Save the script to a file of the user's choosing */
  async function saveFile(): Promise<void> {
    const path = await save({ defaultPath: kind === "spsl" ? "script.spsl" : "lines.txt" });
    if (path) await invoke("text_file_write", { path, text });
  }

  /**
   * Send to the selected tabs, as frmSuperPutty's Editor_ScriptReady, and close
   * @param stepping An SPSL script pauses before each line until Next line (Step)
   */
  async function send(stepping = false): Promise<void> {
    const targets = selectedTargets();
    if (targets.length === 0) {
      context.status("No selected sessions to send to");
      return;
    }
    if (kind === "spsl") {
      const issues = help ? await help.check() : [];
      const errors = issues.filter((i) => i.error).length;
      if (errors && !(await ask(`The script has ${errors} problem${errors === 1 ? "" : "s"} (see Check script). Send it anyway?`, { title: "Script Editor", kind: "warning" }))) return;
      const filled = await fillScriptPlaceholders(text);
      if (filled === null) return;
      const script = await resolveSnippet(filled, "Script Editor");
      if (script === null) return;
      for (const tab of targets) runScriptIn(tab, script, "Script", false, "", stepping);
    } else {
      // each line as the toolbar sends it: the text, then Enter; no placeholders or prompts
      for (const line of text.split("\n")) {
        for (const tab of targets) {
          tab.host.markOutput(false);
          await tab.host.type(lineFeedsAsEnter(line.replace(/\r$/, "")) + "\r");
        }
      }
    }
    context.status(`Sent the script to ${targets.length} session${targets.length === 1 ? "" : "s"}`);
    onClose();
  }

  /** Drag the window by its title */
  function drag(event: PointerEvent): void {
    const start = { x: event.clientX - at.x, y: event.clientY - at.y };
    const move = (e: PointerEvent) => (at = { x: Math.max(0, e.clientX - start.x), y: Math.max(0, e.clientY - start.y) });
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
</script>

<div class="editor" role="dialog" aria-label="Script Editor" style:left="{at.x}px" style:top="{at.y}px">
  <header role="toolbar" tabindex="-1" aria-label="Move the Script Editor" onpointerdown={drag}>
    <span>Script Editor</span>
    <button class="close" aria-label="Close" onclick={onClose}>×</button>
  </header>
  <div class="columns" class:spsl={kind === "spsl"}>
    <div class="library">
      <div class="title">Saved scripts</div>
      <div class="list" role="listbox" aria-label="Saved scripts">
        {#each saved as entry (entry.name + entry.kind)}
          <div class="entry" class:on={chosen === entry} role="option" aria-selected={chosen === entry} tabindex="-1"
            onclick={() => (chosen = entry)} onkeydown={(e) => e.key === "Enter" && void insertSaved(entry)}
            ondblclick={() => void insertSaved(entry)}>{entry.name} <span>· {entry.kind === "spsl" ? "SPSL" : "lines"}</span></div>
        {:else}
          <p class="muted">None yet</p>
        {/each}
      </div>
      <div class="row">
        <button disabled={!chosen} onclick={() => void insertSaved(chosen)}>Insert</button>
        <button onclick={() => (naming = true)}>Save as…</button>
        <button disabled={!chosen} onclick={() => void remove()}>Delete</button>
      </div>
    </div>
    <ScriptBox bind:this={box} bind:value={text} language={kind === "spsl" ? "spsl" : "plain"} label="Script"
      placeholder={kind === "spsl" ? "#!/bin/spsl\nWAITFOR #\nSENDLINE show version" : "one line per command"} />
    {#if kind === "spsl"}
      <SpslHelp bind:this={help} script={text} onInsert={insert} />
    {/if}
  </div>
  <footer>
    <button onclick={() => void openFile()}>Open file…</button>
    <button onclick={() => void saveFile()}>Save to file…</button>
    <span class="grow"></span>
    <label>Run as:
      <select bind:value={kind}>
        <option value="lines">Send lines</option>
        <option value="spsl">SPSL script</option>
      </select>
    </label>
    {#if kind === "spsl"}
      <button onclick={() => void send(true)} title="Run it a line at a time: Next line in the status bar runs each one">Step</button>
    {/if}
    <button class="primary" onclick={() => void send()}>Send</button>
  </footer>
</div>

{#if naming}
  <Prompt title="Save script as" value={chosen?.name ?? ""} onCancel={() => (naming = false)} onOk={(name) => void saveAs(name)}
    validate={(name) => (/^[A-Za-z0-9 ._-]{1,60}$/.test(name.trim()) ? "" : "Use 1 to 60 letters, digits, spaces, dots, dashes or underscores.")} />
{/if}

<style>
  .editor {
    position: fixed;
    z-index: 120;
    width: min(880px, 96vw);
    height: 440px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0 12px 12px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 8px 0 0;
    color: var(--text-muted);
    cursor: move;
    outline: none;
  }

  .close {
    border: 0;
    background: none;
    font-size: 18px;
  }

  .columns {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 190px 1fr;
    gap: 8px;
  }

  .columns.spsl {
    grid-template-columns: 190px 1fr 280px;
  }

  .library {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-height: 0;
  }

  .title,
  .muted,
  .entry span {
    color: var(--text-muted);
  }

  .list {
    flex: 1;
    overflow: auto;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
  }

  .entry {
    padding: 2px 6px;
    cursor: default;
  }

  .entry.on {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .muted {
    margin: 6px;
  }

  .row,
  footer {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .grow {
    flex: 1;
  }

  select,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
  }

  button:disabled {
    opacity: 0.5;
  }
</style>
