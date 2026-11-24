/**
 * Tests for clipboardOwner.ts: the tab whose copy is on the clipboard is told when something else takes it. The
 * clipboard's change number is a counter the test moves on, standing in for another app's copy, and the half-second
 * look is driven by Vitest's fake timers.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { ClipboardOwner, type Holder } from "./clipboardOwner";

/** A tab that records the texts it was told were taken */
function tab(): Holder & { told: string[] } {
  const told: string[] = [];
  return { told, taken: (text) => told.push(text) };
}

describe("ClipboardOwner", () => {
  let number = 1;
  let owner: ClipboardOwner;
  beforeEach(() => {
    vi.useFakeTimers();
    number = 1;
    owner = new ClipboardOwner(async () => number, 500);
  });
  afterEach(() => vi.useRealTimers());

  it("tells the tab when another app copies something after its highlight", async () => {
    const one = tab();
    await owner.copied(one, "uplink");
    await vi.advanceTimersByTimeAsync(1500);
    expect(one.told).toEqual([]);
    number++; // copied in another app
    await vi.advanceTimersByTimeAsync(500);
    expect(one.told).toEqual(["uplink"]);
    // told once: the clipboard is no longer its
    number++;
    await vi.advanceTimersByTimeAsync(1500);
    expect(one.told).toEqual(["uplink"]);
  });

  it("tells the tab at once when another tab copies, and watches for the new one", async () => {
    const [one, two] = [tab(), tab()];
    await owner.copied(one, "uplink");
    number++;
    await owner.copied(two, "Gi1/0/24");
    expect(one.told).toEqual(["uplink"]);
    expect(two.told).toEqual([]);
    number++;
    await vi.advanceTimersByTimeAsync(500);
    expect(two.told).toEqual(["Gi1/0/24"]);
  });

  it("doesn't take a tab copying again (Copy on Select as the highlight grows) for something else", async () => {
    const one = tab();
    await owner.copied(one, "up");
    number++;
    await owner.copied(one, "uplink");
    await vi.advanceTimersByTimeAsync(1500);
    expect(one.told).toEqual([]);
  });

  it("stops watching for a tab that closes", async () => {
    const one = tab();
    await owner.copied(one, "uplink");
    owner.release(one);
    number++;
    await vi.advanceTimersByTimeAsync(1500);
    expect(one.told).toEqual([]);
  });

  it("takes a clipboard it can't read for unchanged", async () => {
    let readable = true;
    owner = new ClipboardOwner(async () => (readable ? number : null), 500);
    const one = tab();
    await owner.copied(one, "uplink");
    readable = false;
    await vi.advanceTimersByTimeAsync(1500);
    expect(one.told).toEqual([]);
  });
});
