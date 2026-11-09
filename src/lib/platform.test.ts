/**
 * Tests for the words that differ by system (platform.ts): the system told from each web view's user agent, and
 * Linux's own words, which were Windows' before Linux was told apart. Pure functions, so nothing is stood in.
 */
import { describe, expect, it } from "vitest";
import { systemOf, wordsFor } from "./platform";

describe("the system", () => {
  it("is told from each web view's user agent: WKWebView's Mac, WebView2's Windows, WebKitGTK's Linux", () => {
    expect(systemOf("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)")).toBe("mac");
    expect(systemOf("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0")).toBe("windows");
    expect(systemOf("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15")).toBe("linux");
  });
});

describe("Linux's words", () => {
  it("are its own: its name, the Secret Service, ~/.ssh, its fonts, and the shell a fetch runs in", () => {
    const linux = wordsFor("linux");
    expect(linux.system).toBe("Linux");
    expect(linux.secretStore).toMatch(/Secret Service/);
    expect(linux.sshKeys).toBe("~/.ssh keys");
    expect(linux.sshConfig).toBe("~/.ssh/config");
    expect(linux.monospaceFonts).toContain("DejaVu Sans Mono");
    expect(linux.fetchShell).toMatch(/~\/\.bashrc/);
    // none of Windows' or the Mac's (but the serial note, which says a COM name shares a session with Windows)
    const { serialNote, ...rest } = linux;
    expect(serialNote).toMatch(/share the session with Windows/);
    expect(JSON.stringify(rest)).not.toMatch(/Windows|PowerShell|C:\\\\|Keychain|zshrc|Finder/);
  });

  it("name Linux's serial devices and how to get 1Password's CLI there, which has no one command", () => {
    const linux = wordsFor("linux");
    expect(linux.serialExample).toBe("e.g. /dev/ttyUSB0, or COM1");
    expect(linux.serialNote).toMatch(/\/dev\/ttyUSB… and \/dev\/ttyACM…, and need your user in the dialout group/);
    expect(linux.onePasswordInstall).toEqual({ words: expect.stringMatching(/1password-cli package, from its apt or dnf repository/) });
    expect(wordsFor("mac").onePasswordInstall).toEqual({ command: "brew install 1password-cli" });
    expect(wordsFor("windows").onePasswordInstall).toEqual({ command: "winget install 1password-cli" });
  });

  it("leave the Mac's and Windows' as they were", () => {
    expect(wordsFor("mac")).toMatchObject({ system: "macOS", secretStore: "the Keychain", fetchShell: expect.stringContaining("~/.zshrc") });
    expect(wordsFor("windows")).toMatchObject({ system: "Windows", secretStore: "Windows Credential Manager", fetchShell: "PowerShell" });
  });
});
