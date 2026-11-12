// The highlighting rule sets a tab is given, with the built-in ones in use and turned off (Settings > Terminal >
// Built-in highlighting rules): off, only the team's and the user's own are offered, so Automatic can't pick one of
// the app's over theirs, but a rule set of theirs that includes a built-in one still gets its rules. The app's
// bundled files are stood in for by `fetch`, the folders' by mockIPC's highlighting_load, and the rule sets are
// compiled by the page's own rules.js, taken as text from the app's files (Vite's ?raw) and run as the page runs it.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";
import { highlighting, pageRuleSets } from "./highlighting";
import { settings } from "../settings/store.svelte";
import rulesScript from "../../../public/web/shared/rules.js?raw";

/** The app's own rule sets, as its `public/web/output` has them */
const BUNDLED: Record<string, unknown> = {
  "defaults.json": { rules: ["cisco.rules.json", "generic.rules.json"], themes: ["putty.theme.json"] },
  "rules/cisco.rules.json": { id: "cisco", name: "Cisco IOS", detect: ["^\\S+#"], rules: [{ match: "down", style: "error" }] },
  "rules/generic.rules.json": { id: "generic", name: "Generic", rules: [{ match: "\\d+\\.\\d+\\.\\d+\\.\\d+", style: "ip" }] },
  "themes/putty.theme.json": { name: "PuTTY", colors: {} },
};

/** The user's own, in their highlighting folder: one that replaces the app's Cisco by its id, and one of their own that builds on Generic */
const OWN = [
  { id: "cisco", name: "Our Cisco", detect: ["^\\S+#"], rules: [{ match: "err-disabled", style: "error" }] },
  { id: "ours", name: "Our switches", detect: ["^sw\\d+#"], include: ["generic"], rules: [{ match: "trunk", style: "keyword" }] },
];

/** What the page's rules.js makes of rule sets: `window.SpRules`, as the terminal page has it */
interface SpRules {
  compile(raw: unknown[]): { sets: { id: string; rules: unknown[] }[]; problems: string[] };
  detect(sets: unknown[], line: string): { id: string } | null;
}

beforeAll(() => {
  // the page's script, run as the page runs it: it puts SpRules on the window
  window.eval(rulesScript);
  vi.stubGlobal("fetch", async (url: string) => {
    const file = BUNDLED[url.replace("/web/output/", "")];
    return { json: async () => file, text: async () => JSON.stringify(file) };
  });
  mockIPC((cmd) => (cmd === "highlighting_load" ? { ruleSets: OWN, themes: [], problems: [] } : null));
});

afterEach(() => {
  settings.terminalPrefs = {};
});

/** The ids of rule sets, in order */
const ids = (sets: Record<string, unknown>[]) => sets.map((set) => set.id);

describe("The highlighting rule sets", () => {
  it("are the app's and the user's, one of the user's replacing the app's of the same id", async () => {
    const files = await highlighting();
    expect(ids(files.ruleSets)).toEqual(["cisco", "generic", "ours"]);
    expect(files.ruleSets[0].name).toBe("Our Cisco");
    expect(files.included).toEqual([]);
  });

  it("are only the user's with Built-in highlighting rules off, and the themes are still there", async () => {
    settings.terminalPrefs = { builtInRules: "false" };
    const files = await highlighting();
    // their Cisco is theirs, though it has the app's id; the app's Generic is gone
    expect(ids(files.ruleSets)).toEqual(["cisco", "ours"]);
    expect(files.ruleSets[0].name).toBe("Our Cisco");
    expect(files.themes.map((t) => t.name)).toEqual(["PuTTY"]);
    // and back when it's turned on again, without reading the files a second time
    settings.terminalPrefs = { builtInRules: "true" };
    expect(ids((await highlighting()).ruleSets)).toEqual(["cisco", "generic", "ours"]);
  });

  it("still give a rule set of the user's the rules of a built-in one it includes, which the page neither lists nor picks", async () => {
    settings.terminalPrefs = { builtInRules: "false" };
    const files = await highlighting();
    expect(files.included).toEqual([{ ...(BUNDLED["rules/generic.rules.json"] as object), hidden: true }]);

    const rules = (window as unknown as { SpRules: SpRules }).SpRules;
    const compiled = rules.compile(pageRuleSets(files));
    expect(compiled.problems).toEqual([]);
    expect(compiled.sets.map((set) => set.id)).toEqual(["cisco", "ours"]);
    // Our switches: Generic's rule for addresses, then its own
    expect(compiled.sets[1].rules).toHaveLength(2);
    // without the hidden set, it would have only its own
    expect(rules.compile(files.ruleSets).sets[1].rules).toHaveLength(1);
  });
});
