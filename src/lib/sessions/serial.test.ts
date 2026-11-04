/**
 * Tests for a serial session's line settings as PuTTY's -sercfg reads and writes them: pure functions, so nothing
 * is stood in.
 */
import { describe, expect, it } from "vitest";
import { DEFAULT_LINE, applySercfg, lineOf, toSercfg, withLine } from "./serial";

describe("serial line settings (PuTTY's -sercfg)", () => {
  it("reads settings in any order, as PuTTY does", () => {
    expect(applySercfg(DEFAULT_LINE, "R,115200,2,e,7")).toEqual({ speed: 115200, dataBits: 7, parity: "e", stopBits: "2", flow: "R" });
  });

  it("uses PuTTY's defaults when there's no -sercfg", () => {
    expect(lineOf("-l admin")).toEqual(DEFAULT_LINE);
    expect(toSercfg(DEFAULT_LINE)).toBe("9600,8,n,1,X");
  });

  it("replaces -sercfg in place and keeps the other arguments", () => {
    const line = { ...DEFAULT_LINE, speed: 115200 };
    expect(withLine("-l admin -sercfg 9600,8,n,1,X -v", line)).toBe("-l admin -sercfg 115200,8,n,1,X -v");
    expect(withLine("-l admin", line)).toBe("-l admin -sercfg 115200,8,n,1,X");
  });
});
