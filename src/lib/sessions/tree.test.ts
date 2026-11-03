import { describe, expect, it } from "vitest";
import type { SessionView } from "./api";
import { ALL_SEARCH_FIELDS, DEFAULT_SEARCH_FIELDS, buildTree, searchFieldsOf, searchProblem, searchTest, toggledSearchFields } from "./tree";

function session(name: string, extra: Partial<SessionView> = {}): SessionView {
  return {
    id: `Lab/${name}`, store: "saved", name, folder: "Lab", protocol: "SSH", host: "", port: 22, username: "", profile: "",
    icon: "", color: "", note: "", extraArgs: "", script: "", updatedBy: "", locked: false, lockedFields: "", pending: false, pendingNote: "", ...extra,
  } as SessionView;
}

const SW1 = session("Core-SW1", { host: "10.0.0.1", note: "Rack 4" });
const SW2 = session("access-sw22", { host: "10.0.1.22" });
const names = (test: ((s: SessionView) => boolean) | null) => [SW1, SW2].filter((s) => !test || test(s)).map((s) => s.name);

describe("the Sessions search", () => {
  it("ignores case by default, in the name, host, username, folder and note", () => {
    expect(names(searchTest("sw"))).toEqual(["Core-SW1", "access-sw22"]);
    expect(names(searchTest("rack"))).toEqual(["Core-SW1"]);
    expect(searchTest("  ")).toBeNull();
  });

  it("matches case when asked", () => {
    expect(names(searchTest("SW", { caseSensitive: true, regex: false }))).toEqual(["Core-SW1"]);
  });

  it("takes a regular expression, with or without case", () => {
    expect(names(searchTest("sw\\d{2}$", { caseSensitive: false, regex: true }))).toEqual(["access-sw22"]);
    expect(names(searchTest("^core", { caseSensitive: true, regex: true }))).toEqual([]);
    expect(names(searchTest("^10\\.0\\.0\\.", { caseSensitive: false, regex: true }))).toEqual(["Core-SW1"]);
  });

  it("matches nothing with a broken regular expression, and says why", () => {
    const options = { caseSensitive: false, regex: true };
    expect(names(searchTest("sw(", options))).toEqual([]);
    expect(searchProblem("sw(", options)).not.toBe("");
    expect(searchProblem("sw(", { caseSensitive: false, regex: false })).toBe("");
  });

  it("filters the tree with the options", () => {
    const tree = buildTree("saved", "Saved sessions", [SW1, SW2], "^access", [], false, { caseSensitive: false, regex: true });
    expect(tree.count).toBe(1);
    expect(tree.folders[0].sessions.map((s) => s.name)).toEqual(["access-sw22"]);
  });
});

describe("Search in…", () => {
  const router = session("edge-router", { icon: "router#1E63B4", protocol: "Telnet", profile: "CyberArk" });
  const fields = (f: string[]) => ({ caseSensitive: false, regex: false, fields: f as never });

  it("doesn't look at the icon, protocol or profile unless asked", () => {
    expect(searchTest("telnet")!(router)).toBe(false);
    expect(searchTest("cyberark")!(router)).toBe(false);
  });

  it("finds the icon by its key or its title, without its colour", () => {
    expect(searchTest("router", fields(["icon"]))!(router)).toBe(true);
    expect(searchTest("1E63B4", fields(["icon"]))!(router)).toBe(false);
  });

  it("finds the protocol (SSH when none is set) and the profile", () => {
    expect(searchTest("telnet", fields(["protocol"]))!(router)).toBe(true);
    expect(searchTest("ssh", fields(["protocol"]))!(session("plain", { protocol: "" }))).toBe(true);
    expect(searchTest("cyberark", fields(["profile"]))!(router)).toBe(true);
  });

  it("finds the port, the extra arguments without their password, and the script", () => {
    const s = session("sw", { port: 2222, extraArgs: "-L 8080:localhost:80 -pw ********", script: "scripts/login.spsl" });
    expect(searchTest("2222", fields(["port"]))!(s)).toBe(true);
    expect(searchTest("8080", fields(["extraArgs"]))!(s)).toBe(true);
    expect(searchTest("login.spsl", fields(["script"]))!(s)).toBe(true);
  });

  it("looks only where it's told", () => {
    expect(searchTest("edge", fields(["host"]))!(router)).toBe(false);
  });
});

describe("Search in…'s choices", () => {
  it("ticks and unticks in order, back to the defaults as none chosen, and keeps the last field", () => {
    expect(toggledSearchFields(DEFAULT_SEARCH_FIELDS, "icon")).toEqual(["name", "folder", "host", "username", "note", "icon"]);
    expect(toggledSearchFields(["name", "folder", "host", "username", "note", "icon"], "icon")).toEqual([]);
    expect(toggledSearchFields(["icon"], "icon")).toEqual(["icon"]);
    expect(toggledSearchFields(DEFAULT_SEARCH_FIELDS, "all")).toEqual(ALL_SEARCH_FIELDS);
  });

  it("reads the settings, leaving out names it doesn't know", () => {
    expect(searchFieldsOf([])).toEqual(DEFAULT_SEARCH_FIELDS);
    expect(searchFieldsOf(["icon", "bogus"])).toEqual(["icon"]);
  });
});
