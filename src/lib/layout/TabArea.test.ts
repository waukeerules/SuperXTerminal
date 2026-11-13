// The tab area (dockview) with session tabs: switching tabs puts the keyboard in the chosen tab's terminal, a tab
// takes its profile's Tab colour, each split's ▾ lists only its own tabs, and the Sessions tree is told whether each
// open session is connecting (yellow, its tab too, until the far end answers), connected (green, its tab too) or its tabs'
// sessions have all ended (the red cross the tab shows). The terminal pages are stood in for by their iframes' postMessage (jsdom loads no
// page), which records the "focus" the tab area sends the page; the Rust side by mockIPC, which answers with nothing
// but hands over each tab's channel (pty_open), on which the test starts the session, and the profiles stored.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Channel } from "@tauri-apps/api/core";
import { gridShape, TabArea } from "./TabArea";
import { DEFAULT_PROFILE, emptyProfile, PROFILES_CHANGED, type Profile } from "../settings/profiles";
import { settings } from "../settings/store.svelte";

/** Have a tab's page say it's ready, so its session starts (set up in beforeEach) */
let ready: (id: string) => void;

/** The tab area under test, inside an element in the document */
let tabs: TabArea;
let element: HTMLElement;
/** The tab each "focus" message went to, in order */
let focused: string[];
/** Each tab's channel, by tab id, once its session has asked for a pseudo terminal */
let channels: Map<string, Channel<unknown>>;
/** Each Remote Desktop connection's channel, in the order they were opened (rdp_open) */
let desktops: Channel<unknown>[];
/** How the next file transfer's connection goes (sftp_open): what it opens with, or the error it fails with */
let transfer: () => Promise<unknown>;
/** The profiles stored, as profiles_list gives them */
let profiles: Profile[];
/** What the tab area last told the Sessions tree: each open session's state */
let reported: ReadonlyMap<string, string>;

beforeEach(() => {
  // dockview watches its element's size; jsdom has no ResizeObserver, and these tests need no sizes
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  channels = new Map();
  desktops = [];
  transfer = async () => ({ id: 1, address: "admin@sw1", home: "/home/admin", local: "/Users/me", remote: "/home/admin", scp: false });
  profiles =[emptyProfile(DEFAULT_PROFILE)];
  let opening: string | null = null;
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "pty_open" && opening) channels.set(opening, payload.onEvent as Channel<unknown>);
    if (cmd === "pty_open") return channels.size;
    if (cmd === "sftp_open") return transfer();
    if (cmd === "local_list" || cmd === "sftp_list") return { path: payload.path, entries: [] };
    if (cmd === "rdp_open") {
      desktops.push(payload.onEvent as Channel<unknown>);
      return { id: desktops.length, width: 800, height: 600, fixed: false, smartSizing: true, scale: 100, keyboard: 0 };
    }
    if (cmd === "profiles_list") return profiles;
    if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
    return null;
  });
  /** The page of tab `id` saying it's ready, which starts its session */
  ready = (id: string) => {
    opening = id;
    const frame = tabs.api.getPanel(id)!.view.content.element.querySelector("iframe")!;
    window.dispatchEvent(new MessageEvent("message", { data: { channel: "sxt-page", tab: id, message: { type: "ready" } }, source: frame.contentWindow, origin: location.origin }));
  };
  focused = [];
  element = document.createElement("div");
  document.body.append(element);
  reported = new Map();
  tabs = new TabArea(element, (states) => (reported = states));
});

afterEach(() => {
  tabs.api.dispose();
  element.remove();
});

/** Open a session's tab (in an environment colour, if given), and record the "focus" its page is sent */
function open(title: string, environment: string | null = null): string {
  const id = tabs.openSession("saved", `Lab/${title}`, title, false, "terminal", environment);
  const frame = tabs.api.getPanel(id)!.view.content.element.querySelector("iframe")!;
  frame.contentWindow!.postMessage = ((envelope: { tab: string; message: { type: string } }) => {
    if (envelope.message.type === "focus") focused.push(envelope.tab);
  }) as typeof window.postMessage;
  return id;
}

