// Which systems' tabs get Windows' pseudo console's workarounds (PageHost.ts's conptyWaits): Windows alone. Linux's
// tabs have a Unix pseudo terminal, as the Mac's do, and were given them when only the Mac was told from Windows.
import { describe, expect, it } from "vitest";
import { conptyWaits } from "./PageHost";

describe("ConPTY's workarounds", () => {
  it("are Windows' alone: the Mac and Linux have a Unix pseudo terminal", () => {
    expect(conptyWaits("windows")).toBe(true);
    expect(conptyWaits("mac")).toBe(false);
    expect(conptyWaits("linux")).toBe(false);
  });
});
