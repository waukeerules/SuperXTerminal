// What fromProfile.ts lists under Edit Session's tabs as a session's profile's: what it sets in words, else Default
// Settings', nothing the app does anyway, and nothing the session sets itself. Plain functions over stand-in profiles.
import { beforeEach, describe, expect, it } from "vitest";
import { settings } from "../settings/store.svelte";
import { profileRows, type SessionOwn } from "./fromProfile";
import { DEFAULT_PROFILE, emptyProfile, type Profile } from "../settings/profiles";

/** A session that sets nothing itself */
const NONE: SessionOwn = { jumpHost: false, algorithms: false, buttonBar: false, script: false };

/** Default Settings with these, and a profile "Cisco" with these */
function profiles(defaults: Partial<Profile>, cisco: Partial<Profile>): Profile[] {
  return [{ ...emptyProfile(DEFAULT_PROFILE), ...defaults }, { ...emptyProfile("Cisco"), ...cisco }];
}

/** The rows as "Label: value (whose) -> field" */
const said = (rows: ReturnType<typeof profileRows>) => rows.map((r) => `${r.label}: ${r.value}${r.whose === "Cisco" ? "" : ` (${r.whose})`} -> ${r.field}`);

describe("what a profile gives a session", () => {
  it("lists the terminal's settings in words, Default Settings' where the profile leaves one", () => {
    const all = profiles({ scrollback: 20000, fontFamily: "Consolas" }, { charset: "cp437", keyBackspace: "bs", colours: "dark", fontSize: 14, script: "scripts/cisco.spsl", buttonBar: "Cisco" });
    expect(said(profileRows("terminal", all, "Cisco", NONE))).toEqual([
      `Font: Consolas (${DEFAULT_PROFILE}) -> fontFamily`,
      "Font size: 14 pt -> fontSize",
      "Colours: Dark -> colours",
      "Backspace sends: Control-H -> keyBackspace",
      "Character set: CP437 (DOS, line drawing) -> charset",
      `Scrollback: 20000 lines (${DEFAULT_PROFILE}) -> scrollback`,
      "Button bar: Cisco -> buttonBar",
      "Login script: scripts/cisco.spsl -> script",
    ]);
  });

  it("leaves out what the app does anyway, what the profile sets back to it, and what the session sets itself", () => {
    // block cursor, Control-? and UTF-8 are the app's own; the profile's "off" wins over Default Settings' "on"
    const all = profiles({ compression: true, cursor: "bar" }, { cursor: "block", keyBackspace: "del", charset: "", compression: false, script: "x.spsl", buttonBar: "Cisco" });
    expect(said(profileRows("terminal", all, "Cisco", { ...NONE, script: true, buttonBar: true }))).toEqual([]);
    expect(said(profileRows("connection", all, "Cisco", NONE))).toEqual([]);
  });

  it("lists the login and algorithm settings for the SSH tab", () => {
    const all = profiles({ olderAlgorithms: true }, { keyFile: "~/.ssh/lab_key", offerKeys: false, algorithms: "kex=diffie-hellman-group14-sha1", addressFamily: "ipv4" });
    expect(said(profileRows("ssh", all, "Cisco", NONE))).toEqual([
      "Key file: ~/.ssh/lab_key -> keyFile",
      "Offer SSH keys: off: only the key file and password -> offerKeys",
      `Allow older algorithms: on (${DEFAULT_PROFILE}) -> olderAlgorithms`,
      "SSH algorithms: lists of its own -> algorithms",
      "Internet protocol: IPv4 -> addressFamily",
    ]);
    // the session's own algorithm lists win
    expect(profileRows("ssh", all, "Cisco", { ...NONE, algorithms: true }).some((r) => r.field === "algorithms")).toBe(false);
  });

  it("says a profile's tunnels are off, and which are kept", () => {
    const all = profiles({}, { tunnelsOff: true, forwards: [{ kind: "local", listen: "8080", to: "localhost:80" }] });
    expect(said(profileRows("connection", all, "Cisco", NONE))).toEqual(["Tunnels: off (kept: Local 8080 → localhost:80) -> forwards"]);
  });

  it("says a key file in 1Password is its item, through 1Password's SSH agent", () => {
    settings.onePassword = true;
    const all = profiles({}, { keyFile: '"op://Private/SSH Key - Lab Server/private key"' });
    expect(said(profileRows("ssh", all, "Cisco", NONE))).toEqual(["Key file: Private/SSH Key - Lab Server in 1Password, through its SSH agent -> keyFile"]);
  });

  it("says a key file in 1Password isn't used while 1Password is turned off", () => {
    settings.onePassword = false;
    const all = profiles({}, { keyFile: "op://Private/SSH Key - Lab Server/private key" });
    expect(said(profileRows("ssh", all, "Cisco", NONE))).toEqual([
      "Key file: Private/SSH Key - Lab Server in 1Password, not used: 1Password is turned off (Settings > General) -> keyFile",
    ]);
  });

  it("follows Based on, naming the session's profile", () => {
    const all = [...profiles({}, { charset: "latin1" }), { ...emptyProfile("Cisco old"), basedOn: "Cisco" }];
    expect(profileRows("terminal", all, "Cisco old", NONE)).toEqual([{ label: "Character set", value: "ISO-8859-1 (Latin-1)", whose: "Cisco old", field: "charset" }]);
  });
});
