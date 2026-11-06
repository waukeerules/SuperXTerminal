// Edit Button with a script: the script panel beside it lists Rhai's functions for a Rhai script (SPSL's commands
// for an SPSL one), and the script's problems show under its box as it's written, with no Check script to press:
// a mistake typed appears a moment later, goes when it's fixed, and clicking it puts the cursor on its line. OK asks
// before saving a script that has one. A Rhai script's env.name with no value in Environment Variables is warned of.
// The panel's Saved scripts: Edit… opens one to change and save again (Escape leaving Edit Button open), Save this
// script as… keeps the button's script as a new one (asking before replacing one), and Delete asks first.
// The Rust side (rhai_check; the sessions, the saved scripts in `library`, Environment Variables in `env`; the dialog
// plugin's ask) is stood in for by mockIPC; the script box's page is never ready in jsdom, so each script box is its
// plain textarea.
import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { mockIPC } from "@tauri-apps/api/mocks";
import EditButton from "./EditButton.svelte";
import type { BarButton } from "./bars";

/** What rhai_check answers for a script: its problems */
let rhaiProblems: (text: string) => { line: number; message: string }[];
/** The saved scripts, by "name.kind", as the scripts folder holds them */
let library: Map<string, string>;
/** Environment Variables with values, by lower-case name */
let env: Record<string, string>;
/** The questions the dialog plugin was asked, each answered with `reply` */
let asked: string[];
let reply: string;

beforeEach(() => {
  rhaiProblems = () => [];
  library = new Map();
  env = {};
  asked = [];
  reply = "Cancel";
  mockIPC((cmd, args) => {
    if (cmd === "rhai_check") return rhaiProblems((args as { source: string }).source);
    if (cmd === "sessions_load") return { sessions: [], folders: [] };
    const listed = () => [...library.keys()].sort().map((k) => ({ name: k.slice(0, k.lastIndexOf(".")), kind: k.slice(k.lastIndexOf(".") + 1) }));
    if (cmd === "scripts_list") return listed();
    if (cmd === "script_load") {
      const { name, kind } = args as { name: string; kind: string };
      return library.get(`${name}.${kind}`);
    }
    if (cmd === "script_save") {
      const { name, kind, text } = args as { name: string; kind: string; text: string };
      for (const key of [...library.keys()]) if (key.slice(0, key.lastIndexOf(".")).toLowerCase() === name.toLowerCase()) library.delete(key);
      library.set(`${name}.${kind}`, text);
      return listed();
    }
    if (cmd === "script_delete") {
      const { name, kind } = args as { name: string; kind: string };
      library.delete(`${name}.${kind}`);
      return listed();
    }
    if (cmd === "variables_fill") {
      const text = (args as { text: string }).text;
      return { text, missing: (text.match(/\{\w+\}/g) ?? []).filter((m) => !(m.slice(1, -1).toLowerCase() in env)) };
    }
    if (cmd === "plugin:dialog|message") {
      asked.push((args as { message: string }).message);
      return reply;
    }
    return null;
  });
});

/** A script button of an action */
const scriptButton = (text: string, action = "RhaiScript"): BarButton => ({ label: "Uptime", tooltip: "", color: "", icon: "", action, text });

/** How many times Edit Button was cancelled (its Cancel, or Escape) */
let cancelled = 0;

/** Edit a button; what OK saved, as it's saved */
function edit(button: BarButton) {
  const saved: BarButton[] = [];
  cancelled = 0;
  render(EditButton, { button, customs: new Map(), onSave: (b: BarButton) => saved.push(b), onCancel: () => cancelled++ });
  return saved;
}

/** The script box (its plain textarea in jsdom) */
const scriptBox = () => screen.getByRole("textbox", { name: "Script" }) as HTMLTextAreaElement;
/** What the problems under the script say */
const problemsShown = () => screen.getByRole("region", { name: "Problems" }).textContent?.replace(/\s+/g, " ").trim();

