/**
 * The document tab area: dockview, holding one panel per open tab.
 *
 * It stands in for SuperXPuTTY's DockPanelSuite `DockPanel`. App.svelte creates it; each terminal tab is a
 * `TerminalPanel`, an iframe of `web/terminal` with a `PageHost` behind it.
 */
import {
  createDockview,
  themeDark,
  themeLight,
  type DockviewApi,
  type GroupPanelPartInitParameters,
  type IContentRenderer,
  type ITabRenderer,
  type TabPartInitParameters,
} from "dockview-core";
import { mount, unmount } from "svelte";
import type { CustomIcon } from "../icons/custom";
import { PageHost, type QuickConnect, type Target } from "../terminal/PageHost";
import SessionTab from "./SessionTab.svelte";
import { isDark } from "../settings/store.svelte";

/** What a terminal tab is given when it's added: what to run, whether it's a local shell, and how its tab looks */
interface TerminalParams {
  target: Target;
  local: boolean;
  icon: string;
  /** The environment colour: the tab's colour and the frame around the terminal; null for none */
  environment: string | null;
}

/** A tab's label: SessionTab, with the session's icon and colour */
class TabLabel implements ITabRenderer {
  readonly element = document.createElement("div");
  private label: ReturnType<typeof mount> | null = null;

  constructor(private readonly tabs: TabArea) {
    this.element.className = "session-tab";
  }

  /** Draw the label for its panel */
  init(parameters: TabPartInitParameters): void {
    const { icon, environment } = parameters.params as TerminalParams;
    this.label = mount(SessionTab, {
      target: this.element,
      props: {
        title: parameters.title,
        icon,
        environment,
        iconSize: this.tabs.iconSize,
        customs: this.tabs.customs,
        onClose: () => parameters.api.close(),
      },
    });
  }

  /** Free the label */
  dispose(): void {
    if (this.label) void unmount(this.label);
  }
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
    const { target, local, environment } = parameters.params as TerminalParams;
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
    );
    this.tabs.hosts.set(id, this.host);
    frame.src = `/web/terminal/index.html?tab=${encodeURIComponent(id)}`;
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
  /** Tabs whose "Accept commands" is off */
  private readonly refusing = new Set<string>();
  /** Told when the active tab changes, or tabs open or close */
  private readonly listeners = new Set<() => void>();

  /**
   * Create the tab area inside `element`
   * @param onOpenChanged told the open sessions ("store:id") whenever a session's tab opens or closes
   */
  constructor(element: HTMLElement, private readonly onOpenChanged: (open: Set<string>) => void) {
    this.api = createDockview(element, {
      theme: isDark() ? themeDark : themeLight,
      createComponent: () => new TerminalPanel(this),
      // dockview only uses createTabComponent for a named tab component, so make ours the default
      defaultTabComponent: "session",
      createTabComponent: () => new TabLabel(this),
    });
    this.api.onDidRemovePanel((panel) => {
      this.hosts.delete(panel.id);
      this.refusing.delete(panel.id);
      if (this.sessions.delete(panel.id)) this.onOpenChanged(new Set(this.sessions.values()));
      this.changed();
    });
    this.api.onDidActivePanelChange(() => this.changed());
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

  /** Open a tab for Quick connect's one-off session */
  openQuickConnect(quick: QuickConnect, title: string, local: boolean): void {
    this.add(title, { target: { kind: "quickConnect", ...quick }, local, icon: local ? "terminal" : "computer", environment: null });
  }

  /** Add a terminal tab and show it; returns its panel id */
  private add(title: string, params: TerminalParams): string {
    const id = `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
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

  /** Set the tabs that accept commands (Select sessions to send to) */
  setAccepting(ids: Set<string>): void {
    this.refusing.clear();
    for (const id of this.hosts.keys()) if (!ids.has(id)) this.refusing.add(id);
    this.changed();
  }

  /** Put the keyboard back in the active tab, after a button or Send Commands */
  focusActive(): void {
    const panel = this.api.activePanel;
    const frame = panel?.view.content.element.querySelector("iframe");
    frame?.contentWindow?.focus();
    frame?.contentWindow?.postMessage({ channel: "sxt-host", tab: panel?.id, message: { type: "focus" } }, window.location.origin);
  }

  /** Follow the app's light or dark look (App calls this when it changes) */
  setDark(dark: boolean): void {
    this.api.updateOptions({ theme: dark ? themeDark : themeLight });
  }

  /** The tab menu's commands, from a tab's page: close it, the others, those to its right, or all */
  command(id: string, command: string): void {
    const panel = this.api.getPanel(id);
    if (!panel) return;
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
