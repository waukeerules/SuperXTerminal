<!--
  The Send Commands toolbar, as SuperXPuTTY's: the Command box (Enter sends it, with Enter, to the selected tabs;
  Up and Down go through the history; any Ctrl+key goes straight to them), Send, the mask, the Script Editor, then
  "Select sessions to send to" and the icon group. {prompt:} and {secret:} values are asked for before sending;
  masked commands never go in the history, which lasts until the app closes (SuperXPuTTY's default).
-->
<script lang="ts">
  import Icon from "../icons/Icon.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { settings } from "../settings/store.svelte";
  import { context, selectedTargets, selection } from "./sending.svelte";
  import { resolveSnippet } from "../scripting/asking.svelte";
  import { keySequence, type KeyPress } from "../scripting/keys";
  import { lineFeedsAsEnter } from "../scripting/spsl";
  import SelectTabs from "./SelectTabs.svelte";

  let { onOpenEditor }: { onOpenEditor: () => void } = $props();

  let text = $state("");
  let masked = $state(false);
  /** Sent commands, newest first, without repeats */
  let history = $state<string[]>([]);
  /** Where Up and Down are in the history; -1 for the box's own text */
  let place = -1;
  let selecting = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let box: HTMLInputElement;

  const size = $derived(Math.round(settings.iconSize * 0.85));

  /** Send the box's text, then Enter, to the selected tabs */
  async function send(): Promise<void> {
    const typed = text;
    const targets = selectedTargets();
    if (targets.length === 0) {
      context.status("No selected sessions to send to");
      return;
    }
    const resolved = await resolveSnippet(typed, "Send command");
    if (resolved === null) return;
    for (const tab of targets) {
      tab.host.markOutput(false);
      void tab.host.type(lineFeedsAsEnter(resolved) + "\r");
    }
    // the text as typed, with its {prompt:} markers, so it asks again next time
    if (!masked && typed) history = [typed, ...history.filter((h) => h !== typed)];
    place = -1;
    text = "";
    context.status(`Sent command to ${targets.length} session${targets.length === 1 ? "" : "s"}`);
  }

  /** The key a keyboard event is, for Ctrl+key */
  function pressOf(e: KeyboardEvent): KeyPress | null {
    const names: Record<string, string> = {
      Enter: "ENTER", Tab: "TAB", Escape: "ESC", Backspace: "BACKSPACE", ArrowUp: "UP", ArrowDown: "DOWN",
      ArrowLeft: "LEFT", ArrowRight: "RIGHT", Home: "HOME", End: "END", PageUp: "PGUP", PageDown: "PGDN",
      Insert: "INSERT", Delete: "DELETE",
    };
    const mods = { ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey };
    if (names[e.key]) return { name: names[e.key], char: null, ...mods };
    if (/^F\d{1,2}$/.test(e.key)) return { name: e.key, char: null, ...mods };
    // the character the key types without Ctrl (e.code is the key, whatever Ctrl does to e.key)
    const letter = /^Key([A-Z])$/.exec(e.code)?.[1];
    const digit = /^Digit(\d)$/.exec(e.code)?.[1];
    const punct: Record<string, string> = { BracketLeft: "[", BracketRight: "]", Backslash: "\\", Minus: "-", Slash: "/", Space: " " };
    const ch = letter ? (e.shiftKey ? letter : letter.toLowerCase()) : digit ?? punct[e.code];
    return ch ? { name: null, char: ch, ...mods } : null;
  }

  function keydown(e: KeyboardEvent): void {
    if (e.key === "Enter" && !e.ctrlKey) {
      e.preventDefault();
      void send();
    } else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !e.ctrlKey && history.length) {
      e.preventDefault();
      // Up goes to older entries, wrapping round
      place = e.key === "ArrowUp" ? (place + 1) % history.length : place <= 0 ? history.length - 1 : place - 1;
      text = history[place];
    } else if (e.ctrlKey && !e.altKey && !["Control", "Shift", "Alt", "Meta"].includes(e.key)) {
      // any Ctrl+key goes straight to the selected tabs, as a terminal would send it
      const press = pressOf(e);
      if (!press) return;
      e.preventDefault();
      const sequence = keySequence(press);
      const targets = selectedTargets();
      for (const tab of targets) void tab.host.type(sequence);
      text = "";
      context.status(targets.length ? `Sent Ctrl+${e.code.replace(/^(Key|Digit)/, "")} to ${targets.length} session${targets.length === 1 ? "" : "s"}` : "No selected sessions to send to");
    }
  }

  /** The history, from the box's drop-down */
  function showHistory(event: MouseEvent): void {
    event.stopPropagation();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: rect.left,
      y: rect.bottom,
      items: history.length
        ? history.map((h) => ({ label: h, action: () => { text = h; box.focus(); } }))
        : [{ label: "No commands sent yet", disabled: "Commands you send are listed here" }],
    };
  }

  /** The icon groups: the session icons of the open tabs */
  function showGroups(event: MouseEvent): void {
    event.stopPropagation();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const icons = [...new Set((context.tabs?.tabList() ?? []).map((t) => t.icon.split("#")[0].toLowerCase()))].sort();
    menu = {
      x: rect.left,
      y: rect.bottom,
      items: [
        { label: "All tabs", bold: !selection.iconGroup, action: () => (selection.iconGroup = "") },
        { separator: true },
        ...icons.map((icon) => ({ label: icon, bold: selection.iconGroup === icon, action: () => (selection.iconGroup = icon) })),
      ],
    };
  }

  /** Focus the box (Send Commands' shortcut) */
  export function focus(): void {
    box?.focus();
  }
