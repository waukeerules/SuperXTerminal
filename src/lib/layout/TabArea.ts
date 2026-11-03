/**
 * The document tab area: dockview, holding one panel per open tab.
 *
 * It stands in for SuperXPuTTY's DockPanelSuite `DockPanel`. App.svelte creates it; each terminal tab is a
 * `TerminalPanel`, an iframe of `web/terminal` with a `PageHost` behind it.
 */
import {
  createDockview,
  type IHeaderActionsRenderer,
  type IGroupHeaderProps,
  type IDockviewGroupPanel,
  themeDark,
  themeLight,
  type DockviewApi,
  type SerializedDockview,
  type SerializedGridObject,
  type GroupPanelPartInitParameters,
  type IContentRenderer,
  type ITabRenderer,
  type TabPartInitParameters,
} from "dockview-core";
import { mount, unmount } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import type { CustomIcon } from "../icons/custom";
import { PageHost, type QuickConnect, type Target } from "../terminal/PageHost";
import { OutputHost } from "../terminal/OutputHost";
import { RdpHost } from "../terminal/RdpHost";
import type { ScriptTarget } from "../scripting/spsl";
import SessionTab from "./SessionTab.svelte";
import FileTransfer from "../transfer/FileTransfer.svelte";
import { isDark } from "../settings/store.svelte";
import { tabStates } from "./tabStates.svelte";

/** What a terminal tab is given when it's added: what to run, whether it's a local shell, and how its tab looks */
export interface TerminalParams {
  target: Target;
  local: boolean;
  icon: string;
  /** The environment colour: the tab's colour and the frame around the terminal; null for none */
  environment: string | null;
  /** Move to New Window: the running session (its pty id) this tab takes over from another window */
  adopt?: number;
}

/** A group in a saved layout: its tabs (panel ids) and the one in front (dockview's GroupPanelViewState) */
type GroupPanelViewState = { views: string[]; activeView?: string; id: string };

/** A tab on its way to another window (Move to New Window): its running session, titles, settings and session */
export interface MovingTab {
  ptyId: number;
  title: string;
  original: string;
  params: TerminalParams;
  session: string;
}

/** A tab's label: SessionTab, with the session's icon and colour */
class TabLabel implements ITabRenderer {
  readonly element = document.createElement("div");
  private label: ReturnType<typeof mount> | null = null;

  constructor(private readonly tabs: TabArea) {
    this.element.className = "session-tab";
  }

  /** Draw the label for its panel, again whenever the tab is renamed */
  init(parameters: TabPartInitParameters): void {
    const draw = (title: string) => {
      if (this.label) void unmount(this.label);
      const { icon, environment } = parameters.params as TerminalParams;
      this.label = mount(SessionTab, {
        target: this.element,
        props: {
          title,
          icon,
          environment,
          iconSize: this.tabs.iconSize,
          customs: this.tabs.customs,
          tabId: parameters.api.id,
          onClose: () => parameters.api.close(),
        },
      });
    };
    draw(parameters.title);
    parameters.api.onDidTitleChange((e) => draw(e.title));
    // the tab's own menu, as SuperXPuTTY's (App builds it)
    this.element.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      parameters.api.setActive();
      this.tabs.onTabMenu?.(parameters.api.id, event.clientX, event.clientY);
    });
  }

  /** Free the label */
  dispose(): void {
    if (this.label) void unmount(this.label);
  }
}

/**
 * The ▾ at the right of the tab strip, as DockPanelSuite's document list button in SuperXPuTTY: it lists the open
 * tabs, the active one ticked, and a click switches to one (App draws the list)
 */
class TabListButton implements IHeaderActionsRenderer {
  readonly element = document.createElement("div");
  private group: IDockviewGroupPanel | null = null;

  constructor(private readonly tabs: TabArea) {
    this.element.className = "tab-actions";
    // the close button for "Beside the tab list" (Settings > General > Tabs): it closes this group's active tab;
    // shown only with that setting (App's .close-on-right)
    const close = document.createElement("button");
    close.className = "tab-list-button tab-close-button";
    close.textContent = "✕";
    close.title = "Close the active tab";
    close.setAttribute("aria-label", "Close the active tab");
    close.addEventListener("click", (event) => {
      event.stopPropagation();
      this.group?.activePanel?.api.close();
    });
    const list = document.createElement("button");
    list.className = "tab-list-button";
    list.textContent = "▾";
    list.title = "Open tabs";
    list.setAttribute("aria-label", "Open tabs");
    list.addEventListener("click", (event) => {
      // not a click elsewhere, which closes menus
      event.stopPropagation();
      const rect = list.getBoundingClientRect();
      this.tabs.onTabList?.(rect.right, rect.bottom);
    });
    this.element.append(close, list);
  }

