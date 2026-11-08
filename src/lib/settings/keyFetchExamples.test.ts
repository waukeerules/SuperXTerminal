/**
 * Tests for the Fetch command examples on both systems: a pure function, so nothing is stood in.
 */
import { describe, expect, it } from "vitest";
import { keyFetchExamples } from "./keyFetchExamples";

describe("Fetch command examples", () => {
  for (const mac of [true, false]) {
    it(`each writes to {key_file} and has a lifetime and key file (${mac ? "macOS" : "Windows"})`, () => {
      for (const example of keyFetchExamples(mac)) {
        expect(example.command, example.name).toContain("{key_file}");
        expect(example.lifetime, example.name).toBeGreaterThan(0);
        expect(example.keyFile, example.name).toMatch(/^~\/\.ssh\//);
      }
    });
  }

  it("fetches CyberArk's key as its key target names it: the AD username, the region, then key@ the gateway", () => {
    for (const mac of [true, false]) {
      const cyberArk = keyFetchExamples(mac).find((e) => e.name.startsWith("CyberArk"))!;
      expect(cyberArk.command).toBe(`sftp "{ad_username}@{cyberark_region}@key@{cyberark_gateway}:/key" "{key_file}"`);
      expect(cyberArk.note).not.toContain("{domain}");
    }
  });

  it("gives Windows PowerShell's commands where they differ", () => {
    const test = keyFetchExamples(false).find((e) => e.name.startsWith("Test"))!;
    expect(test.command).toContain("Remove-Item");
    expect(keyFetchExamples(true).find((e) => e.name.startsWith("Test"))!.command).toContain("rm -f");
  });
});

describe("the 1Password example", () => {
  it("writes an SSH Key item's private key in OpenSSH's format, and points to the agent as the better way", () => {
    for (const mac of [true, false]) {
      const example = keyFetchExamples(mac).find((e) => e.name === "1Password (an SSH Key item)")!;
      expect(example.command).toBe('op read "op://Private/SSH key/private key?ssh-format=openssh" --out-file "{key_file}" --force');
      expect(example.note).toMatch(/SSH agent is usually better/);
    }
  });
});
