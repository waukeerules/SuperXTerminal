/**
 * A Remote Desktop tab's host: the canvas the remote screen is drawn on, and the keyboard and mouse sent back, as
 * SuperXPuTTY's RDP tab (Microsoft's control there; IronRDP in the core here, src-tauri/src/rdp.rs).
 *
 * The desktop is the tab's size (Fit the tab, as SuperXPuTTY's default) in points, and follows the tab when it changes
 * size. ⌘ shortcuts stay SuperXTerminal's (⌘W closes the tab, ⌘T opens a shell);
 * Control and Option are Windows' Ctrl and Alt.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import { askLogin, askOne } from "../scripting/asking.svelte";

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
};

/** What the tab is: a stored session */
export interface RdpTarget {
  store: "saved" | "private" | "database";
  id: string;
}

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
   * The desktop's size: the tab's, in points (not Retina pixels, a quarter of the pixels to send and draw, as
   * Microsoft's Mac client does unless "Optimize for Retina displays" is on), at Windows' 100% scale
   */
  private size(): { width: number; height: number; scale: number } {
    const rect = this.element.getBoundingClientRect();
    return { width: Math.max(200, Math.round(rect.width)), height: Math.max(200, Math.round(rect.height)), scale: 100 };
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
    this.show(`Connecting to ${this.title()}…`);
    const channel = new Channel<ArrayBuffer | Event>();
    channel.onmessage = (message) => this.receive(message);
    const { width: w, height: h, scale } = this.size();
    try {
      this.id = await invoke<number>("rdp_open", { ...this.target, width: w, height: h, scale, username: this.username, password: this.password, onEvent: channel });
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

  private show(text: string): void {
    this.overlay.textContent = text;
    this.overlay.style.display = "flex";
  }

  private end(text: string, _clean: boolean): void {
    this.ended = true;
    this.id = null;
    this.show(text);
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
        void this.connect();
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
      if (this.ended) void this.connect();
    });

    const key = (e: KeyboardEvent, down: boolean) => {
      // ⌘ shortcuts are SuperXTerminal's (⌘W, ⌘T, ⌘D ...)
      if (e.metaKey || e.key === "Meta") return;
      const code = SCANCODES[e.code];
      if (!code || this.id === null) return;
      e.preventDefault();
      e.stopPropagation();
      if (down) this.held.set(e.code, code);
      else this.held.delete(e.code);
      void invoke("rdp_key", { id: this.id, scancode: code[0], extended: code[1], down }).catch(() => {});
    };
    c.addEventListener("keydown", (e) => {
      if (this.ended && e.key === "Enter") {
        void this.connect();
        return;
      }
      key(e, true);
    });
    c.addEventListener("keyup", (e) => key(e, false));
    c.addEventListener("blur", () => this.releaseKeys());
  }

  /** Let go of every key still held (the tab lost the keyboard: ⌘Tab, a click elsewhere) */
  private releaseKeys(): void {
    if (this.id !== null) {
      for (const [, [scancode, extended]] of this.held) void invoke("rdp_key", { id: this.id, scancode, extended, down: false }).catch(() => {});
    }
    this.held.clear();
  }

  /** The tab changed size: the remote desktop follows, once the resizing settles */
  private sizeChanged(): void {
    clearTimeout(this.resizeTimer);
    this.resizeTimer = setTimeout(() => {
      if (this.id === null) return;
      const { width, height, scale } = this.size();
      if (width === this.canvas.width && height === this.canvas.height) return;
      void invoke("rdp_resize", { id: this.id, width, height, scale }).catch(() => {});
    }, 400);
  }

  /** The keyboard into the remote desktop */
  focus(): void {
    this.canvas.focus();
  }

  /** The tab closed: disconnect */
  dispose(): void {
    this.disposed = true;
    this.observer.disconnect();
    clearTimeout(this.resizeTimer);
    if (this.id !== null) void invoke("rdp_close", { id: this.id }).catch(() => {});
    this.id = null;
  }
}
