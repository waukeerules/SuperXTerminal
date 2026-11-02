/**
 * The host side of one terminal tab's page (SuperXPuTTY's `web/terminal`, in an iframe).
 *
 * In SuperXPuTTY this is `Gui/TerminalControl.cs`. The page talks to it through `window.chrome.webview`, which
 * `public/web/host/webview.js` maps to `postMessage` between the iframe and this window. This class answers the page
 * and connects it to a pseudo terminal in the Rust core (`src-tauri/src/pty.rs`).
 *
 * Output is passed on one chunk at a time: the page answers each "data" with "ack" once xterm.js has drawn it, as
 * with TerminalControl, so a flood of output can't outrun the page.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import { highlighting } from "./highlighting";

/** What the Rust core sends on a tab's channel (`PtyEvent`) */
type PtyEvent =
  | { type: "data"; data: string }
  | { type: "exit"; code: number | null }
  | { type: "notice"; text: string; kind: string }
  | { type: "started"; local: boolean; look: Look };

/** The profile's cursor and bell for the page (`Look` in `src-tauri/src/launch.rs`) */
interface Look {
  cursor: string;
  blink: boolean;
  bell: string;
  /** The session's or its folder's highlighting rules; "" for the default */
  rules: string;
}

/** What a tab runs (`Target` in `src-tauri/src/pty.rs`) */
export type Target =
  | { kind: "localShell" }
  | { kind: "session"; store: "saved" | "private"; id: string }
  | ({ kind: "quickConnect" } & QuickConnect);

/** Quick connect's toolbar fields (`QuickConnect` in `src-tauri/src/launch.rs`) */
export interface QuickConnect {
  protocol: string;
  host: string;
  username: string;
  password: string;
  profile: string;
}

/** A message from the page; every message has a type, and the rest depends on it */
type PageMessage = { type: string; [key: string]: unknown };

/** Text the Send Key menu offers, as SuperXPuTTY's `TerminalKeys.MenuKeys` */
const MENU_KEYS = [
  { key: "^C", label: "Ctrl+C", hint: "interrupt" },
  { key: "^Z", label: "Ctrl+Z", hint: "end, leave config" },
  { key: "^6", label: "Ctrl+Shift+6", hint: "Cisco escape" },
  { key: "^]", label: "Ctrl+]", hint: "Telnet escape" },
  { key: "^D", label: "Ctrl+D", hint: "end of input" },
  { key: "^\\", label: "Ctrl+\\", hint: "quit" },
  { key: "ESC", label: "Esc", hint: "" },
];

/**
 * A stand-in theme for when no theme files are found (SuperXPuTTY's defaults are missing from its Git repository):
 * the look the terminal has without one. The page applies colours, the session's cursor colour included, only
 * through a theme, so with none the cursor stayed white. It has no highlighting styles, so rules colour nothing.
 */
const STAND_IN_THEMES = [{ name: "PuTTY", base: "vs-dark", colors: { background: "#000000", foreground: "#FFFFFF" } }];

/** What each Send Key entry types */
const KEY_TEXT: Record<string, string> = {
  "^C": "\x03", "^Z": "\x1a", "^6": "\x1e", "^]": "\x1d", "^D": "\x04", "^\\": "\x1c", ESC: "\x1b",
};

export class PageHost {
  /** The pseudo terminal's id, once it's open */
  private ptyId: number | null = null;
  /** Output waiting for the page to acknowledge the chunk before it */
  private queue: string[] = [];
  /** A chunk has been sent and not yet acknowledged */
  private waiting = false;
  /** Settles once the page has its settings; output is held until then */
  private configured: Promise<void> | null = null;
  private settled = false;
  /** The size the page last asked for, kept until the pseudo terminal is open */
  private size = { cols: 80, rows: 24 };
  private readonly onWindowMessage = (event: MessageEvent) => this.receive(event);

  /**
   * @param tab the tab's id, also in the iframe's URL, so messages reach the right page
   * @param frame the iframe holding the page
   * @param target what the tab runs
   * @param local whether it's a local shell (the page treats Ctrl+C and Open in PuTTY differently)
   * @param cursorColor the session's environment colour, which the cursor is drawn in as in SuperXPuTTY; null for none
   * @param onExit called when a local shell ends, so its tab closes; a session's tab stays open instead, so its last
   *   output (e.g. why ssh couldn't connect) can be read. SuperXPuTTY's reconnect comes in M4
   * @param onTabCommand the page's tab menu items: "close", "closeOthers", "closeRight" or "closeAll"
   */
  constructor(
    private readonly tab: string,
    private readonly frame: HTMLIFrameElement,
    private readonly target: Target,
    private readonly local: boolean,
    private readonly cursorColor: string | null,
    private readonly onExit: () => void,
    private readonly onTabCommand: (command: string) => void,
  ) {
    window.addEventListener("message", this.onWindowMessage);
  }

