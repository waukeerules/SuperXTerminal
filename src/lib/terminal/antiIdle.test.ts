// The anti-idle string's escapes, as KiTTY's SendKeyboardPlus turns them into keys: the strings KiTTY's own examples
// and its users write, and the ones left out.
import { describe, expect, it } from "vitest";
import { antiIdleKeys } from "./antiIdle";

describe("antiIdleKeys", () => {
  it("types a character by its hex code, and takes a trailing backslash as the end", () => {
    // Ctrl+E: the end of the line on a Cisco, which shows nothing
    expect(antiIdleKeys("\\x05\\")).toBe("\x05");
    expect(antiIdleKeys("\\x05")).toBe("\x05");
  });

  it("types keys by their virtual-key code: KiTTY's space and backspace", () => {
    expect(antiIdleKeys(" \\k08\\")).toBe(" \b");
    expect(antiIdleKeys("\\k1B")).toBe("\x1b");
  });

  it("knows \\r, \\n, \\t, \\h and \\\\", () => {
    expect(antiIdleKeys("a\\rb\\nc\\td\\he\\\\f")).toBe("a\rb\nc\td\be\\f");
  });

  it("leaves out KiTTY's pauses and Ctrl+Break, and types any other backslash as it is", () => {
    expect(antiIdleKeys("a\\pb\\s03c\\cd")).toBe("abcd");
    expect(antiIdleKeys("\\q \\xZZ")).toBe("\\q \\xZZ");
  });

  it("types plain text as it is", () => {
    expect(antiIdleKeys("")).toBe("");
    expect(antiIdleKeys(" ")).toBe(" ");
  });
});
