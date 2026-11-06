// Settings > Profiles: which fields show as changed, what's greyed while there are changes, Save and Revert, the
// list's Duplicate, Export and Import, and Server load (off unless chosen; greyed where it can't work). The Rust side is stood in for by mockIPC: the profiles live in `stored`, every
// save and delete is recorded in `calls`, and the file chosen to import holds `inFile`.
import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import ProfilesTab from "./ProfilesTab.svelte";
import { DEFAULT_PROFILE, emptyProfile, type Profile } from "./profiles";
import { isMac } from "../shortcuts";

/** The profiles stored, which the stand-ins for profiles_save and profiles_delete change */
let stored: Profile[];
/** Every command the tab sent, with its arguments */
let calls: { cmd: string; args: Record<string, unknown> }[];
/** The profiles in the file chosen to import, as profiles_read_file reads them */
let inFile: Profile[];

beforeEach(() => {
  stored = [
    { ...emptyProfile(DEFAULT_PROFILE), olderAlgorithms: true },
    { ...emptyProfile("root"), username: "root", port: 2222 },
    { ...emptyProfile("CyberArk"), keyFile: "~/.ssh/cyberark_key", keyFetch: "pam-key {key_file}", keyLifetime: 240 },
  ];
  calls = [];
  inFile = [];
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: payload });
    if (cmd === "profiles_list") return stored;
    if (cmd === "profiles_save") {
      const profile = payload.profile as Profile;
      const previous = String(payload.previousName ?? "");
      stored = [...stored.filter((p) => p.name !== (previous || profile.name)), { ...profile }];
      return stored;
    }
    if (cmd === "profiles_delete") {
      stored = stored.filter((p) => p.name !== payload.name);
      return stored;
    }
    // the dialog plugin's "Are you sure?": yes; the file chosen to save to or open
    if (cmd === "plugin:dialog|ask") return true;
    if (cmd === "plugin:dialog|save") return "/tmp/root profile.json";
    if (cmd === "plugin:dialog|open") return JSON.stringify(payload).includes("SPSL script") ? "/Users/me/scripts/omv-login.spsl" : "/tmp/profiles.json";
    if (cmd === "profiles_read_file") return inFile;
    if (cmd === "buttonbars_load") return [{ name: "Default", buttons: [], tabIcons: [] }, { name: "Cisco", buttons: [], tabIcons: [] }];
    // the key fetch's key file: 37 minutes old, or 5 hours for the "old" one
    if (cmd === "profile_key_age") return String(payload.keyFile).includes("old") ? 5 * 3600 : 37 * 60;
    return null;
  });
});

/** Render the tab and wait for the list to load */
async function open(initial: string | null = null) {
  render(ProfilesTab, { initial });
  await screen.findByRole("option", { name: "root" });
}

/** A button by its name */
const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;
/** The dots beside fields that have changed */
const dots = () => screen.queryAllByLabelText("changed");

