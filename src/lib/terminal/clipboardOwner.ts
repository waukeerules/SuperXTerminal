/**
 * Which tab's highlight is on the clipboard, so it can be cleared when something else takes the clipboard, as PuTTY
 * clears its selection when it loses the clipboard (WM_DESTROYCLIPBOARD, or X's SelectionClear). Copying in another
 * tab hands the clipboard over at once; a copy anywhere else (another app, a text box in this one, a Remote Desktop
 * tab) is seen by the clipboard's change number moving on (clipboard_stamp, src-tauri/src/clipboard.rs), looked at
 * every half second, and only while a tab holds the clipboard.
 *
 * PageHost tells it of each copy ([`ClipboardOwner.copied`]) and lets go when its tab closes ([`release`]); the tab
 * whose copy was taken is told with the text it copied, and its page clears the highlight if it's still that text
 * (terminal.js, "clipboardTaken"), so a selection made since without copying it stays.
 */
import { invoke } from "@tauri-apps/api/core";

/** A tab that can hold the clipboard: told when something else has taken it */
export interface Holder {
  /** Something else was copied after this tab's `text` */
  taken(text: string): void;
}

/** How often the clipboard's change number is looked at while a tab holds it, in milliseconds */
export const WATCH_MS = 500;

/** Keeps track of the tab whose copy is on the clipboard */
export class ClipboardOwner {
  /** The tab whose copy is on the clipboard, the text it copied, and the clipboard's number just after */
  private held: { holder: Holder; text: string; stamp: number | null } | null = null;
  /** The half-second look at the clipboard, while a tab holds it */
  private timer: ReturnType<typeof setInterval> | null = null;
  /** Counts copies, so a look that began before the latest copy doesn't take it for another app's */
  private copies = 0;

  /**
   * @param stamp The clipboard's change number now (null when it can't be read, which is never taken for a change)
   * @param every How often to look, in milliseconds
   */
  constructor(private stamp: () => Promise<number | null>, private every = WATCH_MS) {}

  /**
   * A tab has just put `text` on the clipboard. The tab that held it before, if another, is told it was taken.
   * Call once the text is on the clipboard, so the number read is the one the copy left.
   */
  async copied(holder: Holder, text: string): Promise<void> {
    const before = this.held;
    const copy = ++this.copies;
    this.held = { holder, text, stamp: null };
    if (before && before.holder !== holder) before.holder.taken(before.text);
    const stamp = await this.stamp();
    if (copy !== this.copies || this.held?.holder !== holder) return;
    this.held.stamp = stamp;
    this.watch();
  }

  /** A tab is closing: it holds the clipboard no more, and the clipboard isn't looked at for it */
  release(holder: Holder): void {
    if (this.held?.holder !== holder) return;
    this.held = null;
    this.stop();
  }

  /** Look at the clipboard's change number every so often, while a tab holds it */
  private watch(): void {
    if (this.timer) return;
    this.timer = setInterval(() => void this.look(), this.every);
  }

  /** Stop looking at the clipboard */
  private stop(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  /** Whether the clipboard has changed since the copy; if it has, its tab is told and lets go */
  async look(): Promise<void> {
    const held = this.held;
    const copy = this.copies;
    if (!held || held.stamp === null) return;
    const now = await this.stamp();
    // a copy in this app since, or the tab closed: that copy is the one to watch now
    if (copy !== this.copies || this.held !== held || now === null || now === held.stamp) return;
    this.held = null;
    this.stop();
    held.holder.taken(held.text);
  }
}

/** The clipboard's change number, from the system (the Mac's change count, Windows' sequence number, on Linux the text's) */
async function systemStamp(): Promise<number | null> {
  return invoke<number>("clipboard_stamp").catch(() => null);
}

/** The app's one, shared by every tab */
export const clipboardOwner = new ClipboardOwner(systemStamp);