</script>

<div class="toolbar" aria-label="Send Commands">
  <label for="send-command"><Icon key="terminal" {size} />Command</label>
  <div class="combo">
    <input id="send-command" bind:this={box} bind:value={text} type={masked ? "password" : "text"} autocomplete="off"
      spellcheck="false" onkeydown={keydown} />
    <button class="drop" title="History" aria-label="History" onclick={showHistory}>▾</button>
  </div>
  <button class="glyph" title="Send to the selected sessions" aria-label="Send" onclick={() => void send()}><Icon key="check" {size} glyph /></button>
  <button class="glyph" title={masked ? "Typing is masked (click to show it)" : "Typing shows (click to mask it, e.g. for a password)"}
    aria-label="Mask" aria-pressed={masked} onclick={() => (masked = !masked)}><Icon key={masked ? "masked" : "unmasked"} {size} glyph /></button>
  <button class="glyph" title="Run Script" aria-label="Script Editor" onclick={onOpenEditor}><Icon key="script" {size} glyph /></button>
  <span class="separator"></span>
  <button class="glyph" title="Select sessions to send to" aria-label="Select sessions" onclick={() => (selecting = true)}><Icon key="selecttabs" {size} glyph /></button>
  <button class="glyph" title={selection.iconGroup ? `Sending only to tabs with the ${selection.iconGroup} icon` : "Select target icon-group"}
    aria-label="Icon group" onclick={showGroups}><Icon key={selection.iconGroup || "stop"} {size} glyph={!selection.iconGroup} /></button>
</div>

{#if selecting}
  <SelectTabs onClose={() => (selecting = false)} />
{/if}

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

<svelte:window onclick={() => (menu = null)} />

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    background: var(--bg-bar);
    border-bottom: 1px solid var(--border);
  }

  label {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-right: 6px;
  }

  .combo {
    display: flex;
    width: 400px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 2px;
  }

  .combo input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 2px 4px;
    font-family: Menlo, "Lucida Console", monospace;
    font-size: 12px;
  }

  .drop {
    border: 0;
    background: transparent;
    padding: 0 6px;
  }

  .glyph {
    display: flex;
    padding: 1px;
    background: none;
    border: 1px solid transparent;
    border-radius: 3px;
  }

  .glyph:hover {
    border-color: var(--border);
  }

  .separator {
    width: 1px;
    align-self: stretch;
    margin: 2px 4px;
    background: var(--border);
  }
</style>