  init(parameters: IGroupHeaderProps): void {
    this.group = parameters.group;
  }

  dispose(): void {}
}

/** What a Highlighted Output panel is given: the tab it follows, its log, and how its tab looks */
interface OutputParams {
  source: string;
  logPath: string | null;
  title: string;
  icon: string;
  environment: string | null;
}

/** A Highlighted Output panel: SuperXPuTTY's web/output page (Monaco) following a tab's log */
class OutputPanel implements IContentRenderer {
  readonly element: HTMLElement;
  private host: OutputHost | null = null;

  constructor(private readonly tabs: TabArea) {
    this.element = document.createElement("div");
    this.element.className = "terminal-panel";
  }

  init(parameters: GroupPanelPartInitParameters): void {
    const id = parameters.api.id;
    const { source, logPath, title } = parameters.params as OutputParams;
    const frame = document.createElement("iframe");
    frame.className = "terminal-frame";
    frame.setAttribute("aria-label", `Highlighted output of ${title}`);
    this.host = new OutputHost(id, frame, logPath, title);
    this.tabs.outputs.set(source, this.host);
    frame.src = `/web/output/index.html?tab=${encodeURIComponent(id)}`;
    this.tabs.watchFrame(frame, id);
    this.element.append(frame);
  }

  dispose(): void {
    this.host?.dispose();
    for (const [source, host] of this.tabs.outputs) if (host === this.host) this.tabs.outputs.delete(source);
    this.host = null;
  }
}

/** A Remote Desktop tab: IronRDP in the core, drawn on a canvas (RdpHost), as SuperXPuTTY's RDP tab */
class RdpPanel implements IContentRenderer {
  readonly element: HTMLElement;
  private host: RdpHost | null = null;

  constructor(private readonly tabs: TabArea) {
    this.element = document.createElement("div");
    this.element.className = "terminal-panel";
  }

  init(parameters: GroupPanelPartInitParameters): void {
    const { target, environment } = parameters.params as TerminalParams;
    if (target.kind !== "session") return;
    // the environment colour frames the remote desktop, as it frames a terminal
    if (environment) {
      this.element.style.padding = "3px";
      this.element.style.background = environment;
    }
    this.host = new RdpHost({ store: target.store, id: target.id }, () => parameters.api.title ?? "");
    this.tabs.rdpHosts.set(parameters.api.id, this.host);
    this.element.append(this.host.element);
    void this.host.connect();
  }

  dispose(): void {
    this.host?.dispose();
    for (const [id, host] of this.tabs.rdpHosts) if (host === this.host) this.tabs.rdpHosts.delete(id);
    this.host = null;
  }
}

/** A file transfer tab: the session's files beside this Mac's (FileTransfer.svelte) */
class SftpPanel implements IContentRenderer {
  readonly element: HTMLElement;
  private view: ReturnType<typeof mount> | null = null;

  constructor() {
    this.element = document.createElement("div");
    this.element.className = "terminal-panel";
  }

  init(parameters: GroupPanelPartInitParameters): void {
    const { target, environment } = parameters.params as TerminalParams;
    if (target.kind !== "session") return;
    if (environment) {
      this.element.style.padding = "3px";
      this.element.style.background = environment;
    }
    this.view = mount(FileTransfer, { target: this.element, props: { store: target.store, id: target.id } });
  }

  dispose(): void {
    if (this.view) void unmount(this.view);
    this.view = null;
  }
}

/** What a Compare Output panel is given: the two tabs it compares, and their titles */
interface CompareParams {
  left: string;
  right: string;
  leftTitle: string;
  rightTitle: string;
  icon: string;
  environment: string | null;
}

/**
 * Compare Output: web/compare (Monaco's diff editor) over two tabs' outputs, their session logs, sent again when
 * the page asks (Refresh, Follow). A tab that has closed keeps the output it last had.
 */
class ComparePanel implements IContentRenderer {
  readonly element: HTMLElement;
  private frame: HTMLIFrameElement | null = null;
  private params: CompareParams | null = null;
  private id = "";
  /** Each side's output as last read, kept when its tab closes */
  private last = { left: "", right: "" };
  private readonly onMessage = (event: MessageEvent) => {
    const data = event.data;
    if (event.source !== this.frame?.contentWindow || event.origin !== window.location.origin) return;
    if (data?.channel === "sxt-compare-page" && data.tab === this.id && data.type === "refresh") void this.send();
  };

  constructor(private readonly tabs: TabArea) {
    this.element = document.createElement("div");
    this.element.className = "terminal-panel";
  }

