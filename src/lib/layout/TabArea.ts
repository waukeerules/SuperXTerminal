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
import type { MenuItem } from "../ui/Menu.svelte";
import { PageHost, type QuickConnect, type Target } from "../terminal/PageHost";
import { OutputHost } from "../terminal/OutputHost";
import { RdpHost } from "../terminal/RdpHost";
import type { ScriptTarget } from "../scripting/spsl";
import type { Forward } from "../settings/profiles";
import SessionTab from "./SessionTab.svelte";
import FileTransfer from "../transfer/FileTransfer.svelte";
import { isDark } from "../settings/store.svelte";
import { tabStates } from "./tabStates.svelte";

/**
 * How wide a tab's name may be, for the tabs' --tab-name-max (SessionTab.svelte): Settings > General > Tabs' Longest
 * tab name in characters, as CSS's ch; "none" for 0, the whole name however long
 * @param max The setting: characters, or 0
 */
export function tabNameWidth(max: number): string {
  return max > 0 ? `${Math.floor(max)}ch` : "none";
}

/** A group of tabs (a split), as dockview makes them */
type Group = ReturnType<DockviewApi["addGroup"]>;

/**
 * How much wider than high a tile in Grid View should be: an 80 × 24 terminal in a usual font is about 1.6 times as wide
 * as it is high, so a grid that gives each tile room for that shape shows the most of each terminal
 */
const TILE_ASPECT = 1.6;

/**
 * Grid View's columns and rows for `count` tabs in an area `width` × `height` (points): the one whose tiles hold the
 * largest terminal-shaped rectangle ([[TILE_ASPECT]]), so a wide monitor gets more columns than rows (6 tabs on a
 * 2560 × 1300 window: 3 × 2), and a tall one more rows. Fewer columns wins a tie. With no size known (a window not
 * laid out yet), it's as square as it can be.
 * @param count how many tabs, at least 1
 * @returns the columns and rows; the last row may have fewer tabs
 */
export function gridShape(count: number, width: number, height: number): { columns: number; rows: number } {
  const n = Math.max(1, Math.floor(count));
  if (!(width > 0 && height > 0)) {
    const columns = Math.ceil(Math.sqrt(n));
    return { columns, rows: Math.ceil(n / columns) };
  }
  let best = { columns: 1, rows: n, fits: -1 };
  for (let columns = 1; columns <= n; columns += 1) {
    const rows = Math.ceil(n / columns);
    const fits = Math.min(width / columns / TILE_ASPECT, height / rows);
    if (fits > best.fits) best = { columns, rows, fits };
  }
  return { columns: best.columns, rows: best.rows };
}

/** What a terminal tab is given when it's added: what to run, whether it's a local shell, and how its tab looks */
/**
 * An open session's state, as the Sessions tree shows it: connecting (a yellow dot, as its tab shows, until the far end
 * answers), connected (green), or disconnected (the tab's red cross)
 */
