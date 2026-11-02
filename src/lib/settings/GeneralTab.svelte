<!--
  Settings > General: the settings folder (where the team's Sessions.xml is), the look, the sessions tree's icon and
  text size, and the environment colour name rules. These are SuperXPuTTY's Options > Paths (settings folder) and GUI
  (theme, session tree font, icon size) for the Mac; its Windows-only rows (PuTTY, plink, WinSCP...) aren't here.
-->
<script lang="ts">
  import { chooseSettingsFolder, saveSettings, settings } from "./store.svelte";
  import { open } from "@tauri-apps/plugin-dialog";

  /** Choose where kept session logs go */
  async function chooseLogFolder(): Promise<void> {
    const path = await open({ directory: true, title: "Choose the folder for session logs" });
    if (typeof path === "string") await saveSettings({ sessionLogFolder: path });
  }
  import { parseRules } from "../icons/environment";
  import EnvironmentColours from "./EnvironmentColours.svelte";

  let editingColours = $state(false);
  const colourRules = $derived(parseRules(settings.environmentColorRules));
</script>

<div class="general">
  <fieldset>
    <legend>Settings folder</legend>
    <p class="hint">The folder with the team's Sessions.xml, Folders.xml, icons and highlighting rules: a shared or
      synced folder (OneDrive, SMB) works, as on Windows.</p>
    <div class="row">
      <input readonly value={settings.settingsFolder ?? "Not chosen yet"} aria-label="Settings folder" />
      <button onclick={chooseSettingsFolder}>Choose…</button>
    </div>
    <label class="check" title="Untick to use only Private and Database sessions: Sessions.xml and Folders.xml are then neither read nor written (they're left as they are), and imports go to Private sessions">
      <input type="checkbox" checked={settings.useSessionsXml}
        onchange={(e) => void saveSettings({ useSessionsXml: e.currentTarget.checked })} /> Use Sessions.xml (Saved sessions)</label>
  </fieldset>

  <fieldset>
    <legend>Look</legend>
    <label>Theme
      <select value={settings.theme} onchange={(e) => void saveSettings({ theme: e.currentTarget.value })}>
        <option value="system">Follow macOS</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
      </select>
    </label>
    <label>Icon size
      <select value={String(settings.iconSize)} onchange={(e) => void saveSettings({ iconSize: Number(e.currentTarget.value) })}>
        <option value="16">16 px</option>
        <option value="20">20 px</option>
        <option value="24">24 px (default)</option>
        <option value="32">32 px</option>
      </select>
    </label>
    <label>Tree text size
      <select value={String(settings.treeFontSize)} onchange={(e) => void saveSettings({ treeFontSize: Number(e.currentTarget.value) })}>
        {#each [12, 13, 14, 15, 16, 18] as size}<option value={String(size)}>{size} px{size === 14 ? " (default)" : ""}</option>{/each}
      </select>
    </label>
  </fieldset>

  <fieldset>
    <legend>Dropped sessions</legend>
    <p class="hint">A session that ends without logging out (the network dropped, the device restarted) keeps its tab;
      Enter reconnects it. It can also reconnect by itself, as SuperXPuTTY's Options > Advanced.</p>
    <label class="check"><input type="checkbox" checked={settings.autoReconnect}
      onchange={(e) => void saveSettings({ autoReconnect: e.currentTarget.checked })} /> Reconnect automatically</label>
    <label>Attempts
      <input type="number" min="1" max="100" value={settings.autoReconnectAttempts} disabled={!settings.autoReconnect}
        onchange={(e) => void saveSettings({ autoReconnectAttempts: Math.max(1, Number(e.currentTarget.value) || 6) })} />
    </label>
    <label>Seconds between attempts
      <input type="number" min="1" max="3600" value={settings.autoReconnectDelaySeconds} disabled={!settings.autoReconnect}
        onchange={(e) => void saveSettings({ autoReconnectDelaySeconds: Math.max(1, Number(e.currentTarget.value) || 10) })} />
    </label>
  </fieldset>

  <fieldset>
    <legend>Session logs</legend>
    <p class="hint">Every tab logs what it shows, for Search Output and Highlighted Output. Kept logs stay; otherwise a
      tab's log is deleted when it closes.</p>
    <label class="check"><input type="checkbox" checked={settings.sessionLogEnabled}
      onchange={(e) => void saveSettings({ sessionLogEnabled: e.currentTarget.checked })} /> Keep a log of every session (one file per session, named with its start time)</label>
    <div class="row">
      <input value={settings.sessionLogFolder} placeholder="Documents/SuperXTerminal/logs" aria-label="Log folder" disabled={!settings.sessionLogEnabled}
        onchange={(e) => void saveSettings({ sessionLogFolder: e.currentTarget.value.trim() })} />
      <button disabled={!settings.sessionLogEnabled} onclick={chooseLogFolder}>Choose…</button>
    </div>
  </fieldset>

  <fieldset>
    <legend>Environment colours</legend>
    <p class="hint">A session with no colour of its own, and no folder colour, takes the first rule matching its name
      or a folder's name: * and ? are wildcards, e.g. Prod* in red. Edit… here or Tools > Environment Colours changes them.</p>
    <div class="row">
      <span class="rules">
        {#each colourRules as rule}<span class="rule"><i style:background={rule.color}></i>{rule.pattern}</span>{:else}No rules{/each}
      </span>
      <button onclick={() => (editingColours = true)}>Edit…</button>
    </div>
  </fieldset>
</div>

{#if editingColours}
  <EnvironmentColours onClose={() => (editingColours = false)} />
{/if}

<style>
  .rules {
    flex: 1;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    align-items: center;
    min-width: 0;
  }

  .rule {
    display: inline-flex;
    gap: 5px;
    align-items: center;
  }

  .rule i {
    width: 12px;
    height: 12px;
    border-radius: 3px;
  }

  .general {
    display: grid;
    gap: 12px;
    align-content: start;
    height: 100%;
    overflow: auto;
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    display: grid;
    gap: 8px;
    padding: 8px 12px 12px;
  }

  .hint {
    margin: 0;
    color: var(--text-muted);
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .row input {
    flex: 1;
  }

  label {
    display: grid;
    grid-template-columns: 170px 220px;
    align-items: center;
    gap: 8px;
  }

  label.check {
    display: flex;
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
</style>
