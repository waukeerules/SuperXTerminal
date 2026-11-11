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
import { isDark, settings } from "../settings/store.svelte";
import { keySequence, parseSendKey } from "../scripting/keys";
import type { ScriptTarget } from "../scripting/spsl";
import { askChoice, askOne } from "../scripting/asking.svelte";
import { allPrefs, onPrefsChanged, pref, setPrefs } from "./prefs";
import { DEFAULT_PROFILE, listProfiles, profileColours, profileFont, profileKeys, profileScrollback, PROFILES_CHANGED, resolvedProfile, saveProfile, type Forward } from "../settings/profiles";
import { holdTunnels, profileTunnels, releaseTunnels, sameTunnels, tunnelNotices } from "./tunnels";
import { reconnectDelay, retries, sshFailure, type SshFailure } from "./sshFailure";
import { antiIdleKeys } from "./antiIdle";
import { running } from "../commands/sending.svelte";
import { checkPaste } from "./pasting.svelte";
import { MONOSPACE_FONTS, SERIAL_DEVICES, SYSTEM_KIND, THIS_COMPUTER, type System } from "../platform";
import { clipboardText } from "../clipboard";
import { isMac, loadShortcuts, shortcutLabel } from "../shortcuts";
import { listen } from "@tauri-apps/api/event";
import type { MenuItem } from "../ui/Menu.svelte";

/** What the Rust core sends on a tab's channel (`PtyEvent`) */
type PtyEvent =
  | { type: "data"; data: string }
  | { type: "exit"; code: number | null }
  | { type: "notice"; text: string; kind: string }
  | { type: "started"; local: boolean; look: Look; link: string };

/** The profile's cursor, bell, colours and font for the page (`Look` in `src-tauri/src/launch.rs`) */
interface Look {
  cursor: string;
  blink: boolean;
  bell: string;
  /** The session's or its folder's highlighting rules; "" for the default */
  rules: string;
  /** The profile the session uses; absent for a tab with none (a local shell's is Default Settings) */
  profile?: string;
  /** The theme the profile's Colours choice names ("Dark" or "Light"); "" for Settings > Terminal's */
  theme?: string;
  /** The profile's colours ("#rrggbb"); "" for Settings > Terminal's or the theme's */
  background?: string;
  foreground?: string;
  selection?: string;
  cursorColor?: string;
  /** The profile's tab colour ("#rrggbb"), else Default Settings'; "" or absent for none */
  tabColor?: string;
  /** The profile's font, else Default Settings'; "" or absent for the theme's */
  fontFamily?: string;
  /** The profile's font size in points, else Default Settings'; 0 or absent for the theme's */
  fontSize?: number;
  /** What Backspace, Home/End and F1-F12 send (the profile's, else Default Settings', else PuTTY's); absent: PuTTY's */
  keys?: { backspace: string; homeEnd: string; functionKeys: string };
  /** Lines kept to scroll back through (the profile's, else Default Settings'); 0 or absent for Settings > Terminal's */
  scrollback?: number;
  /** Whether the tab's log is kept (the profile's Session log, else Default Settings'); null or absent: Settings > General's */
  keepLog?: boolean | null;
  /** The profile's anti-idle string, as KiTTY writes it; "" or absent for none */
  antiIdle?: string;
  /** Seconds without typing before it's typed; 0 with no string */
  antiIdleSeconds?: number;
  /** The tunnels ssh was given (the profile's, launch.rs's forwarding); absent or none for anything but SSH */
  tunnels?: Forward[];
}

/** What a tab runs (`Target` in `src-tauri/src/pty.rs`) */
export type Target =
  | { kind: "localShell" }
  | { kind: "session"; store: "saved" | "private" | "database"; id: string }
  /** A profile's key fetch, run in a session's tab before it connects (never a tab of its own) */
  | { kind: "keyFetch"; profile: string }
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

/** One row of the page's right-click menu as it sends it ("menu"): a separator, or an item that may hold a submenu */
type PageMenuItem = { separator: true } | { label: string; key?: string; enabled?: boolean; checked?: boolean; items?: PageMenuItem[] };

/**
 * The page's right-click menu as the window's own Menu draws it. Choosing an item tells the page which (its path:
 * the item's place in the menu, then in its submenu), and the page runs it, as its own menu would have
 * @param items The rows the page sent
 * @param choose Tell the page an item was chosen
 * @param path Where these rows are: [] for the menu, [n] for the submenu of its nth row
 */
