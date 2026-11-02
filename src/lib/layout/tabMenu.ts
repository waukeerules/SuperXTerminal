/**
 * A tab's right-click menu, as SuperXPuTTY's ctlPuttyPanel menu for the built-in terminal: New session (every
 * session, by folder), Duplicate, Copy host address, Save session as; Rename tab, Refresh tab, Accept commands;
 * the output items (M4); Send Key and Special Command; Copy all, Restart (or Reconnect), Clear scrollback, Reset;
 * Close, Close others, Close others to the right, Close all.
 */
import type { MenuItem } from "../ui/Menu.svelte";
import type { SessionView } from "../sessions/api";
import type { TabArea } from "./TabArea";

/** What the menu needs from the window */
export interface TabMenuHost {
  tabs: TabArea;
  sessions: SessionView[];
  open: (session: SessionView) => void;
  saveAs: (store: SessionView["store"], id: string, title: string) => void;
  rename: (id: string, title: string) => void;
  status: (text: string) => void;
}

/** SuperXPuTTY's Send Key entries */
const MENU_KEYS = [
  ["^C", "Ctrl+C", "interrupt"],
  ["^Z", "Ctrl+Z", "end, leave config"],
  ["^6", "Ctrl+Shift+6", "Cisco escape"],
  ["^]", "Ctrl+]", "Telnet escape"],
  ["^D", "Ctrl+D", "end of input"],
  ["^\\", "Ctrl+\\", "quit"],
  ["ESC", "Esc", ""],
];

const LATER = "Comes in M4";

/** The sessions as a menu, folders as submenus, as ctlPuttyPanel's New session */
function sessionsMenu(sessions: SessionView[], open: (s: SessionView) => void): MenuItem[] {
  type Node = { folders: Map<string, Node>; sessions: SessionView[] };
  const root: Node = { folders: new Map(), sessions: [] };
  for (const session of sessions) {
    let node = root;
    for (const part of session.folder.split("/").filter(Boolean)) {
      if (!node.folders.has(part)) node.folders.set(part, { folders: new Map(), sessions: [] });
      node = node.folders.get(part)!;
    }
    node.sessions.push(session);
  }
  const byName = (a: string, b: string) => a.localeCompare(b, undefined, { sensitivity: "base", numeric: true });
  const build = (node: Node): MenuItem[] => [
    ...[...node.folders.keys()].sort(byName).map((name) => ({ label: name, items: build(node.folders.get(name)!) })),
    ...node.sessions.sort((a, b) => byName(a.name, b.name)).map((s) => ({ label: s.name, action: () => open(s) })),
  ];
  return build(root);
}

/** The menu for the tab `id` */
export function tabMenu(host: TabMenuHost, id: string): MenuItem[] {
  const { tabs } = host;
  const list = tabs.tabList();
  const tab = list.find((t) => t.id === id);
  const params = tabs.paramsOf(id);
  if (!tab || !params) return [];
  const target = params.target;
  const session = target.kind === "session" ? host.sessions.find((s) => s.store === target.store && s.id === target.id) : undefined;
  const address = target.kind === "quickConnect" ? target.host : session?.host ?? "";
  const port = session?.port ?? 0;
  const group = tabs.api.getPanel(id)?.group.panels ?? [];
  const index = group.findIndex((p) => p.id === id);
  const roots: [string, SessionView["store"]][] = [["Saved sessions", "saved"], ["Private sessions", "private"], ["Database sessions", "database"]];
  return [
    {
      label: "New session",
      items: roots
        .map(([name, store]) => ({ name, sessions: host.sessions.filter((s) => s.store === store) }))
        .filter((r) => r.sessions.length)
        .map((r) => ({ label: r.name, items: sessionsMenu(r.sessions, host.open) })),
    },
    { label: "Duplicate session", action: () => tabs.duplicate(id) },
    {
      label: "Copy host address",
      disabled: !address && "This tab has no host",
      // with the port, as SuperXPuTTY copies it ("host port")
      action: () => void navigator.clipboard.writeText(port ? `${address} ${port}` : address),
    },
    {
      label: "Save session as...",
      disabled: !session && "Only a tab opened from a session can be saved as one",
      action: () => session && host.saveAs(session.store, session.id, tab.title),
    },
    { separator: true },
    { label: "Rename tab", action: () => host.rename(id, tab.title) },
    { label: "Refresh tab", action: () => tab.host.refresh() },
    { label: "Accept commands", checked: tab.acceptCommands, action: () => tabs.setAccept(id, !tab.acceptCommands) },
    { separator: true },
    { label: "Search Output...", disabled: LATER },
    { label: "Highlighted Output", disabled: LATER },
    { label: "Watch for Activity", disabled: LATER },
    { label: "Watch for Silence", disabled: LATER },
    { separator: true },
    {
      label: "Send Key",
      items: [
        ...MENU_KEYS.map(([key, label, hint]) => ({ label: hint ? `${label}  (${hint})` : label, action: () => void tab.host.sendKey(key) })),
        { separator: true } as MenuItem,
        { label: "Other Key...", action: () => void tab.host.sendKey(null) },
      ],
    },
    {
      label: "Special Command",
      items: [{ label: "Not in the built-in terminal: ssh can't send them", disabled: true }],
    },
    { separator: true },
    { label: "Copy all to clipboard", action: () => tab.host.action("copyAll") },
    tab.host.isEnded
      ? { label: "Reconnect", action: () => void tab.host.reconnect() }
      : { label: "Restart session", disabled: params.local && "A local shell starts again with New Local Shell", action: () => void tab.host.reconnect() },
    { label: "Clear scrollback", action: () => tab.host.action("clearScrollback") },
    { label: "Reset terminal", action: () => tab.host.action("reset") },
    {
      label: "Reset terminal and clear scrollback",
      action: () => {
        tab.host.action("reset");
        tab.host.action("clearScrollback");
      },
    },
    { separator: true },
    { label: "Close", action: () => tabs.command(id, "close") },
    { label: "Close others", disabled: list.length < 2, action: () => tabs.command(id, "closeOthers") },
    { label: "Close others to the right", disabled: index === group.length - 1, action: () => tabs.command(id, "closeRight") },
    { label: "Close all", disabled: list.length < 2, action: () => tabs.command(id, "closeAll") },
  ];
}
