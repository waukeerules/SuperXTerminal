// Tools > Environment Variables: names that clash can't be saved, placeholders used but not defined are listed, the
// preview fills a template with secrets hidden, and Export… and Import… pass the variables on in SuperXPuTTY's file,
// secrets by name only; and a placeholder used in long-named places keeps its Add button in view. The Rust side, and the dialogs choosing the file, are stood in for by mockIPC.
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
/** The OK button */
const ok = () => screen.getByRole("button", { name: "OK" }) as HTMLButtonElement;

describe("Environment Variables", () => {
  it("lists placeholders used but not defined, not built-in ones, and adds one", async () => {
    await open();
    expect(screen.getByText("{cyberark_region}")).toBeTruthy();
    // host_address is used too, but built in: not listed as undefined
    expect(screen.getByText("Used but not defined").closest("section")!.textContent).not.toContain("{host_address}");
    const row = screen.getByText("{cyberark_region}").closest("li") as HTMLElement;
    await userEvent.click(within(row).getByRole("button", { name: "Add" }));
    expect(keys().map((k) => k.value)).toContain("cyberark_region");
  });

  it("won't save a name that's there twice, whatever its case", async () => {
    await open();
    await userEvent.click(screen.getAllByRole("button", { name: "Add" }).at(-1)!);
    await userEvent.type(keys().at(-1)!, "AD_Username");
    expect(ok().disabled).toBe(true);
    expect(screen.getByText("{AD_Username} is there twice")).toBeTruthy();
    await userEvent.click(ok());
    expect(saved).toBeNull();
  });

  it("won't save a built-in name", async () => {
    await open();
    await userEvent.click(screen.getAllByRole("button", { name: "Add" }).at(-1)!);
    await userEvent.type(keys().at(-1)!, "key_file");
    expect(ok().disabled).toBe(true);
    expect(screen.getByText(/\{key_file\} is built in/)).toBeTruthy();
  });

  it("previews a template with secrets as dots, built-ins and unknowns as written", async () => {
    await open();
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
    expect(keys().map((k) => k.value)).toEqual(["ad_username", "admin_password", "domain", "enable_secret"]);
    expect(screen.getByDisplayValue("pat")).toBeTruthy();
    expect(screen.getByDisplayValue("lab.local")).toBeTruthy();
    expect(screen.getByText("Imported 4 variables. Type the value of enable_secret, then OK to save.")).toBeTruthy();
    await userEvent.click(ok());
    expect(saved).toEqual([
      { key: "ad_username", value: "pat", secret: false, hasValue: false, fromWindows: false },
      // stored here already: an empty value keeps it
      { key: "admin_password", value: "", secret: true, hasValue: true, fromWindows: false },
      { key: "domain", value: "lab.local", secret: false, hasValue: false, fromWindows: false },
      { key: "enable_secret", value: "", secret: true, hasValue: false, fromWindows: false },
    ]);
  });

  it("says when a file isn't one of Environment Variables, and changes nothing", async () => {
    importing = "<Sessions/>";
    await open();
    await userEvent.click(screen.getByRole("button", { name: "Import…" }));
    expect(screen.getByText(/isn't an Environment Variables file/)).toBeTruthy();
    expect(keys().map((k) => k.value)).toEqual(["ad_username", "admin_password"]);
  });

  it("shows each place a placeholder is used on its own, to be cut from the front, so Add stays in view", async () => {
    // a long folder or session name pushed Add off the window's right edge: each place is now its own piece, which
    // the window's styles cut from the front ("…Core Switch - with a very long name indeed") so the session's name
    // shows, with "and N more" and Add beside them, never cut
    await open();
    const row = screen.getByText("{admin_username}").closest("li") as HTMLElement;
    const places = [...row.querySelectorAll(".place")].map((p) => p.textContent);
    expect(places).toEqual([
      "session Customers/North America/Region 4/Building 12/Floor 3/Core Switch - with a very long name indeed",
      "session Live Test/Lab Switch",
      "session Live Test/Lab Switchsdfsdf",
    ]);
    expect(row.querySelector(".more")?.textContent).toBe("and 2 more");
    // every place, in full, on hover
    expect(row.querySelector(".where")?.getAttribute("title")?.split("\n")).toHaveLength(5);
    expect(within(row).getByRole("button", { name: "Add" }).parentElement).toBe(row);
  });
});
