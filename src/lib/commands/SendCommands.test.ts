// Send Commands' history kept between runs, as SuperPuTTY's Command bar "Save history for N days" (Settings >
// General > Command bar): commands sent before are in the ▾ list and under Up when the bar opens, each command sent is
// saved while the days are more than 0, and with 0 (the default) none is, the history lasting until the app closes.
// Masked commands are never saved. The Rust side is stood in for by mockIPC: command_history_load gives the history
// saved, and command_history_add notes what's saved; the tabs by `context.tabs`, one tab noting what it's typed.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import SendCommands from "./SendCommands.svelte";
import { context, selection } from "./sending.svelte";
import { settings } from "../settings/store.svelte";
import type { CommandTab, TabArea } from "../layout/TabArea";

/** The commands saved, as command_history_add was given them */
let added: string[];
/** What the tab was typed */
let typed: string[];

beforeEach(() => {
  added = [];
  typed = [];
  mockIPC((cmd, args) => {
    if (cmd === "command_history_load") return ["show version", "show clock"];
    if (cmd === "command_history_add") return void added.push((args as { command: string }).command);
    return null;
  });
  const host = { canReceiveCommands: true, markOutput: () => {}, type: async (text: string) => void typed.push(text) };
  context.tabs = {
    commandTabs: () => [{ id: "t1", title: "sw1", session: "", icon: "switch", active: true, acceptCommands: true, host } as unknown as CommandTab],
    onChange: () => () => {},
    focusActive: () => {},
  } as unknown as TabArea;
});

afterEach(() => {
  context.tabs = null;
  settings.commandHistoryDays = 0;
});

/** The Command box */
const box = () => screen.getByRole("textbox", { name: "Command" }) as HTMLInputElement;

describe("Send Commands' history", () => {
  it("has the commands saved from before, newest first, under Up", async () => {
    const user = userEvent.setup();
    settings.commandHistoryDays = 7;
    render(SendCommands, { onOpenEditor: () => {} });
    await user.click(box());
    await expect.poll(async () => {
      await user.keyboard("{ArrowUp}");
      return box().value;
    }).toBe("show version");
    await user.keyboard("{ArrowUp}");
    expect(box().value).toBe("show clock");
  });

  it("saves each command sent while it's kept for days", async () => {
    const user = userEvent.setup();
    settings.commandHistoryDays = 7;
    render(SendCommands, { onOpenEditor: () => {} });
    await user.type(box(), "show ip route{Enter}");
    await expect.poll(() => typed).toEqual(["show ip route\r"]);
    expect(added).toEqual(["show ip route"]);
  });

  it("saves nothing with 0 days: the history lasts until the app closes", async () => {
    const user = userEvent.setup();
    settings.commandHistoryDays = 0;
    render(SendCommands, { onOpenEditor: () => {} });
    await user.type(box(), "show ip route{Enter}");
    await expect.poll(() => typed).toEqual(["show ip route\r"]);
    expect(added).toEqual([]);
  });

  it("never saves a masked command, such as a password", async () => {
    const user = userEvent.setup();
    settings.commandHistoryDays = 7;
    render(SendCommands, { onOpenEditor: () => {} });
    await user.click(screen.getByRole("button", { name: "Mask" }));
    await user.type(screen.getByLabelText("Command"), "S3cret!{Enter}");
    await expect.poll(() => typed).toEqual(["S3cret!\r"]);
    expect(added).toEqual([]);
  });
});

/**
 * Which tabs a command goes to, as SuperXPuTTY's GetSelectedPuttyPanels: a command typed once reaches several
 * devices, so the ones left out matter as much as the ones sent to. Five tabs stand in for the window's, each noting
 * what it's typed under its title.
 */
describe("A command sent", () => {
  /** What each tab was typed, by its title */
  let got: Record<string, string[]>;
  /** What the status bar was told */
  let said: string[];

  /** A tab for the window's list, typing into `got` */
  function tab(title: string, icon: string, is: { visible?: boolean; accepts?: boolean; typable?: boolean } = {}): CommandTab {
    const host = { canReceiveCommands: is.typable ?? true, markOutput: () => {}, type: async (text: string) => void (got[title] ??= []).push(text) };
    return { id: title, title, session: "", icon, active: false, visible: is.visible ?? true, acceptCommands: is.accepts ?? true, host } as unknown as CommandTab;
  }

  /** The titles of the tabs that were typed the command */
  const reached = () => Object.keys(got).sort();

  beforeEach(() => {
    got = {};
    said = [];
    context.status = (text) => void said.push(text);
    const tabs = [
      tab("sw1", "switch"),
      // behind another tab in its group
      tab("sw2", "switch", { visible: false }),
      // Accept commands unticked (Select sessions to send to)
      tab("rtr", "router", { accepts: false }),
      // a Remote Desktop: there's nothing to type a command into
      tab("desktop", "computer", { typable: false }),
      // its icon in a colour of its own
      tab("fw", "Switch#C62828"),
    ];
    context.tabs = { commandTabs: () => tabs, onChange: () => () => {}, focusActive: () => {} } as unknown as TabArea;
  });

  afterEach(() => {
    settings.sendToVisibleOnly = false;
    selection.iconGroup = "";
    context.status = () => {};
  });

  /** Type a command in the box and press Enter */
  async function send(command: string): Promise<void> {
    const user = userEvent.setup();
    render(SendCommands, { onOpenEditor: () => {} });
    await user.type(box(), `${command}{Enter}`);
  }

  it("goes to every tab that accepts commands, in front or not, and to none that doesn't or can't be typed into", async () => {
    await send("show clock");
    await expect.poll(reached).toEqual(["fw", "sw1", "sw2"]);
    expect(got.sw2).toEqual(["show clock\r"]);
  });

  it("goes only to the tabs showing with Send to visible tabs only", async () => {
    settings.sendToVisibleOnly = true;
    await send("show clock");
    // what shows is what's sent to, as SuperXPuTTY: Accept commands is for the tabs that don't show
    await expect.poll(reached).toEqual(["fw", "rtr", "sw1"]);
  });

  it("goes only to the tabs with the icon chosen, whatever its colour or case", async () => {
    selection.iconGroup = "switch";
    await send("show clock");
    await expect.poll(reached).toEqual(["fw", "sw1", "sw2"]);
    expect(got.rtr).toBeUndefined();
  });

  it("goes nowhere, and says so, when no tab is left to send to", async () => {
    selection.iconGroup = "router";
    await send("reload");
    await expect.poll(() => said).toContain("No selected sessions to send to");
    expect(reached()).toEqual([]);
  });
});
