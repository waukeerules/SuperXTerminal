// Edit Session's tabs, which mark the one with a change unsaved; and its Password: the session's own, kept in the Keychain under its id, and Move -pw here, which takes the
// stored -pw there and saves the session without it. Also its notes: a profile the user doesn't have (not while the
// profiles are still being read), and {placeholders} with no value, with the way to Environment Variables. The Rust
// side is stood in for by mockIPC: what's saved, what's stored as the session's password, the profiles and the
// variables, which can be held back to show the window before they arrive.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import EditSession from "./EditSession.svelte";
import type { SessionFields } from "./api";
import { VARIABLES_CHANGED } from "../settings/EnvironmentVariables.svelte";
import { context } from "../commands/sending.svelte";

const SESSION: SessionFields = {
  name: "sw1", folder: "Lab", protocol: "SSH", host: "192.168.0.50", port: 22, username: "admin", profile: "", color: "",
  icon: "switch", highlightRules: "", extraArgs: "-pw •••••• -P 2222", jumpHost: "", script: "", note: "", localPath: "",
  remotePath: "", rdpOptions: "", sshAlgorithms: "", buttonBar: "",
};

let saved: SessionFields | null;
let stored: { id: string; password: string }[];
let moved: string[];
/** What profiles_list waits for before it answers; none: at once */
let profilesWait: Promise<void> | null;
/** The Environment Variables' names, as variables_list gives them */
let variables: string[];
/** The profiles profiles_list gives */
let profileList: Record<string, unknown>[];
/** What the dialogs asked */
let asked: string[];
/** Settings > Profiles opened from the dialog: the profile, and the field scrolled to */
let openedProfiles: [string | undefined, string | undefined][];
/** Each Copy to Scripts Folder's `replace` */
let copies: boolean[];

beforeEach(() => {
  saved = null;
  stored = [];
  moved = [];
  profilesWait = null;
  variables = [];
  profileList = [{ name: "Default Settings" }, { name: "root" }];
  asked = [];
  openedProfiles = [];
  copies = [];
  mockIPC(async (cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "session_password_has") return false;
    if (cmd === "session_password_from_pw") {
      moved.push(String(payload.id));
      return null;
    }
    if (cmd === "session_password_set") {
      stored.push({ id: String(payload.id), password: String(payload.password) });
      return null;
    }
    if (cmd === "session_save") {
      saved = payload.fields as SessionFields;
      return "Lab/sw1";
    }
    if (cmd === "profiles_list") {
      if (profilesWait) await profilesWait;
      return profileList;
    }
    if (cmd === "variables_list") return variables.map((key) => ({ key, value: "", secret: false }));
    if (cmd === "variables_built_in") return [["host_address", ""], ["host_port", ""], ["key_file", ""]];
    // the SSH tab's lists: what this computer's ssh has
    if (cmd === "ssh_algorithms_available") {
      return [{ key: "kex", label: "Key exchange", supported: ["curve25519-sha256", "diffie-hellman-group14-sha1"], defaults: ["curve25519-sha256"] }];
    }
    if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
    if (cmd === "buttonbars_load") return [{ name: "Default", buttons: [], tabIcons: [] }, { name: "Cisco", buttons: [], tabIcons: [] }];
    // Move -pw here and Copy to Scripts Folder ask first, with their own button labels: answered with the OK button
    if (cmd === "plugin:dialog|message") {
      asked.push(String(payload.message));
      const buttons = payload.buttons;
      return typeof buttons === "object" && buttons ? Object.values(buttons as Record<string, string[]>)[0][0] : "Move it";
    }
    // a different jump-host.spsl is in the scripts folder already, until it's replaced
    if (cmd === "script_copy_in") {
      copies.push(payload.replace as boolean);
      return { kind: payload.replace ? "copied" : "exists", spec: "scripts/jump-host.spsl" };
    }
    return null;
  });
});

/** Edit Session open on the session, with what it was saved as given to `onSaved` */
function open(fields: SessionFields = SESSION) {
  render(EditSession, {
    store: "saved", id: "Lab/sw1", fields, folders: [], iconSize: 24, customs: new Map(),
    onSaved: () => {}, onCancel: () => {}, onOpenProfiles: (name?: string, field?: string) => void openedProfiles.push([name, field]),
  });
}

