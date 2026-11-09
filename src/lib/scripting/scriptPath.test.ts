// What scriptPath.ts counts as a full path, which only the system it was picked on finds, for the note under a login
// script field. Plain functions: no Tauri needed (chosenScript is tested through Settings > Profiles, and the
// relative path itself in scripts.rs).
import { describe, expect, it } from "vitest";
import { fullPathNote, isFullPath, isOtherSystemsPath, systemOfPath } from "./scriptPath";

describe("a login script's path", () => {
  it("counts Windows and Unix full paths as one system's, and relative paths and addresses as both's", () => {
    for (const full of ["C:\\Scripts\\login.spsl", "c:/scripts/login.rhai", "\\\\server\\share\\login.spsl", "/Users/me/login.spsl", "~/login.spsl", "file:///Users/me/login.spsl"]) {
      expect(isFullPath(full), full).toBe(true);
    }
    for (const shared of ["", "scripts/login.spsl", "scripts\\login.spsl", "login.rhai", "https://example.com/login.spsl"]) {
      expect(isFullPath(shared), shared).toBe(false);
      expect(fullPathNote(shared)).toBe("");
    }
  });
});

describe("another system's path", () => {
  it("is a Windows one on the Mac and Linux, and a Unix one on Windows: Linux's own /home path isn't another's", () => {
    for (const system of ["mac", "linux"] as const) {
      expect(isOtherSystemsPath("C:\\Scripts\\login.spsl", system), system).toBe(true);
      expect(isOtherSystemsPath("\\\\server\\scripts\\login.spsl", system), system).toBe(true);
      expect(isOtherSystemsPath("/home/me/scripts/login.spsl", system), system).toBe(false);
      expect(isOtherSystemsPath("~/scripts/login.spsl", system), system).toBe(false);
    }
    expect(isOtherSystemsPath("/home/me/scripts/login.spsl", "windows")).toBe(true);
    expect(isOtherSystemsPath("C:\\Scripts\\login.spsl", "windows")).toBe(false);
  });
});

describe("whose path it is", () => {
  it("is named when the path says: Windows' drive or share, the Mac's /Users, Linux's /home; else Mac or Linux", () => {
    expect(systemOfPath("C:\\Scripts\\login.spsl")).toBe("Windows");
    expect(systemOfPath("\\\\server\\login.spsl")).toBe("Windows");
    expect(systemOfPath("/Users/me/login.spsl")).toBe("Mac");
    expect(systemOfPath("/home/me/login.spsl")).toBe("Linux");
    expect(systemOfPath("~/login.spsl")).toBe("Mac or Linux");
  });
});
