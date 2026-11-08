// The Script Editor's saved scripts: one double-clicked (or Insert) into an empty box takes Run as with it, so an
// SPSL script is coloured, checked and sent as SPSL even when new scripts start as Rhai; one put into a script
// already there leaves Run as alone. The Rust side (the scripts folder, Settings) is stood in for by mockIPC, and the
// script box by its plain box, as it is until Monaco's page is up.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import ScriptEditor from "./ScriptEditor.svelte";
import { settings } from "../settings/store.svelte";

/** The saved scripts, as the scripts folder has them, by name and kind */
const SCRIPTS: Record<string, string> = {
  "jump-host:spsl": "#!/bin/spsl\nWAITFOR #\nSENDLINE hostname",
  "uptime:rhai": "wait_prompt();\nsend_line(\"uptime\");",
};

beforeEach(() => {
  Element.prototype.scrollIntoView = () => {};
  // new scripts start as Rhai, as Settings > General > Scripts has it unless changed
  settings.scriptLanguage = "rhai";
  mockIPC((cmd, args) => {
    if (cmd === "scripts_list") return [{ name: "jump-host", kind: "spsl" }, { name: "uptime", kind: "rhai" }];
    if (cmd === "script_load") {
      const { name, kind } = args as { name: string; kind: string };
      return SCRIPTS[`${name}:${kind}`];
    }
    return null;
  });
});

/** Run as, as the footer shows it */
const runAs = () => (screen.getByRole("combobox", { name: /Run as/ }) as HTMLSelectElement).value;
/** The script, as the box has it */
const script = () => (screen.getByLabelText("Script") as HTMLTextAreaElement).value;

describe("the Script Editor's saved scripts", () => {
  it("runs a saved SPSL script double-clicked into an empty box as SPSL, though new scripts start as Rhai", async () => {
    const user = userEvent.setup();
    render(ScriptEditor, { onClose: () => {} });
    expect(runAs()).toBe("rhai");
    await user.dblClick(await screen.findByRole("option", { name: /jump-host/ }));
    await expect.poll(script).toContain("WAITFOR #");
    expect(runAs()).toBe("spsl");
  });

  it("leaves Run as alone for a saved script put into a script already there, which it's part of", async () => {
    const user = userEvent.setup();
    render(ScriptEditor, { onClose: () => {} });
    await user.dblClick(await screen.findByRole("option", { name: /uptime/ }));
    await expect.poll(script).toContain("wait_prompt();");
    expect(runAs()).toBe("rhai");
    await user.click(screen.getByRole("option", { name: /jump-host/ }));
    await user.click(screen.getByRole("button", { name: "Insert" }));
    await expect.poll(script).toContain("WAITFOR #");
    expect(runAs()).toBe("rhai");
  });
});
