/**
 * Tests for comparing release versions in the update check: a pure function, so nothing is stood in.
 */
import { describe, expect, it } from "vitest";
import { isNewer } from "./updates";

describe("isNewer", () => {
  it("compares versions part by part, as numbers", () => {
    expect(isNewer("v0.1.1", "0.1.0")).toBe(true);
    expect(isNewer("0.1.10", "0.1.9")).toBe(true);
    expect(isNewer("1.0", "0.9.9")).toBe(true);
  });

  it("isn't newer when the same or older, whatever the v or missing parts", () => {
    expect(isNewer("v0.1.1", "0.1.1")).toBe(false);
    expect(isNewer("0.1", "0.1.0")).toBe(false);
    expect(isNewer("0.1.0", "0.1.1")).toBe(false);
  });

  it("isn't newer when a tag isn't a version", () => {
    expect(isNewer("latest", "0.1.0")).toBe(false);
  });
});
