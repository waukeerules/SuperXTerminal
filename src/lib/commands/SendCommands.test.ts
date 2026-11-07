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
import { context } from "./sending.svelte";
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
