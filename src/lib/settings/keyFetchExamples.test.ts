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

  it("gives Windows PowerShell's commands where they differ", () => {
    const test = keyFetchExamples(false).find((e) => e.name.startsWith("Test"))!;
    expect(test.command).toContain("Remove-Item");
    expect(keyFetchExamples(true).find((e) => e.name.startsWith("Test"))!.command).toContain("rm -f");
  });
});
