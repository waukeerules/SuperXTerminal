// A profile's terminal colours, Tab colour and font as its tabs take them again when it's saved: its own, else
// Default Settings'.
import { describe, expect, it } from "vitest";
import { DEFAULT_PROFILE, emptyProfile, profileColours, profileFont } from "./profiles";

describe("profileColours", () => {
  const profiles = [
    { ...emptyProfile(DEFAULT_PROFILE), background: "#101010", foreground: "#EEEEEE", selection: "blue" },
    { ...emptyProfile("Production"), background: "#3a0000", cursorColor: "#00ff00" },
  ];

  it("takes the profile's own, else Default Settings', leaving out what isn't #rrggbb", () => {
    expect(profileColours(profiles, "production")).toEqual({ theme: "", background: "#3a0000", foreground: "#eeeeee", selection: "", cursor: "#00ff00", tab: "" });
  });

  it("takes Default Settings' for a profile that isn't there any more", () => {
    expect(profileColours(profiles, "Gone")).toEqual({ theme: "", background: "#101010", foreground: "#eeeeee", selection: "", cursor: "", tab: "" });
  });

  it("names the Dark or Light theme for those choices, and leaves the colours out", () => {
    const withChoice = [...profiles, { ...emptyProfile("Lab"), colours: "light", background: "#3a0000" }, { ...emptyProfile("Night"), colours: "dark" }];
    expect(profileColours(withChoice, "Lab")).toEqual({ theme: "Light", background: "", foreground: "", selection: "", cursor: "", tab: "" });
    expect(profileColours(withChoice, "Night").theme).toBe("Dark");
    // following the theme: none of Default Settings' colours either
    const following = [...profiles, { ...emptyProfile("Plain"), colours: "theme" }];
    expect(profileColours(following, "Plain")).toEqual({ theme: "", background: "", foreground: "", selection: "", cursor: "", tab: "" });
  });

  it("takes the Tab colour whatever the Colours choice: the profile's own, else Default Settings'", () => {
    const tabbed = [
      { ...emptyProfile(DEFAULT_PROFILE), tabColor: "#1F6FEB" },
      { ...emptyProfile("Lab"), colours: "theme", tabColor: "#2DA44E" },
      { ...emptyProfile("Plain"), colours: "dark" },
      { ...emptyProfile("Odd"), tabColor: "green" },
    ];
    expect(profileColours(tabbed, "Lab").tab).toBe("#2da44e");
    expect(profileColours(tabbed, "Plain").tab).toBe("#1f6feb");
    // not #rrggbb: Default Settings'
    expect(profileColours(tabbed, "Odd").tab).toBe("#1f6feb");
    expect(profileColours([emptyProfile(DEFAULT_PROFILE)], "Gone").tab).toBe("");
  });
});

describe("profileFont", () => {
  const profiles = [
    { ...emptyProfile(DEFAULT_PROFILE), fontFamily: "Consolas", fontSize: 11 },
    { ...emptyProfile("Projector"), fontSize: 20 },
    { ...emptyProfile("Lab"), fontFamily: " Courier New ", fontSize: 99 },
  ];

  it("takes the profile's own, else Default Settings', a size brought within what the tab can draw", () => {
    expect(profileFont(profiles, "projector")).toEqual({ family: "Consolas", points: 20 });
    expect(profileFont(profiles, "Lab")).toEqual({ family: "Courier New", points: 36 });
    // a profile that isn't there: Default Settings'
    expect(profileFont(profiles, "gone")).toEqual({ family: "Consolas", points: 11 });
    // none anywhere: the theme's
    expect(profileFont([emptyProfile(DEFAULT_PROFILE)], DEFAULT_PROFILE)).toEqual({ family: "", points: 0 });
  });
});
