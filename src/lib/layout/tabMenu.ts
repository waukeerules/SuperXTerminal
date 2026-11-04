/**
 * A tab's right-click menu, as SuperXPuTTY's ctlPuttyPanel menu for the built-in terminal: New session (every
 * session, by folder), Duplicate, Copy host address, Save session as; Rename tab, Refresh tab, Accept commands;
 * the output items (M4); Send Key and Special Command; Copy all, Restart (or Reconnect), Clear scrollback, Reset;
 * Close, Close others, Close others to the right, Close all. A Remote Desktop tab has its own (rdpMenu).
 */
import type { MenuItem } from "../ui/Menu.svelte";
import type { SessionView } from "../sessions/api";
import type { TabArea } from "./TabArea";
import { specialCommands } from "../terminal/PageHost";
import type { RdpHost } from "../terminal/RdpHost";
import { shortcutLabel } from "../shortcuts";
import { clipboardText } from "../clipboard";

/**
 * A menu item's label with its action's key now, as this system shows it ("Split Right  ⌘D", "Split Right  Ctrl+Shift+D")
 * @param label The item's name
 * @param id The action whose key it is (keys.rs)
 * @returns The label, with the key when the action has one
 */
function withKey(label: string, id: string): string {
  const key = shortcutLabel(id);
  return key ? `${label}  ${key}` : label;
}

/** What the menu needs from the window */
export interface TabMenuHost {
  /** Open Search Output for a tab */
  search: (id: string) => void;
  tabs: TabArea;
  sessions: SessionView[];
  open: (session: SessionView) => void;
  saveAs: (store: SessionView["store"], id: string, title: string) => void;
  rename: (id: string, title: string) => void;
  /** Open a serial tab's Serial Settings */
  serialSettings: (id: string) => void;
  /** Move the tab, with its session running, to a new window */
  moveToNewWindow: (id: string) => void;
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

/** Watch for Silence's choices, as SuperXPuTTY's */
const SILENCE: [number, string][] = [[0, "Off"], [10, "10 seconds"], [30, "30 seconds"], [60, "1 minute"], [300, "5 minutes"]];

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

/** Special Command's items, as PuTTY's: Telnet's commands, a serial Break; ssh can't send any */
function specialItems(protocol: string, send: (command: string) => void): MenuItem[] {
  const specials = specialCommands(protocol.toLowerCase());
  if (specials.length === 0) return [{ label: "Not in the built-in terminal: ssh can't send them", disabled: true }];
  return specials.map(({ label, command }) => ({ label, action: () => send(command) }));
}

/** The menu for the tab `id` */
export function tabMenu(host: TabMenuHost, id: string): MenuItem[] {
  const rdp = host.tabs.rdpHosts.get(id);
  if (rdp) return rdpMenu(host, id, rdp);
  const { tabs } = host;
  const list = tabs.tabList();
  const tab = list.find((t) => t.id === id);
  const params = tabs.paramsOf(id);
  // a Highlighted Output panel's tab
  if (!tab || !params) return [{ label: "Close", action: () => tabs.api.getPanel(id)?.api.close() }];
  const target = params.target;
  const session = target.kind === "session" ? host.sessions.find((s) => s.store === target.store && s.id === target.id) : undefined;
  const address = target.kind === "quickConnect" ? target.host : session?.host ?? "";
  const port = session?.port ?? 0;
  const group = tabs.api.getPanel(id)?.group.panels ?? [];
  const index = group.findIndex((p) => p.id === id);
  const watch = tabs.watchOf(id);
  const protocol = (target.kind === "quickConnect" ? target.protocol : session?.protocol ?? "").toUpperCase();
  const noLog = "The session hasn't started yet";
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
    { label: "Search Output...", disabled: !tab.host.logPath && noLog, action: () => host.search(id) },
    { label: "Highlighted Output", disabled: !tab.host.logPath && noLog, action: () => tabs.openOutput(id) },
    {
      label: "Compare Output With",
      disabled: !tab.host.logPath ? noLog : list.length < 2 && "Open another tab to compare this one's output with",
      items: list.filter((t) => t.id !== id).map((t) => ({ label: t.title, action: () => tabs.openCompare(id, t.id) })),
    },
    {
      label: "Watch for Activity",
      checked: watch.activity,
      disabled: !tab.host.logPath && noLog,
      action: () => tabs.setWatch(id, { activity: !watch.activity }),
    },
    {
      label: "Watch for Silence",
      disabled: !tab.host.logPath && noLog,
      items: SILENCE.map(([seconds, label]) => ({ label, checked: watch.silence === seconds, action: () => tabs.setWatch(id, { silence: seconds }) })),
    },
    { separator: true },
    {
      label: "Send Key",
      items: [
        ...MENU_KEYS.map(([key, label, hint]) => ({ label: hint ? `${label}  (${hint})` : label, action: () => void tab.host.sendKey(key) })),
        { separator: true } as MenuItem,
        ...(protocol === "SERIAL" || protocol === "TELNET"
          ? [{ label: "Break  (stop a Cisco boot at ROMMON)", action: () => void tab.host.special("break") }, { separator: true } as MenuItem]
          : []),
        { label: "Other Key...", action: () => void tab.host.sendKey(null) },
      ],
    },
    { label: "Special Command", items: specialItems(protocol, (command) => void tab.host.special(command)) },
    ...(protocol === "SERIAL"
      ? [{ label: "Serial Settings...", disabled: (tab.host.isEnded || !tab.host.canReceiveCommands) && "The session isn't connected", action: () => host.serialSettings(id) }]
      : []),
    { separator: true },
    { label: "Copy all to clipboard", action: () => tab.host.action("copyAll") },
    ...(tab.host.isReconnecting ? [{ label: "Stop Reconnecting", action: () => tab.host.stopReconnecting() }] : []),
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
    {
      label: withKey("Split Right", "splitRight"),
      disabled: group.length < 2 && "The only tab here: drag one from another group, or open another",
      action: () => tabs.split(id, "right"),
    },
    {
      label: withKey("Split Down", "splitDown"),
      disabled: group.length < 2 && "The only tab here: drag one from another group, or open another",
      action: () => tabs.split(id, "below"),
    },
    ...(tabs.groupCount() > 1 ? [{ label: "Join All Tabs", action: () => tabs.joinAll() } as MenuItem] : []),
    { label: "Move to New Window", action: () => host.moveToNewWindow(id) },
    { separator: true },
    { label: "Close", action: () => tabs.command(id, "close") },
    { label: "Close others", disabled: list.length < 2, action: () => tabs.command(id, "closeOthers") },
    { label: "Close others to the right", disabled: index === group.length - 1, action: () => tabs.command(id, "closeRight") },
    { label: "Close all", disabled: list.length < 2, action: () => tabs.command(id, "closeAll") },
  ];
}