  init(parameters: GroupPanelPartInitParameters): void {
    this.id = parameters.api.id;
    this.params = parameters.params as unknown as CompareParams;
    const frame = document.createElement("iframe");
    frame.className = "terminal-frame";
    frame.setAttribute("aria-label", parameters.api.title ?? "Compare Output");
    frame.src = `/web/compare/index.html?tab=${encodeURIComponent(this.id)}`;
    this.tabs.watchFrame(frame, this.id);
    this.frame = frame;
    window.addEventListener("message", this.onMessage);
    this.element.append(frame);
  }

  /** Read both outputs and send them to the page */
  private async send(): Promise<void> {
    if (!this.params || !this.frame?.contentWindow) return;
    const read = async (id: string, side: "left" | "right") => {
      const path = this.tabs.hosts.get(id)?.logPath;
      if (path) this.last[side] = await invoke<[string, number]>("log_read", { path, offset: 0 }).then(([text]) => text, () => this.last[side]);
      return this.last[side];
    };
    const [left, right] = await Promise.all([read(this.params.left, "left"), read(this.params.right, "right")]);
    this.frame.contentWindow.postMessage(
      { channel: "sxt-compare", left: { title: this.params.leftTitle, text: left }, right: { title: this.params.rightTitle, text: right } },
      "/",
    );
  }

  dispose(): void {
    window.removeEventListener("message", this.onMessage);
  }
}

/** What Send Commands, the button bar and scripts type into: a terminal tab's PageHost or a Remote Desktop's RdpHost */
export interface CommandTarget extends ScriptTarget {
  readonly canReceiveCommands: boolean;
}

/** A tab that commands can go to: a terminal, or (`rdp`) a Remote Desktop */
export interface CommandTab extends Omit<TabInfo, "host"> {
  host: CommandTarget;
  rdp?: boolean;
}

/** An open terminal tab, for Send Commands, the button bar and scripts */
export interface TabInfo {
  id: string;
  title: string;
  /** The session it runs, "store:id"; "" for a Local Shell or Quick connect */
  session: string;
  icon: string;
  host: PageHost;
  /** The tab's "Accept commands" (on by default), which Select sessions to send to sets */
  acceptCommands: boolean;
  /** Showing in its group (not behind another tab) */
  visible: boolean;
  active: boolean;
}

/** One terminal tab's content: the page in an iframe, and its host */
class TerminalPanel implements IContentRenderer {
  readonly element: HTMLElement;
  private host: PageHost | null = null;

  constructor(private readonly tabs: TabArea) {
    this.element = document.createElement("div");
    this.element.className = "terminal-panel";
  }

  /** Load the page for this tab and connect it to a new shell */
  init(parameters: GroupPanelPartInitParameters): void {
    const id = parameters.api.id;
    const { target, local, environment, adopt } = parameters.params as TerminalParams;
    // SuperXPuTTY's frame: the environment colour shows as a 3 px border around the terminal
    if (environment) {
      this.element.style.padding = "3px";
      this.element.style.background = environment;
    }
    const frame = document.createElement("iframe");
    frame.className = "terminal-frame";
    // a label for screen readers; not "title", which WebKit shows as a tooltip over the whole terminal
    frame.setAttribute("aria-label", parameters.api.title ?? "Terminal");
    // the page asks for its host as soon as it starts, so the host listens before the page loads
    this.host = new PageHost(
      id,
      frame,
      target,
      local,
      environment,
      () => parameters.api.close(),
      (command) => this.tabs.command(id, command),
      () => {
        tabStates[id] = { ...tabStates[id], ended: this.host?.isEnded ?? false };
        this.tabs.notify();
      },
    );
    if (adopt !== undefined) this.host.adopt(adopt);
    this.tabs.hosts.set(id, this.host);
    this.host.title = parameters.api.title ?? "";
    parameters.api.onDidTitleChange((e) => {
      if (this.host) this.host.title = e.title;
    });
    frame.src = `/web/terminal/index.html?tab=${encodeURIComponent(id)}`;
    this.tabs.watchFrame(frame, id);
    this.element.append(frame);
  }

  /** The tab closed: end its program */
  dispose(): void {
    this.host?.dispose();
    this.host = null;
  }
}

