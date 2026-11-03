// A profile's terminal colours as its tabs take them again when it's saved: its own, else Default Settings'.
import { describe, expect, it } from "vitest";
import { DEFAULT_PROFILE, emptyProfile, profileColours } from "./profiles";

describe("profileColours", () => {
  const profiles = [
    { ...emptyProfile(DEFAULT_PROFILE), background: "#101010", foreground: "#EEEEEE", selection: "blue" },
    { ...emptyProfile("Production"), background: "#3a0000", cursorColor: "#00ff00" },
  ];

  it("takes the profile's own, else Default Settings', leaving out what isn't #rrggbb", () => {
    expect(profileColours(profiles, "production")).toEqual({ theme: "", background: "#3a0000", foreground: "#eeeeee", selection: "", cursor: "#00ff00" });
  });

  it("takes Default Settings' for a profile that isn't there any more", () => {
    expect(profileColours(profiles, "Gone")).toEqual({ theme: "", background: "#101010", foreground: "#eeeeee", selection: "", cursor: "" });
  });

  it("names the Dark or Light theme for those choices, and leaves the colours out", () => {
    const withChoice = [...profiles, { ...emptyProfile("Lab"), colours: "light", background: "#3a0000" }, { ...emptyProfile("Night"), colours: "dark" }];
    expect(profileColours(withChoice, "Lab")).toEqual({ theme: "Light", background: "", foreground: "", selection: "", cursor: "" });
    expect(profileColours(withChoice, "Night").theme).toBe("Dark");
    // following the theme: none of Default Settings' colours either
    const following = [...profiles, { ...emptyProfile("Plain"), colours: "theme" }];
    expect(profileColours(following, "Plain")).toEqual({ theme: "", background: "", foreground: "", selection: "", cursor: "" });
  });
});
