/**
 * A Remote Desktop tab's host: the canvas the remote screen is drawn on, and the keyboard and mouse sent back, as
 * SuperXPuTTY's RDP tab (Microsoft's control there; IronRDP in the core here, src-tauri/src/rdp.rs).
 *
 * The desktop is the tab's size (Fit the tab, as SuperXPuTTY's default) in points, and follows the tab when it changes
 * size. ⌘ shortcuts stay SuperXTerminal's (⌘W closes the tab, ⌘T opens a shell);
 * Control and Option are Windows' Ctrl and Alt.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { askLogin, askOne } from "../scripting/asking.svelte";
import { settings } from "../settings/store.svelte";
import { isMac } from "../shortcuts";

/** What the core says about the session: connecting, where the pointer is and how it looks, or that it ended */
type Event =
  | { type: "connecting"; host: string }
  | { type: "pointer"; kind: "default" | "hidden" | "position"; x: number; y: number }
  | { type: "ended"; reason: string; clean: boolean };

/** KeyboardEvent.code to the PC (set 1) scancode, and whether it's an extended (E0) key */
const SCANCODES: Record<string, [number, boolean]> = {
  Escape: [0x01, false], Digit1: [0x02, false], Digit2: [0x03, false], Digit3: [0x04, false], Digit4: [0x05, false],
  Digit5: [0x06, false], Digit6: [0x07, false], Digit7: [0x08, false], Digit8: [0x09, false], Digit9: [0x0a, false],
  Digit0: [0x0b, false], Minus: [0x0c, false], Equal: [0x0d, false], Backspace: [0x0e, false], Tab: [0x0f, false],
  KeyQ: [0x10, false], KeyW: [0x11, false], KeyE: [0x12, false], KeyR: [0x13, false], KeyT: [0x14, false],
  KeyY: [0x15, false], KeyU: [0x16, false], KeyI: [0x17, false], KeyO: [0x18, false], KeyP: [0x19, false],
  BracketLeft: [0x1a, false], BracketRight: [0x1b, false], Enter: [0x1c, false], ControlLeft: [0x1d, false],
  KeyA: [0x1e, false], KeyS: [0x1f, false], KeyD: [0x20, false], KeyF: [0x21, false], KeyG: [0x22, false],
  KeyH: [0x23, false], KeyJ: [0x24, false], KeyK: [0x25, false], KeyL: [0x26, false], Semicolon: [0x27, false],
  Quote: [0x28, false], Backquote: [0x29, false], ShiftLeft: [0x2a, false], Backslash: [0x2b, false],
  KeyZ: [0x2c, false], KeyX: [0x2d, false], KeyC: [0x2e, false], KeyV: [0x2f, false], KeyB: [0x30, false],
  KeyN: [0x31, false], KeyM: [0x32, false], Comma: [0x33, false], Period: [0x34, false], Slash: [0x35, false],
  ShiftRight: [0x36, false], NumpadMultiply: [0x37, false], AltLeft: [0x38, false], Space: [0x39, false],
  CapsLock: [0x3a, false], F1: [0x3b, false], F2: [0x3c, false], F3: [0x3d, false], F4: [0x3e, false],
  F5: [0x3f, false], F6: [0x40, false], F7: [0x41, false], F8: [0x42, false], F9: [0x43, false], F10: [0x44, false],
  NumLock: [0x45, false], ScrollLock: [0x46, false], Numpad7: [0x47, false], Numpad8: [0x48, false],
  Numpad9: [0x49, false], NumpadSubtract: [0x4a, false], Numpad4: [0x4b, false], Numpad5: [0x4c, false],
  Numpad6: [0x4d, false], NumpadAdd: [0x4e, false], Numpad1: [0x4f, false], Numpad2: [0x50, false],
  Numpad3: [0x51, false], Numpad0: [0x52, false], NumpadDecimal: [0x53, false], IntlBackslash: [0x56, false],
  F11: [0x57, false], F12: [0x58, false], F13: [0x64, false], F14: [0x65, false], F15: [0x66, false],
  NumpadEnter: [0x1c, true], ControlRight: [0x1d, true], NumpadDivide: [0x35, true], PrintScreen: [0x37, true],
  AltRight: [0x38, true], Home: [0x47, true], ArrowUp: [0x48, true], PageUp: [0x49, true], ArrowLeft: [0x4b, true],
  ArrowRight: [0x4d, true], End: [0x4f, true], ArrowDown: [0x50, true], PageDown: [0x51, true], Insert: [0x52, true],
  Delete: [0x53, true], ContextMenu: [0x5d, true], NumpadEqual: [0x0d, false],
  // the Windows keys, sent only from the tab's Send Keys (⌘ stays the Mac's)
  MetaLeft: [0x5b, true], MetaRight: [0x5c, true],
};