export class TabArea {
  readonly api: DockviewApi;
  private count = 0;
  /** Session icons' size, from the settings; tab icons are drawn at three quarters of it */
  iconSize = 24;
  /** Custom icons, for tabs whose session uses one */
  customs: ReadonlyMap<string, CustomIcon> = new Map();
  /** The session ("store:id") each open tab runs, by panel id, for marking open sessions in the tree */
  private readonly sessions = new Map<string, string>();
  /** Each tab's page host, by panel id */
  readonly hosts = new Map<string, PageHost>();
  /** Highlighted Output panels, by the tab they follow */
  readonly outputs = new Map<string, OutputHost>();
  /** Tabs whose "Accept commands" is off */
  private readonly refusing = new Set<string>();
  /** Told when the active tab changes, or tabs open or close */
  private readonly listeners = new Set<() => void>();
  /** The tab strip's ▾ was clicked at (right, bottom): App shows the open tabs */
  onTabList: ((right: number, bottom: number) => void) | null = null;
  /** A tab was right-clicked: App shows its menu */
  onTabMenu: ((id: string, x: number, y: number) => void) | null = null;
  /** An output alert to show in the status bar (Watch for Activity and Silence) */
  onAlert: ((text: string, background: boolean) => void) | null = null;
  /** Watched tabs, as ctlPuttyPanel's watches: activity on or off, silence in seconds (0 off) */
  private readonly watches = new Map<string, { activity: boolean; silence: number; length: number; lastOutput: number; silenceAlerted: boolean }>();

  /**
   * Create the tab area inside `element`
   * @param onOpenChanged told the open sessions ("store:id") whenever a session's tab opens or closes
   */
  constructor(element: HTMLElement, private readonly onOpenChanged: (open: Set<string>) => void) {
    this.api = createDockview(element, {
      theme: isDark() ? themeDark : themeLight,
      createComponent: (options) =>
        options.name === "output" ? new OutputPanel(this)
        : options.name === "compare" ? new ComparePanel(this)
        : options.name === "rdp" ? new RdpPanel(this)
        : options.name === "sftp" ? new SftpPanel()
        : new TerminalPanel(this),
      // dockview only uses createTabComponent for a named tab component, so make ours the default
      defaultTabComponent: "session",
      createTabComponent: () => new TabLabel(this),
      createRightHeaderActionComponent: () => new TabListButton(this),
      // each tab's page stays where it is in the window and is only placed over its group, so moving a tab to
      // another group (a split) doesn't reload the page, which would start its session again
      defaultRenderer: "always",
    });
    this.api.onDidRemovePanel((panel) => {
      // its Highlighted Output stays, marked closed
      this.outputs.get(panel.id)?.sourceClosed();
      this.outputs.delete(panel.id);
      this.hosts.delete(panel.id);
      this.refusing.delete(panel.id);
      if (this.sessions.delete(panel.id)) this.onOpenChanged(new Set(this.sessions.values()));
      this.changed();
    });
    this.api.onDidActivePanelChange(() => this.changed());
    // after a tab is dragged to another group or split off, the keyboard was nowhere: put it in the active tab (the
    // one moved, or after Join the one in front before), once the drop and its moves are done
    let refocus: ReturnType<typeof setTimeout> | undefined;
    this.api.onDidMovePanel(() => {
      clearTimeout(refocus);
      refocus = setTimeout(() => this.focusActive(), 0);
    });
    // the tab clicked or typed in is the active one, as clicking its tab makes it: clicks in a tab's content never
    // reach dockview (renderer "always" puts it above the groups), so with splits the buttons and Send Commands went
    // to the split last activated by its tab strip. Content in this page (Remote Desktop, file transfer) is followed
    // here; pages in iframes (terminals, Highlighted Output, Compare) by watchFrame, as focus moving from one iframe
    // to another never passes through this page.
    document.addEventListener("focusin", (event) => this.activateHolder(event.target));
    document.addEventListener("pointerdown", (event) => this.activateHolder(event.target), true);
    // as SuperXPuTTY, the watched tabs are looked at about once a second
    setInterval(() => this.checkAlerts(), 1000);
    this.api.onDidAddPanel(() => this.changed());
  }

  /** Open a new Local Shell tab */
  openLocalShell(): void {
    this.count += 1;
    this.add(this.count === 1 ? "Local Shell" : `Local Shell (${this.count})`, {
      target: { kind: "localShell" },
      local: true,
      icon: "terminal",
      environment: null,
    });
  }

  /** Open a tab for a stored session, with its icon and environment colour */
  openSession(store: "saved" | "private" | "database", id: string, title: string, local: boolean, icon: string, environment: string | null): string {
    const panelId = this.add(title, { target: { kind: "session", store, id }, local, icon, environment });
    this.sessions.set(panelId, `${store}:${id}`);
    this.onOpenChanged(new Set(this.sessions.values()));
    this.changed();
    return panelId;
  }

  /** Remote Desktop tabs' hosts, by panel id */
  readonly rdpHosts = new Map<string, RdpHost>();

