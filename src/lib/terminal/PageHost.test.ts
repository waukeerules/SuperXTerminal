// A session's tab when ssh ends: what it says and whether it reconnects by itself, read from ssh's last words, and
// the profile change it offers (F). The Rust side is stood in for by mockIPC: pty_open hands over the tab's channel,
// which the test feeds ssh's output and its end. Also the tab's size reaching the pseudo terminal as the session starts,
// Reconnect, connecting until the far end answers (not ConPTY's own output), the anti-idle string typed after a quiet spell, a paste taking the clipboard from the system, its log kept as its profile says, and Find
// (Ctrl+Shift+F) opening the page's find bar on Windows, where the app catches that key in the page itself.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Channel } from "@tauri-apps/api/core";
import { PageHost, hasText } from "./PageHost";
import { catchShortcuts, loadShortcuts } from "../shortcuts";
import { DEFAULT_PROFILE, emptyProfile, type Profile } from "../settings/profiles";
import { settings } from "../settings/store.svelte";
import { running } from "../commands/sending.svelte";
import type { ScriptRun } from "../scripting/spsl";

/** The tab's channel, once pty_open has been asked for it */
let channel: Channel<unknown> | null;
/** How many times pty_open was asked: a reconnect asks again */
let opened: number;
/** The profiles the tab saved (its profile change) */
let saved: Profile[];
/** The profiles stored, as profiles_list gives them */
let profiles: Profile[];
/** The iframe standing in for the tab's page */
let frame: HTMLIFrameElement;
/** What the host sent the page */
let posted: { type: string; text?: string }[];
/** The host under test, let go of after each test */
let host: PageHost | null;
/** What was typed into the session (pty_write), in order */
let written: string[];
/** pty_resize's sizes, in order */
let resized: { cols: number; rows: number }[];
/** pty_open's size, and what it waits for before it answers (the page's messages can come meanwhile); none: at once */
let openedAt: { cols: number; rows: number } | null;
let openWaits: Promise<void> | null;
/** The clipboard's text, as clipboard_text reads it */
let clipboard: string;
/** What each log_open was told to do with the log: keep it, not, or as Settings > General says (null) */
let logKeep: (boolean | null)[];

beforeEach(() => {
  logKeep = [];
  channel = null;
  opened = 0;
  saved = [];
  posted = [];
  resized = [];
  written = [];
  openedAt = null;
  openWaits = null;
  clipboard = "";
  profiles = [emptyProfile(DEFAULT_PROFILE), { ...emptyProfile("root"), username: "admin" }];
  mockIPC(async (cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "pty_open") {
      opened++;
      channel = payload.onEvent as Channel<unknown>;
      openedAt = { cols: Number(payload.cols), rows: Number(payload.rows) };
      const id = opened;
      if (openWaits) await openWaits;
      return id;
    }
    if (cmd === "pty_write") {
      written.push(String(payload.data));
      return null;
    }
    if (cmd === "pty_resize") {
      resized.push({ cols: Number(payload.cols), rows: Number(payload.rows) });
      return null;
    }
    if (cmd === "clipboard_text") return clipboard;
    if (cmd === "log_open") {
      logKeep.push(payload.keep as boolean | null);
      return { id: 1, path: "/tmp/sw1.log", temporary: payload.keep !== true };
    }
    // Settings > Shortcuts' Find, for the terminal menu's key
    if (cmd === "shortcuts_keys") return [{ id: "find", label: "Find…", default: "", key: "Ctrl+Shift+F" }];
    if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
    if (cmd === "profiles_list") return profiles;
    if (cmd === "profiles_save") {
      const profile = payload.profile as Profile;
      saved.push(profile);
      profiles = profiles.map((p) => (p.name === profile.name ? profile : p));
      return profiles;
    }
    return null;
  });
  Object.assign(settings, { autoReconnect: true, autoReconnectAttempts: 6, autoReconnectDelaySeconds: 10 });
  frame = document.createElement("iframe");
  document.body.append(frame);
  frame.contentWindow!.postMessage = ((envelope: { message: { type: string; text?: string } }) => {
    posted.push(envelope.message);
  }) as typeof window.postMessage;
  host = null;
});

