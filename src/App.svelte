<!--
  The main window, laid out as SuperXPuTTY's: the Quick connect and Send Commands toolbars, the Sessions pane on the
  left, the document tabs, then the button bar and the status bar (with running scripts and their Stop).
-->
<script lang="ts">
  import Icon from "./lib/icons/Icon.svelte";
  import { onMount } from "svelte";
  import { TabArea } from "./lib/layout/TabArea";
  import SessionTree from "./lib/sessions/SessionTree.svelte";
  import Settings from "./lib/settings/Settings.svelte";
  import EnvironmentVariables from "./lib/settings/EnvironmentVariables.svelte";
  import DatabaseSettings from "./lib/sessions/DatabaseSettings.svelte";
  import { DATABASE_SAVED } from "./lib/sessions/api";
  import { isDark, loadSettings, settings } from "./lib/settings/store.svelte";
  import QuickConnect from "./lib/layout/QuickConnect.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { opensAsLocalShell, opensExternally, openExternally, type ImportKind, type SessionView, type Store } from "./lib/sessions/api";
  import { invoke } from "@tauri-apps/api/core";
  import SendCommands from "./lib/commands/SendCommands.svelte";
  import ButtonBar from "./lib/commands/ButtonBar.svelte";
  import ScriptEditor from "./lib/commands/ScriptEditor.svelte";
  import ScriptStatus from "./lib/commands/ScriptStatus.svelte";
  import Ask from "./lib/scripting/Ask.svelte";
  import PasteCheck from "./lib/terminal/PasteCheck.svelte";
  import { context, runScriptIn } from "./lib/commands/sending.svelte";
  import { saveSettings } from "./lib/settings/store.svelte";
  import type { CustomIcon } from "./lib/icons/custom";
  import Menu, { type MenuItem } from "./lib/ui/Menu.svelte";
  import Prompt from "./lib/ui/Prompt.svelte";
  import { tabMenu } from "./lib/layout/tabMenu";
  import SearchOutput from "./lib/terminal/SearchOutput.svelte";
  import SerialSettings from "./lib/terminal/SerialSettings.svelte";
  import type { PageHost } from "./lib/terminal/PageHost";
  import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";

  let tabElement: HTMLDivElement;
  let tabs = $state<TabArea | null>(null);
  let status = $state("Ready");
  let openSessions = $state<ReadonlySet<string>>(new Set());
  /** The Settings window's tab while it's open; null when closed */
  let settingsTab = $state<string | null>(null);
  let variablesOpen = $state(false);
  let databaseOpen = $state(false);
  let editorOpen = $state(false);
  let customs = $state<ReadonlyMap<string, CustomIcon>>(new Map());
  let tree: SessionTree | undefined = $state();
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let renaming = $state<{ id: string; title: string } | null>(null);
  /** The serial tab whose Serial Settings are open */
  let serialTab = $state<{ title: string; host: PageHost; session: { store: Store; id: string } | null } | null>(null);
  /** Search Output windows, one per tab */
  let searches = $state<{ id: string; title: string; logPath: string | null }[]>([]);

  // the menu bar's items (src-tauri/src/lib.rs) arrive as "menu" events with the item's id
  void listen<string>("menu", (event) => {
    if (event.payload === "settings") settingsTab = "General";
    else if (event.payload === "newLocalShell") newShell();
    else if (event.payload === "environmentVariables") variablesOpen = true;
    else if (event.payload === "databaseSessions") databaseOpen = true;
    else if (event.payload === "scriptEditor") editorOpen = true;
    else if (event.payload === "closeTab") tabs?.closeActive();
    else if (event.payload === "find") tabs?.activeHost()?.find();
    else if (event.payload === "clearScrollback") tabs?.activeHost()?.action("clearScrollback");
    else if (event.payload === "toggleSendCommands") void saveSettings({ showSendCommands: !settings.showSendCommands });
    else if (event.payload === "toggleButtonBar") void saveSettings({ showButtonBar: !settings.showButtonBar });
    else if (event.payload.startsWith("import:")) tree?.importSessions(event.payload.slice(7) as ImportKind);
    else if (event.payload === "exportCsv") void tree?.exportSessions(null, "Sessions.csv");
  });

  void loadSettings();

  // tabs follow the theme and icon size chosen in Settings
  $effect(() => {
    const dark = isDark();
    tabs?.setDark(dark);
  });
  $effect(() => {
    if (tabs) tabs.iconSize = settings.iconSize;
  });

  onMount(() => {
    tabs = new TabArea(tabElement, (open) => (openSessions = open));
    context.tabs = tabs;
    context.status = (text) => (status = text);
    context.openSession = (id) => tree?.openById(id) ?? false;
    context.editVariables = () => (variablesOpen = true);
    tabs.onAlert = (text, background) => {
      status = text;
      // in the background, the Dock icon bounces once, as Windows flashes the taskbar button
      if (background) void getCurrentWindow().requestUserAttention(UserAttentionType.Informational).catch(() => {});
    };
    // a Search Output window closes with its tab
    tabs.onChange(() => {
      const open = new Set(tabs?.tabList().map((t) => t.id));
      if (searches.some((s) => !open.has(s.id))) searches = searches.filter((s) => open.has(s.id));
    });
    tabs.onTabMenu = (id, x, y) => {
      if (!tabs || !tree) return;
      const t = tree;
      menu = {
        x,
        y,
        items: tabMenu(
          {
            tabs,
            sessions: t.allSessions(),
            open: (session) => t.openById(session.id),
            saveAs: (store, sessionId, title) => void t.saveSessionAs(store, sessionId, title),
            rename: (tabId, title) => (renaming = { id: tabId, title }),
            serialSettings: (tabId) => {
              const host = tabs?.hosts.get(tabId);
              const target = tabs?.paramsOf(tabId)?.target;
              if (!host) return;
              serialTab = {
                title: tabs?.api.getPanel(tabId)?.title ?? "",
                host,
                session: target?.kind === "session" ? { store: target.store, id: target.id } : null,
              };
            },
            search: (tabId) => {
              if (searches.some((s) => s.id === tabId)) return;
              const host = tabs?.hosts.get(tabId);
              searches = [...searches, { id: tabId, title: tabs?.api.getPanel(tabId)?.title ?? "", logPath: host?.logPath ?? null }];
            },
            status: (text) => (status = text),
          },
          id,
        ),
      };
    };
    tabs.openLocalShell();
    status = "Opened session: Local Shell";
    return () => tabs?.dispose();
  });

  /** Open a session from the tree in a new tab */
  function openSession(session: SessionView, environment: string | null): void {
    // RDP and VNC open in their own apps, as SuperXPuTTY's launchers
    if (opensExternally(session.protocol)) {
      openExternally(session.store, session.id, "open").then((text) => (status = text), (e) => (status = String(e)));
      return;
    }
    const id = tabs?.openSession(session.store, session.id, session.name, opensAsLocalShell(session.protocol), session.icon, environment);
    status = `Opened session: ${session.name} [${session.protocol || "SSH"}]`;
    if (id) void runOwnScript(session, id);
  }

  /**
   * The session's own script (Edit Session's SPSL script), as SuperXPuTTY runs it when the session opens: only a
   * script starting #!/bin/spsl, and its first WAITFOR sees everything since the session started
   */
  async function runOwnScript(session: SessionView, tabId: string): Promise<void> {
    const found = await invoke<{ kind: string; text?: string; url?: string; path?: string }>("session_script", { store: session.store, id: session.id });
    let script = found.text ?? "";
    if (found.kind === "url" && found.url) script = await fetch(found.url).then((r) => (r.ok ? r.text() : "")).catch(() => "");
    if (found.kind === "missing") status = `${session.name}'s script ${found.path} isn't on this Mac`;
    if (!script.startsWith("#!/bin/spsl")) return;
    const tab = tabs?.tabList().find((t) => t.id === tabId);
    if (!tab) return;
    await tab.host.started;
    runScriptIn(tab, script.replace(/\r\n/g, "\n"), "Login script", true);
  }

  /** Open another Local Shell tab (File > New Local Shell, Cmd+T) */
  function newShell(): void {
    tabs?.openLocalShell();
    status = "Opened session: Local Shell";
  }
