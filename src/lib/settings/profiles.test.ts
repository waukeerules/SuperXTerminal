// A profile's terminal colours, Tab colour and font as its tabs take them again when it's saved: its own, else
// Default Settings'.
import { describe, expect, it } from "vitest";
import { baseChain, DEFAULT_PROFILE, emptyProfile, profileColours, profileFont, profileKeys, resolvedProfile } from "./profiles";

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

describe("profileKeys", () => {
  it("takes the profile's own, else Default Settings', else PuTTY's, a style it doesn't know being PuTTY's", () => {
    const profiles = [
      { ...emptyProfile(DEFAULT_PROFILE), keyBackspace: "bs" },
      { ...emptyProfile("Old Unix"), keyHomeEnd: "rxvt", keyFunction: "vt52" },
    ];
    expect(profileKeys(profiles, "old unix")).toEqual({ backspace: "bs", homeEnd: "rxvt", functionKeys: "tilde" });
    expect(profileKeys([emptyProfile(DEFAULT_PROFILE)], DEFAULT_PROFILE)).toEqual({ backspace: "del", homeEnd: "standard", functionKeys: "tilde" });
  });
});

describe("a profile based on another", () => {
  const profiles = [
    { ...emptyProfile(DEFAULT_PROFILE), keyBackspace: "bs", fontSize: 11 },
    { ...emptyProfile("netops"), username: "admin", fontFamily: "Consolas", forwards: [{ kind: "local" as const, listen: "8080", to: "localhost:80" }] },
    { ...emptyProfile("netops-old"), basedOn: "NetOps", algorithms: "kex=diffie-hellman-group1-sha1", logInHere: false },
    { ...emptyProfile("a"), basedOn: "b", username: "a" },
    { ...emptyProfile("b"), basedOn: "a", port: 2200 },
  ];

  it("takes what it leaves empty from that one, its name and Based on its own, as the core does", () => {
    const old = resolvedProfile(profiles, "netops-old")!;
    expect([old.name, old.basedOn, old.username, old.fontFamily, old.algorithms, old.forwards.length]).toEqual(["netops-old", "NetOps", "admin", "Consolas", "kex=diffie-hellman-group1-sha1", 1]);
    // a tab of it takes netops' font, and Default Settings' size and keys
    expect(profileFont(profiles, "netops-old")).toEqual({ family: "Consolas", points: 11 });
    expect(profileKeys(profiles, "netops-old").backspace).toBe("bs");
  });

  it("stops at a loop, and at a profile that isn't there", () => {
    expect(baseChain(profiles, "a")).toEqual(["a", "b"]);
    expect(resolvedProfile(profiles, "a")).toMatchObject({ username: "a", port: 2200 });
    expect(baseChain([{ ...emptyProfile("x"), basedOn: "gone" }], "x")).toEqual(["x"]);
    expect(resolvedProfile(profiles, "nobody")).toBeUndefined();
  });
});