  /** Open a Remote Desktop session in a tab (the built-in client), as SuperXPuTTY shows RDP in a tab */
  openRdp(store: "saved" | "private" | "database", id: string, title: string, icon: string, environment: string | null): string {
    const panelId = `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    this.titles.set(panelId, title);
    this.api.addPanel({
      id: panelId,
      component: "rdp",
      title,
      params: { target: { kind: "session", store, id }, local: false, icon, environment } satisfies TerminalParams,
      renderer: "always",
    });
    this.sessions.set(panelId, `${store}:${id}`);
    this.onOpenChanged(new Set(this.sessions.values()));
    this.changed();
    return panelId;
  }
  /** A session's files in a tab of their own (File transfer (SFTP) in the session's menu) */
  openSftp(store: "saved" | "private" | "database", id: string, title: string, icon: string, environment: string | null): string {
    const panelId = `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    title = `${title} (SFTP)`;
    this.titles.set(panelId, title);
    this.api.addPanel({
      id: panelId,
      component: "sftp",
      title,
      params: { target: { kind: "session", store, id }, local: false, icon, environment } satisfies TerminalParams,
      renderer: "always",
    });
    this.sessions.set(panelId, `${store}:${id}`);
    this.onOpenChanged(new Set(this.sessions.values()));
    this.changed();
    return panelId;
  }

  /** Open a tab for Quick connect's one-off session */
  openQuickConnect(quick: QuickConnect, title: string, local: boolean): void {
    this.add(title, { target: { kind: "quickConnect", ...quick }, local, icon: local ? "terminal" : "computer", environment: null });
  }

  /** Each tab's title as opened, by panel id */
  private readonly titles = new Map<string, string>();

