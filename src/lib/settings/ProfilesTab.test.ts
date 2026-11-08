// Settings > Profiles: which fields show as changed, what's greyed while there are changes, Save and Revert, the
// list's Duplicate, Export and Import, Server load (off unless chosen; on Windows, a key login of its own), the fields
// a profile's tabs take (font, keys, tunnels, scrollback, log, character set), Based on, and the team's templates.
// The Rust side is stood in for by mockIPC: the profiles live in `stored`, every save and delete is recorded in
// `calls`, the file chosen to import holds `inFile`, the settings folder's templates are `templates`, and every
// question is answered with its OK button.
import { beforeEach, describe, expect, it } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import ProfilesTab from "./ProfilesTab.svelte";
import { DEFAULT_PROFILE, emptyProfile, type Profile } from "./profiles";
import { isMac } from "../shortcuts";
import { settings } from "./store.svelte";

/** The profiles stored, which the stand-ins for profiles_save and profiles_delete change */
let stored: Profile[];
/** Every command the tab sent, with its arguments */
let calls: { cmd: string; args: Record<string, unknown> }[];
/** The profiles in the file chosen to import, as profiles_read_file reads them */
let inFile: Profile[];
/** PuTTY's sessions in the .reg file chosen to import, as profiles_read_putty reads them */
let puttyFile: Profile[];
/** The file chosen to import: a .json file unless a test picks a .reg */
let pickedFile: string;
/** The team's templates in the settings folder, as profile_templates reads them */
let templates: Profile[];

beforeEach(() => {
  stored = [
    { ...emptyProfile(DEFAULT_PROFILE), olderAlgorithms: true },
    { ...emptyProfile("root"), username: "root", port: 2222 },
    { ...emptyProfile("CyberArk"), keyFile: "~/.ssh/cyberark_key", keyFetch: "pam-key {key_file}", keyLifetime: 240 },
  ];
  calls = [];
  inFile = [];
  puttyFile = [];
  pickedFile = "/tmp/profiles.json";
  templates = [];
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
    // ask() is a message with OK and Cancel: OK, by its label
    if (cmd === "plugin:dialog|message") {
      const buttons = payload.buttons;
      // custom labels come as { OkCancelCustom: [ok, cancel] }
      return typeof buttons === "object" && buttons ? Object.values(buttons as Record<string, string[]>)[0][0] : "Yes";
    }
    if (cmd === "plugin:dialog|save") return "/tmp/root profile.json";
    if (cmd === "plugin:dialog|open") return JSON.stringify(payload).includes("login script") ? "/Users/me/SuperXTerminal/scripts/omv-login.spsl" : pickedFile;
    // as scripts::as_spec, for a settings folder of /Users/me/SuperXTerminal: a script in there is kept relative
    if (cmd === "script_chosen") return String(payload.path).replace(/^\/Users\/me\/SuperXTerminal\//, "");
    if (cmd === "profiles_read_file") return inFile;
    if (cmd === "profiles_read_putty") return puttyFile;
    if (cmd === "profile_templates") return templates;
    if (cmd === "profile_template_share") return templates;
    if (cmd === "buttonbars_load") return [{ name: "Default", buttons: [], tabIcons: [] }, { name: "Cisco", buttons: [], tabIcons: [] }];
    // the key fetch's key file: 37 minutes old, or 5 hours for the "old" one
    if (cmd === "profile_key_age") return String(payload.keyFile).includes("old") ? 5 * 3600 : 37 * 60;
    return null;
  });
});

/** Render the tab and wait for the list to load */
async function open(initial: string | null = null) {
  render(ProfilesTab, { initial });
  await within(await screen.findByRole("listbox", { name: "Profiles" })).findByRole("option", { name: "root" });
}

/** The list of profiles, to find one in apart from the Based on choices */
const listed = () => within(screen.getByRole("listbox", { name: "Profiles" }));
/** A button by its name */
const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;
/** The dots beside fields that have changed */
const dots = () => screen.queryAllByLabelText("changed");

