// Settings > General > Scripts, "New scripts are": what a new script starts as. The Script Editor's Run as and a new
// button's Action start on Rhai until another choice is made there, then on that choice (SPSL, or lines to send); a
// button already made keeps its own Action. The choice is made in the Settings window itself, then the Script Editor
// and New Button are opened, as the user would. The Rust side (settings_save, the saved scripts' list, the sessions)
// is stood in for by mockIPC, which saves the settings as the core does.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import Settings from "./Settings.svelte";
import ScriptEditor from "../commands/ScriptEditor.svelte";
import EditButton from "../commands/EditButton.svelte";
import { emptyProfile, DEFAULT_PROFILE } from "./profiles";
import { settings } from "./store.svelte";
import type { BarButton } from "../commands/bars";

beforeEach(() => {
  Element.prototype.scrollIntoView = () => {};
  settings.scriptLanguage = "rhai";
  mockIPC((cmd, args) => {
    if (cmd === "profiles_list") return [emptyProfile(DEFAULT_PROFILE)];
    if (cmd === "shortcuts_keys" || cmd === "shortcuts_reserved" || cmd === "scripts_list") return [];
    if (cmd === "highlighting_load") return { themes: [], ruleSets: [], problems: [] };
    if (cmd === "sessions_load") return { sessions: [], folders: [] };
    if (cmd === "rhai_check") return [];
    if (cmd === "settings_save") return (args as { settings: Record<string, unknown> }).settings;
    return null;
  });
});

afterEach(() => {
  settings.scriptLanguage = "rhai";
});

/** Choose what new scripts are in Settings > General, wait for it to be saved as `saved`, then close the window */
async function choose(label: string, saved: string): Promise<void> {
  const user = userEvent.setup();
  render(Settings, { onClose: () => {} });
  await user.selectOptions(screen.getByRole("combobox", { name: "New scripts are" }), label);
  await expect.poll(() => settings.scriptLanguage).toBe(saved);
  cleanup();
}

/** The Script Editor's Run as, as it opens */
function runAs(): string {
  render(ScriptEditor, { onClose: () => {} });
  const chosen = (screen.getByRole("combobox", { name: "Run as:" }) as HTMLSelectElement).selectedOptions[0].text;
  cleanup();
  return chosen;
}

/** Edit Button's Action, for a new button (null) or one already made */
function action(button: BarButton | null = null): string {
  render(EditButton, { button, customs: new Map(), onSave: () => {}, onCancel: () => {} });
  const chosen = (screen.getByRole("combobox", { name: "Action:" }) as HTMLSelectElement).selectedOptions[0].text;
  cleanup();
  return chosen;
}

describe("New scripts are (Settings > General > Scripts)", () => {
  it("starts the Script Editor and a new button on Rhai until something else is chosen", () => {
    expect(runAs()).toBe("Rhai script");
    expect(action()).toBe("Run Rhai script");
  });

  it("starts them on SPSL once SPSL scripts is chosen", async () => {
    await choose("SPSL scripts", "spsl");
    expect(runAs()).toBe("SPSL script");
    expect(action()).toBe("Run SPSL script");
  });

  it("starts them on lines to send once that's chosen: Send lines, and a button's Send string", async () => {
    await choose("Lines to send", "lines");
    expect(runAs()).toBe("Send lines");
    expect(action()).toBe("Send string");
  });

  it("leaves a button already made with its own Action", () => {
    const made: BarButton = { label: "Clock", tooltip: "", color: "", icon: "", action: "SendString", text: "show clock" };
    expect(action(made)).toBe("Send string");
  });
});