/** A tab's label in the tab strip, as the user clicks it */
function label(title: string): HTMLElement {
  const found = [...element.querySelectorAll<HTMLElement>(".session-tab")].find((e) => e.textContent?.includes(title));
  if (!found) throw new Error(`no tab ${title}`);
  return found;
}

describe("switching tabs", () => {
  it("puts the keyboard in the terminal of the tab clicked", async () => {
    const sw1 = open("sw1");
    open("sw2");
    focused = [];
    // dockview switches on the press; the click comes as the button is let go
    label("sw1").dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0 }));
    label("sw1").click();
    await expect.poll(() => focused).toEqual([sw1]);
    expect(tabs.api.activePanel?.id).toBe(sw1);
  });

  it("puts the keyboard in the terminal of the tab chosen from the open tabs' list or Next Tab", async () => {
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    focused = [];
    tabs.activate(sw1);
    await expect.poll(() => focused).toEqual([sw1]);
    tabs.cycle(1);
    await expect.poll(() => focused).toEqual([sw1, sw2]);
  });

  it("puts the keyboard in the tab left in front when Close Tab (Cmd+W) closes one, so it can close the next", async () => {
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    const sw3 = open("sw3");
    focused = [];
    tabs.closeActive();
    expect(tabs.api.getPanel(sw3)).toBeUndefined();
    await expect.poll(() => focused).toEqual([sw2]);
    tabs.closeActive();
    await expect.poll(() => focused).toEqual([sw2, sw1]);
  });

  it("puts the keyboard in the tab left in front when a tab's menu closes it", async () => {
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    focused = [];
    tabs.command(sw2, "close");
    await expect.poll(() => focused).toEqual([sw1]);
  });

  it("puts the keyboard in the tab left in front when a tab's ✕ closes it, a file transfer's too", async () => {
    mockWindows("main");
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    tabs.openSftp("saved", "Lab/sw1", "files", "switch", null);
    focused = [];
    label("files").querySelector<HTMLElement>(".close")!.click();
    await expect.poll(() => focused).toEqual([sw2]);
    label("sw2").querySelector<HTMLElement>(".close")!.click();
    await expect.poll(() => focused).toEqual([sw2, sw1]);
  });

  it("puts the keyboard in the tab left in front when the ✕ beside the tab list closes one, a file transfer's too", async () => {
    mockWindows("main");
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    tabs.openSftp("saved", "Lab/sw1", "files", "switch", null);
    focused = [];
    const close = () => element.querySelector<HTMLElement>(".tab-close-button")!.click();
    close();
    expect(tabs.api.panels.map((p) => p.id)).toEqual([sw1, sw2]);
    await expect.poll(() => focused).toEqual([sw2]);
    close();
    await expect.poll(() => focused).toEqual([sw2, sw1]);
  });

  it("puts the keyboard in the tab left in front when a tab closes by itself with the keyboard in it", async () => {
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    focused = [];
    // as when its shell ends: nothing was clicked, and no Close Tab asked for it
    tabs.api.getPanel(sw2)!.api.close();
    await expect.poll(() => focused).toEqual([sw1]);
  });

  it("leaves the keyboard where it is when a tab closes while it's in a box outside the tabs", async () => {
    const sw1 = open("sw1");
    open("sw2");
    const search = document.createElement("input");
    document.body.append(search);
    search.focus();
    focused = [];
    // the tab behind closes by itself, as when its shell ends
    tabs.api.getPanel(sw1)!.api.close();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(focused).toEqual([]);
    expect(document.activeElement).toBe(search);
    search.remove();
  });
});

