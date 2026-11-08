// Tools > Environment Variables: names that clash can't be saved, placeholders used but not set are rows of their own
// that a click adds, a 1Password reference shows as its item, a row's ⋯ menu marks it Secret or removes it, the
// header's boxes filter by name and by value, the variables set scroll in a box of their own, the preview (folded
// away) fills a template with secrets hidden, and Export… and Import… pass the variables on in SuperXPuTTY's file,
// secrets by name only. The Rust side, and the dialogs choosing the file, are stood in for by mockIPC.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import EnvironmentVariables from "./EnvironmentVariables.svelte";

/** What the window last saved (variables_save), or null if it hasn't */
let saved: unknown = null;
/** What Export… wrote (text_file_write), or null if it hasn't */
let written: { path: string; text: string } | null = null;
/** The file Import… reads (text_file_read) */
let importing = "";

beforeEach(() => {
  saved = null;
  written = null;
  importing = "";
  mockIPC((cmd, args) => {
    if (cmd === "plugin:dialog|save") return "/tmp/EnvironmentVariables.json";
    if (cmd === "plugin:dialog|open") return "/tmp/theirs.json";
    if (cmd === "text_file_write") {
      written = args as { path: string; text: string };
      return null;
    }
    if (cmd === "text_file_read") return importing;
    if (cmd === "variables_list")
      return [
        { key: "ad_username", value: "tim", secret: false, hasValue: false, fromWindows: false },
        { key: "admin_password", value: "", secret: true, hasValue: true, fromWindows: false },
        { key: "enable", value: '"op://Private/Cisco Switch/password"', secret: false, hasValue: false, fromWindows: false },
      ];
    if (cmd === "variables_usage")
      return {
        ad_username: ["profile CyberArk"],
        cyberark_region: ["profile CyberArk", "session Region 4/Switch - CyberArk"],
        admin_username: [
          "session Customers/North America/Region 4/Building 12/Floor 3/Core Switch - with a very long name indeed",
          "session Live Test/Lab Switch",
          "session Live Test/Lab Switchsdfsdf",
          "session Region 4/Switch - with admin password",
          "profile Lab",
        ],
        host_address: ["profile CyberArk"],
      };
    if (cmd === "variables_built_in") return [["host_address", "The session's own host"], ["host_port", "Its port"], ["key_file", "The key"]];
    if (cmd === "variables_save") {
      saved = (args as { variables: unknown }).variables;
      return [];
    }
    return null;
  });
});

/** Open the window, and wait until the stored variables are shown */
async function open() {
  render(EnvironmentVariables, { onClose: () => {} });
  await screen.findByDisplayValue("ad_username");
}

/** Every Key field, in order */
const keys = () => screen.getAllByLabelText("Key") as HTMLInputElement[];
/** The Save button */
const ok = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;
/** Add, at the foot of the window */
const addButton = () => screen.getByRole("button", { name: "Add" });

