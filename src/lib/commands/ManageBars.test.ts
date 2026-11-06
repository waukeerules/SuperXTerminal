// Manage Button Bars, opened from the button bar's right-click menu: a bar imported again as "Cisco (2)" is merged
// back by its hint, a duplicate button is taken off, a button moves to another bar, buttons and bars are duplicated
// as Name-1, Name-2..., and only Save writes
// ButtonBars.xml. The Rust side is stood in for by mockIPC: the bars live in `stored`, which buttonbars_save replaces,
// and the dialog plugin's "Leave without saving?" answers `sure`.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import ButtonBar from "./ButtonBar.svelte";
import type { Bar, BarButton } from "./bars";

/** A button sending `text`, labelled `label` */
const button = (label: string, text = `${label}\n`): BarButton => ({ label, tooltip: "", color: "", icon: "", action: "SendString", text });

/** ButtonBars.xml's bars */
let stored: Bar[];
/** How many times ButtonBars.xml was written */
let saves: number;
/** The answer to "Leave without saving?" */
let sure: boolean;

beforeEach(() => {
  stored = [
    { name: "Cisco", buttons: [button("show run"), button("wr")], tabIcons: [] },
    { name: "Linux", buttons: [button("ls"), button("ls"), button("show run")], tabIcons: [] },
    { name: "Cisco (2)", buttons: [button("show run"), button("show ip int br"), button("wr", "write memory\n")], tabIcons: [] },
  ];
  saves = 0;
  sure = true;
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "buttonbars_load") return structuredClone(stored);
    if (cmd === "buttonbars_save") {
      saves++;
      stored = structuredClone(payload.bars as Bar[]);
      return null;
    }
    // ask() shows a message with the buttons given, and is true when the answer is its OK button's label
    if (cmd === "plugin:dialog|message") return sure ? "Leave" : "Cancel";
    return null;
  });
});

/** Open Manage Button Bars from the bar's right-click menu */
async function openManager(user: ReturnType<typeof userEvent.setup>): Promise<HTMLElement> {
  render(ButtonBar, { customs: new Map() });
  await user.pointer({ keys: "[MouseRight]", target: screen.getByRole("toolbar", { name: "Button bar" }) });
  await user.click(await screen.findByRole("menuitem", { name: "Manage Button Bars…" }));
  return screen.findByRole("dialog", { name: "Manage Button Bars" });
}

/** The buttons listed for the chosen bar, by label, with their mark if any */
function listed(dialog: HTMLElement): string[] {
  return within(dialog).getAllByRole("listitem").map((row) => {
    const mark = row.querySelector(".mark")?.textContent;
    return row.querySelector(".name")!.textContent + (mark ? ` [${mark}]` : "");
  });
}

