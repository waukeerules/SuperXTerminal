// Settings > Profiles > Server load on a Mac, where it works: chosen for a profile and saved with it, and off in
// Default Settings until chosen there. A file of its own, as it stands in a Mac (isMac) for the whole file, and the
// other profile tests are written for jsdom's own user agent, which isn't one. The Rust side is stood in for by
// mockIPC, recording each save.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { DEFAULT_PROFILE, emptyProfile, type Profile } from "./profiles";

vi.mock("../shortcuts", async (original) => ({ ...(await original<typeof import("../shortcuts")>()), isMac: true }));
const { default: ProfilesTab } = await import("./ProfilesTab.svelte");

/** The profiles saved, in order */
let saves: Profile[];

beforeEach(() => {
  saves = [];
  const stored = [emptyProfile(DEFAULT_PROFILE), { ...emptyProfile("linux"), username: "root" }];
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "profiles_list") return stored;
    if (cmd === "profiles_save") {
      saves.push(payload.profile as Profile);
      return stored;
    }
    if (cmd === "buttonbars_load") return [];
    return null;
  });
});

describe("Server load on a Mac", () => {
  it("saves it on for the profile it's chosen in", async () => {
    render(ProfilesTab, { initial: "linux" });
    await screen.findByRole("option", { name: "linux" });
    const load = screen.getByRole("combobox", { name: /^Server load/ }) as HTMLSelectElement;
    expect(load.disabled).toBe(false);
    expect(load.value).toBe("");
    await userEvent.selectOptions(load, "on");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(saves.at(-1)).toMatchObject({ name: "linux", serverLoad: true });
  });

  it("is off in Default Settings until chosen there", async () => {
    render(ProfilesTab, { initial: DEFAULT_PROFILE });
    await screen.findByRole("option", { name: "linux" });
    const load = screen.getByRole("combobox", { name: /^Server load/ }) as HTMLSelectElement;
    expect(load.value).toBe("off");
    expect([...load.options].map((o) => o.value)).toEqual(["on", "off"]);
  });
});