describe("a right-click in the terminal", () => {
  /**
   * The page of tab `id` as the test stands it in: jsdom loads no page, so the frame's own blank document is given
   * the terminal page's #terminal and the tab area is told it has loaded, as it is when the real page does
   */
  function page(id: string): Document {
    const frame = tabs.api.getPanel(id)!.view.content.element.querySelector("iframe")!;
    const doc = frame.contentDocument!;
    if (!doc.getElementById("terminal")) {
      const terminal = doc.createElement("div");
      terminal.id = "terminal";
      // jsdom's frame document may have no html or body yet
      if (!doc.documentElement) doc.append(doc.createElement("html"));
      if (!doc.body) doc.documentElement.append(doc.createElement("body"));
      doc.body.append(terminal);
      frame.dispatchEvent(new Event("load"));
    }
    return doc;
  }

  it("opens the menu in the tab in front, not in a tab behind it that the Mac hands the click to", () => {
    // the Mac's WebView gives the right-click to the newest tab's page, stacked hidden over the one in front
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    const front = page(sw1);
    const behind = page(sw2);
    tabs.activate(sw1);
    const menus: string[] = [];
    front.getElementById("terminal")!.addEventListener("contextmenu", () => menus.push("sw1"));
    behind.getElementById("terminal")!.addEventListener("contextmenu", () => menus.push("sw2"));
    // xterm.js takes the keyboard on a right-click, and a tab whose page gets the keyboard comes to the front
    behind.getElementById("terminal")!.addEventListener("contextmenu", () => behind.defaultView!.dispatchEvent(new FocusEvent("focus")));
    front.elementFromPoint = () => front.getElementById("terminal");
    behind.getElementById("terminal")!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2, clientX: 40, clientY: 30 }));
    expect(tabs.api.activePanel?.id).toBe(sw1);
    expect(menus).toEqual(["sw1"]);
  });

  it("is left alone in the tab in front", () => {
    const sw1 = open("sw1");
    const front = page(sw1);
    const menus: string[] = [];
    front.getElementById("terminal")!.addEventListener("contextmenu", () => menus.push("sw1"));
    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2 });
    front.getElementById("terminal")!.dispatchEvent(event);
    expect(menus).toEqual(["sw1"]);
    expect(event.defaultPrevented).toBe(false);
  });
});

describe("a profile's Tab colour", () => {
  /** Start tab `id`'s session with this profile (its look, as the Rust side sends it) */
  async function start(id: string, profile: string, tabColor: string): Promise<void> {
    ready(id);
    await expect.poll(() => channels.get(id)).toBeDefined();
    (channels.get(id)!.onmessage as (e: unknown) => void)({
      type: "started", local: false, link: "pty",
      look: { cursor: "block", blink: true, bell: "visual", rules: "", profile, tabColor },
    });
  }
  /** The colour a tab's label is drawn in, or null when it has none */
  const colourOf = (title: string) => {
    const tab = label(title).querySelector<HTMLElement>(".tab")!;
    return tab.classList.contains("coloured") ? tab.style.getPropertyValue("--env") : null;
  };

  it("colours the tab of a session using the profile, and again when the profile is saved", async () => {
    const sw1 = open("sw1");
    expect(colourOf("sw1")).toBeNull();
    await start(sw1, "Lab", "#2da44e");
    await expect.poll(() => colourOf("sw1")).toBe("#2da44e");
    // Settings > Profiles saves a new Tab colour
    profiles = [emptyProfile(DEFAULT_PROFILE), { ...emptyProfile("Lab"), tabColor: "#1f6feb" }];
    window.dispatchEvent(new Event(PROFILES_CHANGED));
    await expect.poll(() => colourOf("sw1")).toBe("#1f6feb");
    // and takes it off
    profiles = [emptyProfile(DEFAULT_PROFILE), emptyProfile("Lab")];
    window.dispatchEvent(new Event(PROFILES_CHANGED));
    await expect.poll(() => colourOf("sw1")).toBeNull();
  });

  it("gives way to the session's environment colour, which marks production", async () => {
    const core = open("core1", "#d32f2f");
    await start(core, "Lab", "#2da44e");
    // once the session's start has reached the tab
    await new Promise((r) => setTimeout(r, 20));
    expect(colourOf("core1")).toBe("#d32f2f");
  });
});

