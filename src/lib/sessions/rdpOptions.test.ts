import { describe, expect, it } from "vitest";
import { formatRdpOptions, parseRdpOptions } from "./rdpOptions";

describe("RDP options text", () => {
  it("skips unknown keys and bad values", () => {
    expect(parseRdpOptions("Nonsense=1;;=x")).toEqual({});
  });

  it("writes what it reads", () => {
    const text = formatRdpOptions(parseRdpOptions(formatRdpOptions({ display: "Fixed" })));
    expect(parseRdpOptions(text)).toEqual({ display: "Fixed" });
  });

  it("writes nothing for no options", () => {
    expect(formatRdpOptions({})).toBe("");
  });
});
