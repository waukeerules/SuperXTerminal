// The button bar at the bottom of the window shows the active tab's profile's bar (Settings > Profiles > Button bar)
// over the bar for the tab's icon, once the tab's session has started and says which profile it uses, and again when
// the profile is saved. The tab area is stood in for by `context.tabs` (the active tab, and its change listeners);
// the Rust side by mockIPC: ButtonBars.xml's bars and the profiles stored.
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
  } as unknown as TabArea;
});

afterEach(() => {
  context.tabs = null;
});

/** The bar showing: the bar picker's choice */
const shown = () => (screen.getByTitle("Button bar") as HTMLSelectElement).value;
/** The labels of the buttons showing */
const buttons = () => [...document.querySelectorAll(".bar-button")].map((b) => b.textContent?.trim());

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

  it("shows the bar for the tab's icon again when the profile's bar is taken off", async () => {
    active.profile = "Lab switches";
    render(ButtonBar, { customs: new Map() });
    await expect.poll(shown).toBe("Lab");
    profiles = [emptyProfile(DEFAULT_PROFILE), emptyProfile("Lab switches")];
    window.dispatchEvent(new Event(PROFILES_CHANGED));
    await expect.poll(shown).toBe("Cisco");
  });
});
