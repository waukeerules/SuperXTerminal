/**
 * Tests for the Fetch command examples on the Mac, Windows and Linux: a pure function, so nothing is stood in. Linux's
 * fetch runs in the user's shell as the Mac's does, so it gets the Unix commands, not PowerShell's.
 */
import { describe, expect, it } from "vitest";
import { keyFetchExamples } from "./keyFetchExamples";
import type { System } from "../platform";

const SYSTEMS: System[] = ["mac", "windows", "linux"];

describe("Fetch command examples", () => {
  for (const system of SYSTEMS) {
    it(`each writes to {key_file} and has a lifetime and key file (${system})`, () => {
      for (const example of keyFetchExamples(system)) {
        expect(example.command, example.name).toContain("{key_file}");
        expect(example.lifetime, example.name).toBeGreaterThan(0);
        // in ~/.ssh, as the system writes it: Windows' own form there (launch.rs's expand_home reads it)
        expect(example.keyFile, example.name).toMatch(system === "windows" ? /^%USERPROFILE%\\\.ssh\\[a-z_]+$/ : /^~\/\.ssh\/[a-z_]+$/);
      }
    });
  }

  it("fetches CyberArk's key as CyberArk documents it: username@login_suffix#subdomain, then key@ the gateway", () => {
    for (const system of SYSTEMS) {
      const cyberArk = keyFetchExamples(system).find((e) => e.name.startsWith("CyberArk"))!;
      expect(cyberArk.name).toBe("CyberArk: MFA caching key (sftp)");
      expect(cyberArk.command).toBe(`sftp "{cyberark_username}@{cyberark_login_suffix}#{cyberark_subdomain}@key@{cyberark_gateway}:/key" "{key_file}"`);
    }
  });

  it("names no one's own setup: every placeholder in a command is one its note says to set", () => {
    for (const system of SYSTEMS) {
      for (const example of keyFetchExamples(system)) {
        const used = [...example.command.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).filter((k) => k !== "key_file");
        for (const key of used) expect(example.note, example.name).toContain(`{${key}}`);
      }
    }
  });

  it("gives Windows PowerShell's commands, and the Mac and Linux the shell's, where they differ", () => {
    const test = (system: System) => keyFetchExamples(system).find((e) => e.name.startsWith("Test"))!.command;
    expect(test("windows")).toContain("Remove-Item");
    expect(test("mac")).toContain("rm -f");
    expect(test("linux")).toContain("rm -f");
    expect(test("linux")).not.toContain("Remove-Item");
  });

  it("puts a function of your own where each system's fetch reads it: ~/.zshrc, the PowerShell profile, ~/.bashrc", () => {
    const own = (system: System) => keyFetchExamples(system).find((e) => e.name.endsWith("function of your own"))!;
    expect(own("mac")).toMatchObject({ name: "A shell function of your own", note: expect.stringContaining("~/.zshrc") });
    expect(own("windows")).toMatchObject({ name: "A PowerShell function of your own", note: expect.stringContaining("PowerShell profile") });
    expect(own("linux")).toMatchObject({ name: "A shell function of your own", note: expect.stringContaining("~/.bashrc") });
    expect(own("linux").note).not.toMatch(/PowerShell|zshrc/);
  });
});

describe("the 1Password example", () => {
  it("writes an SSH Key item's private key in OpenSSH's format, and points to the agent as the better way", () => {
    for (const system of SYSTEMS) {
      const example = keyFetchExamples(system).find((e) => e.name === "1Password (an SSH Key item)")!;
      expect(example.command).toBe('op read "op://Private/SSH key/private key?ssh-format=openssh" --out-file "{key_file}" --force');
      expect(example.note).toMatch(/SSH agent is usually better/);
    }
  });
});
