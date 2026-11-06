<!--
  New Button / Edit Button, as SuperXPuTTY's dlgEditCommandButton: Label (required), Tooltip, Action (Send string or
  Run SPSL script, Run Rhai script), Icon (none, a colour circle, or a session icon), and the text to send or the script, in the script
  box (Monaco, with SPSL's or Rhai's colours for a script, and Wrap lines). For a script, its problems show under it
  as it's written (click one to go to its line) and the script panel beside it, and OK
  checks the script first.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../icons/Icon.svelte";
  import IconPicker from "../icons/IconPicker.svelte";
  import SpslHelp from "../scripting/SpslHelp.svelte";
  import ScriptBox from "../scripting/ScriptBox.svelte";
  import WrapLines from "../scripting/WrapLines.svelte";
  import ScriptProblems from "../scripting/ScriptProblems.svelte";
  import { SEND_STRING_HELP } from "../scripting/sendString";
  import type { CustomIcon } from "../icons/custom";
  import type { BarButton } from "./bars";

  interface Props {
    /** The button edited; null for a new one */
    button: BarButton | null;
    /** The user's custom icons, for the icon and Choose Icon */
    customs: ReadonlyMap<string, CustomIcon>;
    /** OK: the button as edited */
    onSave: (button: BarButton) => void;
    /** Cancel, or Escape: nothing changes */
    onCancel: () => void;
  }

  let { button, customs, onSave, onCancel }: Props = $props();

  /** SuperXPuTTY's button colours */
  const COLORS = [
    { name: "Red", color: "#E5534B" }, { name: "Orange", color: "#F0883E" }, { name: "Yellow", color: "#E3B341" },
    { name: "Green", color: "#3FB950" }, { name: "Teal", color: "#2AA198" }, { name: "Blue", color: "#4493F8" },
    { name: "Purple", color: "#A371F7" }, { name: "Gray", color: "#8B949E" },
  ];

  let draft = $state<BarButton>(untrack(() => (button ? { ...button } : { label: "", tooltip: "", color: "", icon: "", action: "SendString", text: "" })));
  let error = $state("");
  let picking = $state(false);
  let textBox: ScriptBox | undefined = $state();
  let problems: ScriptProblems | undefined = $state();

  /** Whether the command is a script (SPSL or Rhai), which shows the script panel */
  const spsl = $derived(draft.action === "SpslScript" || draft.action === "RhaiScript");
  /** The script's language, for the panel and the script box's colours */
  const language = $derived(draft.action === "RhaiScript" ? "rhai" : "spsl");

  /** Put text in the command at the cursor, in place of what's selected */
  function insert(text: string): void {
    textBox?.insert(text);
  }

  /** Check the button has a label and a command, then give it back */
  async function ok(): Promise<void> {
    const label = draft.label.trim();
    if (!label) {
      error = "The button needs a label.";
      return;
    }
    if (spsl && problems) {
      const issues = await problems.check();
      const errors = issues.filter((i) => i.error).length;
      if (errors && !(await ask(`The script has ${errors} problem${errors === 1 ? "" : "s"} (listed under it). Save the button anyway?`, { title: "Edit Button", kind: "warning" }))) return;
    }
    // a button's lines are kept with LF, as SuperXPuTTY stores them
    onSave({ ...draft, label, tooltip: draft.tooltip.trim(), text: draft.text.replace(/\r\n/g, "\n") });
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !picking && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" class:wide={spsl} role="dialog" aria-modal="true" aria-label={button ? "Edit Button" : "New Button"}>
    <header>{button ? "Edit Button" : "New Button"}</header>
    <div class="columns">
      <div class="fields">
        <label><span>Label:</span><input bind:value={draft.label} /></label>
        <label><span>Tooltip:</span><input bind:value={draft.tooltip} placeholder="the text it sends, when empty" /></label>
        <label><span>Action:</span>
          <select bind:value={draft.action}>
            <option value="SendString">Send string</option>
            <option value="SpslScript">Run SPSL script</option>
            <option value="RhaiScript">Run Rhai script</option>
          </select>
        </label>
        <div class="icon-row">
          <span>Icon:</span>
          <button class="swatch" class:on={!draft.color && !draft.icon} title="None" aria-label="None"
            onclick={() => { draft.color = ""; draft.icon = ""; }}>∅</button>
          {#each COLORS as c}
            <button class="swatch" class:on={!draft.icon && draft.color.toUpperCase() === c.color} title={c.name} aria-label={c.name}
              style:background={c.color} onclick={() => { draft.color = c.color; draft.icon = ""; }}></button>
          {/each}
          <label class="custom" title="Custom colour">
            <input type="color" value={draft.color && !draft.icon ? draft.color : "#888888"}
              oninput={(e) => { draft.color = (e.currentTarget as HTMLInputElement).value.toUpperCase(); draft.icon = ""; }} />
          </label>
          <button class="choose" onclick={() => (picking = true)}>
            {#if draft.icon}<Icon key={draft.icon} size={18} {customs} />{/if}Choose icon…
          </button>
          <!-- the script box's switch, here at the row's end rather than in a row of its own above the box -->
          <div class="end"><WrapLines /></div>
        </div>
        <div class="text"><span>{spsl ? "Script:" : "Send:"}</span>
          <div class="script">
            <ScriptBox bind:this={textBox} bind:value={draft.text} language={spsl ? language : "plain"} label={spsl ? "Script" : "Send"} ownWrap={false} />
            {#if spsl}
              {#key language}
                <ScriptProblems bind:this={problems} script={draft.text} {language} onGoTo={(line) => textBox?.goToLine(line)} />
              {/key}
            {/if}
          </div>
        </div>
        <p class="hint">{!spsl ? SEND_STRING_HELP : language === "rhai" ? "Rhai: see the functions in the panel on the right." : "SPSL: one command per line; see the panel on the right."}</p>
      </div>
      {#if spsl}
        {#key language}
          <SpslHelp onInsert={insert} {language} script={draft.text} />
        {/key}
      {/if}
    </div>
    <footer>
      <span class="error">{error}</span>
      <button class="primary" onclick={() => void ok()}>OK</button>
      <button onclick={onCancel}>Cancel</button>
    </footer>
  </div>
</div>

{#if picking}
  <IconPicker value={draft.icon || "workstation"} {customs} onCancel={() => (picking = false)}
    onChoose={(key) => { draft.icon = key; picking = false; }} />
{/if}

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
    width: min(560px, 94vw);
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  /* a script gets most of the window, so a long one needs no scrolling sideways or popping out */
  .dialog.wide {
    width: min(1400px, 96vw);
    height: min(900px, 92vh);
    grid-template-rows: auto minmax(0, 1fr) auto;
  }

  .columns {
    display: grid;
    grid-template-columns: 1fr;
    gap: 12px;
  }

  /* the script takes the width left after the panel, which keeps a readable width of its own */
  .wide .columns {
    grid-template-columns: minmax(0, 1fr) minmax(300px, 400px);
    min-height: 0;
  }

  /* a column, so the script box takes the height the other fields leave */
  .fields {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }

  header,
  .hint {
    color: var(--text-muted);
  }

  .hint {
    margin: 0;
    font-size: 12px;
  }

  label,
  .icon-row {
    display: grid;
    grid-template-columns: 70px 1fr;
    align-items: center;
    gap: 8px;
  }

  .icon-row {
    display: flex;
    flex-wrap: wrap;
  }

  .icon-row > span {
    width: 70px;
  }

  .text {
    display: grid;
    grid-template-columns: 70px 1fr;
    gap: 8px;
    flex: 1;
    min-height: 200px;
  }

  /* the script box, its problems under it: the box takes the height they leave */
  .script {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    min-height: 0;
  }

  input,
  select,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  .swatch {
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 50%;
    font-size: 11px;
  }

  .swatch.on {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .custom {
    display: flex;
  }

  .custom input {
    width: 26px;
    height: 22px;
    padding: 0;
  }

  /* at the row's right end, lined up with the script box's right edge; a div, not a span, which the row makes 70px
     wide for "Icon:" (its text spilled past the box's edge then), and never squeezed */
  .end {
    margin-left: auto;
    flex-shrink: 0;
    display: flex;
    justify-content: flex-end;
  }

  .choose {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .error {
    flex: 1;
    color: #e5534b;
  }
</style>
