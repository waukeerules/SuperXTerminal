// What a script starts with in a tab: the tab's session's values (sessionValues, from the Sessions tree through
// context.sessionOf; a Quick connect tab's typed fields, tabValues), and Environment Variables filled in without
// touching the script's own {variables} or the session's (fillScriptPlaceholders); and which language a session's
// login script is in (openingLanguage). The Rust side is stood in for by mockIPC: variables_fill knows {site} only.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";
import { context, fillScriptPlaceholders, openingLanguage, openingScriptWarning, sessionValues, tabValues } from "./sending.svelte";
import type { SessionView } from "../sessions/api";
import { isMac } from "../shortcuts";

/** A saved SSH session, as the tree has it */
const SW1 = {
  id: "Lab/core-sw1", store: "saved", name: "core-sw1", folder: "Lab", protocol: "SSH", host: "10.0.0.5", port: 0, username: "admin",
  profile: "root", icon: "switch", color: "", note: "", extraArgs: "", script: "", buttonBar: "", updatedBy: "", locked: false,
  lockedFields: "", pending: false, pendingNote: "",
} as SessionView;

/** What variables_fill was asked to fill */
let asked: string[];

beforeEach(() => {
  asked = [];
  context.sessionOf = (key) => (key === "saved:Lab/core-sw1" ? SW1 : undefined);
  mockIPC((cmd, args) => {
    if (cmd !== "variables_fill") return null;
    const text = (args as { text: string }).text;
    asked.push(text);
    const missing = [...new Set(text.match(/\{\w+\}/g) ?? [])].filter((m) => m !== "{site}");
    return { text: text.replace(/\{site\}/g, "lab-a"), missing };
  });
});

afterEach(() => {
  context.sessionOf = () => undefined;
});

describe("a script's start", () => {
  it("has the tab's session's values, the port its protocol's usual one when it has none", () => {
    expect(sessionValues("saved:Lab/core-sw1")).toEqual({
      host_address: "10.0.0.5", host_port: "22", session_name: "core-sw1", session_id: "Lab/core-sw1", session_folder: "Lab",
      session_username: "admin", session_profile: "root", session_protocol: "SSH",
    });
    // a Local Shell or Quick connect has no session
    expect(sessionValues("")).toEqual({});
  });

  it("has a Quick connect tab's typed host and login, which has no saved session", () => {
    const typed = { host_address: "192.168.0.50", host_port: "22", session_name: "192.168.0.50", session_username: "admin", session_profile: "Jump Host", session_protocol: "SSH" };
    expect(tabValues({ session: "", values: typed })).toEqual(typed);
    // a saved session's tab has its own, and a Local Shell none
    expect(tabValues({ session: "saved:Lab/core-sw1" }).host_address).toBe("10.0.0.5");
    expect(tabValues({ session: "" })).toEqual({});
  });

  it("fills Environment Variables in, but leaves the session's and the script's own {variables} for it", async () => {
    const script = "SENDLINE ssh {host_address} # {site}\nSET vlan 20\nSENDLINE vlan {vlan} {1} {repeat}";
    expect(await fillScriptPlaceholders(script)).toBe("SENDLINE ssh {host_address} # lab-a\nSET vlan 20\nSENDLINE vlan {vlan} {1} {repeat}");
    // none of them was asked about: no "define {host_address}?" question
    expect(asked[0]).not.toMatch(/\{(host_address|vlan|1|repeat)\}/);
  });
});

describe("a session's or profile's login script", () => {
  it("is SPSL starting #!/bin/spsl, Rhai starting #!/bin/rhai or named .rhai, and not run otherwise", () => {
    expect(openingLanguage("#!/bin/spsl\nWAITPROMPT", "login.spsl")).toBe("spsl");
    expect(openingLanguage("#!/bin/rhai\nwait_prompt();", "login.txt")).toBe("rhai");
    expect(openingLanguage("wait_prompt();", "scripts/Login.RHAI")).toBe("rhai");
    expect(openingLanguage("wait_prompt();", "https://example.test/login.rhai?v=2")).toBe("rhai");
    // SPSL still needs its marker, as SuperXPuTTY's: a file meant for something else is never run
    expect(openingLanguage("WAITPROMPT", "login.spsl")).toBeNull();
    expect(openingLanguage("echo hi", "notes.txt")).toBeNull();
    expect(openingLanguage("", "")).toBeNull();
  });

  it("is warned of when its file isn't on this computer or its address gives nothing, and not otherwise", () => {
    const here = isMac ? "this Mac" : "this PC";
    // a shared session's script someone else has: saved under the same name here, where the session looks for it
    expect(openingScriptWarning({ kind: "missing", path: "scripts/jump-host.spsl", looked: "/settings/scripts/jump-host.spsl" }, "", "Switch's"))
      .toBe(`Switch's login script scripts/jump-host.spsl isn't on ${here}, so the session opens without it. To run it here, save your own copy as /settings/scripts/jump-host.spsl.`);
    // a full path of this system's: a copy at that path does
    const ours = isMac ? "/Users/me/Downloads/jump-host.spsl" : "C:/Users/me/Downloads/jump-host.spsl";
    expect(openingScriptWarning({ kind: "missing", path: ours, looked: null }, "", "Switch's"))
      .toBe(`Switch's login script ${ours} isn't on ${here}, so the session opens without it. To run it here, save your own copy at that path.`);
    // the other system's full path can't be had here: the scripts folder instead
    const theirs = isMac ? "C:/Users/me/Downloads/jump-host.spsl" : "/Users/me/Downloads/jump-host.spsl";
    expect(openingScriptWarning({ kind: "missing", path: theirs, looked: null }, "", "Switch's"))
      .toBe(`Switch's login script ${theirs} isn't on ${here}, so the session opens without it. That's a ${isMac ? "Windows" : "Mac"} path: to run it here, put a copy in the settings folder's scripts folder and choose it from there.`);
    expect(openingScriptWarning({ kind: "url", url: "https://example.test/login.spsl" }, "", "The Linux profile's"))
      .toBe("The Linux profile's login script https://example.test/login.spsl couldn't be fetched, so the session opens without it.");
    expect(openingScriptWarning({ kind: "url", url: "https://example.test/login.spsl" }, "#!/bin/spsl", "Switch's")).toBe("");
    expect(openingScriptWarning({ kind: "text", path: "scripts/login.spsl" }, "#!/bin/spsl", "Switch's")).toBe("");
    expect(openingScriptWarning({ kind: "none" }, "", "Switch's")).toBe("");
  });
});
