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
  import type { MovingTab } from "./lib/layout/TabArea";
  import LayoutsList from "./lib/layout/LayoutsList.svelte";
  import LogViewer from "./lib/layout/LogViewer.svelte";
  import { checkForUpdates } from "./lib/settings/updates";
  import SessionDetail from "./lib/sessions/SessionDetail.svelte";
  import { AUTO_RESTORE, LAYOUTS_CHANGED, listLayouts, readLayout, renameLayout, saveLayout, type Layout } from "./lib/layout/layouts";
  import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { ask } from "@tauri-apps/plugin-dialog";

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
  /** The layout open now (the status bar shows it), as SuperXPuTTY's CurrentLayout; null for none */
  let currentLayout = $state<string | null>(null);
  /** The session selected in the tree, for Session Detail */
  let selectedSession = $state<SessionView | null>(null);
  /** A name asked for (Save Layout As…, Rename layout) */
  let naming = $state<{ title: string; value: string; onOk: (name: string) => void } | null>(null);
  /** The window's first tabs are open: the automatic layout may be saved from then on */
  let begun = false;
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let renaming = $state<{ id: string; title: string } | null>(null);
  /** The serial tab whose Serial Settings are open */
  let serialTab = $state<{ title: string; host: PageHost; session: { store: Store; id: string } | null } | null>(null);
  /** Search Output windows, one per tab */
  let searches = $state<{ id: string; title: string; logPath: string | null }[]>([]);

  // the menu bar's items (src-tauri/src/lib.rs) arrive as "menu" events with the item's id
  // this window's own: the core sends the menu bar's commands to the window in front only
  void getCurrentWebviewWindow().listen<string>("menu", (event) => {
    if (event.payload === "settings") settingsTab = "General";
    else if (event.payload === "newLocalShell") newShell();
    else if (event.payload === "newWindow") void invoke("window_new", { open: null });
    else if (event.payload === "environmentVariables") variablesOpen = true;
    else if (event.payload === "environmentColours") coloursOpen = true;
    else if (event.payload === "openSession") quickOpen();
    else if (event.payload === "switchSession") quickSwitch();
    else if (event.payload === "splitRight" || event.payload === "splitDown") {
      const id = tabs?.api.activePanel?.id;
      if (id) tabs?.split(id, event.payload === "splitRight" ? "right" : "below");
    } else if (event.payload === "joinAll") tabs?.joinAll();
    else if (event.payload === "saveLayout") {
      if (currentLayout && currentLayout !== AUTO_RESTORE) void saveLayoutAs(currentLayout);
      else askLayoutName();
    } else if (event.payload === "saveLayoutAs") askLayoutName();
    else if (event.payload === "toggleLayouts") void saveSettings({ showLayouts: !settings.showLayouts });
    else if (event.payload === "toggleLogViewer") void saveSettings({ showLogViewer: !settings.showLogViewer });
    else if (event.payload === "toggleSessionsPane") void saveSettings({ showSessionsPane: !settings.showSessionsPane });
    else if (event.payload === "toggleStatusBar") void saveSettings({ showStatusBar: !settings.showStatusBar });
    else if (event.payload === "toggleAlwaysOnTop") void saveSettings({ alwaysOnTop: !settings.alwaysOnTop });
    else if (event.payload === "checkUpdates") void checkForUpdates(true);
    else if (event.payload === "nextTab") tabs?.cycle(1);
    else if (event.payload === "prevTab") tabs?.cycle(-1);
    else if (["duplicateSession", "renameTab", "restartSession", "resetTerminal", "copyAll"].includes(event.payload)) activeTabAction(event.payload);
    else if (event.payload === "gotoCommandBar") void focusBar("send-command", () => saveSettings({ showSendCommands: true }));
    else if (event.payload === "gotoConnectionBar") void focusBar("quick-connect-host");
    else if (event.payload === "focusActiveSession") tabs?.focusActive();
    else if (event.payload === "fullScreen") void toggleFullScreen();
    else if (event.payload === "toggleSessionDetail") void saveSettings({ showSessionDetail: !settings.showSessionDetail });
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
  // saved in another window (or this one): read them again, so every window shows and saves the same settings
  void listen("settings-changed", () => void loadSettings());

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
            moveToNewWindow: (tabId) => void moveToNewWindow(tabId),
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
    void start();
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
    // RDP in a tab with the built-in client, as SuperXPuTTY's (unless Settings > General says Windows App)
    if (session.protocol.toUpperCase() === "RDP" && settings.rdpBuiltIn) {
      tabs?.openRdp(session.store, session.id, session.name, session.icon, environment);
      status = `Opened session: ${session.name} [RDP]`;
      return;
    }
    // VNC (and RDP when Windows App is chosen) open in their own apps, as SuperXPuTTY's launchers
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

  /**
   * What the window opens first: a Local Shell, or the session Connect in new window asked for (once the tree has
   * read the sessions)
   */
  async function start(): Promise<void> {
    try {
      await begin();
    } finally {
      begun = true;
    }
    // as SuperXPuTTY at start: a newer release is mentioned, nothing else
    if (getCurrentWebviewWindow().label === "main" && settings.autoUpdateCheck) setTimeout(() => void checkForUpdates(false), 4000);
  }

  // View > Always on Top, as SuperXPuTTY's AlwaysOnTop: every window, as the setting changes
  $effect(() => {
    void getCurrentWindow().setAlwaysOnTop(settings.alwaysOnTop).catch(() => {});
  });

  // the status bar's messages go in the app's log too, as SuperXPuTTY's ReportStatus
  $effect(() => {
    const text = status;
    if (text && text !== "Ready") void invoke("applog_write", { level: "INFO", text }).catch(() => {});
  });

  async function begin(): Promise<void> {
    const begin = await invoke<{ open: string | null; adopt: MovingTab | null; layout: string | null } | null>("window_start").catch(() => null);
    // Load in New Window
    if (begin?.layout) {
      await openLayout(begin.layout, false);
      return;
    }
    // Move to New Window: the tab another window gave up, its session carrying on here
    if (begin?.adopt && tabs) {
      tabs.adopt(begin.adopt);
      status = `Moved here: ${begin.adopt.title}`;
      return;
    }
    const open = begin?.open;
    if (open) {
      for (let tries = 0; tries < 100; tries++) {
        const session = tree?.allSessions().find((s) => `${s.store}:${s.id}` === open);
        if (session) {
          tree?.openSession(session);
          return;
        }
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
      status = `The session ${open.slice(open.indexOf(":") + 1)} wasn't found`;
      newShell();
      return;
    }
    // the first window opens what it was started with (the command line, a link), as SuperXPuTTY skips the default
    // layout for a session from its command line; else the default layout; others (New Window) a Local Shell
    if (getCurrentWebviewWindow().label === "main") {
      if (await openRequests()) return;
      for (let tries = 0; tries < 100 && !settings.loaded; tries++) await new Promise((resolve) => setTimeout(resolve, 50));
      if (settings.defaultLayoutName && (await openLayout(settings.defaultLayoutName, false))) return;
    }
    newShell();
  }

  /**
   * Open what was asked from outside (SuperXPuTTY's command line, a second start, a link): a session, a layout, or a
   * host as Quick connect opens it. False when there was nothing.
   */
  async function openRequests(): Promise<boolean> {
    type Request =
      | { kind: "session"; id: string }
      | { kind: "layout"; name: string }
      | { kind: "connect"; protocol: string; host: string; username: string; password: string; profile: string };
    const requests = await invoke<Request[]>("requests_take").catch(() => []);
    for (const request of requests) {
      if (request.kind === "layout") {
        await openLayout(request.name, true);
      } else if (request.kind === "session") {
        // the id as SuperXPuTTY's (Saved, then Private), or a Database session's ("@db/..." or "Database/...")
        for (let tries = 0; tries < 100 && (tree?.allSessions().length ?? 0) === 0; tries++) await new Promise((resolve) => setTimeout(resolve, 100));
        const want = request.id.replace(/^Database\//i, "@db/").toLowerCase();
        const sessions = tree?.allSessions() ?? [];
        const session = ["saved", "private", "database"].map((store) => sessions.find((s) => s.store === store && s.id.toLowerCase() === want)).find(Boolean)
          ?? sessions.find((s) => s.store === "database" && s.id.toLowerCase() === `@db/${want}`);
        if (session) tree?.openSession(session);
        else status = `There's no session ${request.id}`;
      } else {
        const scheme = request.host.includes("://") ? request.host.slice(0, request.host.indexOf("://")).toUpperCase() : request.protocol.toUpperCase();
        const local = opensAsLocalShell(scheme);
        const title = request.host.replace(/^\w+:\/\//, "").replace(/^[^@]*@/, "") || "Local Shell";
        tabs?.openQuickConnect({ protocol: request.protocol, host: request.host, username: request.username, password: request.password, profile: request.profile || "Default Settings" }, title, local);
        status = `Opened session: ${title} [${scheme}]`;
      }
    }
    return requests.length > 0;
  }

  // asked to open something while running (a second start, a link): this window, in front, takes it
  void getCurrentWebviewWindow().listen("open-request", () => void openRequests());

  /** The window as a layout: the tabs and how they're split, and the panes */
  function layoutNow(): Layout | null {
    if (!tabs) return null;
    return {
      version: 1,
      tabs: tabs.layoutState(),
      panes: {
        sessionsPaneWidth: settings.sessionsPaneWidth,
        sessionsAutoHide: settings.sessionsAutoHide,
        showLayouts: settings.showLayouts,
        showSessionDetail: settings.showSessionDetail,
      },
    };
  }

  /**
   * Open a layout, as SuperXPuTTY's LoadLayout: the tabs here close and the layout's open, each session starting
   * again; asks first when sessions other than local shells would close. False when it can't be read.
   */
  async function openLayout(name: string, confirm = true): Promise<boolean> {
    const layout = await readLayout(name);
    if (!layout || !tabs) {
      status = name === AUTO_RESTORE ? "The automatic layout hasn't been saved yet" : `The layout ${name} can't be read`;
      return false;
    }
    const area = tabs;
    const open = area.tabList().filter((t) => !area.paramsOf(t.id)?.local);
    if (confirm && open.length > 0) {
      const sure = await ask(`Open layout "${name}"? The ${open.length} open session${open.length === 1 ? "" : "s"} here will close.`,
        { title: "Load layout", kind: "warning", okLabel: "Load" });
      if (!sure) return false;
    }
    const panes = layout.panes ?? {};
    await saveSettings({
      sessionsPaneWidth: panes.sessionsPaneWidth || settings.sessionsPaneWidth,
      sessionsAutoHide: panes.sessionsAutoHide ?? settings.sessionsAutoHide,
      showLayouts: panes.showLayouts ?? settings.showLayouts,
      showSessionDetail: panes.showSessionDetail ?? settings.showSessionDetail,
    });
    area.loadLayout(layout.tabs);
    currentLayout = name;
    status = `Loaded layout: ${name}`;
    return true;
  }

  /** Save the window as a layout of this name */
  async function saveLayoutAs(name: string): Promise<void> {
    const layout = layoutNow();
    if (!layout) return;
    try {
      await saveLayout(name, layout);
      currentLayout = name;
      status = `Saved layout: ${name}`;
      window.dispatchEvent(new Event(LAYOUTS_CHANGED));
    } catch (e) {
      status = `Couldn't save the layout: ${e}`;
    }
  }

  /** Save Layout As…: asks for a name, and before replacing a layout of that name */
  function askLayoutName(): void {
    naming = {
      title: "Save layout as",
      value: currentLayout && currentLayout !== AUTO_RESTORE ? currentLayout : "",
      onOk: async (name) => {
        naming = null;
        const existing = await listLayouts().catch(() => []);
        if (existing.some((n) => n.toLowerCase() === name.toLowerCase())) {
          if (!(await ask(`Replace the layout "${name}"?`, { title: "Save layout", kind: "warning", okLabel: "Replace" }))) return;
        }
        await saveLayoutAs(name);
      },
    };
  }

  /** Rename a layout from the Layouts pane */
  function askLayoutRename(from: string): void {
    naming = {
      title: "Rename layout",
      value: from,
      onOk: async (to) => {
        naming = null;
        try {
          await renameLayout(from, to);
          if (currentLayout === from) currentLayout = to;
          if (settings.defaultLayoutName === from) await saveSettings({ defaultLayoutName: to });
          window.dispatchEvent(new Event(LAYOUTS_CHANGED));
        } catch (e) {
          status = String(e);
        }
      },
    };
  }

  // the automatic layout, when it's the default: saved a moment after the tabs or their arrangement change, so
  // they're back at the next start (SuperXPuTTY saved it on exit, which Cmd+Q can skip)
  $effect(() => {
    if (!tabs || getCurrentWebviewWindow().label !== "main") return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const changed = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        const layout = layoutNow();
        if (begun && settings.defaultLayoutName === AUTO_RESTORE && layout) void saveLayout(AUTO_RESTORE, layout).catch(() => {});
      }, 1500);
    };
    const stopLayout = tabs.onLayoutChange(changed);
    const stopTabs = tabs.onChange(changed);
    return () => {
      clearTimeout(timer);
      stopLayout();
      stopTabs();
    };
  });

  /**
   * Move to New Window: the tab goes to a new window with its session still running; the new window replays what it
   * has shown and carries on
   */
  async function moveToNewWindow(tabId: string): Promise<void> {
    const moving = tabs?.release(tabId);
    if (!moving) {
      status = "This tab has no running session to move";
      return;
    }
    try {
      await invoke("window_new", { open: null, adopt: moving });
    } catch (e) {
      status = `Couldn't open a window: ${e}`;
    }
  }

  /** The menu's actions on the active tab, as its tab menu's (SuperXPuTTY's DuplicateSession, RenameTab, ...) */
  function activeTabAction(action: string): void {
    const id = tabs?.api.activePanel?.id;
    const host = id ? tabs?.hosts.get(id) : undefined;
    if (!id || !tabs || !host) return;
    if (action === "duplicateSession") tabs.duplicate(id);
    else if (action === "renameTab") renaming = { id, title: tabs.api.getPanel(id)?.title ?? "" };
    else if (action === "restartSession") {
      if (tabs.paramsOf(id)?.local && !host.isEnded) status = "A local shell starts again with New Local Shell";
      else void host.reconnect();
    } else if (action === "resetTerminal") host.action("reset");
    else if (action === "copyAll") host.action("copyAll");
  }

  /** Go to Command Bar and Go to Quick Connect, as SuperXPuTTY's: the keyboard in that toolbar's box (shown first if hidden) */
  async function focusBar(id: string, show?: () => Promise<void>): Promise<void> {
    if (!document.getElementById(id) && show) {
      await show();
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    const box = document.getElementById(id) as HTMLInputElement | null;
    box?.focus();
    box?.select();
  }

  /** View > Full Screen, as SuperXPuTTY's FullScreen */
  async function toggleFullScreen(): Promise<void> {
    const win = getCurrentWindow();
    await win.setFullscreen(!(await win.isFullscreen()));
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

  <div class="main" class:autohide={settings.sessionsAutoHide} class:nopane={!settings.showSessionsPane}>
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
        }} onSelected={(s) => (selectedSession = s)} />
        {#if settings.showLayouts}
          <LayoutsList current={currentLayout} onLoad={(name) => void openLayout(name)} onRename={askLayoutRename}
            onLoadInNewWindow={(name) => void invoke("window_new", { open: null, adopt: null, layout: name })}
            onClose={() => void saveSettings({ showLayouts: false })} />
        {/if}
        {#if settings.showSessionDetail}
          <SessionDetail session={selectedSession} {customs} onEdit={(s) => tree?.editSession(s)}
            onClose={() => void saveSettings({ showSessionDetail: false })} />
        {/if}
      </aside>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="splitter" title="Drag to resize; double-click for the usual width" onpointerdown={startResize}
        ondblclick={() => void saveSettings({ sessionsPaneWidth: PANE_DEFAULT })}></div>
    </div>
    <div class="tabs" bind:this={tabElement}></div>
  </div>

  {#if settings.showLogViewer}
    <LogViewer onClose={() => void saveSettings({ showLogViewer: false })} />
  {/if}
  {#if settings.showButtonBar}
    <ButtonBar {customs} />
  {/if}
  <div class="statusbar" class:hidden={!settings.showStatusBar}>
    <span class="status">{status}</span>
    <ScriptStatus />
    {#if currentLayout}<span class="layout" title="The layout open now">{currentLayout}</span>{/if}
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

{#if naming}
  <Prompt title={naming.title} value={naming.value}
    validate={(name) => (!name.trim() ? "Give the layout a name" : /^[A-Za-z0-9 ._()-]{1,60}$/.test(name.trim()) ? "" : "Use 1 to 60 letters, digits, spaces, dots, dashes, underscores or brackets")}
    onOk={(name) => naming?.onOk(name.trim())} onCancel={() => (naming = null)} />
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

  /* View > Sessions off: the pane (and its auto-hide strip) go; the tree stays loaded for the menus and shortcuts */
  .nopane .side,
  .nopane .edge {
    display: none;
  }

  .statusbar.hidden {
    display: none;
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

  /* a Remote Desktop tab (RdpHost): the remote screen scaled to fit, and what's happening over it */
  .tabs :global(.rdp-panel) {
    position: relative;
    width: 100%;
    height: 100%;
    background: #000;
  }

  .tabs :global(.rdp-screen) {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    outline: none;
  }

  /* SmartSizing off: the desktop at its own size, scrolled */
  .tabs :global(.rdp-panel.actual-size) {
    overflow: auto;
  }

  .tabs :global(.rdp-panel.actual-size .rdp-screen) {
    object-fit: fill;
  }

  .tabs :global(.rdp-overlay) {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    text-align: center;
    white-space: pre-wrap;
    color: #ddd;
    background: rgb(0 0 0 / 0.75);
    cursor: pointer;
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

  .statusbar .layout {
    padding: 0 6px;
    border-left: 1px solid var(--border);
    border-right: 1px solid var(--border);
  }
</style>
