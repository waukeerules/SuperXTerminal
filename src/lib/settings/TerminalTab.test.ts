// Settings > Terminal. Word delimiters: the characters a double-click's word ends at, typed in a box, saved as
// typed less its spaces and repeats, and put back by Default.
// Built-in highlighting rules: ticked unless turned off; unticking it saves the preference,
// takes the app's own rule sets out of Highlighting's list, and puts Highlighting back to Automatic when it was on
// one of them. The app's bundled rule sets are stood in for by `fetch`, the user's folder by mockIPC's
// highlighting_load, and settings_save keeps what's saved, as the Rust side does.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import TerminalTab from "./TerminalTab.svelte";
import { settings } from "./store.svelte";

/** The app's own rule sets, as its `public/web/output` has them */
const BUNDLED: Record<string, unknown> = {
  "defaults.json": { rules: ["cisco.rules.json"], themes: ["putty.theme.json"] },
  "rules/cisco.rules.json": { id: "cisco", name: "Cisco IOS", rules: [] },
  "themes/putty.theme.json": { name: "PuTTY", colors: {} },
};

/** The terminal preferences each settings_save was given */
let saved: Record<string, string>[];

beforeEach(() => {
  saved = [];
  vi.stubGlobal("fetch", async (url: string) => {
    const file = BUNDLED[url.replace("/web/output/", "")];
    return { json: async () => file, text: async () => JSON.stringify(file) };
  });
  mockIPC((cmd, args) => {
    if (cmd === "highlighting_load") return { ruleSets: [{ id: "ours", name: "Our switches", rules: [] }], themes: [], problems: [] };
    if (cmd === "settings_save") {
      const next = (args as { settings: { terminalPrefs: Record<string, string> } }).settings;
      saved.push(next.terminalPrefs);
      return next;
    }
    return null;
  });
});

afterEach(() => {
  settings.terminalPrefs = {};
});

/** Highlighting's list, and the rule sets it offers after Automatic and None */
const highlightingList = () => screen.getByLabelText("Highlighting") as HTMLSelectElement;
const offered = () => within(highlightingList()).getAllByRole("option").map((o) => o.textContent).slice(2);
/** The Built-in highlighting rules tick box */
const builtIn = () => screen.getByRole("checkbox", { name: /Built-in highlighting rules/ }) as HTMLInputElement;

describe("Settings > Terminal > Word delimiters", () => {
  /** The box the delimiters are typed in, and the button that puts back the ones the app comes with */
  const box = () => screen.getByLabelText(/Word delimiters/) as HTMLInputElement;
  const byDefault = () => screen.getByRole("button", { name: "Default delimiters" }) as HTMLButtonElement;

  it("shows the ones the app comes with, with nothing to put back", () => {
    render(TerminalTab);
    expect(box().value).toBe("()[]{}',\"`");
    expect(byDefault().disabled).toBe(true);
  });

  it("saves what's typed without its spaces and repeats, and Default puts back the app's own", async () => {
    const user = userEvent.setup();
    render(TerminalTab);
    await user.clear(box());
    // "[[" and "{{" are how the keyboard stand-in is told to type one [ or {
    await user.type(box(), "() [[]#{{}#");
    await user.tab();
    await expect.poll(() => saved.at(-1)).toEqual({ wordSeparators: "()[]#{}" });
    expect(box().value).toBe("()[]#{}");
    await user.click(byDefault());
    await expect.poll(() => saved.at(-1)).toEqual({});
    expect(box().value).toBe("()[]{}',\"`");
  });

  it("keeps none at all, so a word ends only at a space", async () => {
    const user = userEvent.setup();
    render(TerminalTab);
    await user.clear(box());
    await user.tab();
    await expect.poll(() => saved.at(-1)).toEqual({ wordSeparators: "" });
    expect(byDefault().disabled).toBe(false);
  });
});

describe("Settings > Terminal > Built-in highlighting rules", () => {
  it("is on until it's turned off, with the app's rule sets offered beside the user's own", async () => {
    render(TerminalTab);
    await expect.poll(offered).toEqual(["Cisco IOS", "Our switches"]);
    expect(builtIn().checked).toBe(true);
    expect(saved).toEqual([]);
  });

  it("off, saves that and offers only the user's own; on again, the app's are back", async () => {
    const user = userEvent.setup();
    render(TerminalTab);
    await expect.poll(offered).toEqual(["Cisco IOS", "Our switches"]);

    await user.click(builtIn());
    await expect.poll(offered).toEqual(["Our switches"]);
    expect(saved).toEqual([{ builtInRules: "false" }]);
    expect(builtIn().checked).toBe(false);

    await user.click(builtIn());
    await expect.poll(offered).toEqual(["Cisco IOS", "Our switches"]);
    // back to how it comes: nothing kept of it in the settings
    expect(saved.at(-1)).toEqual({});
  });

  it("off, puts Highlighting back to Automatic when it was on one of the app's, and leaves one of the user's chosen", async () => {
    const user = userEvent.setup();
    settings.terminalPrefs = { ruleSet: "cisco" };
    const { unmount } = render(TerminalTab);
    await expect.poll(() => highlightingList().value).toBe("cisco");
    await user.click(builtIn());
    await expect.poll(() => highlightingList().value).toBe("auto");
    expect(saved.at(-1)).toEqual({ builtInRules: "false" });
    unmount();

    settings.terminalPrefs = { ruleSet: "ours" };
    saved = [];
    render(TerminalTab);
    await expect.poll(() => highlightingList().value).toBe("ours");
    await user.click(builtIn());
    await expect.poll(offered).toEqual(["Our switches"]);
    expect(highlightingList().value).toBe("ours");
    expect(saved).toEqual([{ ruleSet: "ours", builtInRules: "false" }]);
  });
});
