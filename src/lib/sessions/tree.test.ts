/**
 * Tests for the Sessions tree's search, building the tree from sessions, and the rows it's drawn as: pure functions,
 * given sessions made here, so nothing is stood in.
 */
import { describe, expect, it } from "vitest";
import type { SessionView } from "./api";
import { ALL_SEARCH_FIELDS, DEFAULT_SEARCH_FIELDS, buildTree, searchFieldsOf, searchProblem, searchTest, toggledSearchFields, treeRows } from "./tree";

/** A Saved SSH session in Lab, with any fields given */
function session(name: string, extra: Partial<SessionView> = {}): SessionView {
  return {
    id: `Lab/${name}`, store: "saved", name, folder: "Lab", protocol: "SSH", host: "", port: 22, username: "", profile: "",
    icon: "", color: "", note: "", extraArgs: "", script: "", buttonBar: "", updatedBy: "", locked: false, lockedFields: "", pending: false, pendingNote: "", ...extra,
  } as SessionView;
}

/** Two switches to search, different in case, host and note */
const SW1 = session("Core-SW1", { host: "10.0.0.1", note: "Rack 4" });
/** The second switch */
const SW2 = session("access-sw22", { host: "10.0.1.22" });
/** The names of the switches a search matches; both when there's no search */
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

describe("building the tree", () => {
  it("sorts names by number and without case, and keeps folders of one name whatever their case", () => {
    const tree = buildTree("saved", "Saved sessions", [
      session("sw10", { folder: "Lab" }), session("SW2", { folder: "lab" }), session("sw1", { folder: "Lab/Core" }),
    ], "");
    expect(tree.folders.map((f) => f.name)).toEqual(["Lab"]);
    expect(tree.folders[0].sessions.map((s) => s.name)).toEqual(["SW2", "sw10"]);
    expect(tree.folders[0].folders[0].sessions.map((s) => s.name)).toEqual(["sw1"]);
    expect(tree.count).toBe(3);
  });
});

describe("the tree's rows, top to bottom as drawn", () => {
  /** A Saved session with this id */
  const at = (id: string) => session(id.slice(id.lastIndexOf("/") + 1), { id, folder: id.includes("/") ? id.slice(0, id.lastIndexOf("/")) : "" });
  const root = buildTree("saved", "Saved sessions", [at("Lab/Core/sw1"), at("Lab/Core/sw2"), at("Lab/r1"), at("Office/ap1"), at("top")], "", [], false);
  /** A row as a test reads it: its name, how deep, and its tree lines */
  const shape = (rows: ReturnType<typeof treeRows>) =>
    rows.map((r) => `${"  ".repeat(r.kind === "folder" ? r.depth : r.depth + 1)}${r.kind === "folder" ? r.node.name + (r.open ? " v" : " >") : r.session.name}${r.last ? " (last)" : ""}`);

  it("shows only the roots while nothing is open", () => {
    const rows = treeRows([root], new Set());
    expect(shape(rows)).toEqual(["Saved sessions > (last)"]);
    expect(rows[0]).toMatchObject({ kind: "folder", first: true, last: true, depth: 0, guides: [] });
  });

  it("puts an open folder's folders, then its sessions, under it, each knowing whether it's the last there", () => {
    const rows = treeRows([root], new Set(["saved:", "saved:Lab", "saved:Lab/Core"]));
    expect(shape(rows)).toEqual([
      "Saved sessions v (last)",
      "  Lab v",
      "    Core v",
      "      sw1",
      "      sw2 (last)",
      "    r1 (last)",
      "  Office >",
      "  top (last)",
    ]);
    // Lab isn't the last in Saved sessions, so its line carries on down beside everything in it; the root's doesn't
    const sw1 = rows.find((r) => r.kind === "session" && r.session.name === "sw1")!;
    expect(sw1.guides).toEqual([false, true, true]);
    // Core is followed by r1 in Lab, so it isn't Lab's last; a folder with no sessions after it would be
    expect(rows.find((r) => r.kind === "folder" && r.node.name === "Core")).toMatchObject({ last: false, depth: 2 });
  });

  it("tells the first root from the last, and a shortcut in Favorites from the session itself", () => {
    const favorites = { ...buildTree("saved", "Favorites", [at("Lab/r1")], "", [], false), key: "favorites:", group: "favorites" as const };
    favorites.sessions = [at("Lab/r1")];
    favorites.folders = [];
    const rows = treeRows([favorites, root], new Set(["favorites:", "saved:", "saved:Lab"]));
    expect(rows[0]).toMatchObject({ first: true, last: false });
    expect(rows.find((r) => r.kind === "folder" && r.node === root)).toMatchObject({ first: false, last: true });
    const picked = rows.filter((r) => r.kind === "session" && r.session.id === "Lab/r1").map((r) => (r.kind === "session" ? r.select : ""));
    expect(picked).toEqual(["favorites:saved:Lab/r1", "saved:Lab/r1"]);
    // every row's key is its own, a folder's unlike a session's of the same name
    expect(new Set(rows.map((r) => r.key)).size).toBe(rows.length);
  });
});