/** Keys Windows takes from a Remote Desktop that a Mac can't type into one, as Microsoft's client's Actions menu */
const RDP_KEYS: [string, string[]][] = [
  ["Ctrl+Alt+Delete", ["ControlLeft", "AltLeft", "Delete"]],
  ["Ctrl+Shift+Esc  (Task Manager)", ["ControlLeft", "ShiftLeft", "Escape"]],
  ["Windows key  (Start)", ["MetaLeft"]],
  ["Windows+R  (Run)", ["MetaLeft", "KeyR"]],
  ["Windows+E  (File Explorer)", ["MetaLeft", "KeyE"]],
  ["Windows+D  (show the desktop)", ["MetaLeft", "KeyD"]],
  ["Windows+L  (lock)", ["MetaLeft", "KeyL"]],
  ["Alt+Tab", ["AltLeft", "Tab"]],
  ["Alt+F4  (close the window)", ["AltLeft", "F4"]],
  ["Print Screen", ["PrintScreen"]],
];

/** A Remote Desktop tab's menu: its keys, typing the Mac's clipboard, a screenshot, reconnecting, and the tab's own */
function rdpMenu(host: TabMenuHost, id: string, rdp: RdpHost): MenuItem[] {
  const { tabs } = host;
  const tab = tabs.commandTabs().find((t) => t.id === id);
  const params = tabs.paramsOf(id);
  const target = params?.target;
  const session = target?.kind === "session" ? host.sessions.find((s) => s.store === target.store && s.id === target.id) : undefined;
  const group = tabs.api.getPanel(id)?.group.panels ?? [];
  const index = group.findIndex((p) => p.id === id);
  const all = tabs.allTabs();
  const offline = !rdp.canReceiveCommands && "Not connected";
  const send = (codes: string[]) => {
    void rdp.sendCombo(codes);
    rdp.focus();
  };
  return [
    {
      label: "Send Keys",
      disabled: offline,
      items: RDP_KEYS.map(([label, codes]) => ({ label, action: () => send(codes) })),
    },
    {
      label: "Type Clipboard Text",
      disabled: offline,
      // for where pasting doesn't reach: the login screen, a console without clipboard sharing
      action: async () => {
        const text = await clipboardText();
        if (!text) {
          host.status("The clipboard has no text to type");
          return;
        }
        await rdp.type(text.replace(/\r\n/g, "\n"));
        rdp.focus();
      },
    },
    {
      label: "Copy Screenshot",
      disabled: offline,
      action: () => void rdp.copyScreenshot().then(() => host.status("Remote desktop copied as a picture")).catch((e) => host.status(`Couldn't copy the screenshot: ${e}`)),
    },
    { separator: true },
    { label: "Duplicate session", action: () => tabs.duplicate(id) },
    {
      label: "Copy host address",
      disabled: !session?.host && "This tab has no host",
      action: () => void navigator.clipboard.writeText(session?.port ? `${session.host} ${session.port}` : session?.host ?? ""),
    },
    { label: "Rename tab", action: () => host.rename(id, tabs.api.getPanel(id)?.title ?? "") },
    { label: "Accept commands", checked: tab?.acceptCommands ?? true, action: () => tabs.setAccept(id, !(tab?.acceptCommands ?? true)) },
    { separator: true },
    ...(rdp.isReconnecting ? [{ label: "Stop Reconnecting", action: () => rdp.stopReconnecting() } as MenuItem] : []),
    { label: "Reconnect", action: () => void rdp.reconnect() },
    { label: "Disconnect", disabled: rdp.isEnded && "Not connected", action: () => rdp.disconnect() },
    { separator: true },
    {
      label: withKey("Split Right", "splitRight"),
      disabled: group.length < 2 && "The only tab here: drag one from another group, or open another",
      action: () => tabs.split(id, "right"),
    },
    {
      label: withKey("Split Down", "splitDown"),
      disabled: group.length < 2 && "The only tab here: drag one from another group, or open another",
      action: () => tabs.split(id, "below"),
    },
    ...(tabs.groupCount() > 1 ? [{ label: "Join All Tabs", action: () => tabs.joinAll() } as MenuItem] : []),
    { separator: true },
    { label: "Close", action: () => tabs.command(id, "close") },
    { label: "Close others", disabled: all.length < 2, action: () => tabs.command(id, "closeOthers") },
    { label: "Close others to the right", disabled: index === group.length - 1, action: () => tabs.command(id, "closeRight") },
    { label: "Close all", disabled: all.length < 2, action: () => tabs.command(id, "closeAll") },
  ];
}
