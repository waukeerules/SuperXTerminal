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
  import EnvironmentColours from "./lib/settings/EnvironmentColours.svelte";
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
  import QuickSelector, { type QuickItem } from "./lib/sessions/QuickSelector.svelte";
  import type { PageHost } from "./lib/terminal/PageHost";
  import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";

  let tabElement: HTMLDivElement;
  let tabs = $state<TabArea | null>(null);
  let status = $state("Ready");
  let openSessions = $state<ReadonlySet<string>>(new Set());
  /** The Settings window's tab while it's open; null when closed */
  let settingsTab = $state<string | null>(null);
  let variablesOpen = $state(false);
  let coloursOpen = $state(false);
  /** The Quick Selector open: its title and entries, and what choosing one does */
  let quick = $state<{ title: string; items: QuickItem[]; choose: (item: QuickItem) => void } | null>(null);
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
    else if (event.payload === "environmentColours") coloursOpen = true;
    else if (event.payload === "openSession") quickOpen();
    else if (event.payload === "switchSession") quickSwitch();
    else if (event.payload === "databaseSessions") databaseOpen = true;
    else if (event.payload === "scriptEditor") editorOpen = true;
    else if (event.payload === "closeTab") tabs?.closeActive();
    else if (event.payload === "find") tabs?.activeHost()?.find();
    else if (event.payload === "clearScrollback") tabs?.activeHost()?.action("clearScrollback");
    else if (event.payload === "toggleSendCommands") void saveSettings({ showSendCommands: !settings.showSendCommands });
    else if (event.payload === "toggleButtonBar") void saveSettings({ showButtonBar: !settings.showButtonBar });
    else if (event.payload === "toggleSessionsAutoHide") setAutoHide(!settings.sessionsAutoHide);
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
    // the tab strip's ▾: the open tabs, the active one ticked; a click switches to it
    tabs.onTabList = (right, bottom) => {
      if (!tabs) return;
      const t = tabs;
      menu = {
        x: right - 240,
        y: bottom + 2,
        items: t.allTabs().map((tab) => ({ label: tab.title, icon: tab.icon, environment: tab.environment, checked: tab.active, action: () => t.activate(tab.id) })),
      };
    };
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

  /** The Sessions pane's width while its edge is dragged; the setting otherwise */
  let paneWidth = $state<number | null>(null);
  const PANE_DEFAULT = 280;
  const clampWidth = (w: number) => Math.round(Math.max(160, Math.min(w, window.innerWidth - 320)));

  /**
   * Drag the Sessions pane's edge, as SuperXPuTTY's docked pane splitter: 8 points wide with a grip line down its
   * middle so it's easy to find; the width is kept in the settings. Tabs' web pages don't take the pointer while
   * dragging, so it isn't lost over a terminal.
   */
  function startResize(event: PointerEvent): void {
    if (event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);
    const startX = event.clientX;
    const startWidth = settings.sessionsPaneWidth || PANE_DEFAULT;
    document.body.classList.add("resizing-pane");
    const move = (e: PointerEvent) => (paneWidth = clampWidth(startWidth + e.clientX - startX));
    const done = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", done);
      handle.removeEventListener("pointercancel", done);
      document.body.classList.remove("resizing-pane");
      if (paneWidth !== null && paneWidth !== settings.sessionsPaneWidth) void saveSettings({ sessionsPaneWidth: paneWidth });
      paneWidth = null;
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", done);
    handle.addEventListener("pointercancel", done);
  }

  /**
   * Auto hide, as DockPanelSuite's unpinned pane in SuperXPuTTY: the Sessions pane folds to a tab at the left edge.
   * Hovering it (or a click) slides the pane out over the tabs; it goes again a couple of seconds after the pointer
   * leaves it, when a session opens, or when a terminal is clicked.
   */
  let slid = $state(false);
  let showTimer: ReturnType<typeof setTimeout> | undefined;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;
  const HIDE_AFTER = 2000;

  function setAutoHide(on: boolean): void {
    slid = false;
    void saveSettings({ sessionsAutoHide: on });
  }

  function slideOut(): void {
    clearTimeout(showTimer);
    clearTimeout(hideTimer);
    slid = true;
  }

  function slideIn(): void {
    clearTimeout(showTimer);
    clearTimeout(hideTimer);
    slid = false;
  }

  /** The pointer left the pane (or the edge tab without it opening): hide after a while, unless it comes back */
  function scheduleHide(): void {
    clearTimeout(showTimer);
    clearTimeout(hideTimer);
    // not while its edge is being dragged to resize it
    if (settings.sessionsAutoHide && slid && !document.body.classList.contains("resizing-pane")) hideTimer = setTimeout(() => (slid = false), HIDE_AFTER);
  }

  // a click in a terminal (its own page) takes the focus from this one: the pane goes, as when another pane is clicked
  $effect(() => {
    const blurred = () => {
      if (settings.sessionsAutoHide && slid && document.activeElement?.tagName === "IFRAME") slideIn();
    };
    window.addEventListener("blur", blurred);
    return () => window.removeEventListener("blur", blurred);
  });

  /** Open a session from the tree in a new tab */
  function openSession(session: SessionView, environment: string | null): void {
    if (settings.sessionsAutoHide) slideIn();
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

  /** Where a session is, for the Quick Selector's detail: its path, after Private/ or Database/ outside Saved sessions */
  function pathOf(session: SessionView): string {
    const path = session.id.replace(/^@db\//, "");
    return session.store === "saved" ? path : `${session.store === "private" ? "Private" : "Database"}/${path}`;
  }

  /** File > Open Session… (Cmd+O): every session, as SuperXPuTTY's Open session */
  function quickOpen(): void {
    const t = tree;
    if (!t) return;
    const sessions = t.allSessions();
    quick = {
      title: "Open Session",
      items: sessions.map((s) => ({ key: `${s.store}:${s.id}`, name: s.name, detail: pathOf(s), icon: s.icon, environment: t.environmentOf(s) })),
      choose: (item) => {
        const session = sessions.find((s) => `${s.store}:${s.id}` === item.key);
        if (session) t.openSession(session);
      },
    };
  }

  /** Window > Switch Session… (Cmd+Shift+O): the open tabs, as SuperXPuTTY's Switch session */
  function quickSwitch(): void {
    const t = tabs;
    if (!t) return;
    const sessions = tree?.allSessions() ?? [];
    quick = {
      title: "Switch Session",
      items: t.allTabs().map((tab) => {
        const session = sessions.find((s) => `${s.store}:${s.id}` === tab.session);
        return { key: tab.id, name: tab.title, detail: session ? pathOf(session) : "", icon: tab.icon, environment: tab.environment, current: tab.active };
      }),
      choose: (item) => t.activate(item.key),
    };
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

{#if coloursOpen}
  <EnvironmentColours onClose={() => (coloursOpen = false)} />
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

  <div class="main" class:autohide={settings.sessionsAutoHide}>
    {#if settings.sessionsAutoHide}
      <!-- the auto-hide strip, as DockPanelSuite's: a tab per folded pane, each only as tall as its name (more panes
           folded to this side would add theirs below) -->
      <div class="edge">
        <button class="edge-tab" class:active={slid} title="Sessions" onclick={slideOut}
          onmouseenter={() => { clearTimeout(hideTimer); showTimer = setTimeout(slideOut, 250); }}
          onmouseleave={scheduleHide}>Sessions</button>
      </div>
    {/if}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="side" class:out={slid} onmouseenter={() => clearTimeout(hideTimer)} onmouseleave={scheduleHide}>
      <aside class="sessions" style:width="{paneWidth ?? clampWidth(settings.sessionsPaneWidth || PANE_DEFAULT)}px">
        <div class="pane-title">
          <span>Sessions</span>
          <button class="pin" class:unpinned={settings.sessionsAutoHide} onclick={() => setAutoHide(!settings.sessionsAutoHide)}
            title={settings.sessionsAutoHide ? "Keep the pane open (dock it)" : "Auto hide: fold the pane to the left edge"}
            aria-label={settings.sessionsAutoHide ? "Dock the Sessions pane" : "Auto hide the Sessions pane"}>
            <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
              <path d="M5 1.5h6M6 1.5v5L4 9h8l-2-2.5v-5M8 9v5.5" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round" stroke-linecap="round" />
            </svg>
          </button>
        </div>
        <SessionTree bind:this={tree} {openSessions} onOpen={openSession} onOpenProfiles={() => (settingsTab = "Profiles")} onLook={(_, icons) => {
          customs = icons;
          if (tabs) tabs.customs = icons;
        }} />
      </aside>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="splitter" title="Drag to resize; double-click for the usual width" onpointerdown={startResize}
        ondblclick={() => void saveSettings({ sessionsPaneWidth: PANE_DEFAULT })}></div>
    </div>
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
  <Menu items={menu.items} x={menu.x} y={menu.y} {customs} onClose={() => (menu = null)} />
{/if}

{#if quick}
  <QuickSelector title={quick.title} items={quick.items} {customs}
    onChoose={(item) => { const q = quick; quick = null; q?.choose(item); setTimeout(() => tabs?.focusActive(), 50); }}
    onClose={() => { quick = null; tabs?.focusActive(); }} />
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
    position: relative;
  }

  .side {
    flex: none;
    display: flex;
  }

  /* auto hide: the pane lies over the tabs beside its edge tab, only while slid out (no transform, so the tree's
     dialogs, which are fixed in place, stay put) */
  .autohide .side {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 22px;
    z-index: 60;
    box-shadow: 4px 0 18px rgb(0 0 0 / 0.35);
    visibility: hidden;
  }

  .autohide .side.out {
    visibility: visible;
  }

  .edge {
    flex: none;
    width: 22px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 6px;
    background: var(--bg-pane);
    border-right: 1px solid var(--border);
  }

  .edge-tab {
    writing-mode: vertical-rl;
    margin: 0 3px;
    padding: 8px 0;
    border: 0;
    border-left: 3px solid var(--border);
    border-radius: 0;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    cursor: default;
  }

  .edge-tab:hover,
  .edge-tab.active {
    color: var(--text);
    border-left-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .pane-title {
    display: flex;
    align-items: center;
  }

  .pane-title span {
    flex: 1;
  }

  .pin {
    display: flex;
    padding: 2px 3px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--text-muted);
  }

  .pin:hover {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }

  /* DockPanelSuite's pin lies on its side while the pane auto-hides */
  .pin.unpinned svg {
    rotate: 90deg;
  }

  .sessions {
    flex: none;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--bg-pane);
  }

  /* the pane's edge: 8 points to grab, with a line down the middle that lightens under the pointer */
  .splitter {
    flex: none;
    width: 8px;
    cursor: col-resize;
    background: linear-gradient(var(--border), var(--border)) center / 1px 100% no-repeat, var(--bg-pane);
    border-right: 1px solid var(--border);
  }

  .splitter:hover,
  :global(body.resizing-pane) .splitter {
    background: linear-gradient(var(--accent), var(--accent)) center / 2px 100% no-repeat, var(--bg-pane);
  }

  :global(body.resizing-pane),
  :global(body.resizing-pane *) {
    cursor: col-resize !important;
    user-select: none;
  }

  :global(body.resizing-pane iframe) {
    pointer-events: none;
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

  /* the tab strip's ▾ (TabArea's TabListButton) */
  .tabs :global(.tab-list-button) {
    height: 100%;
    min-width: 30px;
    padding: 0 8px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font-size: 17px;
    cursor: pointer;
  }

  .tabs :global(.tab-list-button:hover) {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
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
