// A Remote Desktop tab's keyboard against SuperXTerminal's shortcuts, on Windows: Ctrl+W (Close Tab here) goes to the
// remote computer while its Windows key combinations option sends keys there, as Remote Desktop Connection does;
// Ctrl+Shift keys stay the app's; Ctrl+Alt+Home gives the keyboard back. The Rust side is stood in for by mockIPC
// (the shortcuts' keys, rdp_open and the keys sent), the system by the user agent, read when shortcuts.ts loads (so
// both modules are loaded again for each test), and jsdom's missing ResizeObserver and canvas by do-nothing ones.
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

/** W's scancode, and Tab's: what the remote computer is sent for them */
const W = 0x11;
const TAB = 0x0f;

/**
 * A connected Remote Desktop tab in the main window, with the shortcuts caught as App.svelte catches them
 * @param keyboard The Windows key combinations option: 0 on this computer, 1 on the remote, 2 only in full screen
 * @returns The actions run, the keys sent to the remote computer (scancode, down) and the tab's screen
 */
async function rdpTab(keyboard = 1): Promise<{ ran: string[]; sent: [number, boolean][]; screen: HTMLCanvasElement }> {
  vi.resetModules();
  vi.stubGlobal("navigator", { ...navigator, userAgent: "Mozilla/5.0 (Windows NT 10.0)" });
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  mockWindows("main");
  const sent: [number, boolean][] = [];
  mockIPC((cmd, args) => {
    if (cmd === "shortcuts_keys") {
      return [
        // Close Tab on Ctrl+W, as a user may set it (Windows' default is Ctrl+Shift+W), and the tab keys' defaults
        { id: "closeTab", label: "Close Tab", default: "", key: "Ctrl+W" },
        { id: "nextTab", label: "Next Tab", default: "", key: "Ctrl+Tab" },
        { id: "prevTab", label: "Previous Tab", default: "", key: "Ctrl+Shift+Tab" },
      ];
    }
    if (cmd === "rdp_open") return { id: 7, width: 800, height: 600, fixed: false, smartSizing: true, scale: 100, keyboard };
    if (cmd === "rdp_key") {
      const { scancode, down } = args as { scancode: number; down: boolean };
      sent.push([scancode, down]);
    }
    return null;
  });
  const shortcuts = await import("../shortcuts");
  const { RdpHost } = await import("./RdpHost");
  const ran: string[] = [];
  shortcuts.catchShortcuts((id) => ran.push(id));
  await shortcuts.loadShortcuts();
  const host = new RdpHost({ store: "private", id: "Lab/Windows Server" }, () => "Windows Server");
  vi.spyOn(host.element, "getBoundingClientRect").mockReturnValue({ width: 800, height: 600 } as DOMRect);
  document.body.append(host.element);
  await host.connect();
  const screen = host.element.querySelector("canvas")!;
  screen.focus();
  return { ran, sent, screen };
}

/** Press a key in an element, as the keyboard would (down, then up) */
function press(target: Element, init: KeyboardEventInit): void {
  target.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }));
  target.dispatchEvent(new KeyboardEvent("keyup", { bubbles: true, cancelable: true, ...init }));
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

describe("a Remote Desktop tab on Windows", () => {
  it("sends Ctrl+W and Ctrl+Tab to the remote computer, though they're Close Tab and Next Tab here", async () => {
    const { ran, sent, screen } = await rdpTab();
    press(screen, { code: "KeyW", key: "w", ctrlKey: true });
    press(screen, { code: "Tab", key: "Tab", ctrlKey: true });
    expect(ran).toEqual([]);
    expect(sent).toContainEqual([W, true]);
    expect(sent).toContainEqual([TAB, true]);
  });

  it("leaves the Ctrl+Shift keys to SuperXTerminal, so the tab can still be left", async () => {
    const { ran, sent, screen } = await rdpTab();
    press(screen, { code: "Tab", key: "Tab", ctrlKey: true, shiftKey: true });
    expect(ran).toEqual(["prevTab"]);
    expect(sent).not.toContainEqual([TAB, true]);
  });

  it("gives the keyboard back on Ctrl+Alt+Home, after which Ctrl+W closes the tab", async () => {
    const { ran, sent, screen } = await rdpTab();
    press(screen, { code: "Home", key: "Home", ctrlKey: true, altKey: true });
    expect(document.activeElement).not.toBe(screen);
    press(document.activeElement ?? document.body, { code: "KeyW", key: "w", ctrlKey: true });
    expect(ran).toEqual(["closeTab"]);
    expect(sent).not.toContainEqual([W, true]);
  });

  it("leaves every shortcut to SuperXTerminal when its keys are kept on this computer", async () => {
    const { ran, sent, screen } = await rdpTab(0);
    press(screen, { code: "KeyW", key: "w", ctrlKey: true });
    expect(ran).toEqual(["closeTab"]);
    expect(sent).not.toContainEqual([W, true]);
  });
});
