// Edit Session's tabs, which mark the one with a change unsaved; and its Password: the session's own, kept in the Keychain under its id, and Move -pw here, which takes the
// stored -pw there and saves the session without it. Also its notes: a profile the user doesn't have (not while the
// profiles are still being read), and {placeholders} with no value, with the way to Environment Variables. The Rust
// side is stood in for by mockIPC: what's saved, what's stored as the session's password, the profiles and the
// variables, which can be held back to show the window before they arrive.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
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

beforeEach(() => {
  saved = null;
  stored = [];
  moved = [];
  profilesWait = null;
  variables = [];
  profileList = [{ name: "Default Settings" }, { name: "root" }];
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
    // Move -pw here asks first, with its own button labels: answered with its OK button
    if (cmd === "plugin:dialog|message") return "Move it";
    return null;
  });
});

/** Edit Session open on the session, with what it was saved as given to `onSaved` */
function open(fields: SessionFields = SESSION) {
  render(EditSession, {
    store: "saved", id: "Lab/sw1", fields, folders: [], iconSize: 24, customs: new Map(),
    onSaved: () => {}, onCancel: () => {}, onOpenProfiles: () => {},
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
    const note = await screen.findByRole("note");
    expect(note.textContent?.replace(/\s+/g, " ")).toBe(
      'Through the jump server: the tab logs in to 192.168.3.2 as root (profile "Jump Server", Log in here). This session\'s host, username and port are where its SPSL script goes on to: cisco-jump.spsl (Jump Server\'s).',
    );
    // the session's own password isn't the one that logs in
    expect((screen.getByLabelText(/Password/) as HTMLInputElement).placeholder).toBe('not used: the tab logs in as profile "Jump Server"');
  });

  it("says nothing for a profile without Log in here, and the SPSL script field shows the profile's", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "", profile: "Linux" });
    await user.click(screen.getByRole("tab", { name: "Terminal" }));
    await expect.poll(() => (screen.getByLabelText(/SPSL script/) as HTMLInputElement).placeholder).toBe("none: the profile's, omv.spsl (Linux's)");
    expect(screen.queryByRole("note")).toBeNull();
  });
});
