// What a script starts with in a tab: the tab's session's values (sessionValues, from the Sessions tree through
// context.sessionOf), and Environment Variables filled in without touching the script's own {variables} or the
// session's (fillScriptPlaceholders). The Rust side is stood in for by mockIPC: variables_fill knows {site} only.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";
import { context, fillScriptPlaceholders, sessionValues } from "./sending.svelte";
import type { SessionView } from "../sessions/api";

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

  it("fills Environment Variables in, but leaves the session's and the script's own {variables} for it", async () => {
    const script = "SENDLINE ssh {host_address} # {site}\nSET vlan 20\nSENDLINE vlan {vlan} {1} {repeat}";
    expect(await fillScriptPlaceholders(script)).toBe("SENDLINE ssh {host_address} # lab-a\nSET vlan 20\nSENDLINE vlan {vlan} {1} {repeat}");
    // none of them was asked about: no "define {host_address}?" question
    expect(asked[0]).not.toMatch(/\{(host_address|vlan|1|repeat)\}/);
  });
});