afterEach(() => {
  host?.dispose();
  frame.remove();
});

/** The page saying something to its tab */
function page(message: Record<string, unknown>): void {
  window.dispatchEvent(new MessageEvent("message", { data: { channel: "sxt-page", tab: "t1", message }, source: frame.contentWindow, origin: location.origin }));
}

/** Open a session's tab, have ssh print `output`, then end with 255 as it does on any failure of its own */
async function sshEnds(output: string): Promise<void> {
  host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
  page({ type: "ready" });
  await expect.poll(() => channel).not.toBeNull();
  const send = (event: unknown) => (channel!.onmessage as (e: unknown) => void)(event);
  send({ type: "started", local: false, look: { cursor: "block", blink: true, bell: "visual", rules: "", profile: "root" }, link: "pty" });
  send({ type: "data", data: btoa(output) });
  send({ type: "exit", code: 255 });
}

/** The notices the host gave the page, in order */
const notices = () => posted.filter((m) => m.type === "notice").map((m) => m.text ?? "");

describe("the tab's log", () => {
  it("is kept as the profile's Session log says, though the profile's look comes after pty_open has answered", async () => {
    let answer = () => {};
    openWaits = new Promise((resolve) => (answer = resolve));
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    page({ type: "ready" });
    await expect.poll(() => channel).not.toBeNull();
    // pty_open answers first, then the started event with the profile's choice
    answer();
    await new Promise((r) => setTimeout(r, 20));
    (channel!.onmessage as (e: unknown) => void)({ type: "started", local: false, look: { cursor: "block", blink: true, bell: "visual", rules: "", profile: "core", keepLog: true }, link: "pty" });
    await expect.poll(() => logKeep).toEqual([true]);
  });

  it("goes by Settings > General when the profile doesn't say", async () => {
    await sshEnds("");
    await expect.poll(() => logKeep).toEqual([null]);
  });
});

describe("a session's tab when ssh ends", () => {
  it("doesn't reconnect by itself after a refused login, and says where the password is", async () => {
    await sshEnds("(admin@192.168.0.50) Password:\r\nadmin@192.168.0.50: Permission denied (publickey,password).\r\n");
    await expect.poll(() => notices().join("\n")).toContain('Login failed: the username, password or key was refused. Check them in profile "root"');
    expect(notices().at(-1)).toBe("Press Enter to try again.");
    expect(host!.isReconnecting).toBe(false);
    // Enter still tries again
    page({ type: "input", data: "\r" });
    await expect.poll(() => opened).toBe(2);
  });

  it("says a host can't be found, without reconnecting", async () => {
    await sshEnds("ssh: Could not resolve hostname test.localdomain: nodename nor servname provided, or not known\r\n");
    await expect.poll(() => notices().join("\n")).toContain('Can\'t find host "test.localdomain"');
    expect(host!.isReconnecting).toBe(false);
  });

  it("says a jump server doesn't allow forwarding, and how to turn it on, without reconnecting", async () => {
    await sshEnds("channel 0: open failed: administratively prohibited: open failed\r\nstdio forwarding failed\r\nConnection closed by UNKNOWN port 65535\r\n");
    await expect.poll(() => notices().join("\n")).toContain("The jump server won't pass the connection on: it doesn't allow TCP forwarding");
    expect(notices().join("\n")).toContain("AllowTcpForwarding");
    expect(notices().join("\n")).toContain("Log in here");
    expect(host!.isReconnecting).toBe(false);
  });

  it("offers older algorithms for the profile, and F turns them on and reconnects", async () => {
    await sshEnds("Unable to negotiate with 10.0.0.9 port 22: no matching key exchange method found. Their offer: diffie-hellman-group1-sha1\r\n");
    await expect.poll(() => notices().join("\n")).toContain("no key exchange method in common (it offers diffie-hellman-group1-sha1)");
    expect(notices().at(-1)).toBe("Press F to make that change and reconnect, or Enter to reconnect as it is.");
    page({ type: "input", data: "f" });
    await expect.poll(() => saved.length).toBe(1);
    expect(saved[0]).toMatchObject({ name: "root", olderAlgorithms: true });
    await expect.poll(() => opened).toBe(2);
  });

  it("offers turning off Offer SSH keys after too many attempts", async () => {
    await sshEnds("Received disconnect from 10.0.0.9 port 22:2: Too many authentication failures\r\n");
    await expect.poll(() => notices().at(-1)).toBe("Press F to make that change and reconnect, or Enter to reconnect as it is.");
    page({ type: "input", data: "F" });
    await expect.poll(() => saved[0]).toMatchObject({ name: "root", offerKeys: false });
  });

  it("reconnects by itself after a lost connection, with a hint when nothing answered", async () => {
    await sshEnds("ssh: connect to host 10.0.0.9 port 22: Connection refused\r\n");
    await expect.poll(() => notices().join("\n")).toContain("Nothing answered on port 22: the device may be restarting");
    expect(notices().at(-1)).toContain("Reconnecting in 10 s (1 of 6)");
    expect(host!.isReconnecting).toBe(true);
  });
});