describe("Edit Button with a Rhai script", () => {
  it("shows Rhai's functions beside the script, not SPSL's commands, and SPSL's again for an SPSL script", async () => {
    edit(scriptButton("wait_prompt();"));
    expect(screen.getByRole("combobox", { name: /Action/ })).toHaveProperty("value", "RhaiScript");
    expect(screen.getByText('send_line("text")')).toBeTruthy();
    expect(screen.getByText("wait_prompt()")).toBeTruthy();
    expect(screen.queryByText("SENDLINE text")).toBeNull();
    // no Check script tab: the problems are under the script instead
    expect(screen.queryByRole("tab", { name: "Check script" })).toBeNull();

    await fireEvent.change(screen.getByRole("combobox", { name: /Action/ }), { target: { value: "SpslScript" } });
    expect(screen.getByText("SENDLINE text")).toBeTruthy();
    expect(screen.queryByText('send_line("text")')).toBeNull();
  });

  it("shows a problem under the script as it's typed, and takes it away once it's fixed", async () => {
    rhaiProblems = (text) => (text.includes("hots") ? [{ line: 2, message: "Variable not found: hots" }] : []);
    edit(scriptButton("let host = 1;"));
    await expect.poll(problemsShown).toBe("No problems found.");

    await fireEvent.input(scriptBox(), { target: { value: "let host = 1;\nsend_line(hots);" } });
    await expect.poll(problemsShown).toMatch(/^1 problem .*Line 2: Variable not found: hots$/);

    await fireEvent.input(scriptBox(), { target: { value: "let host = 1;\nsend_line(host);" } });
    await expect.poll(problemsShown).toBe("No problems found.");
  });

  it("puts the cursor on a problem's line when it's clicked", async () => {
    rhaiProblems = () => [{ line: 3, message: "Variable not found: hots" }];
    edit(scriptButton("let a = 1;\nlet b = 2;\nsend_line(hots);"));
    (await screen.findByRole("button", { name: /Line 3: Variable not found/ })).click();
    // the third line starts after "let a = 1;\n" and "let b = 2;\n"
    expect(scriptBox().selectionStart).toBe(22);
    expect(document.activeElement).toBe(scriptBox());
  });

  it("asks before OK saves a script with a problem, and saves one without", async () => {
    let broken = true;
    rhaiProblems = () => (broken ? [{ line: 2, message: "Variable not found: hots" }] : []);
    const saved = edit({ ...scriptButton("let host = 1;\nsend_line(hots);"), label: "Go" });

    screen.getByRole("button", { name: "OK" }).click();
    await expect.poll(() => asked.length).toBe(1);
    expect(asked[0]).toMatch(/1 problem \(listed under it\)/);
    expect(saved).toEqual([]);

    broken = false;
    screen.getByRole("button", { name: "OK" }).click();
    await expect.poll(() => saved.length).toBe(1);
    expect(saved[0]).toMatchObject({ action: "RhaiScript", text: "let host = 1;\nsend_line(hots);" });
  });
});

