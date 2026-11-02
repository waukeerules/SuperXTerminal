<!--
  Settings > Shortcuts, as SuperXPuTTY's Options > Shortcuts: every action with its key, which "Set…" changes to the
  next key pressed (Esc to stop). Each action is a menu bar item, so its key works wherever the keyboard is, a
  terminal included (src-tauri/src/keys.rs). A key another action has can be taken from it; the Mac's own (Cmd+Q,
  Cmd+C, ...) can't be used.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { saveSettings } from "./store.svelte";

  interface Row {
    id: string;
    label: string;
    default: string;
    key: string;
  }

  /** keys.rs's RESERVED */
  const RESERVED = ["CmdOrCtrl+Q", "CmdOrCtrl+H", "CmdOrCtrl+Alt+H", "CmdOrCtrl+M", "CmdOrCtrl+C", "CmdOrCtrl+V", "CmdOrCtrl+X",
    "CmdOrCtrl+A", "CmdOrCtrl+Z", "CmdOrCtrl+Shift+Z", "CmdOrCtrl+Tab", "CmdOrCtrl+Space"];

  let rows = $state<Row[]>([]);
  /** The action whose key is being recorded */
  let recording = $state<string | null>(null);
  let message = $state("");

  onMount(async () => {
    rows = await invoke<Row[]>("shortcuts_keys");
  });

  /** Modifiers in one order, so the same key always compares equal */
  function normal(key: string): string {
    if (!key) return "";
    const parts = key.split("+");
    const main = parts.pop() ?? "";
    const order = ["Ctrl", "Alt", "Shift", "CmdOrCtrl"];
    const mods = parts.map((p) => (p === "Option" ? "Alt" : p === "Control" ? "Ctrl" : p === "Cmd" || p === "Command" ? "CmdOrCtrl" : p));
    return [...order.filter((m) => mods.includes(m)), main.toUpperCase()].join("+");
  }

  /** A key as the Mac's menus show it: ⌃⌥⇧⌘ then the key */
  function shown(key: string): string {
    if (!key) return "";
    const parts = normal(key).split("+");
    const main = parts.pop() ?? "";
    const glyph: Record<string, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", CmdOrCtrl: "⌘" };
    const keys: Record<string, string> = { LEFT: "←", RIGHT: "→", UP: "↑", DOWN: "↓", ENTER: "↩", BACKSPACE: "⌫", DELETE: "⌦", SPACE: "Space", TAB: "⇥", PAGEUP: "⇞", PAGEDOWN: "⇟", HOME: "↖", END: "↘" };
    return parts.map((p) => glyph[p] ?? p).join("") + (keys[main] ?? main);
  }

  /** A key press as an accelerator; null for one that can't be a shortcut (no modifier, or a modifier alone) */
  function accelerator(event: KeyboardEvent): string | null {
    const code = event.code;
    const named: Record<string, string> = {
      BracketLeft: "[", BracketRight: "]", Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", Backslash: "\\",
      Minus: "-", Equal: "=", Backquote: "`", ArrowLeft: "Left", ArrowRight: "Right", ArrowUp: "Up", ArrowDown: "Down",
      Enter: "Enter", Space: "Space", Tab: "Tab", Backspace: "Backspace", Delete: "Delete", Home: "Home", End: "End",
      PageUp: "PageUp", PageDown: "PageDown",
    };
    let main = named[code];
    if (!main && /^Key[A-Z]$/.test(code)) main = code.slice(3);
    if (!main && /^Digit\d$/.test(code)) main = code.slice(5);
    if (!main && /^F\d{1,2}$/.test(code)) main = code;
    if (!main) return null;
    const mods = [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", event.metaKey && "CmdOrCtrl"].filter(Boolean) as string[];
    // a plain key would stop it being typed into a terminal; function keys may go alone
    if (mods.length === 0 && !/^F\d/.test(main)) return null;
    return [...mods, main].join("+");
  }

  /** Keep the keys: only those changed from the defaults are stored (and the menu bar changes at once) */
  async function store(): Promise<void> {
    const changes: Record<string, string> = {};
    for (const row of rows) if (normal(row.key) !== normal(row.default)) changes[row.id] = row.key;
    await saveSettings({ shortcuts: changes });
  }

  async function setKey(row: Row, key: string): Promise<void> {
    const other = key ? rows.find((r) => r !== row && normal(r.key) === normal(key)) : undefined;
    if (other) {
      const sure = await ask(`${shown(key)} is ${other.label}'s. Give it to ${row.label} instead (${other.label} is left without one)?`,
        { title: "Shortcuts", kind: "warning", okLabel: "Give it" });
      if (!sure) return;
      other.key = "";
    }
    row.key = key;
    message = key ? `${row.label}: ${shown(key)}` : `${row.label}: no shortcut`;
    await store();
  }

  // while recording, the next key is the shortcut (caught before the Settings window or a terminal sees it)
  $effect(() => {
    if (!recording) return;
    const id = recording;
    const press = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        recording = null;
        message = "";
        return;
      }
      const key = accelerator(event);
      if (!key) {
        if (!["Meta", "Control", "Alt", "Shift"].includes(event.key)) message = "Use a key with ⌘, ⌃ or ⌥ (or a function key alone).";
        return;
      }
      if (RESERVED.some((r) => normal(r) === normal(key))) {
        message = `${shown(key)} is the Mac's own (Quit, Copy, Paste, ...): choose another.`;
        return;
      }
      recording = null;
      const row = rows.find((r) => r.id === id);
      if (row) void setKey(row, key);
    };
    window.addEventListener("keydown", press, true);
    return () => window.removeEventListener("keydown", press, true);
  });
</script>

<div class="shortcuts">
  <p class="hint">Each action is in the menu bar with its key, so the key works anywhere in SuperXTerminal, a terminal
    included. Click Set… and press the new key; Esc stops.</p>
  <div class="table" role="table" aria-label="Shortcuts">
    {#each rows as row (row.id)}
      <div class="row" class:recording={recording === row.id} role="row">
        <span class="label" role="cell">{row.label}</span>
        <span class="key" role="cell">{recording === row.id ? "Press the new key…" : shown(row.key) || "—"}</span>
        <span class="buttons" role="cell">
          <button onclick={() => { recording = recording === row.id ? null : row.id; message = ""; }}>{recording === row.id ? "Cancel" : "Set…"}</button>
          <button disabled={!row.key} onclick={() => void setKey(row, "")}>Clear</button>
          <button disabled={normal(row.key) === normal(row.default)} title={row.default ? `The default: ${shown(row.default)}` : "The default: none"}
            onclick={() => void setKey(row, row.default)}>Default</button>
        </span>
      </div>
    {/each}
  </div>
  <p class="message">{message}</p>
</div>

<style>
  .shortcuts {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    min-height: 0;
  }

  .hint,
  .message {
    margin: 0;
    color: var(--text-muted);
  }

  .message {
    min-height: 1.3em;
  }

  .table {
    flex: 1;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 150px auto;
    gap: 8px;
    align-items: center;
    padding: 3px 8px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  }

  .row.recording {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .key {
    font-family: -apple-system, system-ui, sans-serif;
    letter-spacing: 0.05em;
  }

  .buttons {
    display: flex;
    gap: 4px;
  }

  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 8px;
  }

  button:disabled {
    opacity: 0.45;
  }
</style>