describe("Edit Session's password", () => {
  it("moves a stored -pw into the Keychain and saves the session without it", async () => {
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByRole("button", { name: "Move -pw here" }));
    await expect.poll(() => moved).toEqual(["Lab/sw1"]);
    expect((screen.getByLabelText(/Extra arguments/) as HTMLInputElement).value).toBe("-P 2222");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => saved?.extraArgs).toBe("-P 2222");
  });

  it("keeps a typed password in the Keychain under the session's id, not in its file", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "-P 2222" });
    expect(screen.queryByRole("button", { name: "Move -pw here" })).toBeNull();
    await user.type(screen.getByLabelText(/^Password/), "s3cret");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => stored).toEqual([{ id: "Lab/sw1", password: "s3cret" }]);
    expect(JSON.stringify(saved)).not.toContain("s3cret");
  });
});

describe("Edit Session's tabs", () => {
  it("show one tab's fields at a time, and mark a tab with a change unsaved", async () => {
    const user = userEvent.setup();
    open();
    const general = screen.getByRole("tab", { name: "General" });
    const connection = screen.getByRole("tab", { name: "Connection" });
    expect(general.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel", { name: "General" })).toBeTruthy();
    // the notes are on General
    expect(screen.getByLabelText(/Notes/).closest("[role=tabpanel]")?.getAttribute("aria-label")).toBe("General");
    await user.click(connection);
    expect(connection.getAttribute("aria-selected")).toBe("true");
    expect(connection.querySelector(".dot:not(.off)")).toBeNull();
    // its dot is there all along, so the tab doesn't widen as it shows
    expect(connection.querySelector(".dot.off")).not.toBeNull();
    await user.type(screen.getByLabelText(/Jump host/), "bastion");
    expect(connection.querySelector(".dot:not(.off)")).not.toBeNull();
    expect(general.querySelector(".dot:not(.off)")).toBeNull();
  });
});

describe("Edit Session's Button bar", () => {
  it("saves the bar chosen on the Terminal tab, and shows one no longer among the bars", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, buttonBar: "Retired" });
    await user.click(screen.getByRole("tab", { name: "Terminal" }));
    const bar = screen.getByLabelText(/^Button bar/) as HTMLSelectElement;
    await expect.poll(() => [...bar.options].map((o) => o.text)).toEqual(
      ["Its profile's, else the one for its icon", "Default", "Cisco", "Retired (not found)"],
    );
    expect(bar.value).toBe("Retired");
    await user.selectOptions(bar, "Cisco");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => saved?.buttonBar).toBe("Cisco");
  });
});

describe("Edit Session's Revert", () => {
  it("puts every field back as the window opened with it, and drops a typed password", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "-P 2222" });
    const revert = screen.getByRole("button", { name: "Revert" }) as HTMLButtonElement;
    expect(revert.disabled).toBe(true);
    await user.type(screen.getByLabelText(/Session name/), "-new");
    await user.type(screen.getByLabelText(/^Password/), "s3cret");
    await user.click(screen.getByRole("tab", { name: "Connection" }));
    await user.type(screen.getByLabelText(/Jump host/), "bastion");
    expect(revert.disabled).toBe(false);
    await user.click(revert);
    expect((screen.getByLabelText(/Session name/) as HTMLInputElement).value).toBe("sw1");
    expect((screen.getByLabelText(/Jump host/) as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText(/^Password/) as HTMLInputElement).value).toBe("");
    expect(revert.disabled).toBe(true);
    expect(screen.queryByText(/Unsaved changes/)).toBeNull();
  });
});

describe("Edit Session's SSH and Remote Desktop tabs", () => {
  /** The tabs' names, as shown */
  const tabs = () => screen.getAllByRole("tab").map((t) => t.textContent?.trim());

  it("show for their own connection type only", async () => {
    const user = userEvent.setup();
    open();
    expect(tabs()).toEqual(["General", "Connection", "SSH", "Terminal", "File transfer"]);
    await user.selectOptions(screen.getByLabelText(/Connection type/), "RDP");
    expect(tabs()).toEqual(["General", "Connection", "Remote Desktop", "Terminal", "File transfer"]);
    await user.selectOptions(screen.getByLabelText(/Connection type/), "Telnet");
    expect(tabs()).toEqual(["General", "Connection", "Terminal", "File transfer"]);
  });

  it("go back to General when the type changes away from the tab shown", async () => {
    const user = userEvent.setup();
    open();
    await user.click(screen.getByRole("tab", { name: "SSH" }));
    await user.click(screen.getByRole("tab", { name: "General" }));
    await user.selectOptions(screen.getByLabelText(/Connection type/), "Telnet");
    await user.click(screen.getByRole("tab", { name: "Connection" }));
    // SSH is gone; choosing it again isn't possible, and the window shows a tab that's there
    expect(screen.queryByRole("tab", { name: "SSH" })).toBeNull();
    expect(screen.getByRole("tab", { name: "Connection" }).getAttribute("aria-selected")).toBe("true");
  });

  it("save the session's own algorithm list from the SSH tab, and won't save one with nothing ticked", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "-P 2222" });
    await user.click(screen.getByRole("tab", { name: "SSH" }));
    await user.click(await screen.findByLabelText(/Use its own list/));
    expect(screen.getByRole("tab", { name: /^SSH/ }).querySelector(".dot:not(.off)")).not.toBeNull();
    await user.click(screen.getByLabelText("curve25519-sha256"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(saved).toBeNull();
    expect(screen.getAllByText(/Tick at least one in Key exchange/).length).toBeGreaterThan(0);
    await user.click(screen.getByLabelText("diffie-hellman-group14-sha1"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => saved?.sshAlgorithms).toBe("kex=diffie-hellman-group14-sha1");
  });
});