</script>

{#if databaseOpen}
  <DatabaseSettings onCancel={() => (databaseOpen = false)} onSaved={async () => {
    databaseOpen = false;
    await loadSettings();
    window.dispatchEvent(new Event(DATABASE_SAVED));
  }} />
{/if}

{#if variablesOpen}
  <EnvironmentVariables onClose={() => (variablesOpen = false)} />
{/if}

{#if settingsTab}
  <Settings tab={settingsTab} onClose={() => (settingsTab = null)} />
{/if}

<div class="window">
  <QuickConnect onProblem={(text) => (status = text)} onConnect={(quick, title, local) => {
    tabs?.openQuickConnect(quick, title, local);
    status = `Opened session: ${title} [${quick.protocol}]`;
  }} />
  {#if settings.showSendCommands}
    <SendCommands onOpenEditor={() => (editorOpen = true)} />
  {/if}

  <div class="main">
    <aside class="sessions">
      <div class="pane-title">Sessions</div>
      <SessionTree bind:this={tree} {openSessions} onOpen={openSession} onOpenProfiles={() => (settingsTab = "Profiles")} onLook={(_, icons) => {
        customs = icons;
        if (tabs) tabs.customs = icons;
      }} />
    </aside>
    <div class="tabs" bind:this={tabElement}></div>
  </div>

  {#if settings.showButtonBar}
    <ButtonBar {customs} />
  {/if}
  <div class="statusbar">
    <span class="status">{status}</span>
    <ScriptStatus />
    <span class="version">0.1.0</span>
  </div>
</div>

{#if editorOpen}
  <ScriptEditor onClose={() => (editorOpen = false)} />
{/if}

<Ask />
<PasteCheck />

{#each searches as search (search.id)}
  <SearchOutput title={search.title} logPath={search.logPath} onClose={() => (searches = searches.filter((s) => s !== search))} />
{/each}

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

{#if serialTab}
  <SerialSettings title={serialTab.title} host={serialTab.host} session={serialTab.session} onClose={() => (serialTab = null)} />
{/if}

{#if renaming}
  <Prompt title="Rename tab" value={renaming.title} validate={(name) => (name.trim() ? "" : "Give the tab a name")}
    onOk={(name) => { if (renaming) tabs?.rename(renaming.id, name.trim()); renaming = null; }} onCancel={() => (renaming = null)} />
{/if}

<svelte:window onclick={() => (menu = null)} />

<style>
  .window {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .main {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .sessions {
    width: 280px;
    display: flex;
    flex-direction: column;
    background: var(--bg-pane);
    border-right: 1px solid var(--border);
  }

  .pane-title {
    padding: 6px 8px;
    color: var(--text-muted);
    font-size: 14px;
  }

  .tabs {
    flex: 1;
    min-width: 0;
  }

  .tabs :global(.terminal-panel),
  .tabs :global(.terminal-frame) {
    width: 100%;
    height: 100%;
    border: 0;
    display: block;
  }

  .statusbar {
    display: flex;
    gap: 8px;
    padding: 3px 8px;
    background: var(--bg-bar);
    color: var(--text-muted);
    font-size: 12px;
  }
  .statusbar .status {
    flex: 1;
  }
</style>