  /** Add a terminal tab and show it; returns its panel id */
  private add(title: string, params: TerminalParams): string {
    const id = `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    this.titles.set(id, title);
    this.api.addPanel({
      id,
      component: "terminal",
      title,
      params,
      // keep hidden tabs' pages alive: an iframe that's taken out of the page reloads and loses its terminal
      renderer: "always",
    });
    return id;
  }

  /** Be told when the active tab changes or tabs open or close; returns how to stop */
  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(): void {
    for (const listener of this.listeners) listener();
  }

  /** A tab's session started or ended */
  notify(): void {
    this.changed();
  }

  /** What a tab runs, for its menu: its target and how its tab looks */
  paramsOf(id: string): TerminalParams | null {
    return (this.api.getPanel(id)?.params as unknown as TerminalParams) ?? null;
  }

  /** Rename a tab (its menu's Rename tab) */
  rename(id: string, title: string): void {
    this.api.getPanel(id)?.api.setTitle(title);
  }

  /** Open the same thing again in a new tab (Duplicate session) */
  duplicate(id: string): void {
    const panel = this.api.getPanel(id);
    const params = this.paramsOf(id);
    if (!panel || !params) return;
    if (params.target.kind === "localShell") {
      this.openLocalShell();
      return;
    }
    if (panel.view.contentComponent === "sftp" && params.target.kind === "session") {
      this.openSftp(params.target.store, params.target.id, this.originalTitle(id).replace(/ \(SFTP\)$/, ""), params.icon, params.environment ?? null);
      return;
    }
    if (this.rdpHosts.has(id) && params.target.kind === "session") {
      this.openRdp(params.target.store, params.target.id, this.originalTitle(id), params.icon, params.environment ?? null);
      return;
    }
    const panelId = this.add(this.originalTitle(id), params);
    const session = this.sessions.get(id);
    if (session) {
      this.sessions.set(panelId, session);
      this.onOpenChanged(new Set(this.sessions.values()));
    }
  }

  /** The title a tab opened with, before any rename */
  private originalTitle(id: string): string {
    return this.titles.get(id) ?? this.api.getPanel(id)?.title ?? "";
  }

  /** Highlighted Output beside a tab, as HighlightedOutputPanel.ShowFor: the one it has if it's open */
  openOutput(id: string): void {
    const panelId = `output-${id}`;
    const existing = this.api.getPanel(panelId);
    if (existing) {
      existing.api.setActive();
      return;
    }
    const tab = this.api.getPanel(id);
    const host = this.hosts.get(id);
    if (!tab || !host) return;
    const title = tab.title ?? "";
    this.api.addPanel({
      id: panelId,
      component: "output",
      title: `Output: ${title}`,
      params: { source: id, logPath: host.logPath, title, icon: "document", environment: null } satisfies OutputParams,
      position: { referencePanel: id, direction: "right" },
      renderer: "always",
    });
  }

  /** Compare Output: the two tabs' outputs side by side, in a group beside the first */
  openCompare(left: string, right: string): void {
    const panelId = `compare-${left}-${right}`;
    const existing = this.api.getPanel(panelId);
    if (existing) {
      existing.api.setActive();
      return;
    }
    const [a, b] = [this.api.getPanel(left), this.api.getPanel(right)];
    if (!a || !b) return;
    const params: CompareParams = { left, right, leftTitle: a.title ?? "", rightTitle: b.title ?? "", icon: "document", environment: null };
    this.api.addPanel({
      id: panelId,
      component: "compare",
      title: `Compare: ${params.leftTitle} ↔ ${params.rightTitle}`,
      params: params as unknown as Record<string, unknown>,
      position: { referencePanel: left, direction: "right" },
      renderer: "always",
    });
  }

  /** The active tab's page host, for the Edit menu's Find and Clear Scrollback */
  activeHost(): PageHost | null {
    const id = this.api.activePanel?.id;
    return id ? this.hosts.get(id) ?? null : null;
  }

  /** File > Close Tab (Cmd+W) */
  closeActive(): void {
    this.api.activePanel?.api.close();
  }

  /** A tab's watches */
  watchOf(id: string): { activity: boolean; silence: number } {
    const w = this.watches.get(id);
    return { activity: w?.activity ?? false, silence: w?.silence ?? 0 };
  }

  /** Turn Watch for Activity on or off, or set Watch for Silence's seconds (0 off); either starts from now */
  setWatch(id: string, change: { activity?: boolean; silence?: number }): void {
    const host = this.hosts.get(id);
    if (!host) return;
    const old = this.watchOf(id);
    const activity = change.activity ?? old.activity;
    const silence = change.silence ?? old.silence;
    if (!activity && !silence) this.watches.delete(id);
    else this.watches.set(id, { activity, silence, length: host.outputLength, lastOutput: Date.now(), silenceAlerted: false });
    this.setAlert(id, null);
  }

  /** Show or clear a tab's alert badge */
  private setAlert(id: string, alert: "activity" | "silence" | null): void {
    if ((tabStates[id]?.alert ?? null) === alert) return;
    tabStates[id] = { ...tabStates[id], alert };
  }

  /**
   * Look at the watched tabs, as ctlPuttyPanel.CheckOutputAlerts: new output in a tab not being looked at is
   * Activity; a quiet spell as long as the watch is Silence, once per spell. Looking at the tab clears its alert.
   */
  private checkAlerts(): void {
    const active = this.api.activePanel?.id;
    const front = document.hasFocus();
    for (const [id, watch] of this.watches) {
      const host = this.hosts.get(id);
      if (!host) {
        this.watches.delete(id);
        continue;
      }
      const title = this.api.getPanel(id)?.title ?? "";
      const fresh = host.outputLength !== watch.length;
      if (fresh) {
        watch.length = host.outputLength;
        watch.lastOutput = Date.now();
        watch.silenceAlerted = false;
      }
      if (front && id === active) {
        this.setAlert(id, null);
      } else if (fresh && watch.activity) {
        if ((tabStates[id]?.alert ?? null) !== "activity") this.onAlert?.(`New output in ${title}`, !front);
        this.setAlert(id, "activity");
      } else if (watch.silence > 0 && !watch.silenceAlerted && watch.length > 0 && Date.now() - watch.lastOutput >= watch.silence * 1000) {
        watch.silenceAlerted = true;
        this.setAlert(id, "silence");
        this.onAlert?.(`${title} has been quiet for ${watch.silence} s`, !front);
      }
    }
  }

  /** Turn a tab's Accept commands on or off */
  setAccept(id: string, on: boolean): void {
    if (on) this.refusing.delete(id);
    else this.refusing.add(id);
    this.changed();
  }

  /** Every open tab (Highlighted Output too) in the order they show, group by group, for the ▾ list */
  allTabs(): { id: string; title: string; icon: string; environment: string | null; active: boolean; session: string }[] {
    const active = this.api.activePanel?.id;
    return this.api.groups.flatMap((group) =>
      group.panels.map((panel) => {
        const params = panel.params as unknown as { icon?: string; environment?: string | null };
        return {
          id: panel.id,
          title: panel.title ?? "",
          icon: params.icon ?? "terminal",
          environment: params.environment ?? null,
          active: panel.id === active,
          session: this.sessions.get(panel.id) ?? "",
        };
      }),
    );
  }

  /**
   * The tabs as a layout: dockview's own JSON (the groups, their sizes, each tab's title and what it runs), without
   * Highlighted Output and Compare Output panes (they follow tabs' logs, which new sessions won't have) or Quick connect
   * passwords
   */
  layoutState(): SerializedDockview {
    const state = this.api.toJSON();
    const dropped = new Set(Object.entries(state.panels).filter(([, p]) => p.contentComponent === "output" || p.contentComponent === "compare").map(([id]) => id));
    for (const id of dropped) delete state.panels[id];
    for (const panel of Object.values(state.panels)) {
      const params = { ...(panel.params as unknown as TerminalParams) };
      delete params.adopt;
      if (params.target?.kind === "quickConnect") params.target = { ...params.target, password: "" };
      panel.params = params as unknown as Record<string, unknown>;
    }
    // groups left with no tabs go, and branches left with nothing in them
    const prune = (node: SerializedGridObject<GroupPanelViewState>): SerializedGridObject<GroupPanelViewState> | null => {
      if (node.type === "leaf") {
        const group = node.data as GroupPanelViewState;
        const views = group.views.filter((v) => !dropped.has(v));
        if (views.length === 0) return null;
        return { ...node, data: { ...group, views, activeView: views.includes(group.activeView ?? "") ? group.activeView : views[0] } };
      }
      const children = (node.data as SerializedGridObject<GroupPanelViewState>[]).map(prune).filter((n): n is SerializedGridObject<GroupPanelViewState> => !!n);
      return children.length ? { ...node, data: children } : null;
    };
    const root = prune(state.grid.root);
    state.grid.root = root ?? { type: "branch", data: [] };
    return state;
  }

  /**
   * Open a layout: the tabs here close (their sessions end) and the layout's open, each starting its session again,
   * as SuperXPuTTY's LoadLayout; with no tabs in it, a Local Shell
   */
  loadLayout(state: SerializedDockview | null): void {
    for (const panel of [...this.api.panels]) panel.api.close();
    this.sessions.clear();
    this.titles.clear();
    this.count = 0;
    const tabs = state ? Object.keys(state.panels).length : 0;
    if (state && tabs > 0) {
      this.api.fromJSON(state);
      for (const panel of this.api.panels) {
        const params = panel.params as unknown as TerminalParams;
        this.titles.set(panel.id, panel.title ?? "");
        if (params.target?.kind === "session") this.sessions.set(panel.id, `${params.target.store}:${params.target.id}`);
        if (params.target?.kind === "localShell") this.count += 1;
      }
    } else {
      this.openLocalShell();
    }
    this.onOpenChanged(new Set(this.sessions.values()));
    this.changed();
  }

  /** Be told when the tabs or their arrangement change (for the automatic layout) */
  onLayoutChange(listener: () => void): () => void {
    const subscription = this.api.onDidLayoutChange(listener);
    return () => subscription.dispose();
  }

  /**
   * Move to New Window: the tab leaves this window with its session still running, for another window to `adopt`;
   * null when it has no session to give (it never started)
   */
  release(id: string): MovingTab | null {
    const panel = this.api.getPanel(id);
    const params = this.paramsOf(id);
    const ptyId = this.hosts.get(id)?.release() ?? null;
    if (!panel || !params || ptyId === null) return null;
    const moving = { ptyId, title: panel.title ?? "", original: this.originalTitle(id), params: { ...params, adopt: undefined }, session: this.sessions.get(id) ?? "" };
    panel.api.close();
    return moving;
  }

  /** Take over a tab another window gave up: the same session, carrying on, with its title and look */
  adopt(moving: MovingTab): void {
    const panelId = this.add(moving.original, { ...moving.params, adopt: moving.ptyId });
    if (moving.title !== moving.original) this.rename(panelId, moving.title);
    if (moving.session) {
      this.sessions.set(panelId, moving.session);
      this.onOpenChanged(new Set(this.sessions.values()));
    }
    this.changed();
  }

  /**
   * Split: the tab moves to a new group beside its own (right) or under it (below), as dragging a tab to a group's
   * edge does; with a tab of its own the group has nothing left to split off, so it stays
   */
  split(id: string, direction: "right" | "below"): void {
    const panel = this.api.getPanel(id);
    if (!panel || panel.group.panels.length < 2) return;
    const group = this.api.addGroup({ referenceGroup: panel.group, direction });
    panel.api.moveTo({ group });
    panel.api.setActive();
  }

  /** How many groups the tabs are split into */
  groupCount(): number {
    return this.api.groups.length;
  }

  /** Join all the groups back into one, the active tab staying in front */
  joinAll(): void {
    const active = this.api.activePanel;
    const [first, ...rest] = this.api.groups;
    if (!first) return;
    for (const group of rest) for (const panel of [...group.panels]) panel.api.moveTo({ group: first });
    active?.api.setActive();
  }

  /** Next Tab and Previous Tab, as SuperXPuTTY's: the tab after (or before) the active one, in the order shown, round */
  cycle(by: 1 | -1): void {
    const list = this.allTabs();
    if (list.length < 2) return;
    const at = list.findIndex((t) => t.active);
    const next = list[(at + by + list.length) % list.length];
    this.activate(next.id);
    this.focusActive();
  }

  /** Switch to a tab */
  activate(id: string): void {
    this.api.getPanel(id)?.api.setActive();
  }

  /** The open terminal tabs in the order they show, group by group */
  tabList(): TabInfo[] {
    const active = this.api.activePanel?.id;
    return this.api.groups.flatMap((group) =>
      group.panels.flatMap((panel) => {
        const host = this.hosts.get(panel.id);
        if (!host) return [];
        const params = panel.params as unknown as TerminalParams;
        return [{
          id: panel.id,
          title: panel.title ?? "",
          session: this.sessions.get(panel.id) ?? "",
          icon: params.icon,
          host,
          acceptCommands: !this.refusing.has(panel.id),
          visible: group.activePanel === panel,
          active: panel.id === active,
        }];
      }),
    );
  }

  /** The tabs commands can go to, terminals and Remote Desktops, in the order they show */
  commandTabs(): CommandTab[] {
    const active = this.api.activePanel?.id;
    return this.api.groups.flatMap((group) =>
      group.panels.flatMap((panel): CommandTab[] => {
        const rdp = this.rdpHosts.get(panel.id);
        const host = rdp ?? this.hosts.get(panel.id);
        if (!host) return [];
        const params = panel.params as unknown as TerminalParams;
        return [{
          id: panel.id,
          title: panel.title ?? "",
          session: this.sessions.get(panel.id) ?? "",
          icon: params.icon,
          host,
          rdp: !!rdp,
          acceptCommands: !this.refusing.has(panel.id),
          visible: group.activePanel === panel,
          active: panel.id === active,
        }];
      }),
    );
  }

  /** Set the tabs that accept commands (Select sessions to send to) */
  setAccepting(ids: Set<string>): void {
    this.refusing.clear();
    for (const id of [...this.hosts.keys(), ...this.rdpHosts.keys()]) if (!ids.has(id)) this.refusing.add(id);
    this.changed();
  }

  /** Make the tab whose content holds `target` (the focused or clicked element) the active one, if it isn't */
  private activateHolder(target: EventTarget | null): void {
    if (!(target instanceof Node)) return;
    const panel = this.api.panels.find((p) => p.view.content.element.contains(target));
    if (panel) this.activateIfNot(panel.id);
  }

  /** Make a tab the active one, if it isn't already (each click and focus calls it) */
  private activateIfNot(id: string): void {
    const panel = this.api.getPanel(id);
    if (panel && this.api.activePanel !== panel) panel.api.setActive();
  }

  /**
   * Make tab `id` the active one when its page (same origin, in `frame`) is clicked or gets the keyboard. Its events
   * stay in its own document, so they're watched there, again each time it loads.
   */
  watchFrame(frame: HTMLIFrameElement, id: string): void {
    frame.addEventListener("load", () => {
      const page = frame.contentWindow;
      if (!page) return;
      page.addEventListener("focus", () => this.activateIfNot(id));
      page.document.addEventListener("pointerdown", () => this.activateIfNot(id), true);
    });
  }

  /** Put the keyboard back in the active tab, after a button or Send Commands */
  focusActive(): void {
    const panel = this.api.activePanel;
    const rdp = panel ? this.rdpHosts.get(panel.id) : undefined;
    if (rdp) {
      rdp.focus();
      return;
    }
    const frame = panel?.view.content.element.querySelector("iframe");
    frame?.contentWindow?.focus();
    frame?.contentWindow?.postMessage({ channel: "sxt-host", tab: panel?.id, message: { type: "focus" } }, "/");
  }

  /** Follow the app's light or dark look (App calls this when it changes) */
  setDark(dark: boolean): void {
    this.api.updateOptions({ theme: dark ? themeDark : themeLight });
  }

  /** The tab menu's commands, from a tab's page: close it, the others, those to its right, or all */
  command(id: string, command: string): void {
    const panel = this.api.getPanel(id);
    if (!panel) return;
    if (command === "document") {
      this.openOutput(id);
      return;
    }
    const siblings = panel.group.panels;
    const index = siblings.indexOf(panel);
    const closing =
      command === "close" ? [panel]
      : command === "closeOthers" ? siblings.filter((p) => p !== panel)
      : command === "closeRight" ? siblings.slice(index + 1)
      : command === "closeAll" ? [...siblings]
      : [];
    closing.forEach((p) => p.api.close());
  }

  /** Free the tab area when the window goes */
  dispose(): void {
    this.api.dispose();
  }
}
