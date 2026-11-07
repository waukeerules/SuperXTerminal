// What scriptPath.ts counts as a full path, which only the system it was picked on finds, for the note under a login
// script field. Plain functions: no Tauri needed (chosenScript is tested through Settings > Profiles, and the
// relative path itself in scripts.rs).
import { describe, expect, it } from "vitest";
import { fullPathNote, isFullPath } from "./scriptPath";

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
