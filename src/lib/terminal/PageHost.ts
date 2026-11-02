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

/** What the Rust core sends on a tab's channel (`PtyEvent`) */
type PtyEvent = { type: "data"; data: string } | { type: "exit"; code: number | null };

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
  /** The size the page last asked for, kept until the pseudo terminal is open */
  private size = { cols: 80, rows: 24 };
  private readonly onWindowMessage = (event: MessageEvent) => this.receive(event);

  /**
   * @param tab the tab's id, also in the iframe's URL, so messages reach the right page
   * @param frame the iframe holding the page
   * @param onExit called when the shell ends, so the tab can close
   * @param onTabCommand the page's tab menu items: "close", "closeOthers", "closeRight" or "closeAll"
   */
  constructor(
    private readonly tab: string,
    private readonly frame: HTMLIFrameElement,
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
        this.sendConfig();
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

  /** The page's settings: no highlighting rules or themes yet (M4), the app's light or dark look */
  private sendConfig(): void {
    const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    this.post({
      type: "config",
      ruleSets: [],
      themes: [],
      dark,
      bell: "visual",
      cursor: { style: "block", blink: true },
      local: true,
      serial: false,
      prefs: {},
      fonts: [],
      menuKeys: MENU_KEYS,
      sessionRules: "",
      unlimitedScrollback: 1000000,
    });
  }

  /** Start the login shell at the page's size and pass its output on */
  private async open(): Promise<void> {
    if (this.ptyId !== null) return;
    const channel = new Channel<PtyEvent>();
    channel.onmessage = (event) => {
      if (event.type === "data") {
        this.queue.push(event.data);
        this.flush();
      } else {
        this.onExit();
      }
    };
    try {
      this.ptyId = await invoke<number>("pty_open", { ...this.size, onEvent: channel });
    } catch (error) {
      this.post({ type: "notice", text: `Couldn't start the shell: ${error}`, kind: "error" });
    }
  }

  /** Send the next chunk of output if the page has drawn the last one */
  private flush(): void {
    if (this.waiting || this.queue.length === 0) return;
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