describe("Edit Session's notes", () => {
  it("doesn't say the session's profile is missing while the profiles are still being read", async () => {
    let arrive = () => {};
    profilesWait = new Promise((resolve) => (arrive = resolve));
    open({ ...SESSION, profile: "root" });
    // it said Profile "root" isn't set up for a moment as the window opened
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByText(/isn't set up/)).toBeNull();
    arrive();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(screen.queryByText(/isn't set up/)).toBeNull();
  });

  it("says a profile the user doesn't have is missing, once the profiles are read", async () => {
    open({ ...SESSION, profile: "nosuch" });
    expect(await screen.findByText(/Profile "nosuch" isn't set up/)).toBeTruthy();
  });

  it("marks a placeholder in the username with no value, opens Environment Variables, and lets go once it's defined", async () => {
    const user = userEvent.setup();
    const editVariables = vi.fn();
    context.editVariables = editVariables;
    variables = ["domain"];
    // {host_address} is built in, {domain} defined: only {user} has no value
    open({ ...SESSION, username: "{user}@{domain}@{host_address}" });
    const unset = await screen.findByText("{user}");
    expect(unset.className).toContain("unset");
    expect(screen.queryByText("{domain}")).toBeNull();
    expect(screen.getByText(/has no value yet, so the session won't open/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Environment Variables…" }));
    expect(editVariables).toHaveBeenCalledOnce();
    // defined and saved there
    variables = ["domain", "user"];
    window.dispatchEvent(new Event(VARIABLES_CHANGED));
    await expect.poll(() => screen.queryByText("{user}")).toBeNull();
  });
});

describe("Edit Session with a jump server profile", () => {
  beforeEach(() => {
    profileList = [
      { name: "Default Settings", script: "" },
      { name: "Jump Server", host: "192.168.3.2", username: "root", logInHere: true, script: "cisco-jump.spsl" },
      { name: "Linux", host: "192.168.3.2", script: "omv.spsl" },
    ];
  });

  it("says the tab logs in to the jump server, and the session's host is where its script goes", async () => {
    open({ ...SESSION, extraArgs: "", profile: "Jump Server" });
    const note = await screen.findByRole("note", { name: "Jump server" });
    expect(note.textContent?.replace(/\s+/g, " ")).toBe(
      'Through the jump server: the tab logs in to 192.168.3.2 as root (profile "Jump Server", Log in here). This session\'s host, username and port are where its login script goes on to: cisco-jump.spsl (Jump Server\'s).',
    );
    // the session's own password isn't the one that logs in
    expect((screen.getByLabelText(/Password/) as HTMLInputElement).placeholder).toBe('not used: the tab logs in as profile "Jump Server"');
  });

  it("says nothing for a profile without Log in here, and the login script field shows the profile's", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "", profile: "Linux" });
    await user.click(screen.getByRole("tab", { name: "Terminal" }));
    await expect.poll(() => (screen.getByLabelText(/Login script/) as HTMLInputElement).placeholder).toBe("none: the profile's, omv.spsl (Linux's)");
    expect(screen.queryByRole("note")).toBeNull();
  });
});

describe("Edit Session with a profile that has a jump host", () => {
  beforeEach(() => {
    profileList = [
      { name: "Default Settings", script: "" },
      { name: "Jump Test", jumpHost: "root@omv.example", script: "" },
      { name: "Jump Child", basedOn: "Jump Test", script: "" },
    ];
  });

  /** The note on the General tab saying how the session gets to its host, spaces run together */
  const said = async () => (await screen.findByRole("note", { name: "Jump host" })).textContent?.replace(/\s+/g, " ");

  it("says on the General tab that the session goes through the profile's jump host", async () => {
    open({ ...SESSION, extraArgs: "", profile: "Jump Test" });
    expect(await said()).toBe('Through the jump host root@omv.example (profile "Jump Test"): ssh connects to it first, then on to 192.168.0.50 from there.');
  });

  it("says the same for a profile based on one with a jump host", async () => {
    open({ ...SESSION, extraArgs: "", profile: "Jump Child" });
    expect(await said()).toBe('Through the jump host root@omv.example (profile "Jump Child"): ssh connects to it first, then on to 192.168.0.50 from there.');
  });

  it("names the session's own jump host when it has one, and says nothing with none", async () => {
    open({ ...SESSION, extraArgs: "", profile: "Jump Test", jumpHost: "me@bastion:22" });
    expect(await said()).toBe("Through the jump host me@bastion:22 (this session's Jump host, on the Connection tab): ssh connects to it first, then on to 192.168.0.50 from there.");
    cleanup();
    open({ ...SESSION, extraArgs: "", profile: "root" });
    await new Promise((r) => setTimeout(r, 20));
    expect(screen.queryByRole("note", { name: "Jump host" })).toBeNull();
  });
});

describe("Edit Session with a CyberArk profile", () => {
  /** The CyberArk profile: a gateway as its Host, and a key its Fetch command gets, good for 240 minutes */
  const CYBERARK = { name: "CyberArk", username: "unknown", host: "gw.example.test", keyFile: "~/.ssh/cyberark_key", keyFetch: "sftp ...", keyLifetime: 240, script: "", hasPassword: false };
  /** The session: a switch, its username CyberArk's connection string naming its host */
  const SWITCH = { ...SESSION, extraArgs: "", profile: "CyberArk", username: "{ad_username}#{cyberark_region}@{cyberark_username}#{cyberark_address}@{host_address}#{cyberark_vault}" };

  beforeEach(() => {
    profileList = [{ name: "Default Settings", script: "" }, CYBERARK];
  });

  it("says the tab connects to the gateway with the host in the username, and logs in with the fetched key", async () => {
    variables = ["ad_username", "cyberark_region", "cyberark_username", "cyberark_address", "cyberark_vault"];
    open(SWITCH);
    const text = async (name: string) => (await screen.findByRole("note", { name })).textContent?.replace(/\s+/g, " ").trim();
    expect(await text("Gateway")).toBe(
      'Through the gateway: the tab connects to gw.example.test (profile "CyberArk"\'s Host), and this session\'s host, 192.168.0.50, goes in the username as {host_address}.',
    );
    expect(await text("Key")).toBe(
      'Logs in with the key ~/.ssh/cyberark_key (profile "CyberArk"), fetched by its Fetch command when it\'s missing or older than 240 minutes: the sign-in (a link, a PIN) happens in the tab.',
    );
    expect((screen.getByLabelText(/Password/) as HTMLInputElement).placeholder).toBe("none: the key ~/.ssh/cyberark_key logs in");
  });

  it("says the gateway has no Host, and nothing of a gateway for a username without {host_address}", async () => {
    profileList = [{ name: "Default Settings", script: "" }, { ...CYBERARK, host: "" }];
    open(SWITCH);
    expect((await screen.findByRole("note", { name: "Gateway" })).textContent).toContain('profile "CyberArk"\'s Host, but it has none');
    cleanup();
    open({ ...SWITCH, username: "admin" });
    await screen.findByRole("note", { name: "Key" });
    expect(screen.queryByRole("note", { name: "Gateway" })).toBeNull();
  });
});

describe("Edit Session's login script", () => {
  it("offers to copy a full path's script to the scripts folder, asks before replacing another, and names the copy", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, script: String.raw`C:\Users\me\Downloads\jump-host.spsl` });
    await user.click(screen.getByRole("tab", { name: "Terminal" }));
    const note = screen.getByRole("note", { name: "Login script path" });
    expect(note.textContent).toMatch(/^A full path, found on this (Mac|PC) only\./);
    await user.click(within(note).getByRole("button", { name: "Copy to Scripts Folder" }));
    await expect.poll(() => (screen.getByLabelText(/Login script/) as HTMLInputElement).value).toBe("scripts/jump-host.spsl");
    expect(copies).toEqual([false, true]);
    expect(asked).toEqual(["The scripts folder already has scripts/jump-host.spsl, and it's different. Replace it with this one?"]);
    // a relative path is found from the Mac and Windows alike: nothing more to say
    expect(screen.queryByRole("note", { name: "Login script path" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => saved?.script).toBe("scripts/jump-host.spsl");
  });
});

describe("Edit Session's jump host", () => {
  it("says what a jump server needs: TCP forwarding, and Log in here where it can't", async () => {
    const user = userEvent.setup();
    open();
    await user.click(screen.getByRole("tab", { name: "Connection" }));
    const note = screen.getByRole("note", { name: "What a jump server needs" }).textContent ?? "";
    // sshd's own setting, or a managed server's SSH settings: no particular product named
    expect(note).toContain("must allow TCP forwarding (AllowTcpForwarding in its sshd_config, or its SSH settings)");
    expect(note).toContain("a profile that has Log in here");
  });
});

describe("Edit Session's Connection tab", () => {
  it("lists what the profile adds to the connection, each with a link to it in Settings > Profiles", async () => {
    profileList = [
      { name: "Default Settings", script: "", keepalive: 30, forwards: [] },
      { name: "OMV Tunnels", script: "", compression: true, forwards: [{ kind: "local", listen: "8080", to: "localhost:80" }, { kind: "dynamic", listen: "1080", to: "" }] },
    ];
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "", profile: "OMV Tunnels" });
    await user.click(screen.getByRole("tab", { name: "Connection" }));
    const list = await screen.findByRole("region", { name: "From the profile" });
    expect(within(list).getByRole("heading").textContent).toBe('From profile "OMV Tunnels", when the session opens');
    const rows = within(list).getAllByRole("listitem").map((li) => li.textContent?.replace(/\s+/g, " ").replace(" Edit…", "").trim());
    expect(rows).toEqual([
      "Tunnels: Local 8080 → localhost:80; SOCKS 1080",
      "Compression: on",
      "Keep-alive: every 30 s (Default Settings)",
    ]);
    await user.click(within(within(list).getAllByRole("listitem")[0]).getByRole("button", { name: "Edit…" }));
    await user.click(within(within(list).getAllByRole("listitem")[2]).getByRole("button", { name: "Edit…" }));
    expect(openedProfiles).toEqual([["OMV Tunnels", "forwards"], ["Default Settings", "keepalive"]]);
  });

  it("lists what the profile gives the terminal under the Terminal tab, with links", async () => {
    profileList = [
      { name: "Default Settings", script: "" },
      { name: "Old Cisco", script: "", charset: "cp437", keyBackspace: "bs" },
    ];
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "", profile: "Old Cisco" });
    await user.click(screen.getByRole("tab", { name: "Terminal" }));
    const list = await screen.findByRole("region", { name: "From the profile" });
    const rows = within(list).getAllByRole("listitem").map((li) => li.textContent?.replace(/\s+/g, " ").replace(" Edit…", "").trim());
    expect(rows).toEqual(["Backspace sends: Control-H", "Character set: CP437 (DOS, line drawing)"]);
    await user.click(within(within(list).getAllByRole("listitem")[1]).getByRole("button", { name: "Edit…" }));
    expect(openedProfiles).toEqual([["Old Cisco", "charset"]]);
  });

  it("offers to add tunnels to a profile that adds nothing", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "", profile: "root" });
    await user.click(screen.getByRole("tab", { name: "Connection" }));
    const list = await screen.findByRole("region", { name: "From the profile" });
    expect(list.textContent).toContain("No tunnels, forwarding or other connection settings.");
    await user.click(within(list).getByRole("button", { name: "Add tunnels…" }));
    expect(openedProfiles).toEqual([["root", "forwards"]]);
  });
});

describe("Edit Session's Jump host ▾", () => {
  it("fills the jump host with a saved session chosen by its name or folder, leaving this session out", async () => {
    const asked: [string, string | undefined][] = [];
    context.pickSession = (title, onChoose, except) => {
      asked.push([title, except]);
      // a Database session: its path without the "@db/" its id has
      onChoose({ store: "database", id: "@db/Bastions/bastion1", name: "bastion1" } as never);
    };
    const user = userEvent.setup();
    open();
    await user.click(screen.getByRole("tab", { name: "Connection" }));
    await user.click(screen.getByRole("button", { name: "Choose a session as the jump host" }));
    expect(asked).toEqual([["Jump Host", "saved:Lab/sw1"]]);
    // its list first, so it's that session where the same path is in another list
    expect((within(screen.getByRole("tabpanel", { name: "Connection" })).getByLabelText(/Jump host/) as HTMLInputElement).value).toBe("Database/Bastions/bastion1");
    context.pickSession = () => {};
  });
});
