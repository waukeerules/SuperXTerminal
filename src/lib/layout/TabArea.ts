/**
 * The document tab area: dockview, holding one panel per open tab.
 *
 * It stands in for SuperXPuTTY's DockPanelSuite `DockPanel`. App.svelte creates it; each terminal tab is a
 * `TerminalPanel`, an iframe of `web/terminal` with a `PageHost` behind it.
 */
import { createDockview, themeDark, themeLight, type DockviewApi, type IContentRenderer, type GroupPanelPartInitParameters } from "dockview-core";
import { PageHost, type Target } from "../terminal/PageHost";

/** What a terminal tab is given when it's added: what to run, and whether it's a local shell */
interface TerminalParams {
  target: Target;
  local: boolean;
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
    const { target, local } = parameters.params as TerminalParams;
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
      () => parameters.api.close(),
      (command) => this.tabs.command(id, command),
    );
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

  /** Create the tab area inside `element` */
  constructor(element: HTMLElement) {
    this.api = createDockview(element, {
      theme: matchMedia("(prefers-color-scheme: dark)").matches ? themeDark : themeLight,
      createComponent: () => new TerminalPanel(this),
    });
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", (event) => {
      this.api.updateOptions({ theme: event.matches ? themeDark : themeLight });
    });
  }

  /** Open a new Local Shell tab */
  openLocalShell(): void {
    this.count += 1;
    this.add(this.count === 1 ? "Local Shell" : `Local Shell (${this.count})`, { target: { kind: "localShell" }, local: true });
  }

  /** Open a tab for a stored session */
  openSession(store: "saved" | "private", id: string, title: string, local: boolean): void {
    this.add(title, { target: { kind: "session", store, id }, local });
  }

  /** Add a terminal tab and show it */
  private add(title: string, params: TerminalParams): void {
    this.api.addPanel({
      id: `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      component: "terminal",
      title,
      params,
      // keep hidden tabs' pages alive: an iframe that's taken out of the page reloads and loses its terminal
      renderer: "always",
    });
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
