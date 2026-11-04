// Tools > Environment Variables: names that clash can't be saved, placeholders used but not defined are listed, and
// the preview fills a template with secrets hidden. The Rust side is stood in for by mockIPC.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import EnvironmentVariables from "./EnvironmentVariables.svelte";

/** What the window last saved (variables_save), or null if it hasn't */
let saved: unknown = null;

beforeEach(() => {
  saved = null;
  mockIPC((cmd, args) => {
    if (cmd === "variables_list")
      return [
        { key: "ad_username", value: "tim", secret: false, hasValue: false, fromWindows: false },
        { key: "admin_password", value: "", secret: true, hasValue: true, fromWindows: false },
      ];
    if (cmd === "variables_usage")
      return {
        ad_username: ["profile CyberArk"],
        cyberark_region: ["profile CyberArk", "session Region 4/Switch - CyberArk"],
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
});