describe("Settings > Profiles", () => {
  it("opens on the profile asked for, matched whatever its case", async () => {
    await open("ROOT");
    expect(screen.getByRole("option", { name: "root" }).getAttribute("aria-selected")).toBe("true");
    expect((screen.getByLabelText("Username") as HTMLInputElement).value).toBe("root");
  });

  it("has Save and Revert greyed until something changes, then marks the field", async () => {
    await open("root");
    expect(button("Save").disabled).toBe(true);
    expect(button("Revert").disabled).toBe(true);
    expect(dots()).toHaveLength(0);

    await userEvent.type(screen.getByLabelText("Username"), "x");
    expect(button("Save").disabled).toBe(false);
    expect(screen.getByText("Unsaved changes (1)")).toBeTruthy();
    expect(dots()).toHaveLength(1);
  });

  it("greys New, Remove and the other profiles while there are changes", async () => {
    await open("root");
    await userEvent.type(screen.getByLabelText("Username"), "x");
    expect(button("New").disabled).toBe(true);
    expect(button("Remove").disabled).toBe(true);
    expect(screen.getByRole("option", { name: DEFAULT_PROFILE }).getAttribute("aria-disabled")).toBe("true");
  });

  it("doesn't count retyping the saved value as a change", async () => {
    await open("root");
    // its label also holds the note "0: not set"
    const port = screen.getByLabelText(/^Port/) as HTMLInputElement;
    await userEvent.clear(port);
    await userEvent.type(port, "2222");
    expect(dots()).toHaveLength(0);
    expect(button("Save").disabled).toBe(true);
  });

  it("puts the fields back with Revert", async () => {
    await open("root");
    const username = screen.getByLabelText("Username") as HTMLInputElement;
    await userEvent.type(username, "x");
    await userEvent.click(button("Revert"));
    expect(username.value).toBe("root");
    expect(button("Save").disabled).toBe(true);
  });

  it("saves the changed profile under its old name", async () => {
    await open("root");
    await userEvent.type(screen.getByLabelText("Username"), "x");
    await userEvent.click(button("Save"));
    const save = calls.find((c) => c.cmd === "profiles_save");
    expect(save?.args.previousName).toBe("root");
    expect((save?.args.profile as Profile).username).toBe("rootx");
    expect(await screen.findByText("Saved.")).toBeTruthy();
    expect(dots()).toHaveLength(0);
  });

  it("duplicates from the list's menu under the next free name, without the password", async () => {
    await open("root");
    await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Duplicate" }));
    const save = calls.find((c) => c.cmd === "profiles_save");
    expect((save?.args.profile as Profile).name).toBe("root-1");
    expect(save?.args.previousName).toBe("");
    expect(save?.args.password).toBeNull();
    expect(await screen.findByRole("option", { name: "root-1" })).toBeTruthy();
  });

  it("exports the profile right-clicked to the file chosen, and says its password isn't in it", async () => {
    stored = stored.map((p) => (p.name === "root" ? { ...p, hasPassword: true } : p));
    await open("root");
    await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Export…" }));
    expect(await screen.findByText(`Exported "root". Its password isn't in the file.`)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "profiles_export")?.args).toEqual({ name: "root", path: "/tmp/root profile.json" });
  });

  it("imports the profiles in a file, one already here under the next free name, without passwords", async () => {
    inFile = [{ ...emptyProfile("root"), username: "admin" }, { ...emptyProfile("lab"), port: 2200 }];
    await open("root");
    await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Import…" }));
    expect(await screen.findByText(/Imported 2 profiles\. Already here, so imported "root" as "root-1"\./)).toBeTruthy();
    const saves = calls.filter((c) => c.cmd === "profiles_save");
    expect(saves.map((c) => [(c.args.profile as Profile).name, c.args.previousName, c.args.password])).toEqual([["root-1", "", null], ["lab", "", null]]);
    // the root here is left as it was; the imported one is beside it, and the last one opens
    expect(stored.find((p) => p.name === "root")?.username).toBe("root");
    expect(stored.find((p) => p.name === "root-1")?.username).toBe("admin");
    expect(screen.getByRole("option", { name: "lab" }).getAttribute("aria-selected")).toBe("true");
  });

  it("shows how old a fetched key is, and when the next session will fetch another", async () => {
    await open("CyberArk");
    expect(await screen.findByText(/Key: 37 min old, good for 4 h\./)).toBeTruthy();
    const keyFile = screen.getByLabelText("Key file") as HTMLInputElement;
    await userEvent.clear(keyFile);
    await userEvent.type(keyFile, "~/.ssh/old_key");
    expect(await screen.findByText(/The key is 5 h old, past its 4 h: the next session fetches a new one\./)).toBeTruthy();
  });

  it("fills the key fetch from an example, and says what Log in & cache key needs", async () => {
    await open("root");
    expect(screen.getByText("Set a Fetch command first (Examples has some)")).toBeTruthy();
    expect(button("Log in & cache key").disabled).toBe(true);
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Fetch command examples" }), "CyberArk (MFA caching)");
    expect((screen.getByRole("textbox", { name: /^Fetch command/ }) as HTMLInputElement).value).toContain("@key@{cyberark_gateway}:/key");
    expect((screen.getByLabelText(/^Lifetime/) as HTMLInputElement).value).toBe("240");
    expect((screen.getByLabelText("Key file") as HTMLInputElement).value).toBe("~/.ssh/cyberark_key");
    // changed, so the button waits for Save
    expect(screen.getByText("Save the profile first: the fetch runs its saved command")).toBeTruthy();
  });

  it("can't delete Default Settings from the menu", async () => {
    await open();
    await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByRole("option", { name: DEFAULT_PROFILE }) });
    const remove = within(screen.getByRole("menu")).getByRole("menuitem", { name: "Delete…" });
    expect(remove.getAttribute("aria-disabled")).toBe("true");
  });

  it("has Server load off unless chosen, greyed where it can't work (Windows; this test's jsdom isn't a Mac)", async () => {
    await open("root");
    const load = screen.getByRole("combobox", { name: /^Server load/ }) as HTMLSelectElement;
    // a profile takes Default Settings', which is off: a Cisco is never asked
    expect(load.value).toBe("");
    expect(load.disabled).toBe(!isMac);
  });

  it("greys the colours until Colours is Custom, then saves the profile's own background", async () => {
    await open("root");
    const background = () => screen.getByLabelText("Background") as HTMLInputElement;
    const colours = screen.getByRole("combobox", { name: /^Colours/ }) as HTMLSelectElement;
    expect(colours.value).toBe("default");
    expect(background().disabled).toBe(true);
    await userEvent.selectOptions(colours, "custom");
    expect(background().disabled).toBe(false);
    await fireEvent.input(background(), { target: { value: "#3a0000" } });
    expect(screen.getByText("#3a0000")).toBeTruthy();
    expect(dots()).toHaveLength(2);
    await userEvent.click(button("Save"));
    const save = calls.find((c) => c.cmd === "profiles_save");
    expect(save?.args.profile).toMatchObject({ colours: "custom", background: "#3a0000" });
    await userEvent.click(screen.getAllByRole("button", { name: "Default's" })[0]);
    expect(dots()).toHaveLength(1);
    expect(background().value).toBe("#000000");
  });

  it("sets a Tab colour whatever Colours is, and puts Default Settings' back", async () => {
    await open("root");
    const tab = () => screen.getByLabelText("Tab colour") as HTMLInputElement;
    /** What the hint beside the picker says */
    const hint = () => tab().nextElementSibling!.textContent;
    // Colours is From Default Settings (the theme), and the Tab colour can still be set
    expect(tab().disabled).toBe(false);
    expect(hint()).toBe("None");
    await fireEvent.input(tab(), { target: { value: "#2da44e" } });
    expect(hint()).toBe("#2da44e");
    expect(dots()).toHaveLength(1);
    await userEvent.click(button("Save"));
    const save = calls.find((c) => c.cmd === "profiles_save");
    expect(save?.args.profile).toMatchObject({ name: "root", colours: "", tabColor: "#2da44e" });
    // its last "Default's" button is the Tab colour's
    await userEvent.click(screen.getAllByRole("button", { name: "Default's" }).at(-1)!);
    expect(dots()).toHaveLength(1);
    expect(hint()).toBe("None");
  });

  it("saves the Button bar chosen, and keeps showing one that's no longer among the bars", async () => {
    stored = [...stored, { ...emptyProfile("Old"), buttonBar: "Retired" }];
    await open("root");
    const bar = () => screen.getByRole("combobox", { name: /^Button bar/ }) as HTMLSelectElement;
    expect([...bar().options].map((o) => o.text)).toEqual(["The one for the tab's icon", "Default", "Cisco"]);
    await userEvent.selectOptions(bar(), "Cisco");
    expect(dots()).toHaveLength(1);
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "root", buttonBar: "Cisco" });
    // a bar since renamed or deleted
    await userEvent.click(screen.getByRole("option", { name: "Old" }));
    expect(bar().value).toBe("Retired");
    expect(bar().selectedOptions[0].text).toBe("Retired (not in the button bars)");
  });

  it("saves an SPSL script for the profile's sessions, typed or chosen, and shows Default Settings' when it has none", async () => {
    stored = stored.map((p) => (p.name === DEFAULT_PROFILE ? { ...p, script: "every.spsl" } : p));
    await open("root");
    const script = () => screen.getByLabelText("SPSL script") as HTMLInputElement;
    expect(script().value).toBe("");
    expect(script().placeholder).toBe("From Default Settings (every.spsl)");
    await userEvent.type(script(), "linux-login.spsl");
    expect(dots()).toHaveLength(1);
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "root", script: "linux-login.spsl" });
    // Choose… puts the file picked in its place
    const choose = within(script().closest("label")!).getByRole("button", { name: "Choose…" });
    await userEvent.click(choose);
    await expect.poll(() => script().value).toBe("/Users/me/scripts/omv-login.spsl");
  });

  it("ticks Log in here for a jump server, and doesn't offer it on Default Settings", async () => {
    await open("root");
    const box = () => screen.getByRole("checkbox", { name: /Log in here/ }) as HTMLInputElement;
    expect(box().checked).toBe(false);
    await userEvent.click(box());
    expect(dots()).toHaveLength(1);
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "root", logInHere: true });
    // Default Settings is every session's, so it would send them all to one host
    await userEvent.click(screen.getByRole("option", { name: DEFAULT_PROFILE }));
    expect(screen.queryByRole("checkbox", { name: /Log in here/ })).toBeNull();
  });

  it("greys the colours again for Dark, and keeps them for later", async () => {
    stored = stored.map((p) => (p.name === "root" ? { ...p, background: "#3a0000" } : p));
    await open("root");
    const colours = screen.getByRole("combobox", { name: /^Colours/ }) as HTMLSelectElement;
    // colours from before there was a choice: Custom
    expect(colours.value).toBe("custom");
    await userEvent.selectOptions(colours, "dark");
    expect((screen.getByLabelText("Background") as HTMLInputElement).disabled).toBe(true);
    expect(screen.getAllByText("the Dark theme's").length).toBe(4);
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ colours: "dark", background: "#3a0000" });
  });

  it("lists Default Settings' terminal choices once each, its blank shown as the built-in one", async () => {
    await open(DEFAULT_PROFILE);
    const blinks = screen.getByRole("combobox", { name: /^Cursor blinks/ }) as HTMLSelectElement;
    expect([...blinks.options].map((o) => o.text)).toEqual(["Yes", "No"]);
    expect(blinks.value).toBe("on");
    expect([...(screen.getByRole("combobox", { name: /^Bell/ }) as HTMLSelectElement).options].map((o) => o.text)).toEqual(["Flash", "Sound", "None"]);
    expect([...(screen.getByRole("combobox", { name: /^Cursor$/ }) as HTMLSelectElement).options].map((o) => o.text)).toEqual(["Block", "Underline", "Vertical line"]);
    expect(dots()).toHaveLength(0);
  });
});
