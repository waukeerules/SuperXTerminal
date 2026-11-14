// Quick connect's Host field as a list of hosts: how it's split, and how a pasted column of hosts goes in. Plain
// functions, so nothing is stood in for.
import { describe, expect, it } from "vitest";
import { pastedHosts, splitHosts } from "./quickHosts";

describe("the hosts typed in Quick connect", () => {
  it("are split at commas, semicolons, spaces and new lines", () => {
    expect(splitHosts("10.0.0.1, 10.0.0.2;sw1.lab.example\n10.0.0.3  10.0.0.4")).toEqual(["10.0.0.1", "10.0.0.2", "sw1.lab.example", "10.0.0.3", "10.0.0.4"]);
  });

  it("keep each one's own user, port and connection type", () => {
    expect(splitHosts("admin@10.0.0.1:2222, telnet://10.0.0.2, [fe80::1]:22")).toEqual(["admin@10.0.0.1:2222", "telnet://10.0.0.2", "[fe80::1]:22"]);
  });

  it("open one tab for a host typed twice, whatever its case", () => {
    expect(splitHosts("sw1, SW1, sw2, sw1")).toEqual(["sw1", "sw2"]);
  });

  it("are none for an empty field, or one of separators alone", () => {
    expect(splitHosts("")).toEqual([]);
    expect(splitHosts(" , ;\n")).toEqual([]);
  });

  it("are the one host, as before, when one is typed", () => {
    expect(splitHosts("  192.168.0.5 ")).toEqual(["192.168.0.5"]);
  });
});

describe("hosts pasted into Quick connect", () => {
  it("go in with commas between when they were on lines of their own, not run together", () => {
    expect(pastedHosts("10.0.0.1\r\n10.0.0.2\n\n10.0.0.3\n")).toBe("10.0.0.1, 10.0.0.2, 10.0.0.3");
  });

  it("are left to the box when they're on one line", () => {
    expect(pastedHosts("10.0.0.1, 10.0.0.2")).toBeNull();
  });
});
