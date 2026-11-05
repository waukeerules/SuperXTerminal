// Local overrides: the on/off switch that keeps the values, and filling from and saving presets. The Rust side is stood
// in for by mockIPC: the session's overrides live in `stored`, its presets in `presets`, and what's saved is recorded.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import Overrides from "./Overrides.svelte";
import type { Override, SessionFields } from "./api";

/** The session the window is opened on: a Database session, as the user's CyberArk one */
const SESSION: SessionFields = {
  name: "Switch - CyberArk", folder: "Region 4", protocol: "SSH", host: "192.168.0.50", port: 22,
  username: "{email}#{cyberark_region}@…", profile: "CyberArk", color: "", icon: "switch", highlightRules: "",
  extraArgs: "", jumpHost: "", script: "", note: "", localPath: "", remotePath: "", rdpOptions: "", sshAlgorithms: "", buttonBar: "",
};
/** Its id, the key its overrides are kept by */
const ID = "@db/Region 4/Switch - CyberArk";

/** The overrides stored, as the stand-in for overrides_get gives them */
let stored: Record<string, Override>;
/** The presets stored, which the stand-in for override_presets_set changes */
let presets: Record<string, Override>;
/** What the window last saved (overrides_set), or null if it hasn't */
let saved: [string, Override][] | null;

beforeEach(() => {
  stored = { [ID]: { Port: 0, Username: "admin", PuttyProfile: "Default Settings" } };
  presets = { "Local AD login": { Port: 0, Username: "{ad_username}", PuttyProfile: "Default Settings" } };
  saved = null;
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "overrides_get") return stored;
    if (cmd === "session_get") return SESSION;
    if (cmd === "profiles_list") return [{ name: "Default Settings" }, { name: "CyberArk" }];
    if (cmd === "override_presets_get") return presets;
    if (cmd === "override_presets_set") {
      const name = String(payload.name);
      if (payload.value) presets = { ...presets, [name]: payload.value as Override };
      else delete presets[name];
      return presets;
    }
    if (cmd === "overrides_set") {
      saved = payload.changes as [string, Override][];
      return stored;
    }
    return null;
  });
});

/** Open the window on the session, and wait until its stored values are shown */
async function open() {
  render(Overrides, { sessions: [{ id: ID, store: "database", name: SESSION.name }], onSaved: () => {}, onCancel: () => {} });
  // ready once the stored values are shown beside the fields
  await screen.findByText("Session: 192.168.0.50");
}

/** The Use these overrides switch */
const useSwitch = () => screen.getByRole("checkbox", { name: /Use these overrides/ }) as HTMLInputElement;
/** A field by its label */
const field = (label: RegExp) => screen.getByRole("textbox", { name: label }) as HTMLInputElement;

describe("Local overrides", () => {
  it("is on for a session whose overrides are on", async () => {
    await open();
    expect(useSwitch().checked).toBe(true);
  });

  it("turned off, saves the session's values with Disabled, so they're kept", async () => {
    await open();
    await userEvent.click(useSwitch());
    expect(useSwitch().checked).toBe(false);
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(saved).not.toBeNull();
    const [[id, value]] = saved!;
    expect(id).toBe(ID);
    expect(value.Disabled).toBe(true);
    expect(value.Username).toBe("admin");
    expect(value.PuttyProfile).toBe("Default Settings");
  });

  it("starts off for a session whose overrides are off, and saves it on without Disabled", async () => {
    stored[ID].Disabled = true;
    await open();
    expect(useSwitch().checked).toBe(false);
    await userEvent.click(useSwitch());
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(saved![0][1].Disabled).toBeUndefined();
  });

  it("fills and ticks only a preset's fields", async () => {
    stored = {};
    await open();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Fill from preset" }), "Local AD login");
    expect(field(/Login username/).value).toBe("{ad_username}");
    expect((screen.getByRole("checkbox", { name: /Login username/ }) as HTMLInputElement).checked).toBe(true);
    // Host isn't in the preset: still unticked
    expect((screen.getByRole("checkbox", { name: /^Host/ }) as HTMLInputElement).checked).toBe(false);
  });

  it("saves what's ticked as a preset, without the on/off", async () => {
    await open();
    await userEvent.click(useSwitch());
    await userEvent.click(screen.getByRole("button", { name: "Save as preset…" }));
    const prompt = screen.getByRole("form", { name: "Save as preset" });
    await userEvent.type(within(prompt).getByRole("textbox"), "My own login");
    await userEvent.click(within(prompt).getByRole("button", { name: "OK" }));
    expect(presets["My own login"]).toMatchObject({ Username: "admin", PuttyProfile: "Default Settings" });
    expect(presets["My own login"].Disabled).toBeUndefined();
    expect(await screen.findByText('Saved preset "My own login".')).toBeTruthy();
  });
});
