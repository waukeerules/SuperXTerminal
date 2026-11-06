// The tab area (dockview) with session tabs: switching tabs puts the keyboard in the chosen tab's terminal, a tab
// takes its profile's Tab colour, each split's ▾ lists only its own tabs, and the Sessions tree is told whether each
// open session is connecting (yellow, its tab too, until the far end answers), connected (green, its tab too) or its tabs'
// sessions have all ended (the red cross the tab shows). The terminal pages are stood in for by their iframes' postMessage (jsdom loads no
// page), which records the "focus" the tab area sends the page; the Rust side by mockIPC, which answers with nothing
// but hands over each tab's channel (pty_open), on which the test starts the session, and the profiles stored.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Channel } from "@tauri-apps/api/core";
import { TabArea } from "./TabArea";
import { DEFAULT_PROFILE, emptyProfile, PROFILES_CHANGED, type Profile } from "../settings/profiles";

/** Have a tab's page say it's ready, so its session starts (set up in beforeEach) */
let ready: (id: string) => void;

/** The tab area under test, inside an element in the document */
let tabs: TabArea;
let element: HTMLElement;
/** The tab each "focus" message went to, in order */
let focused: string[];
/** Each tab's channel, by tab id, once its session has asked for a pseudo terminal */
let channels: Map<string, Channel<unknown>>;
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
  profiles = [emptyProfile(DEFAULT_PROFILE)];
  let opening: string | null = null;
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "pty_open" && opening) channels.set(opening, payload.onEvent as Channel<unknown>);
    if (cmd === "pty_open") return channels.size;
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
