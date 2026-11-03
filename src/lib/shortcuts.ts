/**
 * Keyboard shortcuts in the page, and keys as this system shows them.
 *
 * The actions and their keys are src-tauri/src/keys.rs's (Settings > Shortcuts), with the Mac's defaults (⌘ keys) or
 * Windows' (Ctrl+Shift keys, which leave a terminal its Ctrl keys). On the Mac the menu bar catches them wherever the
 * keyboard is. On Windows the menu's keys don't fire while the page has the keyboard, which is nearly always, so
 * `catchShortcuts` catches them in the main window and in every tab's page (same-origin iframes) and runs them as
 * the menu would. Labels in menus and hints come from `keyLabel`, `shortcutLabel` and `commandLabel`.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Whether this is the Mac: ⌘ keys, shown as ⌃⌥⇧⌘ glyphs, and a menu bar that catches its own keys */
export const isMac = /Mac/.test(navigator.userAgent);

/** One action of the Shortcuts editor: keys.rs's Row */
export interface ShortcutRow {
  id: string;
  label: string;
  default: string;
  key: string;
}

/** The actions and their keys now, as keys.rs gives them; refreshed when the settings change */
let rows: ShortcutRow[] = [];
/** True while the Shortcuts editor is recording a key, which mustn't run the action it's being given to */
let paused = false;

/** Read the actions' keys again (at the start, and when Settings > Shortcuts changes them) */
export async function loadShortcuts(): Promise<ShortcutRow[]> {
  rows = await invoke<ShortcutRow[]>("shortcuts_keys").catch(() => rows);
  return rows;
}

/** Stop or start catching shortcuts, for the Shortcuts editor while it records a key */
export function pauseShortcuts(on: boolean): void {
  paused = on;
}

/**
 * A key with its modifiers in one order, so the same key always compares equal; CmdOrCtrl is Ctrl off the Mac
 * @param key An accelerator, e.g. "CmdOrCtrl+Shift+T"
 * @returns The key in one form, e.g. "Shift+CmdOrCtrl+T" on the Mac, "Ctrl+Shift+T" on Windows
 */
export function normalKey(key: string): string {
  if (!key) return "";
  const parts = key.split("+");
  // "Ctrl++" would split into empty parts: the key itself is the last, whatever it is
  const main = parts.pop() || "+";
  const mods = parts.filter(Boolean).map((p) => {
    if (p === "Option") return "Alt";
    if (p === "Control") return "Ctrl";
    if (p === "Cmd" || p === "Command" || p === "CmdOrCtrl" || p === "CommandOrControl") return isMac ? "CmdOrCtrl" : "Ctrl";
    return p;
  });
  const order = ["Ctrl", "Alt", "Shift", "CmdOrCtrl"];
  return [...order.filter((m) => mods.includes(m)), main.toUpperCase()].join("+");
}

/**
 * A key as this system's menus show it: ⌃⌥⇧⌘ and the key on the Mac ("⇧⌘T"), "Ctrl+Shift+T" on Windows
 * @param key An accelerator
 * @returns The label, or "" for none
 */
export function keyLabel(key: string): string {
  if (!key) return "";
  const parts = normalKey(key).split("+");
  const main = parts.pop() ?? "";
  if (!isMac) {
    const names: Record<string, string> = { LEFT: "Left", RIGHT: "Right", UP: "Up", DOWN: "Down", ENTER: "Enter", BACKSPACE: "Backspace", DELETE: "Delete", SPACE: "Space", TAB: "Tab", PAGEUP: "PgUp", PAGEDOWN: "PgDn", HOME: "Home", END: "End", INSERT: "Ins" };
    return [...parts, names[main] ?? main].join("+");
  }
  const glyph: Record<string, string> = { Ctrl: "⌃", Alt: "⌥", Shift: "⇧", CmdOrCtrl: "⌘" };
  const keys: Record<string, string> = { LEFT: "←", RIGHT: "→", UP: "↑", DOWN: "↓", ENTER: "↩", BACKSPACE: "⌫", DELETE: "⌦", SPACE: "Space", TAB: "⇥", PAGEUP: "⇞", PAGEDOWN: "⇟", HOME: "↖", END: "↘" };
  return parts.map((p) => glyph[p] ?? p).join("") + (keys[main] ?? main);
}

