import { describe, expect, it } from "vitest";
import type { SessionView } from "./api";
import { buildTree, searchProblem, searchTest } from "./tree";

function session(name: string, extra: Partial<SessionView> = {}): SessionView {
  return {
    id: `Lab/${name}`, store: "saved", name, folder: "Lab", protocol: "SSH", host: "", port: 22, username: "", profile: "",
    icon: "", color: "", note: "", updatedBy: "", locked: false, lockedFields: "", pending: false, pendingNote: "", ...extra,
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