describe("Manage Button Bars", () => {
  it("merges a bar imported again into the one it copies, keeping the button that differs, marked", async () => {
    const user = userEvent.setup();
    const dialog = await openManager(user);
    expect(within(dialog).getByText(/"Cisco \(2\)" looks like a copy of "Cisco"/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: 'Merge into "Cisco"' }));
    expect(within(dialog).getAllByRole("option").map((o) => o.querySelector(".name")!.textContent)).toEqual(["Cisco", "Linux"]);
    expect(listed(dialog)).toEqual(["show run", "wr [Same label]", "show ip int br", "wr [Same label]"]);
    expect(within(dialog).getByRole("status").textContent).toContain("2 buttons added, 1 already there");
    expect(saves).toBe(0);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(stored.map((b) => b.name)).toEqual(["Cisco", "Linux"]);
    expect(stored[0].buttons.map((b) => b.text)).toEqual(["show run\n", "wr\n", "show ip int br\n", "write memory\n"]);
  });

  it("takes off a duplicate button, and moves a ticked one to another bar", async () => {
    const user = userEvent.setup();
    const dialog = await openManager(user);
    await user.click(within(dialog).getByRole("option", { name: /Linux/ }));
    expect(listed(dialog)).toEqual(["ls", "ls [Duplicate]", "show run"]);
    await user.click(within(dialog).getByRole("button", { name: "Remove Duplicates" }));
    expect(listed(dialog)).toEqual(["ls", "show run"]);
    // "show run" is already on Cisco: moving it takes it off Linux without doubling it there
    await user.click(within(dialog).getByRole("checkbox", { name: 'Tick "show run"' }));
    await user.click(within(dialog).getByRole("button", { name: "Move to ▾" }));
    await user.click(await screen.findByRole("menuitem", { name: "Cisco" }));
    expect(listed(dialog)).toEqual(["ls"]);
    expect(within(dialog).getByRole("status").textContent).toContain('Moved 0 buttons to "Cisco"; 1 button already there');
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(stored.find((b) => b.name === "Linux")!.buttons.map((b) => b.label)).toEqual(["ls"]);
    expect(stored.find((b) => b.name === "Cisco")!.buttons.map((b) => b.label)).toEqual(["show run", "wr"]);
  });

  it("copies a button to another bar", async () => {
    const user = userEvent.setup();
    const dialog = await openManager(user);
    await user.click(within(dialog).getByRole("checkbox", { name: 'Tick "wr"' }));
    await user.click(within(dialog).getByRole("button", { name: "Copy to ▾" }));
    await user.click(await screen.findByRole("menuitem", { name: "Linux" }));
    expect(listed(dialog)).toEqual(["show run", "wr"]);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(stored.find((b) => b.name === "Linux")!.buttons.map((b) => b.label)).toEqual(["ls", "ls", "show run", "wr"]);
  });

  it("duplicates the ticked buttons beside themselves as -1, then -2, ticking the copies", async () => {
    const user = userEvent.setup();
    const dialog = await openManager(user);
    await user.click(within(dialog).getByRole("checkbox", { name: 'Tick "show run"' }));
    await user.click(within(dialog).getByRole("checkbox", { name: 'Tick "wr"' }));
    await user.click(within(dialog).getByRole("button", { name: "Duplicate" }));
    expect(listed(dialog)).toEqual(["show run", "show run-1", "wr", "wr-1"]);
    expect((within(dialog).getByRole("checkbox", { name: 'Tick "show run-1"' }) as HTMLInputElement).checked).toBe(true);
    expect((within(dialog).getByRole("checkbox", { name: 'Tick "show run"' }) as HTMLInputElement).checked).toBe(false);
    expect(within(dialog).getByRole("status").textContent).toContain("Duplicated 2 buttons");
    // a second copy of the same button takes the next number free
    await user.click(within(dialog).getByRole("checkbox", { name: 'Tick "show run-1"' }));
    await user.click(within(dialog).getByRole("checkbox", { name: 'Tick "wr-1"' }));
    await user.click(within(dialog).getByRole("checkbox", { name: 'Tick "show run"' }));
    await user.click(within(dialog).getByRole("button", { name: "Duplicate" }));
    expect(listed(dialog)).toEqual(["show run", "show run-2", "show run-1", "wr", "wr-1"]);
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    const cisco = stored.find((b) => b.name === "Cisco")!.buttons;
    expect(cisco.map((b) => b.label)).toEqual(["show run", "show run-2", "show run-1", "wr", "wr-1"]);
    expect(cisco[1].text).toBe("show run\n");
  });

  it("copies a bar as Name-1, then Name-2", async () => {
    const user = userEvent.setup();
    const dialog = await openManager(user);
    const bars = () => within(dialog).getAllByRole("option").map((o) => o.querySelector(".name")!.textContent);
    await user.click(within(dialog).getByRole("button", { name: "Copy" }));
    await user.click(within(dialog).getAllByRole("option").find((o) => o.querySelector(".name")!.textContent === "Cisco")!);
    await user.click(within(dialog).getByRole("button", { name: "Copy" }));
    expect(bars()).toEqual(["Cisco", "Cisco-2", "Cisco-1", "Linux", "Cisco (2)"]);
  });

  it("writes nothing on Cancel", async () => {
    const user = userEvent.setup();
    const dialog = await openManager(user);
    await user.click(within(dialog).getByRole("button", { name: "Delete bar" }));
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await expect.poll(() => screen.queryByRole("dialog", { name: "Manage Button Bars" })).toBeNull();
    expect(saves).toBe(0);
  });

  it("stays open with the changes when Leave is declined", async () => {
    sure = false;
    const user = userEvent.setup();
    const dialog = await openManager(user);
    await user.click(within(dialog).getByRole("button", { name: "Delete bar" }));
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.getByRole("dialog", { name: "Manage Button Bars" })).toBe(dialog);
    expect(within(dialog).getAllByRole("option")).toHaveLength(2);
  });
});
