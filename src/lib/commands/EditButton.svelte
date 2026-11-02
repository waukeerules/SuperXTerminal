<!--
  New Button / Edit Button, as SuperXPuTTY's dlgEditCommandButton: Label (required), Tooltip, Action (Send string or
  Run SPSL script), Icon (none, a colour circle, or a session icon), and the text to send or the script. For a script
  the SPSL panel shows beside it, and OK checks the script first.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../icons/Icon.svelte";
  import IconPicker from "../icons/IconPicker.svelte";
  import SpslHelp from "../scripting/SpslHelp.svelte";
  import { SEND_STRING_HELP } from "../scripting/sendString";
  import type { CustomIcon } from "../icons/custom";
  import type { BarButton } from "./bars";

  interface Props {
    button: BarButton | null;
    customs: ReadonlyMap<string, CustomIcon>;
    onSave: (button: BarButton) => void;
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
  let textBox: HTMLTextAreaElement;
  let help: SpslHelp | undefined = $state();

  const spsl = $derived(draft.action === "SpslScript");

  function insert(text: string): void {
    const at = textBox.selectionStart ?? draft.text.length;
    draft.text = draft.text.slice(0, at) + text + draft.text.slice(textBox.selectionEnd ?? at);
    queueMicrotask(() => textBox.focus());
  }

  async function ok(): Promise<void> {
    const label = draft.label.trim();
    if (!label) {
      error = "The button needs a label.";
      return;
    }
    if (spsl && help) {
      const issues = await help.check();
      const errors = issues.filter((i) => i.error).length;
      if (errors && !(await ask(`The script has ${errors} problem${errors === 1 ? "" : "s"} (see Check script). Save the button anyway?`, { title: "Edit Button", kind: "warning" }))) return;
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
        </div>
        <label class="text"><span>{spsl ? "Script:" : "Send:"}</span>
          <textarea bind:this={textBox} bind:value={draft.text} rows="9" spellcheck="false"></textarea>
        </label>
        <p class="hint">{spsl ? "SPSL: one command per line; see the panel on the right." : SEND_STRING_HELP}</p>
      </div>
      {#if spsl}
        <SpslHelp bind:this={help} script={draft.text} onInsert={insert} />
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

  .dialog.wide {
    width: min(980px, 96vw);
  }

  .columns {
    display: grid;
    grid-template-columns: 1fr;
    gap: 12px;
  }

  .wide .columns {
    grid-template-columns: 540px 1fr;
    height: 440px;
  }

  .fields {
    display: grid;
    gap: 8px;
    align-content: start;
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

  label.text {
    align-items: start;
  }

  textarea {
    font-family: Menlo, "Lucida Console", monospace;
    font-size: 12px;
    resize: vertical;
  }

  input,
  select,
  textarea,
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
