// Settings > Layouts: the saved layouts listed ("<Auto Restore>" first, the default and the one open marked), and
// from there Load, Load in New Window, the tabs open now saved as a new layout or over one, Rename, Delete and the
// default layout. What needs the window's tabs (loading, saving, renaming the one open) is the App's, stood in for by
// `actions`, which records each call; the Rust side by mockIPC: the layouts are `stored`, deletes and settings saves
// recorded in `calls`, and every question answered with its OK button.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import LayoutsTab, { type LayoutActions } from "./LayoutsTab.svelte";
import { settings } from "./store.svelte";
import { AUTO_RESTORE } from "../layout/layouts";

/** The saved layouts' names, as layouts_list gives them (with "<Auto Restore>" put first) */
let stored: string[];
/** Every command sent, with its arguments */
let calls: { cmd: string; args: Record<string, unknown> }[];
/** What the App was asked to do */
let done: string[];

/** The App's side: each call recorded; a save adds the layout, a rename renames it */
const actions: LayoutActions = {
  load: async (name) => void done.push(`load ${name}`),
  loadInNewWindow: (name) => void done.push(`new window ${name}`),
  save: async (name) => {
    done.push(`save ${name}`);
    if (!stored.includes(name)) stored = [...stored, name];
  },
  rename: async (from, to) => {
    done.push(`rename ${from} to ${to}`);
    stored = stored.map((n) => (n === from ? to : n));
  },
};

beforeEach(() => {
  stored = ["Core switches", "Lab"];
  calls = [];
  done = [];
  settings.defaultLayoutName = "Lab";
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: payload });
    if (cmd === "layouts_list") return [AUTO_RESTORE, ...stored];
    if (cmd === "layout_delete") {
      stored = stored.filter((n) => n !== payload.name);
      return null;
    }
    if (cmd === "settings_save") return payload.settings;
    // ask(): OK, by its label
    if (cmd === "plugin:dialog|message") {
      const buttons = payload.buttons;
      return typeof buttons === "object" && buttons ? Object.values(buttons as Record<string, string[]>)[0][0] : "Yes";
    }
    return null;
  });
});

/** Render the tab, `current` being the layout open in the window, and wait for the list */
async function open(current: string | null = null) {
  render(LayoutsTab, { current, actions });
  await screen.findByRole("option", { name: /Core switches/ });
}

/** The list of layouts */
const list = () => within(screen.getByRole("listbox", { name: "Layouts" }));
/** A button by its name */
const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

describe("Settings > Layouts", () => {
  it("lists the layouts, <Auto Restore> first, the default and the one open marked", async () => {
    await open("Core switches");
    const names = list().getAllByRole("option").map((o) => o.textContent?.replace(/\s+/g, " ").trim());
    expect(names).toEqual([AUTO_RESTORE, "Core switches open now", "Lab default"]);
  });

  it("loads the layout chosen, here or in a new window", async () => {
    await open();
    await userEvent.click(list().getByRole("option", { name: /Core switches/ }));
    await userEvent.click(button("Load"));
    await userEvent.click(button("Load in New Window"));
    expect(done).toEqual(["load Core switches", "new window Core switches"]);
  });

  it("saves the tabs open now as a new layout, and asks before replacing one of that name", async () => {
    await open();
    await userEvent.type(screen.getByLabelText("New layout's name"), "Firewalls");
    await userEvent.click(button("Save Current Tabs as New"));
    expect(done).toEqual(["save Firewalls"]);
    expect(await list().findByRole("option", { name: /Firewalls/ })).toBeTruthy();
    await userEvent.type(screen.getByLabelText("New layout's name"), "lab");
    await userEvent.click(button("Save Current Tabs as New"));
    expect(String(calls.find((c) => c.cmd === "plugin:dialog|message")?.args.message)).toContain('Replace the layout "Lab"');
    expect(done).toEqual(["save Firewalls", "save Lab"]);
  });

  it("saves the tabs open now over the layout chosen, after asking", async () => {
    await open();
    await userEvent.click(list().getByRole("option", { name: /Core switches/ }));
    await userEvent.click(button("Save Current Tabs Here"));
    expect(String(calls.find((c) => c.cmd === "plugin:dialog|message")?.args.message)).toContain('Replace the layout "Core switches"');
    expect(done).toEqual(["save Core switches"]);
  });

  it("renames the layout chosen", async () => {
    await open();
    await userEvent.click(list().getByRole("option", { name: /Core switches/ }));
    await userEvent.click(button("Rename…"));
    const name = screen.getByLabelText("Layout's new name");
    await userEvent.clear(name);
    await userEvent.type(name, "Core{Enter}");
    expect(done).toEqual(["rename Core switches to Core"]);
    expect(await list().findByRole("option", { name: "Core" })).toBeTruthy();
  });

  it("deletes the layout chosen after asking, but never <Auto Restore> or the default", async () => {
    await open();
    await userEvent.click(list().getByRole("option", { name: AUTO_RESTORE }));
    expect(button("Delete").disabled).toBe(true);
    expect(button("Rename…").disabled).toBe(true);
    await userEvent.click(list().getByRole("option", { name: /Lab/ }));
    expect(button("Delete").disabled).toBe(true);
    await userEvent.click(list().getByRole("option", { name: /Core switches/ }));
    await userEvent.click(button("Delete"));
    expect(calls.find((c) => c.cmd === "layout_delete")?.args).toEqual({ name: "Core switches" });
    await expect.poll(() => list().queryByRole("option", { name: /Core switches/ })).toBeNull();
  });

  it("sets the layout chosen as the one opened at start, or none", async () => {
    await open();
    await userEvent.click(list().getByRole("option", { name: /Core switches/ }));
    await userEvent.click(button("Set as Default"));
    expect(calls.filter((c) => c.cmd === "settings_save").at(-1)?.args.settings).toMatchObject({ defaultLayoutName: "Core switches" });
    await userEvent.click(button("No Default"));
    expect(calls.filter((c) => c.cmd === "settings_save").at(-1)?.args.settings).toMatchObject({ defaultLayoutName: "" });
  });
});