export function windowMenu(items: readonly PageMenuItem[], choose: (path: number[]) => void, path: number[] = []): MenuItem[] {
  return items.map((item, index) => {
    if ("separator" in item) return { separator: true };
    const at = [...path, index];
    return {
      label: item.label,
      key: item.key || undefined,
      disabled: item.enabled === false,
      checked: item.checked,
      items: item.items ? windowMenu(item.items, choose, at) : undefined,
      action: item.items ? undefined : () => choose(at),
    };
  });
}

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
 * The keys the terminal's menu shows for Copy, Paste and Find, as this app has them: the Mac's ⌘C and ⌘V (the menu
 * bar's), and Find's key in Settings > Shortcuts ("" for none). On Windows Copy and Paste keep PuTTY's Ctrl+Insert
 * and Shift+Insert, which work there (as Ctrl+Shift+C and V do)
 */
function menuKeyLabels(): Record<string, string> {
  const find = shortcutLabel("find");
  return isMac ? { copy: "⌘C", paste: "⌘V", find } : { find };
}

/** How long a new tab's log waits for its profile's choice before going by Settings > General's */
const LOOK_WAIT_MS = 2000;

/**
 * A stand-in theme for when no theme files are found (SuperXPuTTY's defaults are missing from its Git repository):
 * the look the terminal has without one. The page applies colours, the session's cursor colour included, only
 * through a theme, so with none the cursor stayed white. It has no highlighting styles, so rules colour nothing.
 */
const STAND_IN_THEMES = [{ name: "PuTTY", base: "vs-dark", colors: { background: "#000000", foreground: "#FFFFFF" } }];


/** The bell's sound: a short tone (Windows plays its default beep) */
function beep(): void {
  const audio = new AudioContext();
  const tone = audio.createOscillator();
  const volume = audio.createGain();
  tone.frequency.value = 880;
  volume.gain.setValueAtTime(0.15, audio.currentTime);
  volume.gain.exponentialRampToValueAtTime(0.001, audio.currentTime + 0.15);
  tone.connect(volume).connect(audio.destination);
  tone.start();
  tone.stop(audio.currentTime + 0.15);
  tone.onended = () => void audio.close();
}

/** The Special Commands a connection can send, as PuTTY's menu: Telnet's, a serial Break; ssh and Raw none */
export function specialCommands(link: string): { label: string; command: string }[] {
  if (link === "serial") return [{ label: "Break", command: "break" }];
  if (link !== "telnet") return [];
  return [
    ["Break", "break"], ["Are You There", "ayt"], ["Synch", "synch"], ["Erase Character", "ec"], ["Erase Line", "el"],
    ["Go Ahead", "ga"], ["No Operation", "nop"], ["Abort Process", "abort"], ["Abort Output", "ao"],
    ["Interrupt Process", "interrupt"], ["Suspend Process", "susp"], ["End Of Record", "eor"], ["End Of File", "eof"],
  ].map(([label, command]) => ({ label, command }));
}

/** What the core's error starts with when a serial session's COM name needs a device (launch::SERIAL_DEVICE_NEEDED) */
const SERIAL_DEVICE_NEEDED = "SERIAL_DEVICE:";

/** What it starts with when an SSH session has no username and its profile asks for one (launch::USERNAME_NEEDED) */
const USERNAME_NEEDED = "USERNAME:";

/** What it starts with when the profile's key must be fetched first, then the profile and why (launch::KEY_FETCH_NEEDED) */
const KEY_FETCH_NEEDED = "KEY_FETCH:";

/**
 * Key fetches running in this window, by profile: tabs opening together with one profile share its fetch (one sign-in),
 * each waiting for whether it worked
 */
const keyFetches = new Map<string, Promise<boolean>>();

/**
 * How long a session must go on after its first output with text in it (hasText) to count as connected (the far end answering): an ssh that
 * fails at once ("Network is unreachable", "Connection refused") prints its message and ends within a few
 * milliseconds, well inside this, so its tab and Sessions tree entry go from connecting straight to disconnected,
 * without a moment of connected between. Short on the Mac, so a good connection's yellow turns green without a felt
 * lag (it was a second, which showed). On Windows the end comes later: ConPTY keeps ssh's output open after it exits,
 * so the core finds the end by looking every 250 ms and then leaving 500 ms for the last output (EXIT_POLL and
 * EXIT_GRACE in pty.rs). Inside that, a reconnect that timed out went green between ssh's "Connection timed out" and
 * its end; so on Windows it's longer than both, with a little over for the message to arrive. Linux's pseudo terminal
 * ends as the Mac's does, so it waits as the Mac does ([`conptyWaits`])
 */
const ANSWERED_AFTER_MS = conptyWaits(SYSTEM_KIND) ? 1000 : 300;

/**
 * Whether a system's tabs run in Windows' pseudo console (ConPTY), and so need its workarounds: a later "connected",
 * the size settled before a tab opens and held until the session's first text (it printed ssh's first line twice
 * otherwise). Windows only: the Mac and Linux have a Unix pseudo terminal, which takes a resize at any time and ends
 * when the program does
 * @param system which system
 */
export function conptyWaits(system: System): boolean {
  return system === "windows";
}

/**
 * Whether output holds text, not only terminal codes: what's left once escape sequences (a window title's included,
 * whose words would otherwise look like text), control characters and spaces are taken out. Windows' pseudo console
 * (ConPTY) sends codes of its own the moment ssh starts (modes, a cursor position query, a cleared screen, ssh.exe as
 * the title), before ssh has reached anything; counted as the far end answering, a tab reconnecting on Windows went
 * green while ssh was still trying to connect
 */
export function hasText(output: string): boolean {
  const text = output
    // OSC (a window title...), to BEL or ST
    .replace(/\x1b\][\s\S]*?(?:\x07|\x1b\\)/g, "")
    // CSI (modes, a cursor position query, clearing, colours)
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
    // DCS, SOS, PM and APC strings, to ST
    .replace(/\x1b[PX^_][\s\S]*?\x1b\\/g, "")
    // the other escapes: a character set, keypad modes, a reset
    .replace(/\x1b[ -/]*[0-~]/g, "")
    .replace(/[\x00-\x20\x7f]/g, "");
  return text.length > 0;
}

/** How long a new tab waits for the page to give its size once it has its settings, before opening at the one it has */
const SETTLE_SIZE_MS = 200;
/** And a moment more after that, for a second change the first brings (the height after the width) */
const SETTLE_SIZE_AFTER_MS = 30;

/** A session that stayed up this long starts its reconnect attempts again (TerminalControl.StableAfter) */
const STABLE_AFTER_MS = 60_000;

/** How often a tab looks whether its anti-idle string is due, at most: its seconds are counted to within this */
const ANTI_IDLE_TICK_MS = 5_000;

/** How much of the latest output is kept to read how a session ended */
const TAIL_LIMIT = 4096;

/** How much finished output is kept for WAITFOR; half is dropped when it's exceeded, as TerminalControl */
const RECENT_OUTPUT_LIMIT = 256 * 1024;