describe("a session's tab size", () => {
  // the page measures itself once it's configured, which "started" does while pty_open is still running: that size
  // has to reach the pseudo terminal, or Windows' ConPTY draws for 80x24 (the prompt in the middle of the output)
  it("reaches the pseudo terminal when the page gives it while the session is starting", async () => {
    let answer = () => {};
    openWaits = new Promise((resolve) => (answer = resolve));
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    page({ type: "ready" });
    await expect.poll(() => openedAt).toEqual({ cols: 80, rows: 24 });
    page({ type: "size", cols: 132, rows: 45 });
    answer();
    await expect.poll(() => resized.at(-1)).toEqual({ cols: 132, rows: 45 });
  });

  it("isn't sent again when it hasn't changed since the session started", async () => {
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    page({ type: "size", cols: 132, rows: 45 });
    page({ type: "ready" });
    await expect.poll(() => openedAt).toEqual({ cols: 132, rows: 45 });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(resized).toEqual([]);
  });
});

describe("Reconnect", () => {
  // closing the old connection kills its ssh, whose exit comes after the new one has started (on Windows most of a
  // second later): taken as the new session's, it ended it, and the tab reconnected again and again
  it("isn't ended by the old connection's exit coming after the new one has started", async () => {
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    page({ type: "ready" });
    await expect.poll(() => channel).not.toBeNull();
    const old = channel!;
    const sendOn = (to: Channel<unknown>, event: unknown) => (to.onmessage as (e: unknown) => void)(event);
    sendOn(old, { type: "started", local: false, look: { cursor: "block", blink: true, bell: "visual", rules: "" }, link: "pty" });
    await host.reconnect();
    expect(opened).toBe(2);
    sendOn(channel!, { type: "started", local: false, look: { cursor: "block", blink: true, bell: "visual", rules: "" }, link: "pty" });
    // the old ssh, killed, tells of its end
    sendOn(old, { type: "exit", code: 255 });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(host.isReconnecting).toBe(false);
    expect(host.canReceiveCommands).toBe(true);
    expect(notices().filter((n) => n.startsWith("Reconnecting in") || n.startsWith("Session ended"))).toEqual([]);
  });
});

