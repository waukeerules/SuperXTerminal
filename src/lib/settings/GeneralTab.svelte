<!--
  Settings > General: the settings folder (where the team's Sessions.xml is), the look, the sessions tree's icon and
  text size, and the environment colour name rules. These are SuperXPuTTY's Options > Paths (settings folder) and GUI
  (theme, session tree font, icon size) for the Mac; its Windows-only rows (PuTTY, plink, WinSCP...) aren't here.
-->
<script lang="ts">
  import { SEARCH_FIELDS, searchFieldsOf, toggledSearchFields } from "../sessions/tree";
  import { FILE_MANAGER, ONEPASSWORD_INSTALL, SESSION_LOG_FOLDER, SYSTEM, THIS_COMPUTER } from "../platform";
  import { chooseSettingsFolder, saveSettings, settings, type ScriptLanguage } from "./store.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { checkForUpdates } from "./updates";
  import { shortcutLabel } from "../shortcuts";
  import { OP_REFERENCE_EXAMPLE } from "./onepassword";

  /** Choose where kept session logs go */
  async function chooseLogFolder(): Promise<void> {
    const path = await open({ directory: true, title: "Choose the folder for session logs" });
    if (typeof path === "string") await saveSettings({ sessionLogFolder: path });
  }
  import { parseRules } from "../icons/environment";
  import EnvironmentColours from "./EnvironmentColours.svelte";
  import RestoreBackup from "./RestoreBackup.svelte";
  import UiThemesEditor from "./UiThemesEditor.svelte";
  import { uiThemes } from "./uiThemes.svelte";

  let editingColours = $state(false);
  /** Which sessions file Restore from Backup… is open for; null while it's closed */
  let restoring = $state<"saved" | "private" | null>(null);
  let linksMessage = $state("");
  /** Why a Show in Finder / File Explorer beside a sessions file showed its folder only, or didn't work */
  let fileMessage = $state("");

  /**
   * Show Sessions.xml or PrivateSessions.xml in Finder or File Explorer, selected (its folder, saying why, before
   * anything's been saved to it), whether or not it's in use
   */
  async function showFile(store: "saved" | "private"): Promise<void> {
    try {
      fileMessage = (await invoke<string | null>("sessions_reveal", { store })) ?? "";
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
  /** Rhai's own site, for Settings > General > Scripts */
  const RHAI_SITE = "https://rhai.rs/";
  /** How Rhai scripts work in SuperXTerminal: docs/RHAI.md on GitHub, as a user without the source can read it */
  const RHAI_DOC = "https://github.com/waukeerules/SuperXTerminal/blob/main/docs/RHAI.md";
  /** How SPSL scripts work in SuperXTerminal: docs/SPSL.md on GitHub */
  const SPSL_DOC = "https://github.com/waukeerules/SuperXTerminal/blob/main/docs/SPSL.md";

  /** Edit Themes… is open */
  let editingThemes = $state(false);

  /** Check 1Password's state: waiting for it, what it said, and whether that's a problem (which shows how to install) */
  let opChecking = $state(false);
  let opMessage = $state("");
  let opProblem = $state(false);
  /** 1Password's CLI install command here, and its getting-started page (onepassword.rs's INSTALL, GET_STARTED) */
  const OP_INSTALL = ONEPASSWORD_INSTALL;
  const OP_GET_STARTED = "https://www.1password.dev/cli/get-started";
  /** How SuperXTerminal uses 1Password: docs/1PASSWORD.md on GitHub */
  const OP_DOC = "https://github.com/waukeerules/SuperXTerminal/blob/main/docs/1PASSWORD.md";

  /**
   * Check 1Password: whether its CLI works here, through the 1Password app, listing the vaults it can read, which
   * asks 1Password to approve it as a session's reference would
   */
  async function checkOnePassword(): Promise<void> {
    opChecking = true;
    opMessage = "";
    try {
      opMessage = await invoke<string>("onepassword_check");
      opProblem = false;
    } catch (e) {
      opMessage = String(e);
      opProblem = true;
    } finally {
      opChecking = false;
    }
  }

  /** A link in Settings opens in the default browser: in the app's own window it would replace the app */
  function openLink(event: MouseEvent): void {
    event.preventDefault();
    const url = (event.currentTarget as HTMLAnchorElement).href;
    void invoke("plugin:opener|open_url", { url }).catch(() => {});
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
      <span class="buttons">
        <button onclick={() => (restoring = "saved")} disabled={!settings.settingsFolder}
          title="Put back a copy of Sessions.xml kept before an earlier save">Restore from Backup…</button>
        <button onclick={() => void showFile("saved")} disabled={!settings.settingsFolder}
          title="Sessions.xml, in the settings folder">Show in {FILE_MANAGER}</button>
      </span>
    </div>
    <div class="file">
      <label class="check" title="Untick to use only Saved and Database sessions: PrivateSessions.xml (only yours, on this computer) is then neither read nor written (it's left as it is), and nothing offers Private sessions">
        <input type="checkbox" checked={settings.usePrivateSessions}
          onchange={(e) => void saveSettings({ usePrivateSessions: e.currentTarget.checked })}
          disabled={settings.usePrivateSessions && !savedInUse} /> Use PrivateSessions.xml (Private sessions)</label>
      <span class="buttons">
        <button onclick={() => (restoring = "private")}
          title="Put back a copy of PrivateSessions.xml kept before an earlier save">Restore from Backup…</button>
        <button onclick={() => void showFile("private")}
          title="PrivateSessions.xml, in your own folder on {THIS_COMPUTER}">Show in {FILE_MANAGER}</button>
      </span>
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
    <legend>1Password</legend>
    <label class="check"><input type="checkbox" checked={settings.onePassword}
      onchange={(e) => { opMessage = ""; opProblem = false; void saveSettings({ onePassword: e.currentTarget.checked }); }} /> Use 1Password references</label>
    {#if settings.onePassword}
      <p class="hint">A password, an Environment Variable's value, or a profile's key file can be a 1Password reference
        ({OP_REFERENCE_EXAMPLE}): it's read from 1Password, with its approval, as a session uses it. That needs
        1Password's CLI, linked to the 1Password app (Integrate with 1Password CLI in 1Password's Settings > Developer).
        SSH keys in 1Password need none of this: its SSH agent hands them to ssh by itself.
        <a href={OP_DOC} onclick={openLink}>How SuperXTerminal uses 1Password</a></p>
      <div class="row">
        <button onclick={() => void checkOnePassword()} disabled={opChecking}>{opChecking ? "Waiting for 1Password…" : "Check 1Password"}</button>
        <span class="hint" class:problem={opProblem}>{opMessage}</span>
      </div>
      {#if opProblem}
        <p class="hint">To install it: {#if "command" in OP_INSTALL}<code>{OP_INSTALL.command}</code>{:else}{OP_INSTALL.words}{/if}, then start SuperXTerminal again.
          <a href={OP_GET_STARTED} onclick={openLink}>1Password CLI: get started</a></p>
      {/if}
    {:else}
      <p class="hint">Turn it on to keep passwords and keys in 1Password and have them read, with its approval, as a
        session uses them. Off, nothing is read from 1Password and its CLI never runs. 1Password's SSH agent works
        either way, as ssh reaches it by itself.</p>
    {/if}
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
    <label class="check"><input type="checkbox" checked={settings.tabListSwitchSession}
        onchange={(e) => void saveSettings({ tabListSwitchSession: e.currentTarget.checked })} />
      The tab list (▾) opens Switch Session</label>
    <p class="hint">Its tabs with a box to type in, to find one among many by its name or its folder, as
      {shortcutLabel("switchSession") || "Switch Session"} shows every tab in the window. Off, ▾ lists them plainly.</p>
    <div class="row">
      <label for="tab-name-max">Longest tab name</label>
      <input id="tab-name-max" class="days" type="number" min="0" max="200" value={settings.tabNameMax}
        onchange={(e) => void saveSettings({ tabNameMax: Math.min(200, Math.max(0, Math.floor(Number(e.currentTarget.value) || 0))) })} />
      <span>characters (0: the whole name)</span>
    </div>
    <p class="hint">A longer name is cut short with …, the whole of it shown when the pointer is on the tab. Shorter
      fits more tabs in the strip; 0 never cuts a name, however wide that makes its tab.</p>
    <label class="check"><input type="checkbox" checked={settings.gridView}
        onchange={(e) => void saveSettings({ gridView: e.currentTarget.checked })} />
      Grid View: each tab in a tile of its own</label>
    <p class="hint">The same as View > Grid View ({shortcutLabel("toggleGridView") || "Grid View"}), and it stays as it's
      left: on, every window opens with its tabs tiled, a layout's too, and the grid is made again as tabs open and
      close. Connect … in a Grid in the Sessions tree turns it on.</p>
    <div class="row">
      <label for="grid-view-off">Grid View off</label>
      <select id="grid-view-off" value={settings.gridViewOff}
        onchange={(e) => void saveSettings({ gridViewOff: e.currentTarget.value as "restore" | "join" })}>
        <option value="restore">Puts back the splits from before</option>
        <option value="join">Joins all tabs in one group</option>
      </select>
    </div>
    <p class="hint">What turning View > Grid View off ({shortcutLabel("toggleGridView") || "Grid View"}) does with the
      tiled tabs. Put back, each split returns with its tabs, and a tab opened in the grid joins the split that was in
      front. Join All Tabs always joins them.</p>
  </fieldset>

  <fieldset>
    <legend>Button bar</legend>
    <label class="check"><input type="checkbox" checked={settings.confirmButtons}
      onchange={(e) => void saveSettings({ confirmButtons: e.currentTarget.checked })} /> Ask before a button runs</label>
    <p class="hint">A click on a button at the bottom of the window, meant for something else, would type its command
      into the session. With this on, each button asks first, naming where it will run.</p>
  </fieldset>

  <fieldset>
    <legend>Command bar</legend>
    <div class="row">
      <label for="command-history-days">Keep sent commands for</label>
      <input id="command-history-days" class="days" type="number" min="0" max="3650" value={settings.commandHistoryDays}
        onchange={(e) => void saveSettings({ commandHistoryDays: Math.min(3650, Math.max(0, Math.floor(Number(e.currentTarget.value) || 0))) })} />
      <span>days</span>
    </div>
    <p class="hint">Send Commands' history (Up, Down and its ▾), as SuperPuTTY's Save history. 0 keeps it only until
      SuperXTerminal closes; otherwise commands sent in the last that many days are there next time. Masked commands are
      never kept, and {"{prompt:}"} values are asked for again.</p>
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
    <dl class="languages">
      <dt>Rhai scripts</dt>
      <dd>A small programming language made to be built into apps (<a href={RHAI_SITE} onclick={openLink}>rhai.rs</a>):
        variables, sums, if and else, loops, lists and functions, with the same reach into the tab as SPSL: type, wait
        for the device, ask you something. For a script that has to decide, count or remember, such as checking what
        each port says before changing it. <a href={RHAI_DOC} onclick={openLink}>Rhai in SuperXTerminal</a></dd>
      <dt>SPSL scripts</dt>
      <dd>SuperPuTTY's own script language, one command a line (SENDLINE, WAITFOR, SLEEP...), made for typing and
        waiting: a login, or a few commands in a row. SuperXPuTTY's scripts and buttons are written in it, and run
        here as they are. <a href={SPSL_DOC} onclick={openLink}>SPSL in SuperXTerminal</a></dd>
      <dt>Lines to send</dt>
      <dd>No language: each line is typed into the session as it is, followed by Enter, without waiting for the
        device. For a few fixed commands, such as a button for show interfaces status.</dd>
    </dl>
  </fieldset>

  <fieldset>
    <legend>Look</legend>
    <label class="theme">Theme
      <select value={settings.theme} onchange={(e) => void saveSettings({ theme: e.currentTarget.value })}>
        <option value="system">Follow {SYSTEM}</option>
        <option value="light">Light</option>
        <option value="dark">Dark</option>
        <!-- the user's own (Edit Themes…), by name -->
        {#each uiThemes.list as theme (theme.name)}<option value={theme.name}>{theme.name}</option>{/each}
      </select>
      <button onclick={() => (editingThemes = true)} title="Make themes of your own: the window's background, panes, text, selected rows, checkboxes…">Edit Themes…</button>
    </label>
    {#if editingThemes}<UiThemesEditor onClose={() => (editingThemes = false)} />{/if}
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
      <input value={settings.sessionLogFolder} placeholder={SESSION_LOG_FOLDER} aria-label="Log folder" disabled={!settings.sessionLogEnabled}
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

{#if restoring}
  <RestoreBackup store={restoring} onClose={() => (restoring = null)} />
{/if}

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

  /* what Check 1Password found wrong */
  .hint.problem {
    color: #e5534b;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .row input {
    flex: 1;
  }

  .row input.days {
    flex: 0 0 70px;
  }

  .row {
    align-items: center;
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

  /* Theme, with Edit Themes… at the drop-down's right rather than wrapped under it */
  label.theme {
    grid-template-columns: 170px 220px auto;
    justify-items: start;
  }

  label.theme select {
    justify-self: stretch;
  }

  /* a sessions file's checkbox, with its Restore from Backup and Show in Finder / File Explorer at the right */
  .file {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .file .buttons {
    display: flex;
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

  /* Scripts: what each kind of script is, a name over its explanation */
  .languages {
    display: grid;
    gap: 2px 0;
    margin: 0;
  }

  .languages dt {
    font-weight: 600;
    margin-top: 4px;
  }

  .languages dd {
    margin: 0;
    color: var(--text-muted);
  }

  .languages a {
    color: var(--accent);
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