/** A terminal tab's host: the session behind the tab's page, its log, its reconnects and what scripts and Send Commands type into it */
export class PageHost implements ScriptTarget {
  /** Finished lines kept for WAITFOR, "\n" after each; `recentStart` counts what's been dropped from the front */
  private recent = "";
  private recentStart = 0;
  /** Characters of finished lines ever logged */
  private committedTotal = 0;
  /** The cursor's line as the page last showed it */
  private currentLine = "";
  /** Where WAITFOR starts looking, counted like committedTotal */
  private mark = 0;
  /**
   * The end of the output since the session last started, as it came (before the page shows it): how it ended is
   * read from it, in step with the end itself, where the page's finished lines can still be on their way
   */
  private tail = "";
  /** A change to the profile that may make the next connection work, offered after a failure: F makes it */
  private fix: { profile: string; change: "olderAlgorithms" | "offerKeys" } | null = null;
  /** What the profile's anti-idle string types (antiIdle.ts), and after how long without typing; "" and 0 for none */
  private antiIdle = { keys: "", ms: 0 };
  /** When something was last typed into the session: by the user, a button, the command bar or a script */
  private typedAt = Date.now();
  /** Looks every few seconds whether the anti-idle string is due */
  private antiIdleTimer: ReturnType<typeof setInterval> | null = null;
  /** The program ended: nothing more can be typed */
  private ended = false;
  /**
   * The far end has answered: the session printed something and was still running ANSWERED_AFTER_MS later. Until
   * then it's connecting (its program may be running, as ssh is while it reaches the host, without being connected)
   */
  private answered = false;
  /** Waiting to see the session still running after its first output, to count it answered */
  private answeredTimer: ReturnType<typeof setTimeout> | null = null;
  /** What the connection is: "pty", or a stream's "telnet", "raw" or "serial" (pty_open's started event) */
  private link = "pty";
  /** The tab's session log (SessionLog): its id for writing, its file, and whether it goes when the tab closes */
  private log: { id: number; path: string; temporary: boolean } | null = null;
  /** When the last time stamp went in the log; 0 for none yet, so the first write is stamped */
  private lastLogStamp = 0;
  /** Characters of finished output so far, and when the last came (Watch for Activity and Silence) */
  outputLength = 0;
  lastOutputAt = Date.now();
  /** When the session last started, and how many reconnects have run since it last stayed up (TerminalControl) */
  private startedAt = 0;
  private reconnectAttempts = 0;
  /** The automatic reconnect waiting to run */
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private markStarted: () => void = () => {};
  /** Settles once the tab's program has started (a session's own script waits for it) */
  readonly started = new Promise<void>((resolve) => (this.markStarted = resolve));
  /** The pseudo terminal's id, once it's open */
  private ptyId: number | null = null;
  /** The username typed at "login as:", for this tab's connection and its reconnects; null until asked */
  private loginAs: string | null = null;
  /** What's been typed at "login as:" so far, while the tab asks; null when it isn't asking */
  private typingLogin: string | null = null;
  /** The key fetch running in this tab: told how it ended (its exit code), instead of the tab ending */
  private fetching: ((code: number | null) => void) | null = null;
  /** A key fetch just finished for this open, so a key still wanting one is a problem, not a reason to fetch again */
  private fetchedKey = false;
  /** Move to New Window: the running session this tab takes over instead of starting one */
  private adoptId: number | null = null;
  /** The session went to another window's tab: closing this one leaves it running */
  private released = false;
  /** Output waiting for the page to acknowledge the chunk before it */
  private queue: string[] = [];
  /** A chunk has been sent and not yet acknowledged */
  private waiting = false;
  /** Whether the tab is in front (setShown); the page is told again once it has loaded */
  private visible = true;
  /** Settles once the page has its settings; output is held until then */
  private configured: Promise<void> | null = null;
  /** Notices from outside the tab waiting to go before its first output (notice) */
  private pendingNotices: PageMessage[] = [];
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
   * @param onExit called when the tab should close: a local shell ended, or a session ended cleanly (a logout), as
   *   SuperXPuTTY's TerminalControl. Any other end keeps the tab, with its output, and Enter reconnects
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
    private readonly onState: () => void = () => {},
  ) {
    window.addEventListener("message", this.onWindowMessage);
    this.stopPrefs = onPrefsChanged((prefs) => this.post({ type: "prefs", prefs }));
    // Settings > Shortcuts changed: the terminal's menu shows the new keys
    const stopKeys = listen("settings-changed", async () => {
      await loadShortcuts();
      this.post({ type: "keyLabels", keyLabels: menuKeyLabels() });
    });
    this.stopKeys = () => void stopKeys.then((stop) => stop());
    window.addEventListener(PROFILES_CHANGED, this.onProfilesChanged);
  }

  private readonly stopPrefs: () => void;
  /** Stop following Settings > Shortcuts for the menu's keys */
  private readonly stopKeys: () => void;

  /** The profile the session uses, once it has started; null for none */
  private profile: string | null = null;

  /** The profile the session uses, once it has started (the button bar shows its bar); "" for none */
  get profileName(): string {
    return this.profile ?? "";
  }

  /**
   * The profile's tab colour ("#rrggbb"), for the tab's label when the session has no environment colour; null for
   * none, or before the session has started
   */
  tabColor: string | null = null;

  /**
   * The tunnels the session's ssh opened (Settings > Profiles > Tunnels), as the core gave them when it started: the
   * tab's tunnel icon and Tunnels… show them. None before it starts, and for anything but SSH
   */
  tunnels: Forward[] = [];

  /**
   * Those of its tunnels whose port another of this window's tabs held already, so its ssh couldn't open them: their
   * listen, and that tab's title (tunnels.ts's holdTunnels)
   */
  tunnelsTaken: Map<string, string> = new Map();

  /**
   * The profile's tunnels were changed after the session started: ssh can't add or drop one on a connection it has
   * made, so they take effect when the session restarts, which the tab and Tunnels… say
   */
  tunnelsChanged = false;

  /**
   * A profile was saved or removed: the tab takes its profile's colours, font, keys and scrollback again, its tab's
   * colour included (its log stays kept or temporary as it began)
   */
  private readonly onProfilesChanged = (): void => {
    const name = this.profile;
    if (name === null) return;
    void listProfiles().then((profiles) => {
      const { theme, tab, ...colors } = profileColours(profiles, name);
      this.post({ type: "colors", colors, theme });
      this.post({ type: "profileFont", font: profileFont(profiles, name) });
      this.post({ type: "keys", keys: profileKeys(profiles, name) });
      this.post({ type: "profileScrollback", lines: profileScrollback(profiles, name) });
      this.tabColor = tab || null;
      // an SSH session's tunnels, as its profile now gives them: different from what ssh opened, a restart uses them
      if (this.link === "pty" && !this.local && !this.ended) {
        const changed = !sameTunnels(profileTunnels(resolvedProfile(profiles, name)), this.tunnels);
        if (changed && !this.tunnelsChanged) {
          this.post({ type: "notice", text: "The profile's tunnels changed: Restart session (in the tab's menu) to use them.", kind: "info" });
        }
        this.tunnelsChanged = changed;
      }
      this.onState();
    }).catch(() => {});
  };

  /** Close the pseudo terminal and stop listening; called when the tab closes */
  dispose(): void {
    releaseTunnels(this.tab);
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    if (this.answeredTimer) clearTimeout(this.answeredTimer);
    if (this.antiIdleTimer) clearInterval(this.antiIdleTimer);
    this.stopPrefs();
    this.stopKeys();
    window.removeEventListener(PROFILES_CHANGED, this.onProfilesChanged);
    if (this.log) void invoke("log_close", { id: this.log.id, temporary: this.log.temporary });
    this.log = null;
    window.removeEventListener("message", this.onWindowMessage);
    if (this.ptyId !== null && !this.released) {
      void invoke("pty_close", { id: this.ptyId });
      this.ptyId = null;
    }
  }

  /** The app went light or dark: the page's theme follows it when it's set to (Follow the App's Look) */
  setDark(dark: boolean): void {
    this.post({ type: "look", dark });
  }

  /**
   * The tab came to the front or went behind another tab of its split (TabArea, from dockview). The page is told:
   * behind, it still takes the session's output and logs it, but stops drawing, highlighting and fitting itself to
   * the window, and gives up its WebGL context, until it's in front again (terminal.js setShown). Every tab's page
   * runs on the window's one thread, and a browser keeps about 16 WebGL contexts, so with many tabs open the ones
   * nobody is looking at mustn't work as though they were.
   * @param visible Whether the tab is the one showing in its split
   */
  setShown(visible: boolean): void {
    if (visible === this.visible) return;
    this.visible = visible;
    this.post({ type: "shown", visible });
  }

  /** Send a message to the page */
  private post(message: PageMessage): void {
    this.frame.contentWindow?.postMessage({ channel: "sxt-host", tab: this.tab, message }, "/");
  }

  /** Handle a window message if it's this tab's page talking */
  /**
   * The page's right-click menu, to show at (x, y) in the window. The page is an iframe as big as its tab, so a menu
   * it drew itself was cut off at the tab's edges, which in Grid View is a small tile; the window draws it instead,
   * over everything. Unset (the page's tests), the page draws its own
   */
  onMenu: ((x: number, y: number, items: MenuItem[]) => void) | null = null;

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
        // a tab opened behind another (Connect all, a layout's tabs): the page loads after it was told, so again
        this.post({ type: "shown", visible: this.visible });
        // the page gets its settings once the core says what's starting ("started"), with the profile's look
        await this.open();
        break;
      case "size":
        this.size = { cols: Number(message.cols), rows: Number(message.rows) };
        this.sizeChanged?.();
        if (this.ptyId !== null) await this.resizePty();
        break;
      case "input":
        // the tab asking for a username, before ssh starts
        if (this.typingLogin !== null) {
          this.typeLogin(String(message.data));
          break;
        }
        // while the session is down, Enter reconnects (TerminalControl.OnInput); F makes the fix offered, then reconnects
        if (this.ended) {
          if (this.fix && /^f$/i.test(String(message.data))) await this.makeFix();
          else if (String(message.data).includes("\r")) await this.reconnect();
        } else if (this.ptyId !== null) {
          this.typedAt = Date.now();
          await invoke("pty_write", { id: this.ptyId, data: String(message.data) });
        }
        break;
      case "ack":
        this.waiting = false;
        this.flush();
        break;
      case "copy":
        if (typeof message.text === "string") await navigator.clipboard.writeText(message.text);
        break;
      case "pasteRequest":
        await this.paste(typeof message.text === "string" ? message.text : null);
        break;
      case "pref":
        // a preference changed from the page's own menu: saved, and every tab gets it
        if (typeof message.name === "string") await setPrefs({ [message.name]: message.value == null ? null : String(message.value) });
        break;
      case "bell":
        // the page posts it only for the "sound" bell (a visual bell flashes in the page)
        beep();
        break;
      case "openLink":
        // Ctrl+click on a link: only http and https, in the default browser, as TerminalControl
        if (typeof message.uri === "string" && /^https?:\/\//i.test(message.uri)) {
          await invoke("open_link", { url: message.uri }).catch(() => {});
        }
        break;
      case "special":
        await this.special(String(message.command ?? ""));
        break;
      case "openInPutty":
        this.post({
          type: "notice",
          text: isMac
            ? "There's no PuTTY on the Mac: Connect external, in the session's menu in the Sessions tree, opens it in Terminal."
            : "Open in PuTTY isn't available in SuperXTerminal.",
          kind: "info",
        });
        break;
      case "cleared":
        // Windows' pseudo console keeps its own copy of the screen, which it can't be told to forget; the page drops its
        // redraw after a resize instead (dropConptyRedraw in terminal.js). The Mac's keeps none.
        break;
      case "sendKey":
        await this.sendKey(typeof message.key === "string" ? message.key : null);
        break;
      case "log":
        this.onLog(Array.isArray(message.lines) ? message.lines.map(String) : [], typeof message.current === "string" ? message.current : "");
        break;
      case "tab":
        this.onTabCommand(String(message.command));
        break;
      case "document":
        // the page's Open as Document: Highlighted Output beside the tab
        this.onTabCommand("document");
        break;
      case "menu": {
        // the page's right-click menu, at the pointer in the page: placed in the window by where the tab's frame is
        if (!this.onMenu || !Array.isArray(message.items)) break;
        const frame = this.frame.getBoundingClientRect();
        const items = windowMenu(message.items as PageMenuItem[], (path) => {
          // back to the terminal, as after its own menu, then the page runs the item
          this.frame.focus();
          this.post({ type: "menuChoose", path });
        });
        // headed by the tab's title, so it's plain which terminal was right-clicked, in Grid View especially
        const heading: MenuItem[] = this.title ? [{ label: this.title, heading: true }, { separator: true }] : [];
        this.onMenu(frame.left + Number(message.x ?? 0), frame.top + Number(message.y ?? 0), [...heading, ...items]);
        break;
      }
      // log, document, bell, openInPutty, openLink, pref, cleared: later milestones
    }
  }

  /** Send Key: a menu key ("^C"), or for "Other Key..." one asked for as SENDKEY writes it, as SuperXPuTTY's */
  async sendKey(key: string | null): Promise<void> {
    const written = key ?? (await askOne("Send Key: a key as SENDKEY writes it, e.g. ^C, ESC, F5, ^HOME or {TAB 3}", false));
    if (!written) return;
    try {
      const parsed = parseSendKey(written);
      if (parsed) await this.type(keySequence(parsed.key).repeat(parsed.repeat));
    } catch (error) {
      this.post({ type: "notice", text: String(error instanceof Error ? error.message : error), kind: "error" });
    }
  }

  /** The log's file, for Search Output and Highlighted Output; null until the session starts */
  get logPath(): string | null {
    return this.log?.path ?? null;
  }

  /** Lines the page has finished showing, and the cursor's line: kept for WAITFOR, and written to the session log */
  private onLog(lines: string[], current: string): void {
    if (lines.length) {
      const text = lines.map((l) => l + "\n").join("");
      this.writeLog(text);
      this.outputLength += text.length;
      this.lastOutputAt = Date.now();
      this.recent += text;
      this.committedTotal += text.length;
      if (this.recent.length > RECENT_OUTPUT_LIMIT) {
        const drop = this.recent.length - RECENT_OUTPUT_LIMIT / 2;
        this.recent = this.recent.slice(drop);
        this.recentStart += drop;
      }
    }
    this.currentLine = current;
  }

  /** Add finished lines to the session log: a time stamp first when one is due, CRLF line endings, as TerminalControl */
  private writeLog(text: string): void {
    if (!this.log) return;
    const interval = parseInt(pref("logTimestamps"), 10) || 0;
    const now = new Date();
    let stamp = "";
    if (interval > 0 && now.getTime() - this.lastLogStamp >= interval * 1000) {
      const two = (n: number) => String(n).padStart(2, "0");
      stamp = `--- ${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())} ${two(now.getHours())}:${two(now.getMinutes())}:${two(now.getSeconds())} ---\n`;
      this.lastLogStamp = now.getTime();
    }
    void invoke("log_write", { id: this.log.id, text: (stamp + text).replace(/\n/g, "\r\n") }).catch(() => {});
  }

  /** Type text and key sequences into the session (Send Commands, the button bar, SPSL) */
  async type(text: string): Promise<void> {
    if (!this.canReceiveCommands || !text) return;
    this.typedAt = Date.now();
    await invoke("pty_write", { id: this.ptyId, data: text });
  }

  /**
   * Start sending the profile's anti-idle string (KiTTY's) after `seconds` without typing, and again after each such
   * spell; none when it's "". Counted from now, as the session has just started.
   */
  private startAntiIdle(text: string, seconds: number): void {
    if (this.antiIdleTimer) clearInterval(this.antiIdleTimer);
    this.antiIdleTimer = null;
    const keys = antiIdleKeys(text);
    this.antiIdle = { keys, ms: keys && seconds > 0 ? seconds * 1000 : 0 };
    if (!this.antiIdle.ms) return;
    this.typedAt = Date.now();
    this.antiIdleTimer = setInterval(() => void this.sendAntiIdle(), Math.min(ANTI_IDLE_TICK_MS, this.antiIdle.ms));
  }

  /** Type the anti-idle string if it's due: the session is up, nothing typed for long enough, and no script running here (it would read the echo) */
  private async sendAntiIdle(): Promise<void> {
    if (!this.canReceiveCommands || Date.now() - this.typedAt < this.antiIdle.ms) return;
    if (running.runs.some((r) => r.tabId === this.tab)) return;
    this.typedAt = Date.now();
    await invoke("pty_write", { id: this.ptyId, data: this.antiIdle.keys }).catch(() => {});
  }

  /** Whether the tab's program is running, so commands can be typed into it */
  get canReceiveCommands(): boolean {
    return this.ptyId !== null && !this.ended;
  }

  /**
   * The connection a script is typing into: its pseudo terminal's id, a new one at each reconnect; null while the
   * session is down. A script stops when this changes, rather than type into a dropped or different connection.
   */
  connection(): number | null {
    return this.canReceiveCommands ? this.ptyId : null;
  }

  /** A button's command starts on the cursor's line: the page colours the numbers of its lines (none: "") */
  markCommand(color: string): void {
    this.post({ type: "commandColor", color });
  }

  markOutput(atLineStart: boolean): void {
    this.mark = this.committedTotal + (atLineStart ? 0 : this.currentLine.length);
  }

  outputSinceMark(): string {
    return this.outputFrom(this.mark);
  }

  /** Where the output has got to: the finished lines' characters, then the cursor's line's (a script's SAVE) */
  position(): number {
    return this.committedTotal + this.currentLine.length;
  }

  /** The output since a position, as far back as it's kept (finished lines, then the cursor's line) */
  outputFrom(position: number): string {
    if (position >= this.committedTotal) {
      return this.currentLine.slice(Math.min(this.currentLine.length, position - this.committedTotal));
    }
    const from = Math.max(0, position - this.recentStart);
    return this.recent.slice(from) + this.currentLine;
  }

  advanceMark(characters: number): void {
    this.mark = Math.max(this.mark, this.recentStart) + characters;
  }

  /** Whether the session has ended (the tab menu offers Reconnect) */
  get isEnded(): boolean {
    return this.ended;
  }

  /** Whether it's still connecting: not ended, and the far end hasn't answered yet (the tab's and tree's yellow) */
  get isConnecting(): boolean {
    return !this.ended && !this.answered;
  }

  /**
   * Output came: the first with text in it (hasText; not ConPTY's own codes), after a moment still running, means the
   * far end has answered
   */
  private heard(output: string): void {
    // the session's first text: a size kept from before it goes now
    if (!this.printed && hasText(output)) {
      this.printed = true;
      if (this.sizeHeld) {
        this.sizeHeld = false;
        void this.resizePty();
      }
    }
    if (this.answered || this.answeredTimer || !hasText(output)) return;
    this.answeredTimer = setTimeout(() => {
      this.answeredTimer = null;
      if (this.ended) return;
      this.answered = true;
      this.onState();
    }, ANSWERED_AFTER_MS);
  }

  /** A new connection (the first, or a reconnect): connecting again until its far end answers */
  private notAnswered(): void {
    if (this.answeredTimer) clearTimeout(this.answeredTimer);
    this.answeredTimer = null;
    this.answered = false;
    this.printed = false;
    this.sizeHeld = false;
  }

  /**
   * The session ended some other way than a logout: keep the tab and its output, and reconnect by itself when the
   * settings say so, as TerminalControl.OnExited. A session that stayed up a minute starts its attempts again, and
   * each attempt waits twice as long as the one before (up to two minutes), so a device's reload is ridden out.
   * An ssh failure that reconnecting can't mend (a refused login, an unknown host, older algorithms) says what to do
   * instead, and doesn't reconnect by itself: a wrong password tried again and again can lock the account.
   */
  private async dropped(): Promise<void> {
    if (Date.now() - this.startedAt > STABLE_AFTER_MS) this.reconnectAttempts = 0;
    const failure = this.link === "pty" && !this.local ? sshFailure(this.tail) : ({ kind: "other" } as SshFailure);
    this.fix = null;
    const advice = await this.advice(failure);
    if (advice) this.post({ type: "notice", text: advice, kind: retries(failure) ? "info" : "error" });
    if (!retries(failure)) {
      this.post({ type: "notice", text: this.fix ? "Press F to make that change and reconnect, or Enter to reconnect as it is." : "Press Enter to try again.", kind: "info" });
      return;
    }
    const max = Math.max(1, settings.autoReconnectAttempts);
    const delay = reconnectDelay(Math.max(1, settings.autoReconnectDelaySeconds), this.reconnectAttempts);
    if (settings.autoReconnect && this.reconnectAttempts < max) {
      this.post({
        type: "notice",
        text: `Session ended. Reconnecting in ${delay} s (${this.reconnectAttempts + 1} of ${max}); press Enter to reconnect now.`,
        kind: "info",
      });
      this.reconnectTimer = setTimeout(() => void this.reconnect(), delay * 1000);
    } else {
      this.post({ type: "notice", text: "Session ended. Press Enter to reconnect.", kind: "info" });
    }
    this.onState();
  }

  /** What to tell the user about how the session ended, offering a fix where the profile has one; "" for nothing */
  private async advice(failure: SshFailure): Promise<string> {
    const name = this.profile ?? DEFAULT_PROFILE;
    const inProfile = `in profile "${name}" (Settings > Profiles)`;
    switch (failure.kind) {
      case "login":
        return `Login failed: the username, password or key was refused. Check them ${inProfile}. Not reconnecting by itself, so a wrong password can't lock the account.`;
      case "unknownHost":
        return `Can't find host "${failure.host}": check the session's host name, or the network, VPN or DNS.`;
      case "tooManyAttempts": {
        const profile = await this.profileNamed(name);
        if (profile && profile.offerKeys !== false) this.fix = { profile: name, change: "offerKeys" };
        return this.fix
          ? `The device closed the connection after too many login attempts: ssh offered its keys before the password, and each one counts. Offer SSH keys can be turned off ${inProfile}.`
          : `The device closed the connection after too many login attempts. Check the username and password ${inProfile}.`;
      }
      case "algorithms": {
        const profile = await this.profileNamed(name);
        const older = profile?.olderAlgorithms ?? (await this.profileNamed(DEFAULT_PROFILE))?.olderAlgorithms ?? false;
        if (profile && !older) this.fix = { profile: name, change: "olderAlgorithms" };
        return this.fix
          ? `The device offers only older algorithms: no ${failure.what} in common (it offers ${failure.offer}). Allow older algorithms can be turned on ${inProfile}.`
          : `No ${failure.what} in common with the device (it offers ${failure.offer}), even with older algorithms allowed: add it to the algorithm lists ${inProfile}.`;
      }
      case "refused":
        return `Nothing answered on port ${failure.port}: the device may be restarting, or SSH is off or on another port.`;
      case "unreachable":
        return "No route to the device: check the network or VPN.";
      case "jumpForwarding":
        return `The jump server won't pass the connection on: it doesn't allow TCP forwarding, which a Jump host needs. Turn on AllowTcpForwarding in its /etc/ssh/sshd_config (or TCP forwarding in its SSH settings), or log in through it with a profile that has Log in here.`;
      case "jumpConnect":
        return `The jump server couldn't reach the device (${failure.reason}): check the host and port as the jump server sees them, and its network or firewall.`;
      default:
        return "";
    }
  }

  /** A profile by name, whatever its case; undefined when it isn't there (or the profiles can't be read) */
  private async profileNamed(name: string) {
    const profiles = await listProfiles().catch(() => []);
    return profiles.find((p) => p.name.toLowerCase() === name.trim().toLowerCase());
  }

  /** Make the change offered (F) to the session's profile, and reconnect with it */
  private async makeFix(): Promise<void> {
    const fix = this.fix;
    this.fix = null;
    if (!fix) return;
    const profile = await this.profileNamed(fix.profile);
    if (!profile) {
      this.post({ type: "notice", text: `Profile "${fix.profile}" isn't there any more. Press Enter to reconnect.`, kind: "error" });
      return;
    }
    const changed = fix.change === "olderAlgorithms" ? { ...profile, olderAlgorithms: true } : { ...profile, offerKeys: false };
    try {
      await saveProfile(changed, profile.name);
    } catch (e) {
      this.post({ type: "notice", text: `Couldn't change the profile: ${e}. Press Enter to reconnect.`, kind: "error" });
      return;
    }
    this.post({
      type: "notice",
      text: fix.change === "olderAlgorithms" ? `Allow older algorithms is on in profile "${profile.name}".` : `Offer SSH keys is off in profile "${profile.name}".`,
      kind: "info",
    });
    await this.reconnect();
  }

  /** Whether an automatic reconnect is waiting (the tab menu offers Stop Reconnecting) */
  get isReconnecting(): boolean {
    return this.reconnectTimer !== null;
  }

  /** Cancel the waiting automatic reconnect; Enter still reconnects */
  stopReconnecting(): void {
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    this.post({ type: "notice", text: "Stopped reconnecting. Press Enter to reconnect.", kind: "info" });
    this.onState();
  }

  /** Connect again in the same tab, keeping the scrollback (Reconnect, Restart session) */
  async reconnect(): Promise<void> {
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    this.reconnectAttempts++;
    if (this.ptyId !== null) {
      const old = this.ptyId;
      this.ptyId = null;
      await invoke("pty_close", { id: old });
    }
    this.ended = false;
    this.queue = [];
    this.waiting = false;
    this.post({ type: "notice", text: "Reconnecting...", kind: "info" });
    await this.open();
    this.onState();
  }

  /** One of the page's actions: "copyAll", "clearScrollback", "reset" or "find" */
  action(name: string): void {
    this.post({ type: "action", name });
  }

  /**
   * Edit > Find… (Cmd+F, Ctrl+Shift+F on Windows): the page's own find bar. Asked for as an action, not as the
   * Ctrl+Shift+F the page opens it on: on Windows that's Find's shortcut too, which the app catches in the page
   * (shortcuts.ts), so the key came back here as Find again, round and round, and never reached the page
   */
  find(): void {
    this.action("find");
  }

  /**
   * A notice in the tab from outside it, e.g. a login script that isn't there (App's runOpeningScript), at the top:
   * it's posted just before the connection's first output goes to the page, which can be before the tab hears its
   * program has started, else once it has started and the page has its settings, for a program that's quiet
   * @param kind "warn" (yellow) for something the session goes on without, "error" (red) or "info" (cyan)
   */
  async notice(text: string, kind: "warn" | "error" | "info" = "warn"): Promise<void> {
    const message = { type: "notice", text, kind };
    if (this.settled) {
      this.post(message);
      return;
    }
    this.pendingNotices.push(message);
    await this.started;
    await this.configured;
    this.postPendingNotices();
  }

  /** Post the notices waiting for the page's settings (notice), in the order they came */
  private postPendingNotices(): void {
    const waiting = this.pendingNotices;
    this.pendingNotices = [];
    for (const message of waiting) this.post(message);
  }

  /** Refresh tab: fit the terminal to the tab again and redraw it */
  refresh(): void {
    this.frame.contentWindow?.dispatchEvent(new Event("resize"));
  }

  /** Paste the given text, or the clipboard's, into the terminal */
  private async paste(text: string | null): Promise<void> {
    const value = text ?? (await clipboardText());
    if (!value) return;
    // several lines are checked first (paste check), as TerminalControl
    const checked = await checkPaste(value, this.title);
    this.post({ type: "focus" });
    if (checked) this.post({ type: "paste", text: checked });
  }

  /** The tab's title, for the paste check's question */
  title = "";

  /** The page's settings: the highlighting rules and themes, and the profile's cursor and bell */
  private async sendConfig(look: Look, early = false): Promise<void> {
    if (look.profile !== undefined) this.profile = look.profile;
    this.tabColor = look.tabColor || null;
    this.onState();
    this.startAntiIdle(look.antiIdle ?? "", look.antiIdleSeconds ?? 0);
    const dark = isDark();
    const files = await highlighting();
    this.post({
      type: "config",
      // none yet when it's only for the page to measure itself (settledSize): its skipped rules are said once, after
      ruleSets: early ? [] : files.ruleSets,
      themes: files.themes.length ? files.themes : STAND_IN_THEMES,
      dark,
      bell: look.bell,
      cursor: { style: look.cursor, blink: look.blink },
      local: this.local,
      serial: false,
      // the Special Commands this tab sends itself (the page then offers them instead of Open in PuTTY)
      specials: specialCommands(this.link),
      // no PuTTY to open the session in: where the tab sends none, the page's menu says why, as the tab's menu does
      putty: false,
      prefs: allPrefs(),
      // the system's own monospaced fonts, for the page's font menu
      fonts: MONOSPACE_FONTS,
      // the profile's Backspace, Home/End and function keys (Settings > Profiles > Terminal), else PuTTY's defaults
      keys: look.keys ?? { backspace: "del", homeEnd: "standard", functionKeys: "tilde" },
      bellOverload: { on: true, count: 5, windowMs: 2000, quietMs: 5000 },
      // the profile's font (Settings > Profiles > Font), else Default Settings'; "" and 0: the theme's
      profileFont: { family: look.fontFamily ?? "", points: look.fontSize ?? 0 },
      menuKeys: MENU_KEYS,
      // the window draws the right-click menu, over every tab, rather than the page inside its own frame ("menu")
      hostMenu: this.onMenu !== null,
      keyLabels: (await loadShortcuts(), menuKeyLabels()),
      sessionRules: look.rules,
      unlimitedScrollback: 1000000,
      // the profile's Scrollback (else Default Settings'), over Settings > Terminal's; 0 for Settings > Terminal's
      profileScrollback: look.scrollback ?? 0,
      cursorColor: this.cursorColor,
      profileTheme: look.theme ?? "",
      colors: { background: look.background ?? "", foreground: look.foreground ?? "", selection: look.selection ?? "", cursor: look.cursorColor ?? "" },
    });
  }

  /**
   * Move to New Window: give the session up to another window's tab, without ending it; its id, or null when there's
   * none to give (it never started). The tab can then close.
   */
  release(): number | null {
    if (this.ptyId === null) return null;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.released = true;
    return this.ptyId;
  }

  /** Take over a running session (`id`) from another window's tab, before the page is ready */
  adopt(id: number): void {
    this.adoptId = id;
  }

  /** The channel of the tab's connection now: events from one it had before are its last words, not this one's */
  private channel: Channel<PtyEvent> | null = null;

  /** The channel a session's events come in on: output, how it started, notices and its end */
  private outputChannel(): Channel<PtyEvent> {
    const channel = new Channel<PtyEvent>();
    this.channel = channel;
    channel.onmessage = (event) => {
      // a connection closed for a new one (Reconnect) still tells of its end, after the new one has started: on
      // Windows its exit comes most of a second later. Taken as the new session's, it ended it and reconnected again
      if (channel !== this.channel) return;
      if (event.type === "data") {
        // ssh's messages are plain ASCII, so the bytes as characters are enough to read them
        const bytes = atob(event.data);
        this.tail = (this.tail + bytes).slice(-TAIL_LIMIT);
        this.queue.push(event.data);
        this.flush();
        this.heard(bytes);
      } else if (event.type === "started") {
        // output waits until the page has its settings, so the first lines are highlighted too
        this.link = event.link;
        this.configured = this.sendConfig(event.look);
        this.lookArrived(event.look);
        // what the tunnels make reachable, at the top, with their links: a tunnel is otherwise invisible
        this.tunnels = event.look.tunnels ?? [];
        this.tunnelsChanged = false;
        // a port another tab holds can't be this one's too: its ssh goes on without it, and the tab says whose it is
        this.tunnelsTaken = holdTunnels(this.tab, this.title, this.tunnels);
        for (const { text, kind } of tunnelNotices(this.tunnels, this.tunnelsTaken)) this.post({ type: "notice", text, kind });
      } else if (event.type === "notice") {
        this.post({ type: "notice", text: event.text, kind: event.kind });
      } else if (event.type === "exit") {
        // a key fetch ending isn't the session ending: the tab goes on to connect (or says why it can't)
        if (this.fetching) {
          const done = this.fetching;
          this.fetching = null;
          done(event.code ?? null);
          return;
        }
        this.ended = true;
        // its tunnels closed with it: their ports are another tab's to open
        releaseTunnels(this.tab);
        this.notAnswered();
        // a tab of its own for a key fetch (Log in & cache key): it stays, saying how it went; Enter runs it again
        if (this.target.kind === "keyFetch") {
          this.post(event.code === 0
            ? { type: "notice", text: "The key is cached: sessions using the profile log in with it. Press Enter to fetch again.", kind: "info" }
            : { type: "notice", text: `The key fetch didn't finish${event.code == null ? "" : ` (exit code ${event.code})`}. Press Enter to try again.`, kind: "error" });
          this.onState();
          return;
        }
        // a clean end (a logout) or a local shell closes the tab, as PuTTY's window would
        if (this.local || event.code === 0) this.onExit();
        else void this.dropped();
        this.onState();
      }
    };
    return channel;
  }

  /** Start the tab's program at the page's size and pass its output on */
  private async open(): Promise<void> {
    if (this.ptyId !== null) return;
    this.notAnswered();
    this.tail = "";
    this.fix = null;
    const channel = this.outputChannel();
    if (this.adoptId !== null) {
      // the session another window gave up: it replays how it started and what it has shown, then carries on here
      const id = this.adoptId;
      this.adoptId = null;
      try {
        await invoke("pty_attach", { id, onEvent: channel });
        this.ptyId = id;
        this.startedAt = Date.now();
        await invoke("pty_resize", { id, ...this.size }).catch(() => {});
        if (!this.log) this.log = await this.openLog();
        this.markStarted();
      } catch (error) {
        if (!this.configured) await this.sendConfig({ cursor: "block", blink: true, bell: "visual", rules: "" });
        this.post({ type: "notice", text: String(error), kind: "error" });
        this.ended = true;
        this.onState();
      }
      return;
    }
    try {
      await this.openPty({ target: this.target, onEvent: channel, loginAs: this.loginAs });
      this.fetchedKey = false;
      this.startedAt = Date.now();
      // one log for the tab, kept across reconnects, as SuperXPuTTY's
      if (!this.log) this.log = await this.openLog();
      this.markStarted();
    } catch (error) {
      let text = String(error);
      // the profile's key is missing or too old: fetch it in this tab (its sign-in shows here), then connect
      if (text.startsWith(KEY_FETCH_NEEDED)) {
        const [profile, ...why] = text.slice(KEY_FETCH_NEEDED.length).split("\n");
        const reason = why.join(" ");
        if (!this.fetchedKey) {
          await this.fetchKey(profile, reason);
          return;
        }
        // fetched, and still not good: say so rather than fetch again and again
        this.fetchedKey = false;
        text = `The key fetch finished, but ${reason.replace(/: fetching a new one\.$/, "").replace(/^./, (c) => c.toLowerCase())}. Check profile "${profile}"'s Fetch command and Key file. Press Enter to try again.`;
      }
      // an SSH session with no username, whose profile asks for one: "login as:" in the tab, as PuTTY and SecureCRT
      if (text.startsWith(USERNAME_NEEDED)) {
        // kept as configured, as output (the prompt) waits for it; notices are posted straight to the page
        if (!this.configured) this.configured = this.sendConfig({ cursor: "block", blink: true, bell: "visual", rules: "" });
        await this.configured;
        this.askLogin();
        return;
      }
      // a serial session's COM name with no device yet: asked once, remembered, and tried again
      if (text.startsWith(SERIAL_DEVICE_NEEDED)) {
        const name = text.slice(SERIAL_DEVICE_NEEDED.length);
        if (await this.chooseSerialDevice(name)) {
          await this.open();
          return;
        }
        text = `${name} isn't matched to a serial device on ${THIS_COMPUTER} yet. Press Enter to choose one.`;
      }
      // nothing started, so the page has had no settings yet
      if (!this.configured) await this.sendConfig({ cursor: "block", blink: true, bell: "visual", rules: "" });
      this.post({ type: "notice", text, kind: "error" });
      // the tab stays; Enter tries again
      this.ended = true;
      this.onState();
    }
  }

  /**
   * Fetch a profile's key in this tab, then open the session: the fetch's own output and prompts (CyberArk's link to
   * Okta, the PIN) show here, and what's typed goes to it. Another tab already fetching for the profile is waited for.
   */
  private async fetchKey(profile: string, reason: string): Promise<void> {
    if (!this.configured) this.configured = this.sendConfig({ cursor: "block", blink: true, bell: "visual", rules: "" });
    await this.configured;
    this.post({ type: "notice", text: reason, kind: "info" });
    const key = profile.toLowerCase();
    let ok: boolean;
    const running = keyFetches.get(key);
    if (running) {
      this.post({ type: "notice", text: `Waiting for the key fetch in another tab (profile "${profile}")…`, kind: "info" });
      ok = await running;
    } else {
      const fetch = this.runKeyFetch(profile);
      keyFetches.set(key, fetch);
      try {
        ok = await fetch;
      } finally {
        keyFetches.delete(key);
      }
    }
    if (ok) {
      this.fetchedKey = true;
      await this.open();
    } else {
      this.ended = true;
      this.onState();
    }
  }

  /**
   * Open the tab's pseudo terminal at the page's size, and pass on a size the page gave while it was opening: the
   * page measures itself once it's configured, which the "started" event does while pty_open is still running, so its
   * size used to be kept but never sent. On Windows ConPTY then drew for 80x24 with its own cursor moves, putting the
   * prompt and the ends of long lines in the middle of the output; on the Mac the host only wrapped lines early.
   * @param args pty_open's arguments but the size
   */
  /** Called with the look as the session's "started" event comes (set by `look` below, so declared before it) */
  private lookArrived: (look: Look) => void = () => {};
  /** The look the session started with, once its "started" event has come (for the log's Keep); see [`openLog`] */
  private look: Promise<Look | null> = new Promise((resolve) => (this.lookArrived = resolve));

  /**
   * Start the tab's log, kept or temporary as its profile's Session log says, else as Settings > General does. The
   * profile's choice comes with the session's "started" event, which can come just after pty_open answers: wait for
   * it a moment, then go by Settings > General
   */
  private async openLog(): Promise<{ id: number; path: string; temporary: boolean } | null> {
    const look = await Promise.race([this.look, new Promise<null>((resolve) => setTimeout(() => resolve(null), LOOK_WAIT_MS))]);
    return invoke<{ id: number; path: string; temporary: boolean }>("log_open", { name: this.title, keep: look?.keepLog ?? null }).catch(() => null);
  }

  /** Told when the page gives a new size, while settledSize waits for it */
  private sizeChanged: (() => void) | null = null;

  /**
   * On Windows, before the pseudo terminal first opens: the page's settings (Wrap Lines off's width, the theme's font,
   * the window's height), so it measures itself and ConPTY opens at the size it keeps. It used to open at the page's
   * first guess and be resized as the page settled, while ssh was still connecting, and ConPTY then printed ssh's first
   * line twice ("Last login" on two lines), about a third of the time: seen in captures of the app's tabs and in ssh
   * logins through ConPTY, never with no resize. The profile's own look comes with the "started" event as before;
   * only a profile's own font can change the size again, and that resize waits for the session's text (resizePty).
   * Not on the Mac or Linux, whose pseudo terminal takes a resize at any time, nor for a tab already configured (a
   * reconnect)
   */
  private async settledSize(): Promise<void> {
    if (!conptyWaits(SYSTEM_KIND) || this.configured) return;
    const changed = new Promise<void>((resolve) => {
      this.sizeChanged = resolve;
      setTimeout(resolve, SETTLE_SIZE_MS);
    });
    await this.sendConfig({ cursor: "block", blink: true, bell: "visual", rules: "" }, true);
    await changed;
    // the height can follow the width a moment later (the bar along the bottom for Wrap Lines off)
    await new Promise((resolve) => setTimeout(resolve, SETTLE_SIZE_AFTER_MS));
    this.sizeChanged = null;
  }

  private async openPty(args: { target: Target; onEvent: ReturnType<PageHost["outputChannel"]>; loginAs: string | null }): Promise<void> {
    await this.settledSize();
    const opening = { ...this.size };
    this.ptyId = await invoke<number>("pty_open", { ...args, ...opening });
    if (this.size.cols !== opening.cols || this.size.rows !== opening.rows) await this.resizePty();
  }

  /**
   * Whether the session has printed any text since it (re)started, and whether a resize is waiting for that. On
   * Windows the pseudo console (ConPTY) printed the program's first line twice ("Last login: ..." on two lines) about
   * a third of the time when it was resized before the program had printed anything, as every tab is in its first
   * moments (Wrap Lines off's width, the height settling). Seen in captures of ssh logins through ConPTY, 8 of 8 clean
   * with no resize or one after the first text. So until then the size is kept, and sent once text comes
   */
  private printed = false;
  private sizeHeld = false;

  /** Tell the pseudo terminal the page's size now, or, on Windows before the session's first text, once it comes */
  private async resizePty(): Promise<void> {
    if (this.ptyId === null) return;
    if (conptyWaits(SYSTEM_KIND) && !this.printed) {
      this.sizeHeld = true;
      return;
    }
    await invoke("pty_resize", { id: this.ptyId, ...this.size }).catch(() => {});
  }

  /** Run the profile's key fetch here; whether it ended well (exit code 0) */
  private async runKeyFetch(profile: string): Promise<boolean> {
    const code = new Promise<number | null>((resolve) => (this.fetching = resolve));
    try {
      await this.openPty({ target: { kind: "keyFetch", profile } satisfies Target, onEvent: this.outputChannel(), loginAs: null });
    } catch (error) {
      this.fetching = null;
      this.post({ type: "notice", text: `${String(error)} Press Enter to try again.`, kind: "error" });
      return false;
    }
    const exit = await code;
    // the fetch's pseudo terminal is done with; the session gets its own
    if (this.ptyId !== null) {
      const id = this.ptyId;
      this.ptyId = null;
      await invoke("pty_close", { id }).catch(() => {});
    }
    if (exit !== 0) {
      this.post({ type: "notice", text: `The key fetch didn't finish${exit === null ? "" : ` (exit code ${exit})`}. Press Enter to try again.`, kind: "error" });
      return false;
    }
    return true;
  }

  /** Show "login as:" and read the username from what's typed in the tab (typeLogin) */
  private askLogin(): void {
    this.typingLogin = "";
    this.ended = false;
    this.show("login as: ");
  }

  /**
   * Typing at "login as:": letters are shown as typed, Backspace and Ctrl+U take them back, Enter connects as that
   * user (an empty line asks again), Ctrl+C gives up until Enter; keys that send sequences (arrows) are ignored
   */
  private typeLogin(data: string): void {
    if (data.startsWith("\x1b")) return;
    for (const ch of data) {
      const typed = this.typingLogin ?? "";
      if (ch === "\r" || ch === "\n") {
        this.show("\r\n");
        const name = typed.trim();
        if (!name) {
          this.askLogin();
          return;
        }
        this.typingLogin = null;
        this.loginAs = name;
        void this.open().then(() => this.onState());
        return;
      } else if (ch === "\x7f" || ch === "\b") {
        if (typed) {
          this.typingLogin = [...typed].slice(0, -1).join("");
          this.show("\b \b");
        }
      } else if (ch === "\x15") {
        this.show("\b \b".repeat([...typed].length));
        this.typingLogin = "";
      } else if (ch === "\x03") {
        this.typingLogin = null;
        this.show("^C\r\n");
        this.post({ type: "notice", text: "Not connected. Press Enter to try again.", kind: "info" });
        this.ended = true;
        this.onState();
        return;
      } else if (ch >= " ") {
        this.typingLogin = typed + ch;
        this.show(ch);
      }
    }
  }

  /** Show text in the terminal as if the program had written it (the "login as:" prompt and what's typed at it) */
  private show(text: string): void {
    let binary = "";
    for (const byte of new TextEncoder().encode(text)) binary += String.fromCharCode(byte);
    this.queue.push(btoa(binary));
    this.flush();
  }

  /**
   * Ask which device a COM name is on this Mac or Linux computer (/dev/cu.*, /dev/ttyUSB*...), and remember it; false
   * if there's none or it's cancelled
   */
  private async chooseSerialDevice(name: string): Promise<boolean> {
    const devices = await invoke<string[]>("serial_devices_list");
    if (devices.length === 0) {
      this.post({ type: "notice", text: `${name}: no serial device is plugged in (none in ${SERIAL_DEVICES}).`, kind: "error" });
      return false;
    }
    const device = await askChoice(`Which serial device is ${name} on ${THIS_COMPUTER}?`, `${name}:`, devices);
    if (!device) return false;
    await invoke("serial_device_set", { name, device });
    return true;
  }

  /** SPSL's SENDBREAK: a serial or Telnet Break; throws when this tab can't send one */
  async sendBreak(): Promise<void> {
    if (this.ptyId === null || this.ended) throw new Error("the session isn't connected");
    await invoke("pty_special", { id: this.ptyId, command: "break" }).catch((e) => {
      throw new Error(`SENDBREAK: ${String(e)}`);
    });
  }

  /** A serial tab's line settings now, as -sercfg ("9600,8,n,1,X"); null if it isn't connected */
  async serialLine(): Promise<string | null> {
    if (this.ptyId === null || this.ended) return null;
    return invoke<string>("pty_serial_get", { id: this.ptyId });
  }

  /** Change a serial tab's line settings while it's open */
  async setSerialLine(sercfg: string): Promise<void> {
    if (this.ptyId === null || this.ended) throw new Error("The session isn't connected");
    await invoke("pty_serial_set", { id: this.ptyId, sercfg });
    this.post({ type: "notice", text: `Serial line now ${sercfg}`, kind: "info" });
  }

  /** A Special Command (Telnet's Break, Are You There...; a serial Break) */
  async special(command: string): Promise<void> {
    if (this.ptyId === null) return;
    await invoke("pty_special", { id: this.ptyId, command }).catch((e) => this.post({ type: "notice", text: String(e), kind: "error" }));
  }

  /** Send the next chunk of output if the page has drawn the last one and has its settings */
  private flush(): void {
    if (this.waiting || this.queue.length === 0) return;
    if (!this.configured) return;
    if (!this.settled) {
      void this.configured.then(() => {
        this.settled = true;
        this.postPendingNotices();
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