describe("a split's ▾ list", () => {
  it("lists only the tabs in its own split, its front one ticked", async () => {
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    const sw3 = open("sw3");
    // sw3 moved to a split on the right, as dragging it there does
    const right = tabs.api.addGroup({ referenceGroup: tabs.api.getPanel(sw1)!.group, direction: "right" });
    tabs.api.getPanel(sw3)!.api.moveTo({ group: right });
    /** The titles App's ▾ list shows when a split's ▾ is clicked, the ticked one marked */
    const listed: string[][] = [];
    tabs.onTabList = (_right, _bottom, group) => listed.push(tabs.allTabs(group).map((t) => t.title + (t.active ? " ✓" : "")));
    const arrows = [...element.querySelectorAll<HTMLButtonElement>("button[aria-label='Open tabs']")];
    expect(arrows).toHaveLength(2);
    arrows.forEach((arrow) => arrow.click());
    expect(listed).toEqual([["sw1", "sw2 ✓"], ["sw3 ✓"]]);
    // the tab switcher and Next Tab still go through them all
    expect(tabs.allTabs().map((t) => t.id)).toEqual([sw1, sw2, sw3]);
  });

  it("closes on a second click on its ▾, and opens again once it's gone; another split's ▾ opens that one's", () => {
    const sw1 = open("sw1");
    open("sw2");
    const right = tabs.api.addGroup({ referenceGroup: tabs.api.getPanel(sw1)!.group, direction: "right" });
    tabs.api.getPanel(open("sw3"))!.api.moveTo({ group: right });
    /** What App was told, in order: "open <first tab's title>" or "close" */
    const told: string[] = [];
    tabs.onTabList = (_right, _bottom, group) => told.push(`open ${tabs.allTabs(group)[0].title}`);
    tabs.onTabListClose = () => told.push("close");
    const [left, rightArrow] = [...element.querySelectorAll<HTMLButtonElement>("button[aria-label='Open tabs']")];
    left.click();
    left.click();
    expect(told).toEqual(["open sw1", "close"]);
    // shown again, then gone another way (a tab chosen, a click elsewhere): App says so, and the ▾ opens it again
    left.click();
    tabs.tabListClosed();
    left.click();
    // with the left one showing, the right split's ▾ opens its own list, not a close
    rightArrow.click();
    expect(told).toEqual(["open sw1", "close", "open sw1", "open sw1", "open sw3"]);
  });
});

describe("what the Sessions tree is told", () => {
  /** Start tab `id`'s session, as the Rust side says it's started (its program running, not yet the far end answering) */
  async function start(id: string): Promise<void> {
    ready(id);
    await expect.poll(() => channels.get(id)).toBeDefined();
    (channels.get(id)!.onmessage as (e: unknown) => void)({
      type: "started", local: false, link: "pty",
      look: { cursor: "block", blink: true, bell: "visual", rules: "", profile: "", tabColor: "" },
    });
  }
  /** The session in tab `id` prints something (base64, as the Rust side sends it) */
  const output = (id: string, text: string) => (channels.get(id)!.onmessage as (e: unknown) => void)({ type: "data", data: btoa(text) });
  /** Tab `id`'s session ends, as the Rust side says when the connection drops */
  const exit = (id: string) => (channels.get(id)!.onmessage as (e: unknown) => void)({ type: "exit", code: 255 });

  it("says connecting until the far end answers, connected once it has, and disconnected when it ends", async () => {
    const sw1 = open("sw1");
    expect(reported.get("saved:Lab/sw1")).toBe("connecting");
    await start(sw1);
    // ssh running isn't the switch answering
    expect(reported.get("saved:Lab/sw1")).toBe("connecting");
    // its tab shows the same, a yellow dot
    await expect.poll(() => label("sw1").querySelector('[aria-label="Connecting"]')).not.toBeNull();
    output(sw1, "Password: ");
    await expect.poll(() => reported.get("saved:Lab/sw1"), { timeout: 3000 }).toBe("connected");
    // and green once connected, as the tree
    await expect.poll(() => label("sw1").querySelector('[aria-label="Connected"]')).not.toBeNull();
    expect(label("sw1").querySelector('[aria-label="Connecting"]')).toBeNull();
    exit(sw1);
    await expect.poll(() => reported.get("saved:Lab/sw1")).toBe("disconnected");
    await expect.poll(() => label("sw1").querySelector('[aria-label="Disconnected"]')).not.toBeNull();
    expect(label("sw1").querySelector('[aria-label="Connected"]')).toBeNull();
    tabs.api.getPanel(sw1)!.api.close();
    expect(reported.has("saved:Lab/sw1")).toBe(false);
  });

  it("goes from connecting straight to disconnected when the connection fails at once, never connected", async () => {
    const sw1 = open("sw1");
    await start(sw1);
    const seen: string[] = [];
    const watch = setInterval(() => seen.push(reported.get("saved:Lab/sw1") ?? ""), 10);
    output(sw1, "ssh: connect to host 192.168.0.187 port 22: Network is unreachable\r\n");
    exit(sw1);
    await new Promise((r) => setTimeout(r, 1500));
    clearInterval(watch);
    expect(reported.get("saved:Lab/sw1")).toBe("disconnected");
    expect(seen).not.toContain("connected");
  });

  it("counts a session open in two tabs as connected while either is", async () => {
    const first = open("sw1");
    const second = tabs.openSession("saved", "Lab/sw1", "sw1 (2)", false, "terminal", null);
    await start(first);
    await start(second);
    output(first, "sw1#");
    output(second, "sw1#");
    await expect.poll(() => reported.get("saved:Lab/sw1"), { timeout: 3000 }).toBe("connected");
    exit(first);
    await new Promise((r) => setTimeout(r, 20));
    expect(reported.get("saved:Lab/sw1")).toBe("connected");
    exit(second);
    await expect.poll(() => reported.get("saved:Lab/sw1")).toBe("disconnected");
  });
});

