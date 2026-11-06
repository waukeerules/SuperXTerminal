// The button bar at the bottom of the window shows the active tab's session's own bar (Edit Session > Terminal),
// else its profile's (Settings > Profiles > Button bar), over the bar for the tab's icon, once the tab's session has
// started and says which profile it uses, and again when the session or profile is saved; and it follows the tabs
// though the window gives it them only after it's drawn, as App does.
// The tab area is stood in for by `context.tabs` (the active tab, and its change listeners); the Rust side by
// mockIPC: ButtonBars.xml's bars and the profiles stored. And a button asks before it runs, unless that's turned off;
// a Rhai script button's script goes to the core (rhai_run) as written, its braces not taken for placeholders, once
// the Environment Variables it reads (env.name) have values.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import { mockIPC } from "@tauri-apps/api/mocks";
import ButtonBar from "./ButtonBar.svelte";
import { context } from "./sending.svelte";
import { settings } from "../settings/store.svelte";
import { DEFAULT_PROFILE, emptyProfile, PROFILES_CHANGED, type Profile } from "../settings/profiles";
import type { Bar, BarButton } from "./bars";
import type { CommandTab, TabArea } from "../layout/TabArea";

/** A button labelled `label` */
const button = (label: string): BarButton => ({ label, tooltip: "", color: "", icon: "", action: "SendString", text: `${label}\n` });

/** The profiles stored, as profiles_list gives them */
let profiles: Profile[];
/** The active tab: a switch's, whose session uses `profile` once started ("" before) */
let active: { icon: string; profile: string };
/** Told when the active tab changes, as TabArea.onChange's listeners */
let listeners: (() => void)[];

beforeEach(() => {
  const bars: Bar[] = [
    { name: "Default", buttons: [], tabIcons: [] },
    { name: "Cisco", buttons: [button("show run")], tabIcons: ["switch"] },
    { name: "Lab", buttons: [button("reload lab")], tabIcons: [] },
  ];
  profiles = [emptyProfile(DEFAULT_PROFILE), { ...emptyProfile("Lab switches"), buttonBar: "Lab" }];
  active = { icon: "switch", profile: "" };
  listeners = [];
  mockIPC((cmd) => {
    if (cmd === "buttonbars_load") return structuredClone(bars);
    if (cmd === "profiles_list") return profiles;
    return null;
  });
  Object.assign(settings, { loaded: true, activeButtonBar: "" });
  context.tabs = {
    commandTabs: () => [{ id: "t1", title: "sw1", session: "saved:Lab/sw1", icon: active.icon, profile: active.profile, active: true } as unknown as CommandTab],
    onChange: (listener: () => void) => {
      listeners.push(listener);
      return () => {};
    },
    focusActive: () => {},
  } as unknown as TabArea;
});

afterEach(() => {
  context.tabs = null;
  context.sessionBar = () => "";
});

/** The bar showing: the bar picker's choice */
const shown = () => (screen.getByTitle("Button bar") as HTMLSelectElement).value;
/** The labels of the buttons showing */
const buttons = () => [...document.querySelectorAll(".bar-button")].map((b) => b.textContent?.trim());