describe("connecting, until the far end answers", () => {
  /**
   * What Windows' pseudo console (ConPTY) sends as soon as ssh starts, before ssh has reached anything: its modes, a
   * cursor position query, a cleared screen and the window title (ssh.exe's path)
   */
  const CONPTY_START = "\x1b[?9001h\x1b[?1004h\x1b[6n\x1b[?25l\x1b[2J\x1b[m\x1b[H\x1b]0;C:\\WINDOWS\\System32\\OpenSSH\\ssh.exe\x07\x1b[?25h";
  const send = (event: unknown) => (channel!.onmessage as (e: unknown) => void)(event);
  const started = () => send({ type: "started", local: false, look: { cursor: "block", blink: true, bell: "visual", rules: "" }, link: "pty" });

  it("counts text as an answer, coloured or not, but not terminal codes, a window title or blank lines alone", () => {
    expect(hasText("(admin@192.168.0.50) Password: ")).toBe(true);
    expect(hasText("\x1b[32mlab-sw1#\x1b[0m")).toBe(true);
    expect(hasText(CONPTY_START)).toBe(false);
    expect(hasText("\x1b]0;admin@lab-sw1: ~\x1b\\")).toBe(false);
    expect(hasText("\r\n\r\n   \x1b[K")).toBe(false);
  });

  it("stays connecting through ConPTY's own start-up output, at first and after a reconnect, until text comes", async () => {
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    page({ type: "ready" });
    await expect.poll(() => channel).not.toBeNull();
    started();
    send({ type: "data", data: btoa(CONPTY_START) });
    await new Promise((resolve) => setTimeout(resolve, 500));
    expect(host.isConnecting).toBe(true);
    // the switch answers: its password prompt is text
    send({ type: "data", data: btoa("(admin@192.168.0.50) Password: ") });
    await expect.poll(() => host!.isConnecting, { timeout: 2000 }).toBe(false);

    // dropped, and reconnecting to a switch that isn't answering yet: ConPTY starts again, ssh says nothing
    send({ type: "exit", code: 255 });
    await host.reconnect();
    started();
    send({ type: "data", data: btoa(CONPTY_START) });
    await new Promise((resolve) => setTimeout(resolve, 500));
    expect(host.isConnecting).toBe(true);
  });

  it("stays connecting on Windows from ssh's \"timed out\" until its end, which the core tells up to 750 ms later", async () => {
    // jsdom isn't the Mac. ConPTY keeps ssh's output open after ssh exits, so the core finds the end by looking every
    // 250 ms and then leaving 500 ms for the last output (pty.rs): the tab went green in between, then red
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    page({ type: "ready" });
    await expect.poll(() => channel).not.toBeNull();
    started();
    send({ type: "data", data: btoa(CONPTY_START) });
    send({ type: "data", data: btoa("ssh: connect to host 192.168.0.50 port 22: Connection timed out\r\n") });
    const seen = new Set<boolean>();
    const watching = setInterval(() => seen.add(host!.isConnecting), 20);
    await new Promise((resolve) => setTimeout(resolve, 750));
    send({ type: "exit", code: 255 });
    clearInterval(watching);
    expect([...seen]).toEqual([true]);
  });
});

/** KiTTY's anti-idle string for Ctrl+E as a profile keeps it, backslashes and all (\x05\): it types Ctrl+E */
const KITTY_CTRL_E = "\\x05\\";

describe("the anti-idle string", () => {
  afterEach(() => {
    running.runs = [];
    vi.useRealTimers();
  });

  /** A session whose profile types `antiIdle` after `seconds` without typing */
  async function idleSession(antiIdle: string, seconds: number): Promise<void> {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    page({ type: "ready" });
    await vi.waitFor(() => expect(channel).not.toBeNull());
    (channel!.onmessage as (e: unknown) => void)({
      type: "started", local: false, link: "pty",
      look: { cursor: "block", blink: true, bell: "visual", rules: "", antiIdle, antiIdleSeconds: seconds },
    });
    await vi.waitFor(() => expect(host!.canReceiveCommands).toBe(true));
  }

  it("is typed after the profile's seconds without typing, and again after each such spell", async () => {
    await idleSession(KITTY_CTRL_E, 60);
    await vi.advanceTimersByTimeAsync(55_000);
    expect(written).toEqual([]);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(written).toEqual(["\x05"]);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(written).toEqual(["\x05", "\x05"]);
  });

  it("waits again from whatever was typed last", async () => {
    await idleSession(KITTY_CTRL_E, 60);
    await vi.advanceTimersByTimeAsync(50_000);
    page({ type: "input", data: "s" });
    await vi.advanceTimersByTimeAsync(50_000);
    expect(written).toEqual(["s"]);
    await vi.advanceTimersByTimeAsync(15_000);
    expect(written).toEqual(["s", "\x05"]);
  });

  it("isn't typed while a script runs in the tab, nor with no string", async () => {
    await idleSession(KITTY_CTRL_E, 60);
    running.runs = [{ run: {} as ScriptRun, tab: "sw1", tabId: "t1", started: Date.now(), source: "" }];
    await vi.advanceTimersByTimeAsync(120_000);
    expect(written).toEqual([]);
    host!.dispose();
    written = [];
    await idleSession("", 60);
    await vi.advanceTimersByTimeAsync(120_000);
    expect(written).toEqual([]);
  });
});