describe("Settings > Profiles", () => {
  it("opens on the profile asked for, matched whatever its case", async () => {
    await open("ROOT");
    expect(listed().getByRole("option", { name: "root" }).getAttribute("aria-selected")).toBe("true");
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
    expect(listed().getByRole("option", { name: DEFAULT_PROFILE }).getAttribute("aria-disabled")).toBe("true");
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
    await userEvent.pointer({ keys: "[MouseRight]", target: listed().getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Duplicate" }));
    const save = calls.find((c) => c.cmd === "profiles_save");
    expect((save?.args.profile as Profile).name).toBe("root-1");
    expect(save?.args.previousName).toBe("");
    expect(save?.args.password).toBeNull();
    expect(await within(await screen.findByRole("listbox", { name: "Profiles" })).findByRole("option", { name: "root-1" })).toBeTruthy();
  });

  it("exports the profile right-clicked to the file chosen, and says its password isn't in it", async () => {
    stored = stored.map((p) => (p.name === "root" ? { ...p, hasPassword: true } : p));
    await open("root");
    await userEvent.pointer({ keys: "[MouseRight]", target: listed().getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Export…" }));
    expect(await screen.findByText(`Exported "root". Its password isn't in the file.`)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "profiles_export")?.args).toEqual({ name: "root", path: "/tmp/root profile.json" });
  });

  it("imports the profiles in a file, one already here under the next free name, without passwords", async () => {
    inFile = [{ ...emptyProfile("root"), username: "admin" }, { ...emptyProfile("lab"), port: 2200 }];
    await open("root");
    await userEvent.pointer({ keys: "[MouseRight]", target: listed().getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Import…" }));
    expect(await screen.findByText(/Imported 2 profiles\. Already here, so imported "root" as "root-1"\./)).toBeTruthy();
    const saves = calls.filter((c) => c.cmd === "profiles_save");
    expect(saves.map((c) => [(c.args.profile as Profile).name, c.args.previousName, c.args.password])).toEqual([["root-1", "", null], ["lab", "", null]]);
    // the root here is left as it was; the imported one is beside it, and the last one opens
    expect(stored.find((p) => p.name === "root")?.username).toBe("root");
    expect(stored.find((p) => p.name === "root-1")?.username).toBe("admin");
    expect(listed().getByRole("option", { name: "lab" }).getAttribute("aria-selected")).toBe("true");
  });

  it("offers New and Import on a right-click in the list's empty space, and a profile's own menu on a profile", async () => {
    inFile = [{ ...emptyProfile("lab"), port: 2200 }];
    await open("root");
    await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByRole("listbox", { name: "Profiles" }) });
    const items = within(screen.getByRole("menu")).getAllByRole("menuitem").map((i) => i.textContent?.trim());
    expect(items).toEqual(["New", "Import…"]);
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Import…" }));
    expect(await screen.findByText(/Imported "lab"\./)).toBeTruthy();
    // a profile still gets its own menu, not the list's
    await userEvent.pointer({ keys: "[MouseRight]", target: listed().getByRole("option", { name: "root" }) });
    expect(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Duplicate" })).toBeTruthy();
  });

  it("imports PuTTY's saved sessions from a reg export as profiles", async () => {
    pickedFile = "/tmp/putty.reg";
    puttyFile = [{ ...emptyProfile("Lab switches"), host: "10.0.0.5", username: "admin" }];
    await open("root");
    await userEvent.pointer({ keys: "[MouseRight]", target: listed().getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Import…" }));
    expect(await screen.findByText(/Imported "Lab switches"\. From PuTTY: its proxies/)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "profiles_read_putty")?.args).toEqual({ path: "/tmp/putty.reg" });
    expect(calls.some((c) => c.cmd === "profiles_read_file")).toBe(false);
    expect(stored.find((p) => p.name === "Lab switches")?.host).toBe("10.0.0.5");
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
    await userEvent.pointer({ keys: "[MouseRight]", target: listed().getByRole("option", { name: DEFAULT_PROFILE }) });
    const remove = within(screen.getByRole("menu")).getByRole("menuitem", { name: "Delete…" });
    expect(remove.getAttribute("aria-disabled")).toBe("true");
  });

  it("explains each kind of tunnel with an everyday use, what the server must allow, and where the tab shows them", async () => {
    await open("root");
    const help = screen.getByRole("note", { name: "About tunnels" }).textContent ?? "";
    expect(help).toContain("Local (the usual one): a port on this PC opens host:port as the server sees it.");
    expect(help).toContain("https://localhost:8443");
    expect(help).toContain("Dynamic: a SOCKS proxy on this PC");
    expect(help).toContain("AllowTcpForwarding");
    expect(help).toContain("tunnel icon (or Tunnels… in its menu)");
  });

  it("names the tunnels' columns, says which side each port is on by type, and adds a sample with a free port", async () => {
    await open("root");
    await userEvent.click(button("Tunnel Samples…"));
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "The server's own web page (HTTP)" }));
    await userEvent.click(button("Tunnel Samples…"));
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Let the server reach a web server on this computer" }));
    // the same sample again: 8080 is taken, so 8081
    await userEvent.click(button("Tunnel Samples…"));
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "The server's own web page (HTTP)" }));
    expect(screen.getByText("Listens on")).toBeTruthy();
    expect(screen.getByText("Goes to")).toBeTruthy();
    const port = (n: number) => screen.getByLabelText(`Tunnel ${n} port`) as HTMLInputElement;
    const to = (n: number) => screen.getByLabelText(`Tunnel ${n} to`) as HTMLInputElement;
    expect([port(1).value, to(1).value, port(3).value]).toEqual(["8080", "localhost:80", "8081"]);
    // a local tunnel listens here and goes on from the server; a remote one the other way round
    expect([port(1).placeholder, to(1).placeholder]).toEqual(["a port on this PC", "host:port, as the server sees it"]);
    expect([port(2).placeholder, to(2).placeholder]).toEqual(["a port on the server", "host:port, as this PC sees it"]);
    await userEvent.click(button("Save"));
    expect((calls.find((c) => c.cmd === "profiles_save")?.args.profile as Profile).forwards).toEqual([
      { kind: "local", listen: "8080", to: "localhost:80" },
      { kind: "remote", listen: "9090", to: "localhost:8000" },
      { kind: "local", listen: "8081", to: "localhost:80" },
    ]);
  });

  it("says under Jump host what a jump server needs", async () => {
    await open("root");
    expect(screen.getByRole("note", { name: "What a jump server needs" }).textContent).toMatch(/^The jump server must allow TCP forwarding/);
  });

  it("names ssh's config file where Windows keeps it, not the Mac's ~/.ssh/config", async () => {
    // this test's jsdom isn't a Mac
    expect(isMac).toBe(false);
    await open("root");
    expect((screen.getByLabelText("SSH config host") as HTMLInputElement).placeholder).toBe(String.raw`a Host in C:\Users\<you>\.ssh\config`);
  });

  it("has Server load off unless chosen, and on Windows saves it on, saying it needs a key login", async () => {
    // this test's jsdom isn't a Mac: Windows, whose readings have a second login of their own (server_load.rs)
    expect(isMac).toBe(false);
    await open("root");
    const load = screen.getByRole("combobox", { name: /^Server load/ }) as HTMLSelectElement;
    // a profile takes Default Settings', which is off: a Cisco is never asked
    expect(load.value).toBe("");
    expect(load.disabled).toBe(false);
    expect(load.closest("label")!.nextElementSibling!.textContent).toMatch(/second login of its own[\s\S]*needs a key/);
    await userEvent.selectOptions(load, "on");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(calls.filter((c) => c.cmd === "profiles_save").at(-1)?.args).toMatchObject({ profile: { name: "root", serverLoad: true } });
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
    await userEvent.click(listed().getByRole("option", { name: "Old" }));
    expect(bar().value).toBe("Retired");
    expect(bar().selectedOptions[0].text).toBe("Retired (not in the button bars)");
  });

  it("saves a font and size for the profile's tabs, and shows Default Settings' when it has none", async () => {
    stored = stored.map((p) => (p.name === DEFAULT_PROFILE ? { ...p, fontFamily: "Courier New", fontSize: 11 } : p));
    await open("root");
    const font = () => screen.getByLabelText("Font") as HTMLInputElement;
    const size = () => screen.getByLabelText(/^Font size/) as HTMLInputElement;
    expect(font().placeholder).toBe("From Default Settings (Courier New)");
    expect(size().closest("label")!.textContent).toContain("0: Default Settings', 11");
    // the system's own monospaced fonts are offered, and any other can be typed
    expect(document.querySelectorAll("#profile-fonts option").length).toBeGreaterThan(2);
    await userEvent.type(font(), "Lucida Console");
    await userEvent.clear(size());
    await userEvent.type(size(), "16");
    expect(dots()).toHaveLength(2);
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "root", fontFamily: "Lucida Console", fontSize: 16 });
  });

  it("adds, changes and removes tunnels, and saves them with agent forwarding and compression", async () => {
    await open("root");
    await userEvent.click(button("Add tunnel"));
    await userEvent.type(screen.getByLabelText("Tunnel 1 port"), "8080");
    await userEvent.type(screen.getByLabelText("Tunnel 1 to"), "localhost:80");
    await userEvent.click(button("Add tunnel"));
    await userEvent.selectOptions(screen.getByLabelText("Tunnel 2 kind"), "dynamic");
    await userEvent.type(screen.getByLabelText("Tunnel 2 port"), "1080");
    // a SOCKS proxy goes wherever it's asked
    expect((screen.getByLabelText("Tunnel 2 to") as HTMLInputElement).disabled).toBe(true);
    await userEvent.click(button("Add tunnel"));
    await userEvent.click(button("Remove tunnel 3"));
    await userEvent.selectOptions(screen.getByRole("combobox", { name: /^Agent forwarding/ }), "on");
    await userEvent.selectOptions(screen.getByRole("combobox", { name: /^Compression/ }), "on");
    expect(dots()).toHaveLength(3);
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({
      name: "root", agentForwarding: true, x11Forwarding: null, compression: true,
      forwards: [{ kind: "local", listen: "8080", to: "localhost:80" }, { kind: "dynamic", listen: "1080", to: "" }],
    });
  });

  it("puts the tunnels back on Revert, the saved profile's untouched", async () => {
    stored = stored.map((p) => (p.name === "root" ? { ...p, forwards: [{ kind: "local", listen: "8080", to: "localhost:80" }] } : p));
    await open("root");
    await userEvent.clear(screen.getByLabelText("Tunnel 1 port"));
    await userEvent.type(screen.getByLabelText("Tunnel 1 port"), "9090");
    await userEvent.click(button("Remove tunnel 1"));
    expect(dots()).toHaveLength(1);
    await userEvent.click(button("Revert"));
    expect((screen.getByLabelText("Tunnel 1 port") as HTMLInputElement).value).toBe("8080");
    expect(dots()).toHaveLength(0);
  });

  it("saves more scrollback and a kept log for a profile, and shows Default Settings' scrollback when it has none", async () => {
    stored = stored.map((p) => (p.name === DEFAULT_PROFILE ? { ...p, scrollback: 20000 } : p));
    await open("root");
    const lines = screen.getByLabelText(/^Scrollback/) as HTMLInputElement;
    expect(lines.closest("label")!.textContent).toContain("0: Default Settings', 20000");
    await userEvent.clear(lines);
    await userEvent.type(lines, "100000");
    await userEvent.selectOptions(screen.getByRole("combobox", { name: /^Session log/ }), "on");
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "root", scrollback: 100000, sessionLog: true });
  });

  it("bases a profile on another, its empty fields then showing that one's, and never offers a loop", async () => {
    stored = [
      ...stored,
      { ...emptyProfile("netops"), username: "admin", fontFamily: "Consolas", basedOn: "" },
      { ...emptyProfile("netops-old"), basedOn: "netops" },
    ];
    await open("netops");
    const basedOn = () => screen.getByRole("combobox", { name: /^Based on/ }) as HTMLSelectElement;
    // not itself, not Default Settings, and not netops-old, which is based on it
    expect([...basedOn().options].map((o) => o.value)).toEqual(["", "root", "CyberArk"]);
    await userEvent.click(listed().getByRole("option", { name: "netops-old" }));
    expect(basedOn().value).toBe("netops");
    // what it leaves empty shows as netops'
    expect((screen.getByLabelText("Font") as HTMLInputElement).placeholder).toBe("From netops (Consolas)");
    expect([...(screen.getByRole("combobox", { name: /^Backspace sends/ }) as HTMLSelectElement).options].map((o) => o.text)[0]).toBe("From netops");
    // based on Default Settings alone again
    await userEvent.selectOptions(basedOn(), "");
    expect((screen.getByLabelText("Font") as HTMLInputElement).placeholder).toBe("the theme's; or an installed font");
    await userEvent.selectOptions(basedOn(), "root");
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "netops-old", basedOn: "root" });
  });

  it("offers the team's templates not set up here, and sets one up as a new profile for the login to be added", async () => {
    templates = [
      { ...emptyProfile("netops"), jumpHost: "bastion", algorithms: "kex=diffie-hellman-group14-sha1", buttonBar: "Cisco" },
      // set up here already: not offered
      { ...emptyProfile("ROOT"), port: 22 },
    ];
    await open();
    const list = listed();
    expect(list.getByText("Team templates")).toBeTruthy();
    expect(list.queryAllByRole("option").map((o) => o.textContent?.trim())).toEqual([DEFAULT_PROFILE, "root", "CyberArk", "netops"]);
    await userEvent.click(list.getByRole("option", { name: "netops" }));
    expect((screen.getByLabelText("Name") as HTMLInputElement).value).toBe("netops");
    expect((screen.getByLabelText("Jump host") as HTMLInputElement).value).toBe("bastion");
    expect(screen.getByText(/add your username and password/)).toBeTruthy();
    await userEvent.type(screen.getByLabelText("Username"), "jsmith");
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args).toMatchObject({
      previousName: "", profile: { name: "netops", username: "jsmith", jumpHost: "bastion", buttonBar: "Cisco" },
    });
    // set up now: no longer offered
    expect(listed().queryByText("Team templates")).toBeNull();
  });

  it("shares a profile with the team from its menu, once there's a settings folder, after asking", async () => {
    settings.settingsFolder = "/team";
    await open();
    await userEvent.pointer({ keys: "[MouseRight]", target: listed().getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Share as Team Template…" }));
    await expect.poll(() => calls.find((c) => c.cmd === "profile_template_share")?.args).toEqual({ name: "root" });
    expect(String(calls.find((c) => c.cmd === "plugin:dialog|message")?.args.message)).toContain("username, key file and password stay here");
    settings.settingsFolder = null;
  });

  it("saves a device's character set, UTF-8 on Default Settings being no choice of its own", async () => {
    await open("root");
    const charset = () => screen.getByRole("combobox", { name: /^Character set/ }) as HTMLSelectElement;
    expect(charset().value).toBe("");
    await userEvent.selectOptions(charset(), "cp437");
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "root", charset: "cp437" });
    await userEvent.click(listed().getByRole("option", { name: DEFAULT_PROFILE }));
    expect(charset().value).toBe("utf8");
    await userEvent.selectOptions(charset(), "latin1");
    await userEvent.selectOptions(charset(), "utf8");
    // back to what it was: nothing to save
    expect(dots()).toHaveLength(0);
  });

  it("saves the keys a device reads another way, and offers From Default Settings except on Default Settings", async () => {
    await open("root");
    const backspace = () => screen.getByRole("combobox", { name: /^Backspace sends/ }) as HTMLSelectElement;
    expect(backspace().value).toBe("");
    await userEvent.selectOptions(backspace(), "bs");
    await userEvent.selectOptions(screen.getByRole("combobox", { name: /^Function keys/ }), "vt100");
    expect(dots()).toHaveLength(2);
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "root", keyBackspace: "bs", keyHomeEnd: "", keyFunction: "vt100" });
    // Default Settings has PuTTY's defaults shown, not a From Default Settings of its own
    await userEvent.click(listed().getByRole("option", { name: DEFAULT_PROFILE }));
    expect(backspace().value).toBe("del");
    expect([...backspace().options].map((o) => o.value)).toEqual(["del", "bs"]);
  });

  it("saves an SPSL script for the profile's sessions, typed or chosen, and shows Default Settings' when it has none", async () => {
    stored = stored.map((p) => (p.name === DEFAULT_PROFILE ? { ...p, script: "every.spsl" } : p));
    await open("root");
    const script = () => screen.getByLabelText("Login script") as HTMLInputElement;
    expect(script().value).toBe("");
    expect(script().placeholder).toBe("From Default Settings (every.spsl)");
    await userEvent.type(script(), "linux-login.spsl");
    expect(dots()).toHaveLength(1);
    await userEvent.click(button("Save"));
    expect(calls.find((c) => c.cmd === "profiles_save")?.args.profile).toMatchObject({ name: "root", script: "linux-login.spsl" });
    // Choose… puts the file picked in its place, relative to the settings folder it's in, so the Mac and Windows
    // both find it; a full path typed is found on one only, and the field says so
    const choose = within(script().closest("label")!).getByRole("button", { name: "Choose…" });
    await userEvent.click(choose);
    await expect.poll(() => script().value).toBe("scripts/omv-login.spsl");
    expect(screen.queryByRole("note", { name: "Login script path" })).toBeNull();
    await userEvent.clear(script());
    await userEvent.type(script(), "/Users/me/omv.spsl");
    expect(screen.getByRole("note", { name: "Login script path" }).textContent).toMatch(/^A full path, found on this (Mac|PC) only\./);
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
    await userEvent.click(listed().getByRole("option", { name: DEFAULT_PROFILE }));
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
