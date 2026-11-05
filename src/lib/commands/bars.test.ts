// The colour a button's command numbers its lines in: the colour the button shows. And which bar shows for the active
// tab: picked by hand, the session's own, the profile's, the tab icon's, Default Settings', the one last picked.
import { describe, expect, it } from "vitest";
import { barFor, buttonColour, type BarButton, type BarChoice } from "./bars";

/** A button sending `ls -la`, with an icon and a circle colour */
const button = (icon: string, color = ""): BarButton => ({ label: "ls -la", tooltip: "", color, icon, action: "SendString", text: "ls -la\\n" });

describe("buttonColour", () => {
  it("takes an icon's chosen colour", () => {
    expect(buttonColour(button("switch#A371F7"))).toBe("#A371F7");
  });

  it("takes an icon's own colour when none is chosen", () => {
    expect(buttonColour(button("switch"))).toBe("#2E7D32");
  });

  it("takes none for a custom picture without a colour, and the chosen one with", () => {
    expect(buttonColour(button("custom:logo"))).toBe("");
    expect(buttonColour(button("custom:logo#FF0000"))).toBe("#FF0000");
  });

  it("takes the circle's colour without an icon, and none for neither", () => {
    expect(buttonColour(button("", "#3FB950"))).toBe("#3FB950");
    expect(buttonColour(button(""))).toBe("");
  });
});

describe("barFor", () => {
  const bars = [
    { name: "Default", tabIcons: [] },
    { name: "Cisco", tabIcons: ["switch"] },
    { name: "Linux", tabIcons: [] },
    { name: "Team", tabIcons: [] },
  ];
  const profiles = [{ name: "Default Settings", buttonBar: "" }, { name: "Lab", buttonBar: "linux" }, { name: "Plain", buttonBar: "" }];
  /** The bar's name for a tab with this icon and profile, and the rest as given */
  const shown = (icon: string, profile: string, more: Partial<BarChoice> = {}) =>
    barFor(bars, { icon, session: "", profile, profiles, chosen: "", picked: false, ...more }).name;

  it("shows the profile's bar over the tab icon's, matched whatever its case", () => {
    expect(shown("switch#A371F7", "lab")).toBe("Linux");
    expect(shown("switch", "Plain")).toBe("Cisco");
  });

  it("shows the session's own bar over its profile's, unless it isn't there any more", () => {
    expect(shown("switch", "Lab", { session: "team" })).toBe("Team");
    expect(shown("switch", "Lab", { session: "Old" })).toBe("Linux");
  });

  it("shows the bar picked by hand until the tab changes", () => {
    expect(shown("switch", "Lab", { picked: true, chosen: "Team", session: "Cisco" })).toBe("Team");
  });

  it("passes over a profile's bar that isn't there any more", () => {
    const gone = [...profiles.slice(0, 2), { name: "Plain", buttonBar: "Old" }];
    expect(shown("switch", "Plain", { profiles: gone })).toBe("Cisco");
  });

  it("takes Default Settings' bar after the tab icon's, for a session with no profile of its own too", () => {
    const withDefault = [{ name: "Default Settings", buttonBar: "Team" }, ...profiles.slice(1)];
    expect(shown("server", "Plain", { profiles: withDefault })).toBe("Team");
    // a session with no profile reports Default Settings: the icon's bar still comes first
    expect(shown("switch", "Default Settings", { profiles: withDefault })).toBe("Cisco");
    expect(shown("server", "Default Settings", { profiles: withDefault })).toBe("Team");
    // no tab at all: the one last picked
    expect(shown("", "", { profiles: withDefault, chosen: "Linux" })).toBe("Linux");
  });

  it("falls back to the one last picked, else the first", () => {
    expect(shown("server", "Plain", { chosen: "team" })).toBe("Team");
    expect(shown("server", "Plain")).toBe("Default");
  });
});