describe("asking before a button runs", () => {
  /** What the button typed into the tab */
  let typed: string[];
  /** The question asked, and the answer given: "Run" or "Cancel" */
  let asked: string[];
  let reply: string;

  beforeEach(() => {
    typed = [];
    asked = [];
    reply = "Run";
    mockIPC((cmd, args) => {
      if (cmd === "buttonbars_load") return [{ name: "Default", buttons: [button("show clock")], tabIcons: [] }];
      if (cmd === "profiles_list") return [];
      if (cmd === "variables_fill") return { text: (args as { text: string }).text, missing: [] };
      // the dialog plugin's ask(): its message, answered with a button's label
      if (cmd === "plugin:dialog|message") {
        asked.push((args as { message: string }).message);
        return reply;
      }
      return null;
    });
    const host = { canReceiveCommands: true, markOutput: () => {}, markCommand: () => {}, type: async (text: string) => void typed.push(text) };
    context.tabs = {
      commandTabs: () => [{ id: "t1", title: "core-sw1", session: "", icon: "switch", profile: "", active: true, host } as unknown as CommandTab],
      onChange: () => () => {},
      focusActive: () => {},
    } as unknown as TabArea;
  });

  it("asks first, naming the button, its command and the tab; Cancel types nothing", async () => {
    Object.assign(settings, { loaded: true, activeButtonBar: "", confirmButtons: true });
    render(ButtonBar, { customs: new Map() });
    const show = await screen.findByRole("button", { name: /show clock/ });
    reply = "Cancel";
    show.click();
    await expect.poll(() => asked.length).toBe(1);
    expect(asked[0]).toMatch(/^Run "show clock" \("show clock"\) in core-sw1\?/);
    await new Promise((r) => setTimeout(r, 50));
    expect(typed).toEqual([]);
    reply = "Run";
    show.click();
    await expect.poll(() => typed.join("")).toBe("show clock\r");
    // the click's run finishes (its status and focus) before the next test's tab area comes
    await new Promise((r) => setTimeout(r, 50));
  });

  it("runs at once with the question turned off (Settings > General > Button bar)", async () => {
    Object.assign(settings, { loaded: true, activeButtonBar: "", confirmButtons: false });
    render(ButtonBar, { customs: new Map() });
    (await screen.findByRole("button", { name: /show clock/ })).click();
    await expect.poll(() => typed.join("")).toBe("show clock\r");
    expect(asked).toEqual([]);
    await new Promise((r) => setTimeout(r, 50));
  });
});

describe("the button bar", () => {
  it("shows the bar for the tab's icon, then its profile's once the session starts", async () => {
    render(ButtonBar, { customs: new Map() });
    await expect.poll(shown).toBe("Cisco");
    // the session starts, using Lab switches
    active.profile = "Lab switches";
    listeners.forEach((l) => l());
    await expect.poll(shown).toBe("Lab");
    expect(buttons()).toEqual(["reload lab"]);
  });

  it("shows the session's own bar over its profile's, and follows Edit Session's change to it", async () => {
    active.profile = "Lab switches";
    // the session's own bar, as the Sessions tree gives it
    context.sessionBar = (key) => (key === "saved:Lab/sw1" ? "Cisco" : "");
    render(ButtonBar, { customs: new Map() });
    await expect.poll(shown).toBe("Cisco");
    // Edit Session takes it off and the tree reads the sessions again (the tree's list is reactive, as context is)
    context.sessionBar = () => "";
    await expect.poll(shown).toBe("Lab");
  });

  it("follows the tabs given after it's drawn, as App gives them in its onMount", async () => {
    // App's onMount runs after the bar's own effects: the bar first sees no tabs
    const tabs = context.tabs;
    context.tabs = null;
    active.profile = "Lab switches";
    render(ButtonBar, { customs: new Map() });
    await expect.poll(shown).toBe("Default");
    context.tabs = tabs;
    await expect.poll(shown).toBe("Lab");
  });

  it("shows the bar for the tab's icon again when the profile's bar is taken off", async () => {
    active.profile = "Lab switches";
    render(ButtonBar, { customs: new Map() });
    await expect.poll(shown).toBe("Lab");
    profiles = [emptyProfile(DEFAULT_PROFILE), emptyProfile("Lab switches")];
    window.dispatchEvent(new Event(PROFILES_CHANGED));
    await expect.poll(shown).toBe("Cisco");
  });
});