describe("Edit Button with an SPSL script", () => {
  it("shows an SPSL mistake under the script as it's typed", async () => {
    edit(scriptButton("WAITFOR #", "SpslScript"));
    await expect.poll(problemsShown).toBe("No problems found.");
    await fireEvent.input(scriptBox(), { target: { value: "WAITFOR #\nSENDLIN show version" } });
    await expect.poll(problemsShown).toMatch(/Line 2: "SENDLIN" isn't an SPSL command/);
  });
});

describe("Environment Variables in a Rhai script", () => {
  it("warns under the script of an env.name with no value, and not once it has one", async () => {
    env = { ad_user: "jdoe" };
    edit(scriptButton("send_line(env.ad_user);\n// env.commented isn't used\nsend_line(env[\"Jump_Host\"]);"));
    await expect.poll(problemsShown).toMatch(/1 warning .*Line 3: env\.jump_host has no value in Environment Variables/);
    expect(problemsShown()).not.toMatch(/ad_user|commented/);
  });
});

describe("session values read as Environment Variables", () => {
  it("says under the script that env.host_address is session.host_address", async () => {
    edit(scriptButton("send_line(`ssh ${env.host_address}`);"));
    await expect.poll(problemsShown).toMatch(/1 problem .*Line 1: env\.host_address: host_address is the tab's session's, not an Environment Variable: write session\.host_address/);
  });
});

describe("the panel's Saved scripts", () => {
  /** Open the Saved scripts tab and choose a script in it */
  async function choose(name: string) {
    screen.getByRole("tab", { name: "Saved scripts" }).click();
    (await screen.findByRole("option", { name: new RegExp(`^${name}`) })).click();
  }

  it("edits a saved script and saves it again, leaving the button's own script alone", async () => {
    library.set("uptime.rhai", "wait_prompt();");
    edit(scriptButton("send_line(\"show clock\");"));
    await choose("uptime");
    screen.getByRole("button", { name: "Edit…" }).click();
    const editor = await screen.findByRole("dialog", { name: "Edit saved script" });
    const box = within(editor).getByRole("textbox", { name: "Saved script" }) as HTMLTextAreaElement;
    expect(box.value).toBe("wait_prompt();");
    await fireEvent.input(box, { target: { value: "wait_prompt();\nsend_line(\"show version\");" } });
    within(editor).getByRole("button", { name: "Save" }).click();
    await expect.poll(() => library.get("uptime.rhai")).toBe("wait_prompt();\nsend_line(\"show version\");");
    expect(screen.queryByRole("dialog", { name: "Edit saved script" })).toBeNull();
    expect(scriptBox().value).toBe("send_line(\"show clock\");");
    expect(await screen.findByRole("status")).toHaveProperty("textContent", 'Saved "uptime"');
  });

  it("Escape closes the saved script being edited, not Edit Button", async () => {
    library.set("uptime.rhai", "wait_prompt();");
    edit(scriptButton("x();"));
    await choose("uptime");
    screen.getByRole("button", { name: "Edit…" }).click();
    await screen.findByRole("dialog", { name: "Edit saved script" });
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Edit saved script" })).toBeNull();
    expect(cancelled).toBe(0);
    expect(library.get("uptime.rhai")).toBe("wait_prompt();");
  });

  it("saves the button's script as a new saved script, of its language", async () => {
    edit(scriptButton("send_line(\"show clock\");"));
    screen.getByRole("tab", { name: "Saved scripts" }).click();
    (await screen.findByRole("button", { name: "Save this script as…" })).click();
    const name = await screen.findByRole("textbox", { name: "" });
    await fireEvent.input(name, { target: { value: "clock" } });
    await fireEvent.submit(name.closest("form")!);
    await expect.poll(() => library.get("clock.rhai")).toBe("send_line(\"show clock\");");
    expect(await screen.findByRole("option", { name: /^clock/ })).toHaveProperty("ariaSelected", "true");
    expect(asked).toEqual([]);
  });

  it("asks before Save this script as… replaces a saved script of that name, and keeps it on Cancel", async () => {
    library.set("Login.spsl", "WAITFOR #");
    edit(scriptButton("send_line(\"x\");"));
    screen.getByRole("tab", { name: "Saved scripts" }).click();
    (await screen.findByRole("button", { name: "Save this script as…" })).click();
    const name = await screen.findByRole("textbox", { name: "" });
    await fireEvent.input(name, { target: { value: "login" } });
    reply = "Cancel";
    await fireEvent.submit(name.closest("form")!);
    await expect.poll(() => asked.length).toBe(1);
    expect(asked[0]).toMatch(/Replace the saved SPSL script "Login"/);
    expect([...library.keys()]).toEqual(["Login.spsl"]);
  });

  it("deletes a saved script only after asking", async () => {
    library.set("old.rhai", "x();");
    edit(scriptButton("y();"));
    await choose("old");
    reply = "Cancel";
    screen.getByRole("button", { name: "Delete" }).click();
    await expect.poll(() => asked.length).toBe(1);
    expect(library.has("old.rhai")).toBe(true);
    reply = "Delete";
    screen.getByRole("button", { name: "Delete" }).click();
    await expect.poll(() => library.has("old.rhai")).toBe(false);
  });
});

describe("Wrap lines in Edit Button", () => {
  it("is on the Icon row, once, rather than in a row of its own over the script", async () => {
    edit(scriptButton("x();"));
    const wraps = screen.getAllByRole("checkbox", { name: /Wrap lines/ });
    expect(wraps).toHaveLength(1);
    expect(wraps[0].closest(".icon-row")).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "Script" }).closest(".script-box")?.querySelector(".bar")).toBeNull();
  });
});
