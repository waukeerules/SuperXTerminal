/**
 * The installers' names in the GitHub release (scripts/release-names.mjs): each of Tauri's six names for 0.8.0, as
 * they were in that release's draft, renamed to say its system. Plain strings in and out; nothing is stood in for.
 */
import { describe, expect, it } from "vitest";
// @ts-expect-error: a plain .mjs script, without types
import { expectedNames, releaseName } from "./release-names.mjs";

describe("release names", () => {
  it("names each installer for its system, as the release lists them", () => {
    const tauri = [
      "SuperXTerminal_0.8.0_aarch64.dmg",
      "SuperXTerminal_0.8.0_x64_en-US.msi",
      "SuperXTerminal_0.8.0_x64-setup.exe",
      "SuperXTerminal_0.8.0_amd64.deb",
      "SuperXTerminal-0.8.0-1.x86_64.rpm",
      "SuperXTerminal_0.8.0_amd64.AppImage",
    ];
    expect(tauri.map((f) => releaseName(f, "0.8.0"))).toEqual(expectedNames("0.8.0"));
  });

  it("leaves a release name as it is, so publishing twice renames nothing", () => {
    for (const name of expectedNames("0.8.0")) expect(releaseName(name, "0.8.0")).toBe(name);
  });

  it("names the AppImage as AppImages are named, without Linux, which the AppImage catalog warns of", () => {
    expect(releaseName("SuperXTerminal_0.9.0_amd64.AppImage", "0.9.0")).toBe("SuperXTerminal-0.9.0-x86_64.AppImage");
    expect(releaseName("SuperXTerminal_0.9.0_aarch64.AppImage", "0.9.0")).toBe("SuperXTerminal-0.9.0-aarch64.AppImage");
  });

  it("says Intel for an Intel Mac's .dmg", () => {
    expect(releaseName("SuperXTerminal_0.9.0_x64.dmg", "0.9.0")).toBe("SuperXTerminal-0.9.0-macOS-Intel.dmg");
  });

  it("knows nothing of another version's installers or other files", () => {
    expect(releaseName("SuperXTerminal_0.7.0_aarch64.dmg", "0.8.0")).toBeNull();
    expect(releaseName("NOTES.md", "0.8.0")).toBeNull();
  });
});