// A Remote Desktop tab has no page: its host (RdpHost) connects once the tab has a size, and draws on a canvas.
// jsdom gives neither, so the tab is given a size, a canvas that draws nothing and an ImageData; the core is
// mockIPC's rdp_open, whose channel the test sends the first piece of screen and the session's end on.
describe("a Remote Desktop tab", () => {
  beforeEach(() => {
    mockWindows("main");
    const real = HTMLDivElement.prototype.getBoundingClientRect;
    vi.spyOn(HTMLDivElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLDivElement) {
      return this.classList.contains("rdp-panel") ? ({ width: 800, height: 600 } as DOMRect) : real.call(this);
    });
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({ putImageData() {} } as unknown as CanvasRenderingContext2D);
    vi.stubGlobal("ImageData", class {});
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  /** One pixel of the remote screen, as the core sends it: "SXF1", where it goes, its size and the screen's, the pixel */
  function screen(): ArrayBuffer {
    const packet = new ArrayBuffer(20);
    new Uint8Array(packet).set(new TextEncoder().encode("SXF1"));
    const view = new DataView(packet);
    [0, 0, 1, 1, 800, 600].forEach((value, i) => view.setUint16(4 + i * 2, value, true));
    return packet;
  }

  it("shows connecting until the desktop is seen, connected once it is, and disconnected when it ends, as the Sessions tree is told", async () => {
    const id = tabs.openRdp("saved", "Servers/ts1", "ts1", "windows", null);
    await expect.poll(() => desktops.length).toBe(1);
    const say = desktops[0].onmessage as (e: unknown) => void;
    await expect.poll(() => label("ts1").querySelector('[aria-label="Connecting"]')).not.toBeNull();
    expect(reported.get("saved:Servers/ts1")).toBe("connecting");
    say(screen());
    await expect.poll(() => label("ts1").querySelector('[aria-label="Connected"]')).not.toBeNull();
    expect(label("ts1").querySelector('[aria-label="Connecting"]')).toBeNull();
    expect(reported.get("saved:Servers/ts1")).toBe("connected");
    say({ type: "ended", reason: "The remote computer ended the session", clean: true });
    await expect.poll(() => label("ts1").querySelector('[aria-label="Disconnected"]')).not.toBeNull();
    expect(reported.get("saved:Servers/ts1")).toBe("disconnected");
    tabs.api.getPanel(id)!.api.close();
    expect(reported.has("saved:Servers/ts1")).toBe(false);
  });
});

// A file transfer tab (FileTransfer.svelte) connects as it opens: the core's sftp_open is stood in for by a promise
// the test settles, as ssh logging in or being refused, and both sides' listings by empty folders.
describe("a file transfer (SFTP) tab", () => {
  // the tab listens for files dropped on its window: a stand-in window for it
  beforeEach(() => mockWindows("main"));

  it("shows connecting until ssh is in, connected once it is, as the Sessions tree is told", async () => {
    let connected: (opened: unknown) => void = () => {};
    transfer = () => new Promise((resolve) => (connected = resolve));
    const id = tabs.openSftp("saved", "Lab/sw1", "sw1", "switch", null);
    await expect.poll(() => label("sw1").querySelector('[aria-label="Connecting"]')).not.toBeNull();
    expect(reported.get("saved:Lab/sw1")).toBe("connecting");
    connected({ id: 1, address: "admin@sw1", home: "/home/admin", local: "/Users/me", remote: "/home/admin", scp: false });
    await expect.poll(() => label("sw1").querySelector('[aria-label="Connected"]')).not.toBeNull();
    expect(label("sw1").querySelector('[aria-label="Connecting"]')).toBeNull();
    expect(reported.get("saved:Lab/sw1")).toBe("connected");
    tabs.api.getPanel(id)!.api.close();
    expect(reported.has("saved:Lab/sw1")).toBe(false);
  });

  it("shows disconnected when the connection fails", async () => {
    transfer = () => Promise.reject("ssh: connect to host 10.0.0.9 port 22: Connection refused");
    tabs.openSftp("saved", "Lab/sw9", "sw9", "switch", null);
    await expect.poll(() => label("sw9").querySelector('[aria-label="Disconnected"]')).not.toBeNull();
    expect(reported.get("saved:Lab/sw9")).toBe("disconnected");
  });
});

describe("Grid View", () => {
  /**
   * The tabs as the grid shows them: rows top to bottom, each the tiles' tab titles left to right (a tile with
   * several tabs lists them all). From dockview's own layout, as nothing is drawn in jsdom
   */
  function rows(): string[][] {
    type Node = { type: "leaf" | "branch"; data: unknown };
    const titles = (node: Node): string[] =>
      node.type === "leaf"
        ? (node.data as { views: string[] }).views.map((v) => tabs.api.getPanel(v)?.title ?? v)
        : (node.data as Node[]).flatMap(titles);
    const { root, orientation } = tabs.api.toJSON().grid;
    const children = root.type === "branch" ? (root.data as Node[]) : [root as Node];
    // a grid of one row is a horizontal branch of tiles; of several, a vertical one of rows
    return orientation === "HORIZONTAL" ? [children.flatMap(titles)] : children.map(titles);
  }

  it("tiles every tab in a split of its own, row by row, the sessions carrying on in the same pages", () => {
    const ids = ["sw1", "sw2", "sw3", "sw4", "sw5"].map((t) => open(t));
    const frames = ids.map((id) => tabs.api.getPanel(id)!.view.content.element.querySelector("iframe"));
    tabs.activate(ids[1]);
    tabs.setGridView(true);
    // jsdom's tab area is 100 × 100 points: 2 terminal-shaped tiles across, 3 rows
    expect(rows()).toEqual([["sw1", "sw2"], ["sw3", "sw4"], ["sw5"]]);
    expect(tabs.groupCount()).toBe(5);
    // moved, not opened again: the same page in each, so its session goes on
    expect(ids.map((id) => tabs.api.getPanel(id)!.view.content.element.querySelector("iframe"))).toEqual(frames);
    expect(tabs.api.activePanel?.id).toBe(ids[1]);
    expect(tabs.inGridView()).toBe(true);
  });

  it("makes the grid again as tabs open and close", async () => {
    const ids = ["sw1", "sw2", "sw3", "sw4"].map((t) => open(t));
    tabs.setGridView(true);
    expect(rows()).toEqual([["sw1", "sw2"], ["sw3", "sw4"]]);
    open("sw5");
    await expect.poll(() => rows()).toEqual([["sw1", "sw2"], ["sw3", "sw4"], ["sw5"]]);
    tabs.command(ids[0], "close");
    await expect.poll(() => rows()).toEqual([["sw2", "sw3"], ["sw4", "sw5"]]);
  });

  it("closes every other tab in the window with Close others, each being a tile of its own, and all with Close all", async () => {
    const ids = ["sw1", "sw2", "sw3", "sw4", "sw5"].map((t) => open(t));
    tabs.setGridView(true);
    // to the right: the tiles after it, row by row, as the grid reads
    tabs.command(ids[2], "closeRight");
    await expect.poll(() => rows()).toEqual([["sw1", "sw2"], ["sw3"]]);
    tabs.command(ids[1], "closeOthers");
    await expect.poll(() => rows()).toEqual([["sw2"]]);
    open("sw6");
    await expect.poll(() => tabs.allTabs().length).toBe(2);
    tabs.command(ids[1], "closeAll");
    await expect.poll(() => tabs.allTabs().length).toBe(0);
  });

  it("joins the tabs back into one group when it's turned off, the active tab in front", () => {
    const ids = ["sw1", "sw2", "sw3"].map((t) => open(t));
    tabs.setGridView(true);
    tabs.activate(ids[2]);
    tabs.setGridView(false);
    expect(tabs.groupCount()).toBe(1);
    expect(rows()).toEqual([["sw1", "sw2", "sw3"]]);
    expect(tabs.api.activePanel?.id).toBe(ids[2]);
    // and a tab opened now goes in that group, not a tile of its own
    open("sw4");
    expect(tabs.groupCount()).toBe(1);
  });
});

describe("Grid View off, putting the splits back", () => {
  type Shape = string[] | Shape[];
  /** The splits as nested lists: a split's tab titles in order, inside a list for each row or column of them */
  function splits(): Shape {
    type Node = { type: "leaf" | "branch"; data: unknown };
    const walk = (node: Node): Shape =>
      node.type === "leaf"
        ? (node.data as { views: string[] }).views.map((v) => tabs.api.getPanel(v)?.title ?? v)
        : (node.data as Node[]).map(walk) as Shape;
    const root = tabs.api.toJSON().grid.root as Node;
    // a branch of one, as dockview sometimes keeps around a single split, is the split itself
    const flat = (shape: Shape): Shape => (shape.length === 1 && Array.isArray(shape[0]) ? flat(shape[0] as Shape) : shape.map((s) => (Array.isArray(s) ? flat(s as Shape) : s)) as Shape);
    return flat(walk(root));
  }
  /** Each split's tab in front, by its first tab's title */
  const inFront = () => Object.fromEntries(tabs.api.groups.map((g) => [g.panels[0]?.title, g.activePanel?.title]));

  /** Six tabs: sw1-sw3 on the left; sw4 and sw5 at the right, over sw6 */
  function arranged(): string[] {
    const ids = ["sw1", "sw2", "sw3", "sw4", "sw5", "sw6"].map((t) => open(t));
    tabs.split(ids[3], "right");
    const right = tabs.api.getPanel(ids[3])!.group;
    tabs.api.getPanel(ids[4])!.api.moveTo({ group: right });
    tabs.api.getPanel(ids[5])!.api.moveTo({ group: right });
    tabs.split(ids[5], "below");
    tabs.activate(ids[1]);
    return ids;
  }

  it("puts each split back with its tabs in order and the one in front, the same pages in them", () => {
    const ids = arranged();
    const before = splits();
    const fronts = inFront();
    const frames = ids.map((id) => tabs.api.getPanel(id)!.view.content.element.querySelector("iframe"));
    tabs.setGridView(true, true);
    expect(tabs.groupCount()).toBe(6);
    tabs.setGridView(false, true);
    expect(splits()).toEqual(before);
    expect(inFront()).toEqual(fronts);
    expect(tabs.api.activePanel?.id).toBe(ids[1]);
    expect(ids.map((id) => tabs.api.getPanel(id)!.view.content.element.querySelector("iframe"))).toEqual(frames);
  });

  it("leaves out a tab closed in the grid, and puts one opened there in the split that was in front", async () => {
    const ids = arranged();
    tabs.setGridView(true, true);
    tabs.command(ids[4], "close");
    open("sw7");
    await expect.poll(() => tabs.groupCount()).toBe(6);
    tabs.setGridView(false, true);
    // sw2 was in front, on the left
    expect(splits()).toEqual([["sw1", "sw2", "sw3", "sw7"], [["sw4"], ["sw6"]]]);
  });

  it("tiles a layout opened in Grid View, and puts back the layout's own splits when it goes off", async () => {
    arranged();
    const layout = tabs.layoutState();
    const before = splits();
    // a window starting in Grid View: one tab, then the saved layout opens over it
    tabs.loadLayout(null);
    tabs.setGridView(true, true);
    tabs.loadLayout(layout);
    await expect.poll(() => tabs.groupCount()).toBe(6);
    tabs.setGridView(false, true);
    expect(splits()).toEqual(before);
  });

  it("joins the tabs instead after Join All Tabs, however it's set", () => {
    arranged();
    tabs.setGridView(true, true);
    tabs.forgetSplits();
    tabs.setGridView(false, true);
    expect(tabs.groupCount()).toBe(1);
  });
});

describe("Grid View in a window of Remote Desktops", () => {
  // a desktop's tab listens for events on its window: a stand-in window for it
  beforeEach(() => mockWindows("main"));
  /** Open a Remote Desktop session's tab */
  const desktop = (title: string) => tabs.openRdp("saved", `Servers/${title}`, title, "windows", null);

  it("keeps desktops alone in one group of tabs, each the whole window, when they're kept apart", async () => {
    tabs.desktopsApart = true;
    desktop("ts1");
    desktop("ts2");
    expect(tabs.onlyDesktops()).toBe(true);
    tabs.setGridView(true);
    expect(tabs.groupCount()).toBe(1);
    desktop("ts3");
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(tabs.groupCount()).toBe(1);
  });

  it("tiles them when they take a tile like the others, and a window with a terminal too", () => {
    desktop("ts1");
    desktop("ts2");
    tabs.setGridView(true);
    expect(tabs.groupCount()).toBe(2);
    tabs.setGridView(false);
    tabs.desktopsApart = true;
    open("sw1");
    expect(tabs.onlyDesktops()).toBe(false);
    tabs.setGridView(true);
    expect(tabs.groupCount()).toBe(3);
  });
});

describe("gridShape", () => {
  it("gives a wide monitor more columns than rows, and a tall one more rows", () => {
    expect(gridShape(2, 2560, 1300)).toEqual({ columns: 2, rows: 1 });
    expect(gridShape(4, 2560, 1300)).toEqual({ columns: 2, rows: 2 });
    expect(gridShape(6, 2560, 1300)).toEqual({ columns: 3, rows: 2 });
    expect(gridShape(9, 2560, 1300)).toEqual({ columns: 3, rows: 3 });
    expect(gridShape(12, 3840, 2000)).toEqual({ columns: 4, rows: 3 });
    expect(gridShape(4, 1080, 1800)).toEqual({ columns: 1, rows: 4 });
  });

  it("is as square as it can be before the window has a size, and one tile for one tab", () => {
    expect(gridShape(5, 0, 0)).toEqual({ columns: 3, rows: 2 });
    expect(gridShape(1, 2560, 1300)).toEqual({ columns: 1, rows: 1 });
  });
});

describe("Watch tabs for activity (Settings > General > Tabs)", () => {
  afterEach(() => {
    settings.watchActivity = false;
  });

  it("watches a tab as it opens when it's on, and not when it's off", () => {
    settings.watchActivity = false;
    const plain = open("sw1");
    expect(tabs.watchOf(plain).activity).toBe(false);
    settings.watchActivity = true;
    const watched = open("sw2");
    expect(tabs.watchOf(watched).activity).toBe(true);
    // the one opened before is as it was
    expect(tabs.watchOf(plain).activity).toBe(false);
    // and the tab's own menu still turns it off for one tab
    tabs.setWatch(watched, { activity: false });
    expect(tabs.watchOf(watched).activity).toBe(false);
  });

  it("turned on or off in Settings, watches or stops watching the tabs already open, their Watch for Silence kept", () => {
    const first = open("sw1");
    const second = open("sw2");
    tabs.setWatch(second, { silence: 30 });
    tabs.watchEvery(true);
    expect([tabs.watchOf(first), tabs.watchOf(second)]).toEqual([{ activity: true, silence: 0 }, { activity: true, silence: 30 }]);
    tabs.watchEvery(false);
    expect([tabs.watchOf(first), tabs.watchOf(second)]).toEqual([{ activity: false, silence: 0 }, { activity: false, silence: 30 }]);
  });
});