/** The modifiers kept in step with each key's event: its flag, the key pressed for it, the keys that are it */
const MODIFIERS: ["ctrlKey" | "shiftKey" | "altKey", string, string[]][] = [
  ["ctrlKey", "ControlLeft", ["ControlLeft", "ControlRight"]],
  ["shiftKey", "ShiftLeft", ["ShiftLeft", "ShiftRight"]],
  ["altKey", "AltLeft", ["AltLeft", "AltRight"]],
];

/** ⌘ and these keys go to Windows as Ctrl and the same key */
const CTRL_FOR_COMMAND: Record<string, string> = { KeyC: "KeyC", KeyV: "KeyV", KeyX: "KeyX", KeyA: "KeyA", KeyZ: "KeyZ" };

/** ⌘ keys SuperXTerminal keeps when ⌘ is Windows' key: closing and opening tabs, quitting, Settings, switching
 *  windows, tab 1-9 */
const APP_COMMANDS = new Set(["KeyW", "KeyT", "KeyQ", "Comma", "Backquote", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9"]);

/** What the tab is: a stored session */
export interface RdpTarget {
  store: "saved" | "private" | "database";
  id: string;
}

/** A Remote Desktop tab's host: connects the session, draws the desktop and passes on the keyboard, mouse and clipboard */
export class RdpHost {
  readonly element: HTMLDivElement;
  private readonly canvas: HTMLCanvasElement;
  private readonly context: CanvasRenderingContext2D;
  private readonly overlay: HTMLDivElement;
  private id: number | null = null;
  private ended = false;
  /** A username and password typed for this tab, kept (in memory only) for reconnecting */
  private username: string | null = null;
  private password: string | null = null;
  /** Keys held down, let go when the tab loses the keyboard so none sticks on the remote side */
  private readonly held = new Map<string, [number, boolean]>();
  private resizeTimer: ReturnType<typeof setTimeout> | undefined;
  private readonly observer: ResizeObserver;
  private pendingMove: { x: number; y: number } | null = null;
  private disposed = false;
  /** The Windows key combinations option: 0 on this computer, 1 on the remote, 2 only in full screen */
  private keyboard = 1;
  /** Stops listening for ⌘ tapped on its own */
  private unlistenTap: (() => void) | null = null;
  /** The window is full screen (kept up to date as the tab resizes) */
  private fullScreen = false;
  /** Caps Lock as last sent */
  private capsLock: boolean | null = null;
  /** Reconnecting after a drop (Settings > General > Dropped sessions): the attempt and its timer */
  private attempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | undefined;
  private countdown: ReturnType<typeof setInterval> | undefined;
  /** The desktop showed since connecting, so a later end is a drop rather than a failed login */
  private shown = false;
  private generation = 0;

  constructor(private readonly target: RdpTarget, private readonly title: () => string) {
    this.element = document.createElement("div");
    this.element.className = "rdp-panel";
    this.canvas = document.createElement("canvas");
    this.canvas.tabIndex = 0;
    this.canvas.className = "rdp-screen";
    this.context = this.canvas.getContext("2d", { alpha: false })!;
    this.overlay = document.createElement("div");
    this.overlay.className = "rdp-overlay";
    this.element.append(this.canvas, this.overlay);
    this.bindInput();
    this.observer = new ResizeObserver(() => this.sizeChanged());
    this.observer.observe(this.element);
  }

  /**
   * The tab's size in points and the screen's scale (200 on Retina): the core makes the desktop that size in points
   * (a quarter of Retina's pixels to send and draw, as Microsoft's Mac client does unless "Optimize for Retina
   * displays" is on), in Retina pixels with the Retina option, or the options' fixed size
   */
  private size(): { width: number; height: number; scale: number } {
    const rect = this.element.getBoundingClientRect();
    return { width: Math.max(200, Math.round(rect.width)), height: Math.max(200, Math.round(rect.height)), scale: Math.round((window.devicePixelRatio || 1) * 100) };
  }

  /** Connect, once the tab has a size */
  async connect(): Promise<void> {
    if (this.disposed) return;
    const { width, height } = this.element.getBoundingClientRect();
    if (width < 50 || height < 50) {
      setTimeout(() => void this.connect(), 100);
      return;
    }
    this.ended = false;
    this.shown = false;
    if (this.attempts === 0) this.show(`Connecting to ${this.title()}…`);
    // what an earlier connection still sends (its end, as it closes) is ignored
    const generation = ++this.generation;
    const channel = new Channel<ArrayBuffer | Event>();
    channel.onmessage = (message) => generation === this.generation && this.receive(message);
    const { width: w, height: h, scale } = this.size();
    try {
      const opened = await invoke<{ id: number; width: number; height: number; fixed: boolean; smartSizing: boolean; scale: number; keyboard: number }>("rdp_open", {
        ...this.target, width: w, height: h, scale, username: this.username, password: this.password, onEvent: channel,
      });
      this.id = opened.id;
      this.keyboard = opened.keyboard;
      // SmartSizing off: the desktop at its own size (in points), scrolled when bigger than the tab, as SuperXPuTTY's
      this.element.classList.toggle("actual-size", !opened.smartSizing);
      this.canvas.style.width = opened.smartSizing ? "" : `${(opened.width * 100) / opened.scale}px`;
      this.canvas.style.height = opened.smartSizing ? "" : `${(opened.height * 100) / opened.scale}px`;
    } catch (error) {
      const text = String(error);
      // no username in the session or its folders: both are asked, as Microsoft's client asks
      if (text.includes("RDP_LOGIN_NEEDED")) {
        const login = await askLogin(`Log in to ${this.title()} (Remote Desktop)`, this.username ?? "");
        if (login?.username) {
          this.username = login.username;
          this.password = login.password || null;
          await this.connect();
          return;
        }
        this.end("No username was given. Click to try again.", false);
        return;
      }
      if (text.includes("RDP_PASSWORD_NEEDED")) {
        const typed = await askOne(`Password for ${this.title()} (Remote Desktop)`, true);
        if (typed) {
          this.password = typed;
          await this.connect();
          return;
        }
        this.end("No password was given. Click to try again.", false);
        return;
      }
      this.end(text, false);
    }
  }

  /** A packet of screen or pointer, or an event */
  private receive(message: ArrayBuffer | Event): void {
    if (message instanceof ArrayBuffer) {
      const head = new TextDecoder().decode(new Uint8Array(message, 0, 4));
      const view = new DataView(message);
      if (head === "SXF1") {
        const [x, y, w, h, fw, fh] = [4, 6, 8, 10, 12, 14].map((at) => view.getUint16(at, true));
        if (this.canvas.width !== fw || this.canvas.height !== fh) {
          this.canvas.width = fw;
          this.canvas.height = fh;
        }
        const pixels = new Uint8ClampedArray(message, 16, w * h * 4);
        this.context.putImageData(new ImageData(pixels, w, h), x, y);
        if (!this.shown) {
          this.shown = true;
          this.attempts = 0;
        }
        if (this.overlay.style.display !== "none" && !this.ended) this.overlay.style.display = "none";
      } else if (head === "SXP1") {
        const [w, h, hx, hy] = [4, 6, 8, 10].map((at) => view.getUint16(at, true));
        this.pointer(new Uint8ClampedArray(message, 12, w * h * 4), w, h, hx, hy);
      }
      return;
    }
    if (message.type === "pointer") {
      if (message.kind === "hidden") this.canvas.style.cursor = "none";
      else if (message.kind === "default") this.canvas.style.cursor = "default";
    } else if (message.type === "ended") {
      // a refused login: what was typed is asked again next time
      if (/login was refused/i.test(message.reason)) this.password = null;
      this.end(message.clean ? `The session ended (${message.reason}). Click to connect again.` : `${message.reason}\n\nClick to try again.`, message.clean);
    }
  }

  /** The remote pointer's shape as the canvas's cursor (drawn at the screen's scale) */
  private pointer(rgba: Uint8ClampedArray, w: number, h: number, hx: number, hy: number): void {
    if (w === 0 || h === 0) return;
    const shape = document.createElement("canvas");
    shape.width = w;
    shape.height = h;
    shape.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(rgba), w, h), 0, 0);
    // the desktop is in points, so the pointer is too
    this.canvas.style.cursor = `url(${shape.toDataURL()}) ${hx} ${hy}, default`;
  }

  /** Show a message over the desktop (connecting, ended, reconnecting) */
  private show(text: string): void {
    this.overlay.textContent = text;
    this.overlay.style.display = "flex";
  }

  /** The session ended: say why, and reconnect after a while when it dropped (Settings > General's reconnect settings) */
  private end(text: string, clean: boolean): void {
    this.ended = true;
    this.id = null;
    this.releaseKeys();
    this.show(text);
    // dropped (it had been showing, or this was a try at reconnecting): again in a while, as a terminal tab does
    const max = settings.autoReconnectAttempts || 6;
    if (!clean && !this.disposed && settings.autoReconnect && (this.shown || this.attempts > 0) && this.attempts < max) {
      this.attempts++;
      let left = Math.max(1, settings.autoReconnectDelaySeconds || 10);
      const say = () => this.show(`${text.split("\n")[0]}\n\nReconnecting in ${left} s (try ${this.attempts} of ${max}). Click to reconnect now.`);
      say();
      this.countdown = setInterval(() => {
        left--;
        if (left > 0) say();
      }, 1000);
      this.reconnectTimer = setTimeout(() => {
        clearInterval(this.countdown);
        this.reconnectTimer = undefined;
        this.show(`Reconnecting to ${this.title()} (try ${this.attempts} of ${max})…`);
        void this.connect();
      }, left * 1000);
    } else if (this.attempts > 0 && !clean) {
      this.attempts = 0;
      this.show(`${text.split("\n")[0]}\n\nGave up reconnecting. Click or press Enter to try again.`);
    }
  }

  /** A point on the canvas as it shows on screen, to the remote desktop's pixels (the picture may be letterboxed) */
  private remote(event: MouseEvent): { x: number; y: number } {
    const rect = this.canvas.getBoundingClientRect();
    const scale = Math.min(rect.width / this.canvas.width, rect.height / this.canvas.height);
    const left = rect.left + (rect.width - this.canvas.width * scale) / 2;
    const top = rect.top + (rect.height - this.canvas.height * scale) / 2;
    const x = Math.round((event.clientX - left) / scale);
    const y = Math.round((event.clientY - top) / scale);
    return { x: Math.max(0, Math.min(this.canvas.width - 1, x)), y: Math.max(0, Math.min(this.canvas.height - 1, y)) };
  }

  /** Pass the canvas's mouse, wheel and keyboard on to the remote desktop */
  private bindInput(): void {
    const c = this.canvas;
    const buttons = ["left", "middle", "right"];
    const mouse = (point: { x: number; y: number }, button: string | null, down: boolean, wheel = 0, horizontal = false) => {
      if (this.id === null) return;
      void invoke("rdp_mouse", { id: this.id, ...point, button, down, wheel, horizontal }).catch(() => {});
    };
    c.addEventListener("mousemove", (e) => {
      // one move a frame is plenty
      const first = this.pendingMove === null;
      this.pendingMove = this.remote(e);
      if (first) {
        requestAnimationFrame(() => {
          if (this.pendingMove) mouse(this.pendingMove, null, false);
          this.pendingMove = null;
        });
      }
    });
    c.addEventListener("mousedown", (e) => {
      c.focus();
      if (this.ended) {
        void this.reconnect();
        return;
      }
      e.preventDefault();
      mouse(this.remote(e), buttons[e.button] ?? "left", true);
    });
    c.addEventListener("mouseup", (e) => {
      if (!this.ended) mouse(this.remote(e), buttons[e.button] ?? "left", false);
    });
    c.addEventListener("contextmenu", (e) => e.preventDefault());
    c.addEventListener("wheel", (e) => {
      e.preventDefault();
      const horizontal = Math.abs(e.deltaX) > Math.abs(e.deltaY);
      const delta = horizontal ? e.deltaX : e.deltaY;
      if (Math.abs(delta) < 4) return;
      // macOS's scrolling: content follows the fingers, as Windows' wheel away from the user scrolls up
      mouse(this.remote(e), null, false, delta < 0 ? 1 : -1, horizontal);
    }, { passive: false });
    this.overlay.addEventListener("mousedown", () => {
      if (this.ended) void this.reconnect();
    });

    const key = (e: KeyboardEvent, down: boolean) => {
      if (this.id === null) return;
      // on Windows the Meta key is the Windows key itself, and goes to the remote computer as it is (below)
      if (isMac && (e.metaKey || e.key === "Meta")) {
        // ⌘ tapped on its own comes from the app (command-tap), as the page never sees it
        if (!down || e.key === "Meta" || e.repeat) return;
        // ⌘C, ⌘V, ⌘X, ⌘A, ⌘Z as Windows' Ctrl ones, as Microsoft's Mac client does
        const letter = CTRL_FOR_COMMAND[e.code];
        if (letter && !e.altKey && !e.ctrlKey) {
          e.preventDefault();
          e.stopPropagation();
          // macOS sends no key-up for a key pressed with ⌘, so the whole press goes now
          void this.commandKey(e.shiftKey && e.code === "KeyZ" ? "KeyY" : letter);
          return;
        }
        // other ⌘ keys as Windows and the key (⌘L locks, ⌘R is Run), when the Windows key combinations option
        // says so; SuperXTerminal keeps the ones to leave or switch the tab (⌘W, ⌘T, ⌘Q, ⌘, ⌘1-9)
        // ⌃⌘F (full screen) stays the app's too, so full screen can always be left
        if (!this.windowsKeys() || APP_COMMANDS.has(e.code) || (e.ctrlKey && e.code === "KeyF") || !SCANCODES[e.code]) return;
        e.preventDefault();
        e.stopPropagation();
        const mods = [e.ctrlKey && "ControlLeft", e.altKey && "AltLeft", e.shiftKey && "ShiftLeft"].filter(Boolean) as string[];
        void this.sendCombo(["MetaLeft", ...mods, e.code]);
        return;
      }
      // Fn+Control+Option+Delete is Ctrl+Alt+Delete, as in Microsoft's Mac client; on Windows, which keeps
      // Ctrl+Alt+Delete for itself, Ctrl+Alt+End is, as in Remote Desktop Connection
      if (down && e.ctrlKey && e.altKey && (e.code === "Delete" || (!isMac && e.code === "End"))) {
        e.preventDefault();
        e.stopPropagation();
        void this.sendCombo(["ControlLeft", "AltLeft", "Delete"]);
        return;
      }
      // Caps Lock goes as the lock's state (macOS reports it on and off, not as presses)
      if (e.code === "CapsLock") {
        this.syncLocks(e, true);
        return;
      }
      const code = SCANCODES[e.code];
      if (!code) return;
      e.preventDefault();
      e.stopPropagation();
      if (down) this.syncLocks(e);
      if (!MODIFIERS.some(([, , codes]) => codes.includes(e.code))) this.syncModifiers(e);
      if (down) this.held.set(e.code, code);
      else this.held.delete(e.code);
      void invoke("rdp_key", { id: this.id, scancode: code[0], extended: code[1], down }).catch(() => {});
    };
    c.addEventListener("keydown", (e) => {
      if (this.ended && e.key === "Enter") {
        void this.reconnect();
        return;
      }
      key(e, true);
    });
    c.addEventListener("keyup", (e) => key(e, false));
    c.addEventListener("blur", () => this.releaseKeys());
    // ⌘ tapped on its own (watched by the app: the page never sees a lone modifier) is the Windows key, Start
    void getCurrentWebviewWindow()
      .listen("command-tap", () => {
        if (document.activeElement === this.canvas && this.windowsKeys()) void this.sendCombo(["MetaLeft"]);
      })
      .then((unlisten) => {
        if (this.disposed) unlisten();
        else this.unlistenTap = unlisten;
      });
    // back in the tab: what was copied on the Mac meanwhile is offered to Windows' clipboard
    c.addEventListener("focus", () => {
      if (this.id !== null) void invoke("rdp_clipboard_announce", { id: this.id }).catch(() => {});
    });
  }

  /** Ctrl, Shift and Option as the key's event says they are: one pressed before the tab had the keyboard (or by
   *  a key without its own key-down, as macOS's automation sends) is pressed in Windows too, one let go meanwhile is
   *  let go */
  private syncModifiers(e: KeyboardEvent): void {
    if (this.id === null) return;
    for (const [flag, primary, codes] of MODIFIERS) {
      const held = codes.filter((c) => this.held.has(c));
      if (e[flag] && held.length === 0) {
        const [scancode, extended] = SCANCODES[primary];
        this.held.set(primary, [scancode, extended]);
        void invoke("rdp_key", { id: this.id, scancode, extended, down: true }).catch(() => {});
      } else if (!e[flag]) {
        for (const c of held) {
          const [scancode, extended] = this.held.get(c)!;
          this.held.delete(c);
          void invoke("rdp_key", { id: this.id, scancode, extended, down: false }).catch(() => {});
        }
      }
    }
  }

  /** Caps Lock as the Mac has it (Num Lock always on: a Mac's keypad types numbers), sent when it changes */
  private syncLocks(e: KeyboardEvent, force = false): void {
    if (this.id === null) return;
    const caps = e.getModifierState("CapsLock");
    if (!force && caps === this.capsLock) return;
    this.capsLock = caps;
    void invoke("rdp_sync", { id: this.id, caps, num: true }).catch(() => {});
  }

  /** ⌘ and a letter as Ctrl and it; ⌘V first offers the Mac's clipboard, giving Windows a moment to take it in */
  private async commandKey(code: string): Promise<void> {
    if (this.id === null) return;
    if (code === "KeyV" && (await invoke<boolean>("rdp_clipboard_announce", { id: this.id }).catch(() => false))) {
      await new Promise((r) => setTimeout(r, 150));
    }
    await this.sendCombo(["ControlLeft", code]);
  }

  /** Press keys in order and let them go in reverse (a combination: Ctrl+Alt+Delete), in one go */
  async sendCombo(codes: string[]): Promise<void> {
    if (this.id === null) return;
    const keys = codes.map((c) => SCANCODES[c]).filter(Boolean);
    const sequence = [...keys.map(([s, x]) => [s, x, true]), ...[...keys].reverse().map(([s, x]) => [s, x, false])];
    // anything held (the Control of a ⌃-click) is let go first, so the combination is exactly these keys
    this.releaseKeys();
    await invoke("rdp_keys", { id: this.id, keys: sequence }).catch(() => {});
  }

  /** Let go of every key still held (the tab lost the keyboard: ⌘Tab, a click elsewhere) */
  private releaseKeys(): void {
    if (this.id !== null) {
      for (const [, [scancode, extended]] of this.held) void invoke("rdp_key", { id: this.id, scancode, extended, down: false }).catch(() => {});
    }
    this.held.clear();
  }

  /** Whether ⌘ and a key go to Windows as the Windows key and it now */
  private windowsKeys(): boolean {
    return this.keyboard === 1 || (this.keyboard === 2 && this.fullScreen);
  }

  /** The tab changed size: the remote desktop follows, once the resizing settles */
  private sizeChanged(): void {
    void getCurrentWindow().isFullscreen().then((on) => (this.fullScreen = on)).catch(() => {});
    clearTimeout(this.resizeTimer);
    this.resizeTimer = setTimeout(() => {
      if (this.id === null) return;
      const { width, height, scale } = this.size();
      void invoke("rdp_resize", { id: this.id, width, height, scale }).catch(() => {});
    }, 400);
  }

  // ---- what Send Commands, the button bar, scripts and the tab's menu use, as a terminal tab's PageHost ----

  /** Connected and logged in, so typing goes somewhere */
  get canReceiveCommands(): boolean {
    return this.id !== null && !this.ended;
  }

  /** Whether the session has ended */
  get isEnded(): boolean {
    return this.ended;
  }

  /** Whether a reconnect is waiting, for the tab's Stop Reconnecting */
  get isReconnecting(): boolean {
    return this.reconnectTimer !== undefined;
  }

  /** An RDP tab has no output to search or log */
  readonly logPath: string | null = null;

  /**
   * Type text, as SuperXPuTTY's RdpControl.SendKeystrokes: characters as Unicode (so the remote keyboard layout
   * doesn't matter), Enter, Tab, Backspace and Escape as keys, other control characters as Ctrl and their letter
   */
  async type(text: string): Promise<void> {
    if (!this.canReceiveCommands || !text) return;
    let run = "";
    const flush = async () => {
      if (run) await invoke("rdp_text", { id: this.id, text: run }).catch(() => {});
      run = "";
    };
    for (const c of text) {
      const n = c.charCodeAt(0);
      if (n >= 1 && n <= 26 && !"\t\r\n\b".includes(c)) {
        await flush();
        await this.sendCombo(["ControlLeft", `Key${String.fromCharCode(64 + n)}`]);
      } else run += c;
    }
    await flush();
  }

  /** WAITFOR has no output to look at in a Remote Desktop: it waits out its time, as in SuperXPuTTY */
  markOutput(_atLineStart: boolean): void {}

  /** A Remote Desktop has no output for WAITFOR to read */
  outputSinceMark(): string {
    return "";
  }

  /** A Remote Desktop has no output for WAITFOR to read, so nothing to move past */
  advanceMark(_characters: number): void {}

  /** SENDBREAK is a serial line's; a Remote Desktop has none */
  async sendBreak(): Promise<void> {
    throw new Error("A Remote Desktop tab can't send a Break");
  }

  /** Connect again (the tab's Reconnect), or after a drop */
  async reconnect(): Promise<void> {
    this.cancelReconnect();
    this.close();
    await this.connect();
  }

  /** Disconnect, keeping the tab (a click connects again) */
  disconnect(): void {
    this.cancelReconnect();
    this.close();
    this.end("Disconnected. Click or press Enter to connect again.", true);
  }

  /** The tab's Stop Reconnecting */
  stopReconnecting(): void {
    this.cancelReconnect();
    if (this.ended) this.show("Not reconnecting. Click or press Enter to connect again.");
  }

  /** Stop the reconnect waiting and its countdown, and start counting tries again */
  private cancelReconnect(): void {
    clearTimeout(this.reconnectTimer);
    clearInterval(this.countdown);
    this.reconnectTimer = undefined;
    this.attempts = 0;
  }

  /** Close the connection, its last messages ignored */
  private close(): void {
    this.generation++;
    if (this.id !== null) void invoke("rdp_close", { id: this.id }).catch(() => {});
    this.id = null;
  }

  /** The remote desktop as a picture on the Mac's clipboard */
  async copyScreenshot(): Promise<void> {
    const blob = await new Promise<Blob | null>((resolve) => this.canvas.toBlob(resolve, "image/png"));
    if (blob) await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
  }

  /** The keyboard into the remote desktop */
  focus(): void {
    this.canvas.focus();
  }

  /** The tab closed: disconnect */
  dispose(): void {
    this.disposed = true;
    this.unlistenTap?.();
    clearTimeout(this.reconnectTimer);
    clearInterval(this.countdown);
    this.observer.disconnect();
    clearTimeout(this.resizeTimer);
    if (this.id !== null) void invoke("rdp_close", { id: this.id }).catch(() => {});
    this.id = null;
  }
}