export type SessionState = "connecting" | "connected" | "disconnected";
/** Each open session's state, by "store:id" */
export type SessionStates = ReadonlyMap<string, SessionState>;

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
          onTunnels: (x: number, y: number) => this.tabs.onTunnels?.(parameters.api.id, x, y),
        },
      });
    };
    draw(parameters.title);
    parameters.api.onDidTitleChange((e) => draw(e.title));
    // clicking a tab leaves the keyboard on the tab itself (dockview makes it focusable), so typing went nowhere:
    // put it in the tab's terminal, as SuperXPuTTY does. A drag ends without a click, so moving a tab doesn't.
    // Dockview switches tabs in the animation frame after the press, which a quick click can come before; a frame
    // asked for now runs after dockview's, with the clicked tab active.
    this.element.addEventListener("click", () => requestAnimationFrame(() => this.tabs.focusActive()));
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
 * The ▾ at the right of the tab strip, as DockPanelSuite's document list button in SuperXPuTTY: it lists the tabs of
 * its own split (a split side by side has its own ▾), the one in front ticked, and a click switches to one (App draws
 * the list)
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
      if (this.group) this.tabs.toggleTabList(rect.right, rect.bottom, this.group.id);
    });
    this.element.append(close, list);
  }

  /** Take the group the buttons belong to: ✕ closes its active tab */
  init(parameters: IGroupHeaderProps): void {
    this.group = parameters.group;
  }

  /** Nothing to let go of: the buttons go with the header */
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

  /** Open the output page for the tab it follows, with an OutputHost behind it */
  init(parameters: GroupPanelPartInitParameters): void {
    const id = parameters.api.id;
    const { source, logPath, title } = parameters.params as OutputParams;
    const frame = document.createElement("iframe");
    frame.className = "terminal-frame";
    frame.setAttribute("aria-label", `Highlighted output of ${title}`);
    // Monaco's own find (⌘F, Ctrl+F) rather than the terminal's (shortcuts.ts)
    frame.dataset.ownKeys = "find";
    this.host = new OutputHost(id, frame, logPath, title);
    this.tabs.outputs.set(source, this.host);
    frame.src = `/web/output/index.html?tab=${encodeURIComponent(id)}`;
    this.tabs.watchFrame(frame, id);
    this.element.append(frame);
  }

  /** The panel closed: stop following the tab's log */
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

  /** Connect the Remote Desktop session and show it, framed in its environment colour */
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

  /** The tab closed: end the Remote Desktop session */
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

  /** Show the session's files, framed in its environment colour */
  init(parameters: GroupPanelPartInitParameters): void {
    const { target, environment } = parameters.params as TerminalParams;
    if (target.kind !== "session") return;
    if (environment) {
      this.element.style.padding = "3px";
      this.element.style.background = environment;
    }
    this.view = mount(FileTransfer, { target: this.element, props: { store: target.store, id: target.id } });
  }

  /** The tab closed: take the file transfer away, which closes its connection */
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

  /** Open the compare page; it asks for the two outputs once it's ready */
  init(parameters: GroupPanelPartInitParameters): void {
    this.id = parameters.api.id;
    this.params = parameters.params as unknown as CompareParams;
    const frame = document.createElement("iframe");
    frame.className = "terminal-frame";
    frame.setAttribute("aria-label", parameters.api.title ?? "Compare Output");
    // Monaco's own find (⌘F, Ctrl+F) rather than the terminal's (shortcuts.ts)
    frame.dataset.ownKeys = "find";
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

  /** The tab closed: stop listening to its page */
  dispose(): void {
    window.removeEventListener("message", this.onMessage);
  }
}

/** What Send Commands, the button bar and scripts type into: a terminal tab's PageHost or a Remote Desktop's RdpHost */
export interface CommandTarget extends ScriptTarget {
  readonly canReceiveCommands: boolean;
  /** A button's command starts here: its lines' numbers take the button's colour (a terminal's; Remote Desktop has none) */
  markCommand?(color: string): void;
  /** The tunnels the session's ssh opened (a terminal's; Remote Desktop has none) */
  readonly tunnels?: readonly Forward[];
  /** The profile's tunnels changed since the session started, so a restart is needed to use them */
  readonly tunnelsChanged?: boolean;
  /** A notice at the top of the tab, such as a login script that isn't there (a terminal's; Remote Desktop has none) */
  notice?(text: string, kind?: "warn" | "error" | "info"): Promise<void>;
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
  /** Session values its scripts get in place of a saved session's: Quick connect's typed host and login; none else */
  values?: Record<string, string>;
  icon: string;
  /** The profile its session uses, once it has started (its button bar shows while the tab is active); "" for none */
  profile: string;
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
        tabStates[id] = {
          ...tabStates[id],
          ended: this.host?.isEnded ?? false,
          connecting: this.host?.isConnecting ?? false,
          tabColor: this.host?.tabColor ?? null,
          tunnels: this.host?.tunnels.length ?? 0,
          tunnelsChanged: this.host?.tunnelsChanged ?? false,
        };
        this.tabs.notify();
      },
    );
    // the terminal's right-click menu, drawn by the window over every tab rather than cut off at this one's edges
    this.host.onMenu = (x, y, items) => this.tabs.onTerminalMenu?.(x, y, items);
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

