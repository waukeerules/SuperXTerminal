<!--
  Settings > Shortcuts, as SuperXPuTTY's Options > Shortcuts: every action with its key, which "Set…" changes to the
  next key pressed (Esc to stop). Each action is a menu bar item, so its key works wherever the keyboard is, a
  terminal included (src-tauri/src/keys.rs; on Windows the page catches them, src/lib/shortcuts.ts). A key another
  action has can be taken from it; the system's own (the Mac's Cmd+Q, Cmd+C...; Windows' Alt+F4 and the terminal's
  Ctrl+Shift+C/V) can't be used.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { saveSettings } from "./store.svelte";
  import { isMac, keyLabel, normalKey, pauseShortcuts, pressedKey, type ShortcutRow } from "../shortcuts";

  /** keys.rs's reserved keys for this system (the Mac's Cmd+Q, Cmd+C...; Windows' Alt+F4, Ctrl+Shift+C...) */
  let reserved: string[] = [];

  let rows = $state<ShortcutRow[]>([]);
  /** The action whose key is being recorded */
  let recording = $state<string | null>(null);
  let message = $state("");

  onMount(async () => {
    rows = await invoke<ShortcutRow[]>("shortcuts_keys");
    reserved = await invoke<string[]>("shortcuts_reserved");
  });

  /** Keep the keys: only those changed from the defaults are stored (and the menu bar changes at once) */
  async function store(): Promise<void> {
    const changes: Record<string, string> = {};
    for (const row of rows) if (normalKey(row.key) !== normalKey(row.default)) changes[row.id] = row.key;
    await saveSettings({ shortcuts: changes });
  }

  /** Give an action a key, or none; a key another action has is taken from it, after asking */
  async function setKey(row: ShortcutRow, key: string): Promise<void> {
    const other = key ? rows.find((r) => r !== row && normalKey(r.key) === normalKey(key)) : undefined;
    if (other) {
      const sure = await ask(`${keyLabel(key)} is ${other.label}'s. Give it to ${row.label} instead (${other.label} is left without one)?`,
        { title: "Shortcuts", kind: "warning", okLabel: "Give it" });
      if (!sure) return;
      other.key = "";
    }
    row.key = key;
    message = key ? `${row.label}: ${keyLabel(key)}` : `${row.label}: no shortcut`;
    await store();
  }

  // while recording, the next key is the shortcut (caught before the Settings window or a terminal sees it, and
  // before Windows' own shortcut catching runs the action it's already given to)
  $effect(() => {
    if (!recording) return;
    const id = recording;
    pauseShortcuts(true);
    const press = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Escape") {
        recording = null;
        message = "";
        return;
      }
      const key = pressedKey(event);
      if (!key) {
        if (!["Meta", "Control", "Alt", "Shift"].includes(event.key)) {
          message = isMac ? "Use a key with ⌘, ⌃ or ⌥ (or a function key, Page Up or Page Down alone)."
            : "Use a key with Ctrl or Alt (or a function key, Page Up or Page Down alone).";
        }
        return;
      }
      if (reserved.some((r) => normalKey(r) === normalKey(key))) {
        message = isMac ? `${keyLabel(key)} is the Mac's own (Quit, Copy, Paste, ...): choose another.`
          : `${keyLabel(key)} is kept for closing the window or the terminal's copy and paste: choose another.`;
        return;
      }
      recording = null;
      const row = rows.find((r) => r.id === id);
      if (row) void setKey(row, key);
    };
    window.addEventListener("keydown", press, true);
    return () => {
      window.removeEventListener("keydown", press, true);
      pauseShortcuts(false);
    };
  });
</script>

<div class="shortcuts">
  <p class="hint">Each action is in the menu bar with its key, so the key works anywhere in SuperXTerminal, a terminal
    included. Click Set… and press the new key; Esc stops.</p>
  <div class="table" role="table" aria-label="Shortcuts">
    {#each rows as row (row.id)}
      <div class="row" class:recording={recording === row.id} role="row">
        <span class="label" role="cell">{row.label}</span>
        <span class="key" role="cell">{recording === row.id ? "Press the new key…" : keyLabel(row.key) || "—"}</span>
        <span class="buttons" role="cell">
          <button onclick={() => { recording = recording === row.id ? null : row.id; message = ""; }}>{recording === row.id ? "Cancel" : "Set…"}</button>
          <button disabled={!row.key} onclick={() => void setKey(row, "")}>Clear</button>
          <button disabled={normalKey(row.key) === normalKey(row.default)} title={row.default ? `The default: ${keyLabel(row.default)}` : "The default: none"}
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
