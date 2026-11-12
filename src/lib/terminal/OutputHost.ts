/**
 * The host side of Highlighted Output (SuperXPuTTY's `web/output` page, Monaco, in an iframe beside a tab), as
 * `Gui/HighlightedOutputPanel.cs`: it follows the tab's session log every 250 ms and sends the page what's new, one
 * chunk (512 KB at most) at a time, each acknowledged before the next. The page can save what it shows, open another
 * log or text file instead (then the tab is no longer followed), open the user's highlighting folder, or reload the
 * rules. When the tab closes, what's left is sent and the title says "(closed)".
 */
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { highlighting, pageRuleSets } from "./highlighting";
import { isDark } from "../settings/store.svelte";

/** A message to or from the output page: its type, and what goes with it */
type PageMessage = { type: string; [key: string]: unknown };

/** One Highlighted Output panel's host: follows a file and feeds the page, as HighlightedOutputPanel */
export class OutputHost {
  /** The file followed: the tab's log, or one opened from the page */
  private path: string | null;
  private offset = 0;
  /** A chunk has been sent and not yet acknowledged */
  private waiting = false;
  private timer: ReturnType<typeof setInterval> | null = null;
  private readonly onWindowMessage = (event: MessageEvent) => this.receive(event);

  /**
   * @param id the panel's id, also in the iframe's URL
   * @param frame the iframe holding the page
   * @param logPath the tab's session log
   * @param title the tab's title, for the page and its Save
   */
  constructor(
    private readonly id: string,
    private readonly frame: HTMLIFrameElement,
    logPath: string | null,
    private title: string,
  ) {
    this.path = logPath;
    window.addEventListener("message", this.onWindowMessage);
  }

  /** The panel closed: stop following and stop listening to the page */
  dispose(): void {
    if (this.timer) clearInterval(this.timer);
    window.removeEventListener("message", this.onWindowMessage);
  }

  /** The tab closed: send what's left, stop following, and say so in the title */
  sourceClosed(): void {
    void this.send().finally(() => {
      if (this.timer) clearInterval(this.timer);
      this.timer = null;
      this.title = `${this.title} (closed)`;
    });
  }

  /** Send the page a message, on the host's channel for this panel */
  private post(message: PageMessage): void {
    this.frame.contentWindow?.postMessage({ channel: "sxt-host", tab: this.id, message }, "/");
  }

  /** Take a message from this panel's page, ignoring any other window's */
  private receive(event: MessageEvent): void {
    if (event.source !== this.frame.contentWindow || event.origin !== window.location.origin) return;
    const envelope = event.data;
    if (!envelope || envelope.channel !== "sxt-page" || envelope.tab !== this.id) return;
    void this.handle(envelope.message as PageMessage);
  }

  /** Answer the page: ready (send the rules, then the file from the start), ack (the next chunk), save, open and the rest */
  private async handle(message: PageMessage): Promise<void> {
    switch (message.type) {
      case "ready":
        await this.sendConfig();
        this.offset = 0;
        this.waiting = false;
        if (!this.timer) this.timer = setInterval(() => void this.send(), 250);
        await this.send();
        break;
      case "ack":
        this.waiting = false;
        await this.send();
        break;
      case "save":
        await this.saveText(String(message.text ?? ""));
        break;
      case "open":
        await this.openFile();
        break;
      case "folder":
        await invoke("open_path", { path: await invoke<string>("highlighting_user_folder") });
        break;
      case "reload":
        await this.sendConfig();
        break;
    }
  }

  /** The rules and themes, the look, and the title */
  private async sendConfig(): Promise<void> {
    const files = await highlighting();
    this.post({ type: "config", ruleSets: pageRuleSets(files), themes: files.themes, dark: isDark(), title: this.title });
  }

  /** Send what's new in the file, if the last chunk has been drawn */
  private async send(): Promise<void> {
    if (this.waiting || !this.path) return;
    try {
      const [data, next, restarted] = await invoke<[string, number, boolean]>("log_chunk", { path: this.path, offset: this.offset });
      if (restarted) {
        this.offset = 0;
        this.post({ type: "restart" });
        return;
      }
      if (!data) return;
      this.offset = next;
      this.waiting = true;
      this.post({ type: "data", data });
    } catch {
      // the log isn't there yet, or has gone with its tab
    }
  }

  /** Save what the page shows, as text with CRLF line endings */
  private async saveText(text: string): Promise<void> {
    const path = await save({ title: "Save Output", defaultPath: `${this.title}.txt`, filters: [{ name: "Text", extensions: ["txt"] }] });
    if (path) await invoke("text_file_write", { path, text });
  }

  /** Show another log or text file instead of the tab's */
  private async openFile(): Promise<void> {
    const path = await open({ title: "Open Log or Text File", filters: [{ name: "Logs and text", extensions: ["log", "txt"] }] });
    if (typeof path !== "string") return;
    this.path = path;
    this.offset = 0;
    this.waiting = false;
    this.title = path.split("/").pop() ?? path;
    this.post({ type: "clear", title: this.title });
    if (!this.timer) this.timer = setInterval(() => void this.send(), 250);
    await this.send();
  }
}
