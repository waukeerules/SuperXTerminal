<!--
  Settings > General: the settings folder (where the team's Sessions.xml is), the look, the sessions tree's icon and
  text size, and the environment colour name rules. These are SuperXPuTTY's Options > Paths (settings folder) and GUI
  (theme, session tree font, icon size) for the Mac; its Windows-only rows (PuTTY, plink, WinSCP...) aren't here.
-->
<script lang="ts">
  import { SEARCH_FIELDS, searchFieldsOf, toggledSearchFields } from "../sessions/tree";
  import { FILE_MANAGER, SYSTEM, THIS_COMPUTER } from "../platform";
  import { chooseSettingsFolder, saveSettings, settings, type ScriptLanguage } from "./store.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { checkForUpdates } from "./updates";
  import { shortcutLabel } from "../shortcuts";

  /** Choose where kept session logs go */
  async function chooseLogFolder(): Promise<void> {
    const path = await open({ directory: true, title: "Choose the folder for session logs" });
    if (typeof path === "string") await saveSettings({ sessionLogFolder: path });
  }
  import { parseRules } from "../icons/environment";
  import EnvironmentColours from "./EnvironmentColours.svelte";

  let editingColours = $state(false);
  let linksMessage = $state("");
  /** Why a Show in Finder / File Explorer beside a sessions file didn't work */
  let fileMessage = $state("");

  /**
   * Show Sessions.xml or PrivateSessions.xml in Finder or File Explorer, selected (its folder, before anything's
   * been saved to it), whether or not it's in use
   */
  async function showFile(store: "saved" | "private"): Promise<void> {
    try {
      fileMessage = "";
      await invoke("sessions_reveal", { store });
    } catch (e) {
      fileMessage = String(e);
    }
  }

  /** Make SuperXTerminal the app for ssh:// and telnet:// links (macOS asks the user to confirm) */
  async function makeDefault(): Promise<void> {
    try {
      await invoke("links_make_default");
      linksMessage = "Done: ssh:// and telnet:// links open here.";
    } catch (e) {
      linksMessage = String(e);
    }
  }
  /** The environment colour rules, for the count beside Environment colours… */
  const colourRules = $derived(parseRules(settings.environmentColorRules));

  /** The Sessions search's fields, as its ⋯ button shows them */
  const searchFields = $derived(searchFieldsOf(settings.treeSearchFields));

  /** Whether Saved sessions are in use: a settings folder chosen and Use Sessions.xml on (then Private can be turned off) */
  const savedInUse = $derived(!!settings.settingsFolder && settings.useSessionsXml);
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
    <div class="file">
      <label class="check" title="Untick to use only Private and Database sessions: Sessions.xml and Folders.xml are then neither read nor written (they're left as they are), and imports go to Private sessions">
        <input type="checkbox" checked={settings.useSessionsXml}
          onchange={(e) => void saveSettings({ useSessionsXml: e.currentTarget.checked })}
          disabled={settings.useSessionsXml && !settings.usePrivateSessions} /> Use Sessions.xml (Saved sessions)</label>
      <button onclick={() => void showFile("saved")} disabled={!settings.settingsFolder}
        title="Sessions.xml, in the settings folder">Show in {FILE_MANAGER}</button>
    </div>
    <div class="file">
      <label class="check" title="Untick to use only Saved and Database sessions: PrivateSessions.xml (only yours, on this computer) is then neither read nor written (it's left as it is), and nothing offers Private sessions">
        <input type="checkbox" checked={settings.usePrivateSessions}
          onchange={(e) => void saveSettings({ usePrivateSessions: e.currentTarget.checked })}
          disabled={settings.usePrivateSessions && !savedInUse} /> Use PrivateSessions.xml (Private sessions)</label>
      <button onclick={() => void showFile("private")}
        title="PrivateSessions.xml, in your own folder on {THIS_COMPUTER}">Show in {FILE_MANAGER}</button>
    </div>
    {#if fileMessage}<p class="hint">{fileMessage}</p>{/if}
    {#if !settings.usePrivateSessions || !savedInUse}
      <!-- sessions need somewhere to go: one of the two stays on -->
      <p class="hint">{savedInUse ? "Private sessions are off, so Saved sessions stay on." : "Private sessions stay on until a settings folder is chosen with Use Sessions.xml on."}</p>
    {/if}
  </fieldset>

  <fieldset>
    <legend>Menu bar</legend>
    <label class="check"><input type="checkbox" checked={settings.showInMenuBar}
      onchange={(e) => void saveSettings({ showInMenuBar: e.currentTarget.checked })} /> Show SuperXTerminal in the menu bar</label>
    <p class="hint">Its menu opens sessions (Favorites, Recent and every folder) without the window first, and
      SuperXTerminal keeps running there when its last window closes, as SuperXPuTTY's Minimize to tray.</p>
  </fieldset>

  <fieldset>
    <legend>Updates</legend>
    <label class="check"><input type="checkbox" checked={settings.autoUpdateCheck}
      onchange={(e) => void saveSettings({ autoUpdateCheck: e.currentTarget.checked })} /> Check for a newer version at start</label>
    <div class="row">
      <label class="repo">GitHub repository
        <input value={settings.updateRepository} placeholder="waukeerules/SuperXTerminal"
          onchange={(e) => void saveSettings({ updateRepository: e.currentTarget.value.trim() || "waukeerules/SuperXTerminal" })} /></label>
      <button onclick={() => void checkForUpdates(true)}>Check Now</button>
    </div>
  </fieldset>

  <fieldset>
    <legend>Other apps</legend>
    <label class="check"><input type="checkbox" checked={settings.filezillaPasswords}
      onchange={(e) => void saveSettings({ filezillaPasswords: e.currentTarget.checked })} /> Give FileZilla the session's password</label>
    <p class="hint">As SuperXPuTTY's "Allow plain-text password arguments". FileZilla (external) can only be given a
      password on its sftp:// address, which any program on {THIS_COMPUTER} can read while FileZilla runs; off, FileZilla asks
      for it. Connect external's Terminal window logs in by itself either way, without showing the password.</p>
  </fieldset>

  <fieldset>
    <legend>Links</legend>
    <p class="hint">superxterminal:// links open sessions and layouts (superxterminal://session/Lab/sw1,
      superxterminal://layout/Core). ssh:// and telnet:// links open in Terminal until SuperXTerminal is made their app.</p>
    <div class="row">
      <button onclick={makeDefault}>Open ssh:// and telnet:// links in SuperXTerminal</button>
      <span class="hint">{linksMessage}</span>
    </div>
  </fieldset>

  <fieldset>
    <legend>Tabs</legend>
    <div class="row">
      <label for="tab-close">Close button</label>
      <select id="tab-close" value={settings.tabClose} onchange={(e) => void saveSettings({ tabClose: e.currentTarget.value as "tab" | "right" })}>
        <option value="tab">On each tab</option>
        <option value="right">Beside the tab list (▾), for the active tab</option>
      </select>
    </div>
    <p class="hint">Beside the tab list, one ✕ stays in the same place at the right of the tab strip and closes the
      tab that's showing; {shortcutLabel("closeTab") || "Close Tab"} and each tab's right-click menu close tabs either way.</p>
  </fieldset>

  <fieldset>
    <legend>Button bar</legend>
    <label class="check"><input type="checkbox" checked={settings.confirmButtons}
      onchange={(e) => void saveSettings({ confirmButtons: e.currentTarget.checked })} /> Ask before a button runs</label>
    <p class="hint">A click on a button at the bottom of the window, meant for something else, would type its command
      into the session. With this on, each button asks first, naming where it will run.</p>
  </fieldset>

  <fieldset>
    <legend>Scripts</legend>
    <div class="row">
      <label for="script-language">New scripts are</label>
      <select id="script-language" value={settings.scriptLanguage}
        onchange={(e) => void saveSettings({ scriptLanguage: e.currentTarget.value as ScriptLanguage })}>
        <option value="rhai">Rhai scripts</option>
        <option value="spsl">SPSL scripts</option>
        <option value="lines">Lines to send</option>
      </select>
    </div>
    <p class="hint">What the Script Editor's Run as starts on, and a new button's Action. A script opened from a file,
      and a button already made, keep their own; either can still be changed there.</p>
  </fieldset>

  <fieldset>
    <legend>Look</legend>
    <label>Theme
      <select value={settings.theme} onchange={(e) => void saveSettings({ theme: e.currentTarget.value })}>
        <option value="system">Follow {SYSTEM}</option>
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
    <label class="check"><input type="checkbox" checked={settings.treeLines}
      onchange={(e) => void saveSettings({ treeLines: e.currentTarget.checked })} /> Show lines in the Sessions tree</label>
    <label class="check"><input type="checkbox" checked={settings.searchWidensPane}
      onchange={(e) => void saveSettings({ searchWidensPane: e.currentTarget.checked })} /> Widen the Sessions pane while searching, so names fit (up to 600 px)</label>
    <!-- the Sessions search's own Aa, .* and ⋯ buttons set the same, as SuperXPuTTY's Search and Search in… -->
    <div class="search-settings">
      <span class="label">Sessions search</span>
      <label class="check"><input type="checkbox" checked={settings.treeSearchCase}
        onchange={(e) => void saveSettings({ treeSearchCase: e.currentTarget.checked })} /> Match case</label>
      <label class="check"><input type="checkbox" checked={settings.treeSearchRegex}
        onchange={(e) => void saveSettings({ treeSearchRegex: e.currentTarget.checked })} /> Regular expression</label>
      <span class="label">Search in</span>
      <div class="fields">
        {#each SEARCH_FIELDS as [field, label] (field)}
          <label class="check"><input type="checkbox" checked={searchFields.includes(field)}
            disabled={searchFields.includes(field) && searchFields.length === 1}
            onchange={() => void saveSettings({ treeSearchFields: toggledSearchFields(searchFields, field) })} /> {label}</label>
        {/each}
      </div>
      <span class="buttons">
        <button onclick={() => void saveSettings({ treeSearchFields: toggledSearchFields(searchFields, "all") })}
          disabled={searchFields.length === SEARCH_FIELDS.length}>All fields</button>
        <button onclick={() => void saveSettings({ treeSearchFields: [] })} disabled={settings.treeSearchFields.length === 0}>Defaults</button>
      </span>
    </div>
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
    <legend>Diagnostics</legend>
    <p class="hint">For working out why an SSH session won't connect or log in. ssh's own messages (ssh -v) show in the
      tab, above its output: the algorithms agreed, the keys offered, the login methods the device allows. Passwords
      never appear in them. Each SSH session's command is also written to View > Log Viewer.</p>
    <label class="check"><input type="checkbox" checked={settings.sshVerbose}
      onchange={(e) => void saveSettings({ sshVerbose: e.currentTarget.checked })} /> Verbose SSH (ssh -v) for sessions opened from now on</label>
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
  .repo {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .repo input {
    flex: 1;
  }

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

  /* a sessions file's checkbox, with its Show in Finder / File Explorer at the right */
  .file {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
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

  .search-settings {
    display: grid;
    gap: 4px;
    margin-top: 4px;
  }

  .search-settings .label {
    color: var(--text-muted);
    font-size: 12px;
    margin-top: 4px;
  }

  .search-settings .fields {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 2px 12px;
  }

  .search-settings .buttons {
    display: flex;
    gap: 8px;
  }
</style>