/** The tab area: dockview's tabs and groups, and what each tab holds (its page host, its watches, its session) */
export class TabArea {
  readonly api: DockviewApi;
  private count = 0;
  /** Session icons' size, from the settings; tab icons are drawn at three quarters of it */
  iconSize = 24;
  /** Custom icons, for tabs whose session uses one */
  customs: ReadonlyMap<string, CustomIcon> = new Map();
  /** The session ("store:id") each open tab runs, by panel id, for marking open sessions in the tree */
  private readonly sessions = new Map<string, string>();
  /** What the Sessions tree was last told, as text, so it's only told again when something changed */
  private reported = "";
  /** Quick connect tabs' typed fields as session values for their scripts ({host_address}...), by panel id */
  private readonly scriptValues = new Map<string, Record<string, string>>();
  /** Each tab's page host, by panel id */
  readonly hosts = new Map<string, PageHost>();
  /** Highlighted Output panels, by the tab they follow */
  readonly outputs = new Map<string, OutputHost>();
  /** Tabs whose "Accept commands" is off */
  private readonly refusing = new Set<string>();
  /** Told when the active tab changes, or tabs open or close */
  private readonly listeners = new Set<() => void>();
  /** A tab strip's ▾ was clicked at (right, bottom): App shows the tabs of its split (`group`, for allTabs) */
  onTabList: ((right: number, bottom: number, group: string) => void) | null = null;
  /** The ▾ of the split whose list is showing was clicked again: App closes the list */
  onTabListClose: (() => void) | null = null;
  /** The split whose ▾ list is showing, so a second click on its ▾ closes it rather than opening it again */
  private tabListShowing: string | null = null;
  /** A tab was right-clicked: App shows its menu */
  onTabMenu: ((id: string, x: number, y: number) => void) | null = null;
  /** A terminal's right-click menu, its items from the tab's page: App shows it at (x, y) in the window */
  onTerminalMenu: ((x: number, y: number, items: MenuItem[]) => void) | null = null;
  /** A tab's tunnel icon was clicked: App shows its tunnels (TunnelsPanel), at the pointer */
  onTunnels: ((id: string, x: number, y: number) => void) | null = null;
  /** An output alert to show in the status bar (Watch for Activity and Silence) */
  onAlert: ((text: string, background: boolean) => void) | null = null;
  /**
   * View > Grid View is on: every tab has a tile of its own, and the grid is laid out again as tabs open and close
   * (setGridView)
   */
  private gridView = false;
  /**
   * Settings > General > Tabs > Remote Desktop in Grid View is "Opens in a new window": a window of Remote Desktops
   * alone isn't tiled, so each desktop still has the whole window (set by App)
   */
  desktopsApart = false;
  /** The tabs are being moved into a grid now, so the moves aren't taken for tabs opening or closing */
  private tiling = false;
  /** A new grid waiting for the tabs opening or closing together (a layout, Connect in a Grid) to be in */
  private retile: ReturnType<typeof setTimeout> | undefined;
  /**
   * The splits as they were when Grid View went on (dockview's own description of them, and the tab in front), to put
   * back when it goes off with Settings > General > Tabs' "Grid View off" on Put back the splits; null when there
   * are none to put back
   */
  private beforeGrid: { grid: SerializedDockview["grid"]; active: string | null } | null = null;
  /** Watched tabs, as ctlPuttyPanel's watches: activity on or off, silence in seconds (0 off) */
  private readonly watches = new Map<string, { activity: boolean; silence: number; length: number; lastOutput: number; silenceAlerted: boolean }>();

  /**
   * Create the tab area inside `element`
   * @param onOpenChanged told each open session's state (by "store:id") whenever a session's tab opens or closes, or
   * a tab's session drops or comes back (the Sessions tree's green dot, or the tab's red cross)
   */
  constructor(element: HTMLElement, private readonly onOpenChanged: (states: SessionStates) => void) {
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
      // dockview's own "⌄ 3" list of the tabs that don't fit: the ▾ beside it (TabListButton) lists all of the split's
      // tabs already, so it only doubled it. The tabs that don't fit stay in the strip, which scrolls sideways
      disableTabsOverflowList: true,
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
      this.scriptValues.delete(panel.id);
      if (this.sessions.delete(panel.id)) this.reportSessions();
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
    this.api.onDidAddPanel(() => {
      this.changed();
      this.regrid();
    });
    this.api.onDidRemovePanel(() => this.regrid());
  }

  /** Open a tab that runs a profile's key fetch on its own (Settings > Profiles > Log in & cache key) */
  openKeyFetch(profile: string): void {
    this.add(`Key fetch: ${profile}`, { target: { kind: "keyFetch", profile }, local: true, icon: "terminal", environment: null });
  }