describe("a paste", () => {
  it("takes the clipboard from the system, so WebView2 doesn't ask to allow it", async () => {
    // the page's own read is what made WebView2 ask "localhost wants to see text and images copied to the clipboard"
    const pageRead = vi.fn(async () => "not this");
    Object.defineProperty(navigator, "clipboard", { value: { readText: pageRead, writeText: vi.fn() }, configurable: true });
    clipboard = "FastEthernet0/5";
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    page({ type: "pasteRequest" });
    await expect.poll(() => posted.filter((m) => m.type === "paste")).toEqual([{ type: "paste", text: "FastEthernet0/5" }]);
    expect(pageRead).not.toHaveBeenCalled();
  });
});

describe("the terminal's menu keys", () => {
  it("are given to the page with its settings: Find's key in Settings > Shortcuts", async () => {
    await sshEnds("");
    const config = await vi.waitFor(() => {
      const found = posted.find((m) => m.type === "config") as unknown as { keyLabels: Record<string, string> } | undefined;
      if (!found) throw new Error("no config yet");
      return found;
    });
    // jsdom isn't the Mac, so Copy and Paste keep PuTTY's keys there; Find is the app's
    expect(config.keyLabels).toEqual({ find: "Ctrl+Shift+F" });
  });
});

describe("Find", () => {
  it("opens the page's find bar for Ctrl+Shift+F in the tab, where Windows' shortcut is that key too", async () => {
    // on Windows the app catches its shortcuts in the tab's page (shortcuts.ts; jsdom isn't the Mac), and Find's is
    // Ctrl+Shift+F, the key the page opens its find bar on. Find sent the page that key, which the app caught as Find
    // again, round and round, and the page never had it
    const pageKeys: string[] = [];
    const typing = frame.contentDocument!.createElement("textarea");
    typing.className = "xterm-helper-textarea";
    typing.addEventListener("keydown", (e) => pageKeys.push(e.key));
    frame.contentDocument!.body.append(typing);
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    catchShortcuts((id) => { if (id === "find") host?.find(); });
    await loadShortcuts();
    frame.contentWindow!.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyF", key: "F", ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true }));
    expect(posted.filter((m) => m.type === "action")).toEqual([{ type: "action", name: "find" }]);
    expect(pageKeys).toEqual([]);
  });
});

describe("a notice from outside the tab", () => {
  it("waits for the program to start, then comes as a warning at the top, before the connection's output", async () => {
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    const told = host.notice("Switch's login script jump-host.spsl isn't on this PC, so the session opens without it.");
    page({ type: "ready" });
    await expect.poll(() => channel).not.toBeNull();
    expect(notices()).toEqual([]);
    const send = (event: unknown) => (channel!.onmessage as (e: unknown) => void)(event);
    send({ type: "started", local: false, look: { cursor: "block", blink: true, bell: "visual", rules: "", profile: "root" }, link: "pty" });
    send({ type: "data", data: btoa("(admin@192.168.0.50) Password: ") });
    await told;
    await expect.poll(() => posted.findIndex((m) => m.type === "output" || m.type === "data")).toBeGreaterThan(-1);
    const warning = posted.findIndex((m) => m.type === "notice");
    expect(posted[warning]).toMatchObject({ kind: "warn", text: expect.stringContaining("so the session opens without it") });
    expect(warning).toBeLessThan(posted.findIndex((m) => m.type === "output" || m.type === "data"));
  });

  it("still comes for a program that says nothing, once it has started", async () => {
    host = new PageHost("t1", frame, { kind: "session", store: "saved", id: "Lab/sw1" }, false, null, () => {}, () => {});
    const told = host.notice("Switch's login script jump-host.spsl isn't on this PC, so the session opens without it.");
    page({ type: "ready" });
    await expect.poll(() => channel).not.toBeNull();
    (channel!.onmessage as (e: unknown) => void)({ type: "started", local: false, look: { cursor: "block", blink: true, bell: "visual", rules: "", profile: "root" }, link: "pty" });
    await told;
    expect(notices()).toEqual(["Switch's login script jump-host.spsl isn't on this PC, so the session opens without it."]);
  });
});
