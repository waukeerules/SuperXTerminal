// Shortcuts caught in the page (catchShortcuts): on the Mac, ⌘W in a tab's page closes the tab once, though the
// menu bar may get the key too (fromMenu drops its copy), and the main window's own ⌘ keys stay its own; on Windows,
// the main window's keys are caught too; and a page that handles keys itself (the script box's Monaco: ⌘F) keeps
// them. The system is stood in for by the user agent, read when shortcuts.ts loads (so it's loaded again for each);
// the Rust side by mockIPC, giving Close Tab's and Find's keys; a tab's page by an iframe.
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";

/**
 * Load shortcuts.ts as on the Mac or Windows, with Close Tab and Find on their keys, and catch shortcuts into `ran`
 * @param ownKeys The iframe's data-own-keys: the actions its page handles itself
 */
async function catching(mac: boolean, ownKeys?: string): Promise<{ ran: string[]; shortcuts: typeof import("./shortcuts"); frame: HTMLIFrameElement }> {
  vi.resetModules();
  vi.stubGlobal("navigator", { ...navigator, userAgent: mac ? "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)" : "Mozilla/5.0 (Windows NT 10.0)" });
  mockIPC((cmd) => {
    if (cmd === "shortcuts_keys") {
      return [
        { id: "closeTab", label: "Close Tab", default: "", key: mac ? "CmdOrCtrl+W" : "Ctrl+Shift+W" },
        { id: "find", label: "Find…", default: "", key: mac ? "CmdOrCtrl+F" : "Ctrl+Shift+F" },
      ];
    }
    return null;
  });
  const frame = document.createElement("iframe");
  if (ownKeys !== undefined) frame.dataset.ownKeys = ownKeys;
  document.body.append(frame);
  const shortcuts = await import("./shortcuts");
  const ran: string[] = [];
  shortcuts.catchShortcuts((id) => ran.push(id));
  await shortcuts.loadShortcuts();
  return { ran, shortcuts, frame };
}

/** Press a key in a window (a tab's page or the main one); whether it was stopped from going on */
function press(target: Window, init: KeyboardEventInit): boolean {
  const event = new (target as typeof window).KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

afterEach(() => {
  vi.unstubAllGlobals();
  document.querySelectorAll("iframe").forEach((f) => f.remove());
});

describe("on the Mac", () => {
  it("closes the tab once for ⌘W in its page, though the menu bar gets the key too", async () => {
    const { ran, shortcuts, frame } = await catching(true);
    expect(press(frame.contentWindow!, { code: "KeyW", key: "w", metaKey: true })).toBe(true);
    expect(ran).toEqual(["closeTab"]);
    // the menu bar's copy of the same press is dropped; a later one from the menu runs
    expect(shortcuts.fromMenu("closeTab")).toBe(false);
    expect(shortcuts.fromMenu("closeTab")).toBe(true);
    expect(shortcuts.fromMenu("closeTab", Date.now() + 1000)).toBe(true);
  });

  it("leaves the main window's ⌘ keys to the menu bar", async () => {
    const { ran } = await catching(true);
    expect(press(window, { code: "KeyW", key: "w", metaKey: true })).toBe(false);
    expect(ran).toEqual([]);
  });
});

describe("on Windows", () => {
  it("catches the keys in the main window and the tabs' pages, and the menu still runs its own", async () => {
    const { ran, shortcuts, frame } = await catching(false);
    press(window, { code: "KeyW", key: "W", ctrlKey: true, shiftKey: true });
    press(frame.contentWindow!, { code: "KeyW", key: "W", ctrlKey: true, shiftKey: true });
    expect(ran).toEqual(["closeTab", "closeTab"]);
    expect(shortcuts.fromMenu("closeTab")).toBe(true);
  });
});

describe("a page that handles keys itself", () => {
  it("keeps every key when its frame says *: the script box's ⌘F is Monaco's find", async () => {
    const { ran, frame } = await catching(true, "*");
    expect(press(frame.contentWindow!, { code: "KeyF", key: "f", metaKey: true })).toBe(false);
    expect(press(frame.contentWindow!, { code: "KeyW", key: "w", metaKey: true })).toBe(false);
    expect(ran).toEqual([]);
  });

  it("keeps only the actions its frame names: Highlighted Output's Find, while ⌘W still closes the tab", async () => {
    const { ran, frame } = await catching(true, "find");
    expect(press(frame.contentWindow!, { code: "KeyF", key: "f", metaKey: true })).toBe(false);
    expect(press(frame.contentWindow!, { code: "KeyW", key: "w", metaKey: true })).toBe(true);
    expect(ran).toEqual(["closeTab"]);
  });
});