/**
 * An action's key now, as this system shows it, for menus that name it ("Split Right  ⌘D")
 * @param id The action's id (keys.rs)
 * @returns The label, or "" when the action has no key
 */
export function shortcutLabel(id: string): string {
  return keyLabel(rows.find((r) => r.id === id)?.key ?? "");
}

/**
 * The command key with another, as this system names it: "⌘C" on the Mac, "Ctrl+C" on Windows
 * @param key The other key, e.g. "C" or "↑"
 * @returns The label
 */
export function commandLabel(key: string): string {
  return isMac ? `⌘${key}` : `Ctrl+${key}`;
}

/**
 * Whether the command key is held: ⌘ on the Mac, Ctrl on Windows (copying a session, selecting every file)
 * @param event The key or mouse event
 * @returns true if it's held
 */
export function commandKey(event: KeyboardEvent | MouseEvent): boolean {
  return isMac ? event.metaKey : event.ctrlKey;
}

/**
 * A key press as an accelerator, in keys.rs's form ("CmdOrCtrl+Shift+T" on the Mac, "Ctrl+Shift+T" on Windows)
 * @param event The key press
 * @returns The accelerator, or null for a press that can't be a shortcut (a modifier alone, or a plain key)
 */
export function pressedKey(event: KeyboardEvent): string | null {
  const code = event.code;
  const named: Record<string, string> = {
    BracketLeft: "[", BracketRight: "]", Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", Backslash: "\\",
    Minus: "-", Equal: "=", Backquote: "`", ArrowLeft: "Left", ArrowRight: "Right", ArrowUp: "Up", ArrowDown: "Down",
    Enter: "Enter", Space: "Space", Tab: "Tab", Backspace: "Backspace", Delete: "Delete", Home: "Home", End: "End",
    PageUp: "PageUp", PageDown: "PageDown", Insert: "Insert",
  };
  let main = named[code];
  if (!main && /^Key[A-Z]$/.test(code)) main = code.slice(3);
  if (!main && /^Digit\d$/.test(code)) main = code.slice(5);
  if (!main && /^F\d{1,2}$/.test(code)) main = code;
  if (!main) return null;
  // Windows' own key (the Windows key) is the system's, never a shortcut here
  if (!isMac && event.metaKey) return null;
  const mods = [event.ctrlKey && "Ctrl", event.altKey && "Alt", event.shiftKey && "Shift", isMac && event.metaKey && "CmdOrCtrl"].filter(Boolean) as string[];
  // a plain key would stop it being typed into a terminal; function keys may go alone
  if (mods.length === 0 && !/^F\d/.test(main)) return null;
  return [...mods, main].join("+");
}

/**
 * On Windows, catch the shortcuts in this window and in its tabs' pages, and run each as its menu item would; on the
 * Mac the menu bar does it. Caught before the page sees the key (a terminal would type it).
 * @param run Runs an action by its id, as the window's "menu" events do
 */
export function catchShortcuts(run: (id: string) => void): void {
  void loadShortcuts();
  void listen("settings-changed", () => void loadShortcuts());
  if (isMac) return;
  const press = (event: KeyboardEvent) => {
    if (paused || event.repeat) return;
    const key = pressedKey(event);
    if (!key) return;
    const wanted = normalKey(key);
    const row = rows.find((r) => r.key && normalKey(r.key) === wanted);
    if (!row) return;
    event.preventDefault();
    event.stopPropagation();
    run(row.id);
  };
  const watched = new WeakSet<Window>();
  const watch = (target: Window | null) => {
    if (!target || watched.has(target)) return;
    try {
      target.addEventListener("keydown", press, true);
      watched.add(target);
    } catch {
      // a page from elsewhere (none today) can't be listened to
    }
  };
  // each tab's page is an iframe, whose window is new each time it loads
  const watchFrame = (frame: HTMLIFrameElement) => {
    watch(frame.contentWindow);
    frame.addEventListener("load", () => watch(frame.contentWindow));
  };
  watch(window);
  document.querySelectorAll("iframe").forEach(watchFrame);
  new MutationObserver((changes) => {
    for (const change of changes) {
      change.addedNodes.forEach((node) => {
        if (node instanceof HTMLIFrameElement) watchFrame(node);
        else if (node instanceof Element) node.querySelectorAll("iframe").forEach(watchFrame);
      });
    }
  }).observe(document.body, { childList: true, subtree: true });
}