describe("a Rhai script button", () => {
  it("runs its script in the core as written, its braces its own rather than placeholders", async () => {
    const text = 'let n = 5;\nif wait_for("{x}", #{ timeout: n }) { status(`${n}`); }';
    const commands: string[] = [];
    let ran: { source: string; session: Record<string, string> } | null = null;
    mockIPC((cmd, args) => {
      commands.push(cmd);
      if (cmd === "buttonbars_load") return [{ name: "Default", buttons: [{ ...button("uptime"), action: "RhaiScript", text }], tabIcons: [] }];
      if (cmd === "profiles_list") return [];
      if (cmd === "rhai_run") {
        ran = args as typeof ran;
        return 1;
      }
      return null;
    });
    const host = { canReceiveCommands: true, markOutput: () => {}, connection: () => 1, type: async () => {} };
    context.tabs = {
      commandTabs: () => [{ id: "t1", title: "core-sw1", session: "", icon: "switch", profile: "", active: true, host } as unknown as CommandTab],
      onChange: () => () => {},
      focusActive: () => {},
    } as unknown as TabArea;
    Object.assign(settings, { loaded: true, activeButtonBar: "", confirmButtons: false });
    render(ButtonBar, { customs: new Map() });
    (await screen.findByRole("button", { name: /uptime/ })).click();
    await expect.poll(() => ran?.source).toBe(text);
    expect(commands).not.toContain("variables_fill");
    await new Promise((r) => setTimeout(r, 50));
  });
});

describe("a Rhai script button that reads Environment Variables", () => {
  it("offers Environment Variables for an env.name with no value, and doesn't run", async () => {
    const commands: string[] = [];
    const asked: string[] = [];
    mockIPC((cmd, args) => {
      commands.push(cmd);
      if (cmd === "buttonbars_load") return [{ name: "Default", buttons: [{ ...button("login"), action: "RhaiScript", text: "send_line(env.ad_user);\nsend_line(env.ad_pass);" }], tabIcons: [] }];
      if (cmd === "profiles_list") return [];
      if (cmd === "variables_fill") return { text: (args as { text: string }).text, missing: ["{ad_pass}"] };
      if (cmd === "plugin:dialog|message") {
        asked.push((args as { message: string }).message);
        return "Cancel";
      }
      return null;
    });
    const host = { canReceiveCommands: true, markOutput: () => {}, connection: () => 1, type: async () => {} };
    context.tabs = {
      commandTabs: () => [{ id: "t1", title: "core-sw1", session: "", icon: "switch", profile: "", active: true, host } as unknown as CommandTab],
      onChange: () => () => {},
      focusActive: () => {},
    } as unknown as TabArea;
    Object.assign(settings, { loaded: true, activeButtonBar: "", confirmButtons: false });
    render(ButtonBar, { customs: new Map() });
    (await screen.findByRole("button", { name: /login/ })).click();
    await expect.poll(() => asked.length).toBe(1);
    expect(asked[0]).toMatch(/\{ad_pass\} has no value\. Define it in Environment Variables\?/);
    await new Promise((r) => setTimeout(r, 50));
    expect(commands).not.toContain("rhai_run");
  });
});

describe("a script button's colour", () => {
  it("numbers the lines its SPSL script types in the button's colour, as a send string's", async () => {
    const log: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "buttonbars_load") return [{ name: "Default", buttons: [{ ...button("clock"), color: "#E5534B", action: "SpslScript", text: "SENDLINE show clock" }], tabIcons: [] }];
      if (cmd === "profiles_list") return [];
      if (cmd === "variables_fill") return { text: (args as { text: string }).text, missing: [] };
      return null;
    });
    const host = {
      canReceiveCommands: true,
      connection: () => 1,
      markOutput: () => {},
      position: () => 0,
      outputFrom: () => "",
      outputSinceMark: () => "",
      advanceMark: () => {},
      markCommand: (color: string) => log.push(`mark ${color}`),
      type: async (text: string) => void log.push(`type ${text}`),
    };
    context.tabs = {
      commandTabs: () => [{ id: "t1", title: "core-sw1", session: "", icon: "switch", profile: "", active: true, host } as unknown as CommandTab],
      onChange: () => () => {},
      focusActive: () => {},
    } as unknown as TabArea;
    Object.assign(settings, { loaded: true, activeButtonBar: "", confirmButtons: false });
    render(ButtonBar, { customs: new Map() });
    (await screen.findByRole("button", { name: /clock/ })).click();
    await expect.poll(() => log).toEqual(["mark #E5534B", "type show clock\r"]);
    await new Promise((r) => setTimeout(r, 100));
  });
});