describe("Environment Variables", () => {
  it("shows placeholders used but not set as rows of their own, not built-in ones, and a click adds one ready to type", async () => {
    await open();
    expect(screen.getByRole("note", { name: "Used but not set" })).toBeTruthy();
    const set = screen.getByRole("button", { name: "Set cyberark_region" });
    // host_address is used too, but built in: no row for it
    expect(screen.queryByRole("button", { name: "Set host_address" })).toBeNull();
    // where it's used: how many places, each on hover
    const used = set.closest(".row")!.querySelector(".used")!;
    expect(used.textContent).toBe("2 places");
    expect(used.getAttribute("title")).toBe("profile CyberArk\nsession Region 4/Switch - CyberArk");
    await userEvent.click(set);
    expect(keys().map((k) => k.value)).toContain("cyberark_region");
    expect(screen.queryByRole("button", { name: "Set cyberark_region" })).toBeNull();
    expect((document.activeElement as HTMLInputElement).getAttribute("aria-label")).toBe("Value");
  });

  it("shows a 1Password reference as the item it names, and its text to edit on a click", async () => {
    await open();
    const reference = screen.getByRole("button", { name: /Private\/Cisco Switch/ });
    expect(reference.textContent).toBe("Private/Cisco Switch1Password");
    await userEvent.click(reference);
    expect(screen.getByDisplayValue('"op://Private/Cisco Switch/password"')).toBe(document.activeElement);
  });

  it("marks a variable Secret, and removes one, from its ⋯ menu", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "More for ad_username" }));
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: /Secret/ }));
    expect(screen.getAllByLabelText("Secret value")).toHaveLength(2);
    await userEvent.click(screen.getByRole("button", { name: "More for ad_username" }));
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Remove" }));
    expect(keys().map((k) => k.value)).toEqual(["admin_password", "enable"]);
  });

  it("filters by name and by value, each in its column's header", async () => {
    await open();
    await userEvent.type(screen.getByLabelText("Filter by name"), "admin");
    expect(keys().map((k) => k.value)).toEqual(["admin_password"]);
    // a placeholder not set, by its name
    expect(screen.getByRole("button", { name: "Set admin_username" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Set cyberark_region" })).toBeNull();
    await userEvent.clear(screen.getByLabelText("Filter by name"));
    // by value: a reference by its text; a secret's value isn't searched, and placeholders not set have none
    await userEvent.type(screen.getByLabelText("Filter by value"), "cisco");
    expect(keys().map((k) => k.value)).toEqual(["enable"]);
    expect(screen.queryByRole("button", { name: /^Set / })).toBeNull();
    await userEvent.clear(screen.getByLabelText("Filter by value"));
    await userEvent.type(screen.getByLabelText("Filter by value"), "nothing like it");
    expect(screen.getByText("No variable matches.")).toBeTruthy();
  });

  it("keeps the variables set in a box of their own, which scrolls, with the ones not set below it", async () => {
    await open();
    const box = screen.getByRole("group", { name: "Variables" });
    expect(box.classList.contains("rows")).toBe(true);
    expect([...box.querySelectorAll("[data-key]")].map((k) => (k as HTMLInputElement).value)).toEqual(["ad_username", "admin_password", "enable"]);
    expect(box.contains(screen.getByRole("button", { name: "Set cyberark_region" }))).toBe(false);
  });

  it("keeps the longer help behind ?, and says a reference needn't be secret", async () => {
    await open();
    expect(screen.queryByRole("note", { name: "How Environment Variables work" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "How these work" }));
    expect(screen.getByRole("note", { name: "How Environment Variables work" }).textContent).toMatch(/needn't be marked Secret/);
  });

  it("won't save a name that's there twice, whatever its case", async () => {
    await open();
    await userEvent.click(addButton());
    await userEvent.type(keys().at(-1)!, "AD_Username");
    expect(ok().disabled).toBe(true);
    expect(screen.getByText("{AD_Username} is there twice")).toBeTruthy();
    await userEvent.click(ok());
    expect(saved).toBeNull();
  });

  it("won't save a built-in name", async () => {
    await open();
    await userEvent.click(addButton());
    await userEvent.type(keys().at(-1)!, "key_file");
    expect(ok().disabled).toBe(true);
    expect(screen.getByText(/\{key_file\} is built in/)).toBeTruthy();
  });

  it("scrolls a folded section into view as it opens, with Try a template's box ready to type in", async () => {
    const shown: string[] = [];
    const scrollIntoView = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) { shown.push(this.querySelector("summary")?.textContent ?? ""); };
    try {
      await open();
      await userEvent.click(screen.getByText("Try a template"));
      await expect.poll(() => shown).toEqual(["Try a template"]);
      expect(document.activeElement).toBe(screen.getByLabelText("Template to preview"));
      await userEvent.click(screen.getByText(/Built-in placeholders/));
      await expect.poll(() => shown).toEqual(["Try a template", "Built-in placeholders (3)"]);
    } finally {
      Element.prototype.scrollIntoView = scrollIntoView;
    }
  });

  it("previews a template with secrets as dots, built-ins and unknowns as written", async () => {
    await open();
    await userEvent.click(screen.getByText("Try a template"));
    await userEvent.type(screen.getByLabelText("Template to preview"), "{{ad_username}@{{host_address} -pw {{admin_password} {{nope}");
    expect(screen.getByLabelText("Preview").textContent).toBe("tim@{host_address} -pw •••• {nope}");
  });

  it("exports every variable, a secret by its name only, even one just typed", async () => {
    await open();
    await userEvent.type(screen.getByLabelText("Secret value"), "hunter2");
    await userEvent.click(screen.getByRole("button", { name: "Export…" }));
    expect(written?.path).toBe("/tmp/EnvironmentVariables.json");
    expect(written?.text).not.toContain("hunter2");
    expect(JSON.parse(written!.text)).toEqual({
      Variables: [
        { Key: "ad_username", Value: "tim", Secret: false },
        { Key: "admin_password", Secret: true },
        { Key: "enable", Value: '"op://Private/Cisco Switch/password"', Secret: false },
      ],
    });
  });

  it("imports values over the ones here, adds new ones, and keeps a secret stored here", async () => {
    importing = JSON.stringify({
      Variables: [
        { Key: "AD_Username", Value: "pat", Secret: false },
        { Key: "domain", Value: "lab.local", Secret: false },
        { Key: "admin_password", Secret: true },
        { Key: "enable_secret", Secret: true, ProtectedValue: "AQAAANCMnd8B" },
      ],
    });
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Import…" }));
    expect(keys().map((k) => k.value)).toEqual(["ad_username", "admin_password", "enable", "domain", "enable_secret"]);
    expect(screen.getByDisplayValue("pat")).toBeTruthy();
    expect(screen.getByDisplayValue("lab.local")).toBeTruthy();
    expect(screen.getByText("Imported 4 variables. Type the value of enable_secret, then Save.")).toBeTruthy();
    await userEvent.click(ok());
    expect(saved).toEqual([
      { key: "ad_username", value: "pat", secret: false, hasValue: false, fromWindows: false },
      // stored here already: an empty value keeps it
      { key: "admin_password", value: "", secret: true, hasValue: true, fromWindows: false },
      { key: "enable", value: '"op://Private/Cisco Switch/password"', secret: false, hasValue: false, fromWindows: false },
      { key: "domain", value: "lab.local", secret: false, hasValue: false, fromWindows: false },
      { key: "enable_secret", value: "", secret: true, hasValue: false, fromWindows: false },
    ]);
  });

  it("says when a file isn't one of Environment Variables, and changes nothing", async () => {
    importing = "<Sessions/>";
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Import…" }));
    expect(screen.getByText(/isn't an Environment Variables file/)).toBeTruthy();
    expect(keys().map((k) => k.value)).toEqual(["ad_username", "admin_password", "enable"]);
  });
});