  /** Close the pseudo terminal and stop listening; called when the tab closes */
  dispose(): void {
    window.removeEventListener("message", this.onWindowMessage);
    if (this.ptyId !== null) {
      void invoke("pty_close", { id: this.ptyId });
      this.ptyId = null;
    }
  }

  /** Send a message to the page */
  private post(message: PageMessage): void {
    this.frame.contentWindow?.postMessage({ channel: "sxt-host", tab: this.tab, message }, window.location.origin);
  }

  /** Handle a window message if it's this tab's page talking */
  private receive(event: MessageEvent): void {
    if (event.source !== this.frame.contentWindow || event.origin !== window.location.origin) return;
    const envelope = event.data;
    if (!envelope || envelope.channel !== "sxt-page" || envelope.tab !== this.tab) return;
    void this.handle(envelope.message as PageMessage);
  }

  /** Answer one message from the page, as TerminalControl does */
  private async handle(message: PageMessage): Promise<void> {
    switch (message.type) {
      case "ready":
        // the page gets its settings once the core says what's starting ("started"), with the profile's look
        await this.open();
        break;
      case "size":
        this.size = { cols: Number(message.cols), rows: Number(message.rows) };
        if (this.ptyId !== null) await invoke("pty_resize", { id: this.ptyId, ...this.size });
        break;
      case "input":
        if (this.ptyId !== null) await invoke("pty_write", { id: this.ptyId, data: String(message.data) });
        break;
      case "ack":
        this.waiting = false;
        this.flush();
        break;
      case "copy":
        if (typeof message.text === "string") await navigator.clipboard.writeText(message.text);
        break;
      case "pasteRequest":
        // paste check (several lines) comes in M4; until then every paste goes straight through
        await this.paste(typeof message.text === "string" ? message.text : null);
        break;
      case "sendKey":
        // "Other Key..." (no key) needs the Send Key dialog, which comes in M3
        if (typeof message.key === "string" && this.ptyId !== null) {
          const text = KEY_TEXT[message.key];
          if (text) await invoke("pty_write", { id: this.ptyId, data: text });
        }
        break;
      case "tab":
        this.onTabCommand(String(message.command));
        break;
      // log, document, bell, openInPutty, openLink, pref, cleared: later milestones
    }
  }

  /** Paste the given text, or the clipboard's, into the terminal */
  private async paste(text: string | null): Promise<void> {
    const value = text ?? (await navigator.clipboard.readText().catch(() => ""));
    if (value) this.post({ type: "paste", text: value });
  }

  /** The page's settings: the highlighting rules and themes, and the profile's cursor and bell */
  private async sendConfig(look: Look): Promise<void> {
    const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    const files = await highlighting();
    this.post({
      type: "config",
      ruleSets: files.ruleSets,
      themes: files.themes.length ? files.themes : STAND_IN_THEMES,
      dark,
      bell: look.bell,
      cursor: { style: look.cursor, blink: look.blink },
      local: this.local,
      serial: false,
      prefs: {},
      fonts: [],
      menuKeys: MENU_KEYS,
      sessionRules: look.rules,
      unlimitedScrollback: 1000000,
      cursorColor: this.cursorColor,
    });
  }

  /** Start the tab's program at the page's size and pass its output on */
  private async open(): Promise<void> {
    if (this.ptyId !== null) return;
    const channel = new Channel<PtyEvent>();
    channel.onmessage = (event) => {
      if (event.type === "data") {
        this.queue.push(event.data);
        this.flush();
      } else if (event.type === "started") {
        // output waits until the page has its settings, so the first lines are highlighted too
        this.configured = this.sendConfig(event.look);
      } else if (event.type === "notice") {
        this.post({ type: "notice", text: event.text, kind: event.kind });
      } else if (event.type === "exit") {
        if (this.local) this.onExit();
        else this.post({ type: "notice", text: "Session ended.", kind: "info" });
      }
    };
    try {
      this.ptyId = await invoke<number>("pty_open", { target: this.target, ...this.size, onEvent: channel });
    } catch (error) {
      // nothing started, so the page has had no settings yet
      await this.sendConfig({ cursor: "block", blink: true, bell: "visual", rules: "" });
      this.post({ type: "notice", text: String(error), kind: "error" });
    }
  }

  /** Send the next chunk of output if the page has drawn the last one and has its settings */
  private flush(): void {
    if (this.waiting || this.queue.length === 0) return;
    if (!this.configured) return;
    if (!this.settled) {
      void this.configured.then(() => {
        this.settled = true;
        this.flush();
      });
      return;
    }
    // join what's waiting into one chunk; base64 can't be joined, so decode and re-encode
    const data = this.queue.length === 1 ? this.queue[0] : joinBase64(this.queue);
    this.queue = [];
    this.waiting = true;
    this.post({ type: "data", data });
  }
}

/** Join base64 chunks into one: decode each to bytes, then encode the whole */
function joinBase64(chunks: string[]): string {
  let binary = "";
  for (const chunk of chunks) binary += atob(chunk);
  return btoa(binary);
}