  /** Open a new Local Shell tab, numbered after the first */
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
    this.reportSessions();
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
    this.reportSessions();
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
    this.reportSessions();
    this.changed();
    return panelId;
  }

  /** Open a tab for Quick connect's one-off session; returns its panel id */
  openQuickConnect(quick: QuickConnect, title: string, local: boolean): string {
    return this.add(title, { target: { kind: "quickConnect", ...quick }, local, icon: local ? "terminal" : "computer", environment: null });
  }

  /** Give a tab's scripts these session values (Quick connect's typed fields), as a saved session's tab has its own */
  setScriptValues(panelId: string, values: Record<string, string>): void {
    this.scriptValues.set(panelId, values);
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

  /** Tell the listeners the tabs have changed */
  private changed(): void {
    for (const listener of this.listeners) listener();
  }

  /** A tab's session started or ended: its label changes, and the Sessions tree may have to say so */
  notify(): void {
    this.reportSessions();
    this.changed();
  }

  /**
   * Each open session's state, for the Sessions tree, from its tabs': "connected" if any tab is (one is
   * enough: a second tab on a dropped device shows its own cross), else "connecting" if any tab is, else
   * "disconnected" (every tab shows the red cross), as each tab's page host has it: connecting until the far end
   * answers (PageHost.isConnecting), ended once the session has. Only terminal tabs have a page host: a Remote
   * Desktop or SFTP tab's session is connected here, as its tab shows neither.
   */
  private sessionStates(): Map<string, SessionState> {
    const rank: Record<SessionState, number> = { disconnected: 0, connecting: 1, connected: 2 };
    const states = new Map<string, SessionState>();
    for (const [panelId, key] of this.sessions) {
      // a terminal tab's page host; none for a Remote Desktop or SFTP tab
      const host = this.hosts.get(panelId);
      const state: SessionState = host?.isEnded ? "disconnected" : host?.isConnecting ? "connecting" : "connected";
      const before = states.get(key);
      if (!before || rank[state] > rank[before]) states.set(key, state);
    }
    return states;
  }

  /** Tell the Sessions tree each open session's state, when it has changed since it was last told */
  private reportSessions(): void {
    const states = this.sessionStates();
    const text = JSON.stringify([...states].sort());
    if (text === this.reported) return;
    this.reported = text;
    this.onOpenChanged(states);
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
      this.reportSessions();
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

  /**
   * File > Close Tab (Cmd+W), with the keyboard then in the tab left in front. The keyboard was in the closed tab's
   * page, which goes with it, so it was nowhere and a second Cmd+W never reached the app
   */
  closeActive(): void {
    this.api.activePanel?.api.close();
    setTimeout(() => this.focusActive(), 0);
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

  /**
   * Every open tab (Highlighted Output too) in the order they show, group by group, for the tab switcher and Next Tab
   * @param inGroup only this split's tabs (a group's id), for its ▾ list; `active` is then the one in front in it,
   *   which is ticked whether or not the split has the keyboard
   */
  allTabs(inGroup?: string): { id: string; title: string; icon: string; environment: string | null; active: boolean; session: string }[] {
    const groups = inGroup === undefined ? this.api.groups : this.api.groups.filter((g) => g.id === inGroup);
    return groups.flatMap((group) =>
      group.panels.map((panel) => {
        const active = inGroup === undefined ? this.api.activePanel?.id : group.activePanel?.id;
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
   * Highlighted Output and Compare Output panes (they follow tabs' logs, which new sessions won't have), key fetches'
   * tabs, or Quick connect passwords
   */
  layoutState(): SerializedDockview {
    const state = this.api.toJSON();
    // a key fetch's tab too: a layout opening shouldn't start a sign-in
    const dropped = new Set(
      Object.entries(state.panels)
        .filter(([, p]) => p.contentComponent === "output" || p.contentComponent === "compare" || (p.params as unknown as TerminalParams | undefined)?.target?.kind === "keyFetch")
        .map(([id]) => id),
    );
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
   * as SuperXPuTTY's LoadLayout; with no tabs in it, a Local Shell. In Grid View they're tiled once they're all in, and
   * the layout's own splits are what Grid View off puts back
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
    // in Grid View the layout's tabs are about to be tiled (regrid): its splits are the ones to put back when Grid
    // View goes off, not those of the tabs it replaced
    if (this.gridView) this.beforeGrid = { grid: this.api.toJSON().grid, active: this.api.activePanel?.id ?? null };
    this.reportSessions();
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
      this.reportSessions();
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

  /**
   * Tile the tabs in a grid: each tab (Highlighted Output too) in a split of its own, in the order they show, row by
   * row, the columns and rows from [[gridShape]] for the tab area's size, the tiles the same size, and the active tab
   * still in front. Each page stays where it is and only moves, as with a split, so every session carries on.
   */
  tile(): void {
    const active = this.api.activePanel;
    const panels = this.allTabs().map((t) => this.api.getPanel(t.id)).filter((p): p is NonNullable<typeof p> => !!p);
    if (panels.length === 0) return;
    // a window of Remote Desktops alone, with them kept apart from the grid: tabs in one group, each the whole window
    if (this.desktopsApart && this.onlyDesktops()) {
      this.tiling = true;
      try {
        this.joinAll();
      } finally {
        this.tiling = false;
      }
      return;
    }
    const { columns, rows } = gridShape(panels.length, this.api.width, this.api.height);
    this.tiling = true;
    try {
      this.joinAll();
      // row by row: each row's first tile across the whole width under the rows before (a group added at the grid's
      // edge, with no group to go beside), then its tiles to the right of it. Added beside a tile in a column of
      // them, dockview put a "right" tile under it, as a row of its own
      const rowStarts: Group[] = [];
      const tiles: Group[] = [];
      for (let r = 0; r < rows; r += 1) {
        const start = r === 0 ? this.api.groups[0] : this.api.addGroup({ direction: "below" });
        rowStarts.push(start);
        tiles.push(start);
        const inRow = Math.min(columns, panels.length - r * columns);
        for (let c = 1; c < inRow; c += 1) tiles.push(this.api.addGroup({ referenceGroup: tiles[tiles.length - 1], direction: "right" }));
      }
      panels.forEach((panel, i) => {
        if (panel.group !== tiles[i]) panel.api.moveTo({ group: tiles[i] });
      });
      // each new split took half of the one it came from: even them out, row by row and across each row (the last of
      // each takes what's left)
      const width = this.api.width;
      const height = this.api.height;
      if (width > 0 && height > 0) {
        rowStarts.slice(0, -1).forEach((start) => start.api.setSize({ height: height / rows }));
        rowStarts.forEach((_, r) => {
          const row = tiles.slice(r * columns, (r + 1) * columns);
          row.slice(0, -1).forEach((tile) => tile.api.setSize({ width: width / row.length }));
        });
      }
    } finally {
      this.tiling = false;
    }
    (active ?? panels[0]).api.setActive();
  }

  /** Whether this window has tabs and every one is a Remote Desktop (a window Grid View sent desktops to) */
  onlyDesktops(): boolean {
    const panels = this.api.panels;
    return panels.length > 0 && panels.every((p) => p.view.contentComponent === "rdp");
  }

  /** Whether View > Grid View is on in this window */
  inGridView(): boolean {
    return this.gridView;
  }

  /**
   * View > Grid View on or off: on, the splits as they are now are noted and the tabs tiled ([[tile]]), and kept
   * tiled as tabs open and close. Off, the splits from before are put back ([[restoreSplits]]), or with `restore`
   * false the tabs join into one group, as Join All Tabs; the active tab stays in front either way.
   * @param restore Settings > General > Tabs' Grid View off: true to put the splits back, false to join the tabs
   */
  setGridView(on: boolean, restore = false): void {
    if (on === this.gridView) return;
    this.gridView = on;
    clearTimeout(this.retile);
    if (on) {
      this.beforeGrid = { grid: this.api.toJSON().grid, active: this.api.activePanel?.id ?? null };
      this.tile();
      return;
    }
    const before = this.beforeGrid;
    this.beforeGrid = null;
    if (restore && before) this.restoreSplits(before.grid, before.active);
    else this.joinAll();
  }

  /**
   * Join All Tabs while Grid View is on: what was noted when it went on is forgotten, so turning it off joins the
   * tabs rather than putting the old splits back
   */
  forgetSplits(): void {
    this.beforeGrid = null;
  }

  /**
   * Put the splits back as `grid` describes them (noted as Grid View went on): each split where it was, about its
   * size, with its tabs in their order and the one that was in front of it. A tab closed since is left out, and a
   * split left with none goes; a tab opened since joins the split that had `active`, the tab in front then. Only
   * the groups are made again: the tabs move into them, so their sessions carry on (dockview's fromJSON would make
   * every tab again, starting its session over).
   */
  private restoreSplits(grid: SerializedDockview["grid"], active: string | null): void {
    type Node = SerializedGridObject<GroupPanelViewState>;
    const open = new Set(this.api.panels.map((p) => p.id));
    const prune = (node: Node): Node | null => {
      if (node.type === "leaf") {
        const group = node.data as GroupPanelViewState;
        const views = group.views.filter((v) => open.has(v));
        if (views.length === 0) return null;
        return { ...node, data: { ...group, views, activeView: views.includes(group.activeView ?? "") ? group.activeView : views[0] } };
      }
      const children = (node.data as Node[]).map(prune).filter((n): n is Node => !!n);
      return children.length ? { ...node, data: children } : null;
    };
    const root = prune(grid.root as Node);
    if (!root) {
      this.joinAll();
      return;
    }
    const leaves = (node: Node): GroupPanelViewState[] => (node.type === "leaf" ? [node.data as GroupPanelViewState] : (node.data as Node[]).flatMap(leaves));
    const known = new Set(leaves(root).flatMap((l) => l.views));
    const added = this.allTabs().map((t) => t.id).filter((id) => !known.has(id));
    const home = leaves(root).find((l) => active !== null && l.views.includes(active)) ?? leaves(root)[0];
    home.views = [...home.views, ...added];
    const inFront = this.api.activePanel;
    this.tiling = true;
    try {
      this.joinAll();
      // each branch's splits are all made while each is still one group, then each is split in turn the other way:
      // made one at a time, a split beside one already split the other way was put beside that one alone
      const groups = new Map<Node, Group>();
      const build = (node: Node, group: Group, horizontal: boolean): void => {
        groups.set(node, group);
        if (node.type === "leaf") return;
        const children = node.data as Node[];
        const made = [group];
        for (let i = 1; i < children.length; i += 1) made.push(this.api.addGroup({ referenceGroup: made[i - 1], direction: horizontal ? "right" : "below" }));
        children.forEach((child, i) => build(child, made[i], !horizontal));
      };
      build(root, this.api.groups[0], grid.orientation === "HORIZONTAL");
      for (const [node, group] of groups) {
        if (node.type !== "leaf") continue;
        const data = node.data as GroupPanelViewState;
        data.views.forEach((id, index) => this.api.getPanel(id)?.api.moveTo({ group, index, skipSetActive: true }));
        // the tab that was in front in it; the window's own tab in front is put back last
        if (data.activeView) this.api.getPanel(data.activeView)?.api.setActive();
      }
      // about the sizes they had: each split but the last in a branch, by its first group
      const firstGroup = (node: Node): Group => (node.type === "leaf" ? groups.get(node)! : firstGroup((node.data as Node[])[0]));
      const size = (node: Node, horizontal: boolean): void => {
        if (node.type === "leaf") return;
        const children = node.data as Node[];
        children.slice(0, -1).forEach((child) => {
          const at = (child as { size?: number }).size;
          if (at && at > 0) firstGroup(child).api.setSize(horizontal ? { width: at } : { height: at });
        });
        children.forEach((child) => size(child, !horizontal));
      };
      if (this.api.width > 0 && this.api.height > 0) size(root, grid.orientation === "HORIZONTAL");
    } finally {
      this.tiling = false;
    }
    inFront?.api.setActive();
  }

  /**
   * In Grid View, a tab opened or closed: the grid is made again for the tabs there are now, once the tabs opening
   * together are all in (a layout, Connect in a Grid), not once for each
   */
  private regrid(): void {
    if (!this.gridView || this.tiling) return;
    clearTimeout(this.retile);
    this.retile = setTimeout(() => this.tile(), 0);
  }

  /** Next Tab and Previous Tab, as SuperXPuTTY's: the tab after (or before) the active one, in the order shown, round */
  cycle(by: 1 | -1): void {
    const list = this.allTabs();
    if (list.length < 2) return;
    const at = list.findIndex((t) => t.active);
    const next = list[(at + by + list.length) % list.length];
    this.activate(next.id);
  }

  /**
   * Switch to a tab (the open tabs' ▾ list, the tab switcher, Next Tab), with the keyboard in it. After the list or
   * menu that chose it has closed, which would otherwise take the keyboard back to where it was.
   */
  activate(id: string): void {
    this.api.getPanel(id)?.api.setActive();
    setTimeout(() => this.focusActive(), 0);
  }

  /**
   * A split's ▾ was clicked: its tabs' list opens (onTabList), or closes (onTabListClose) when it's this split's list
   * that's showing, as a menu's own button toggles it. Another split's ▾ opens that split's list in its place
   */
  toggleTabList(right: number, bottom: number, group: string): void {
    if (this.tabListShowing === group) {
      this.tabListShowing = null;
      this.onTabListClose?.();
      return;
    }
    this.tabListShowing = group;
    this.onTabList?.(right, bottom, group);
  }

  /** App's ▾ list has gone (a tab chosen, Escape, a click elsewhere, or another menu in its place) */
  tabListClosed(): void {
    this.tabListShowing = null;
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
          values: this.scriptValues.get(panel.id),
          icon: params.icon,
          profile: host.profileName,
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
          values: this.scriptValues.get(panel.id),
          icon: params.icon,
          profile: rdp ? "" : (host as PageHost).profileName,
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
   * stay in its own document, so they're watched there, again each time it loads. A right-click that reaches the
   * page while its tab is hidden is passed on to the tab in front (passRightClick).
   */
  watchFrame(frame: HTMLIFrameElement, id: string): void {
    frame.addEventListener("load", () => {
      const page = frame.contentWindow;
      if (!page) return;
      page.addEventListener("focus", () => this.activateIfNot(id));
      page.document.addEventListener("pointerdown", () => this.activateIfNot(id), true);
      // capture: before xterm.js, which takes the keyboard on a right-click (and so would bring its tab to the front)
      page.document.addEventListener("contextmenu", (event) => this.passRightClick(frame, id, event), true);
    });
  }

  /**
   * A right-click that reached the page of tab `id` while the tab is hidden behind another: stopped there, and given
   * to the page shown at that spot instead, so its menu opens where the user clicked. The Mac's WebView does this:
   * every tab's page stays loaded, stacked in the same place (dockview's renderer "always"), and it hands the
   * right-click (contextmenu) to the newest one, hidden or not, though the press itself (pointerdown) goes to the one
   * shown. Without this, the hidden tab's terminal took the keyboard and came to the front, menu and all.
   * @param frame The hidden tab's iframe
   * @param id The hidden tab
   * @param event The right-click, in the hidden page's coordinates
   */
  private passRightClick(frame: HTMLIFrameElement, id: string, event: MouseEvent): void {
    const panel = this.api.getPanel(id);
    if (!panel || panel.api.isVisible) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    const frameOf = (p: { view: { content: { element: HTMLElement } } } | undefined) =>
      p?.view.content.element.querySelector("iframe") ?? null;
    // the point in this window, then the shown tab's page there: any split's, else this tab's own split's front tab
    const box = frame.getBoundingClientRect();
    const x = box.left + event.clientX;
    const y = box.top + event.clientY;
    const inside = (f: HTMLIFrameElement) => {
      const r = f.getBoundingClientRect();
      return x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    };
    const shown = this.api.panels.filter((p) => p.api.isVisible).map(frameOf).filter((f): f is HTMLIFrameElement => !!f);
    const target = shown.find(inside) ?? frameOf(panel.group.activePanel ?? undefined);
    const doc = target?.contentDocument;
    const view = target?.contentWindow as (Window & typeof globalThis) | null | undefined;
    if (!target || target === frame || !doc || !view) return;
    const r = target.getBoundingClientRect();
    const clientX = x - r.left;
    const clientY = y - r.top;
    const under = doc.elementFromPoint(clientX, clientY) ?? doc.getElementById("terminal") ?? doc.body;
    under?.dispatchEvent(new view.MouseEvent("contextmenu", {
      bubbles: true, cancelable: true, composed: true, button: 2, buttons: 2, clientX, clientY,
      screenX: event.screenX, screenY: event.screenY,
      ctrlKey: event.ctrlKey, shiftKey: event.shiftKey, altKey: event.altKey, metaKey: event.metaKey,
    }));
  }

  /** Put the keyboard in the active tab: after a tab is clicked or switched to, a button, or Send Commands */
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
    // terminals whose theme follows the app's look
    for (const host of this.hosts.values()) host.setDark(dark);
  }

  /** The tab menu's commands, from a tab's page: close it, the others, those to its right, or all (the keyboard then in the tab left in front) */
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
    // the keyboard may have been in a tab just closed: put it in the one left in front, as Close Tab does
    if (closing.length) setTimeout(() => this.focusActive(), 0);
  }

  /** Free the tab area when the window goes */
  dispose(): void {
    this.api.dispose();
  }
}
