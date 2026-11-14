// A saved session made from a Quick connect tab: the toolbar's fields as Edit Session's. A plain function, so
// nothing is stood in for.
import { describe, expect, it } from "vitest";
import { sessionFromQuick } from "./fromQuickConnect";

const QUICK = { protocol: "SSH", host: "10.0.0.1", username: "netops", password: "secret", profile: "Lab" };

describe("a session saved from a Quick connect tab", () => {
  it("has the host, login and profile typed, on SSH's usual port, and is named as the tab", () => {
    expect(sessionFromQuick(QUICK, "10.0.0.1")).toMatchObject({ name: "10.0.0.1", folder: "", protocol: "SSH", host: "10.0.0.1", port: 22, username: "netops", profile: "Lab" });
  });

  it("doesn't keep the password typed", () => {
    expect(JSON.stringify(sessionFromQuick(QUICK, "10.0.0.1"))).not.toContain("secret");
  });

  it("takes a user and port typed with the host, and is named for the host alone", () => {
    const session = sessionFromQuick({ ...QUICK, host: "admin@sw1.lab.example:2222", username: "" }, "admin@sw1.lab.example:2222");
    expect(session).toMatchObject({ name: "sw1.lab.example", host: "sw1.lab.example", port: 2222, username: "admin" });
  });

  it("uses the Login box before a user typed with the host, as the tab did", () => {
    expect(sessionFromQuick({ ...QUICK, host: "admin@sw1" }, "admin@sw1").username).toBe("netops");
  });

  it("takes the connection type typed with the host, spelt as Edit Session lists it, and its usual port", () => {
    expect(sessionFromQuick({ ...QUICK, host: "telnet://10.0.0.2/" }, "10.0.0.2")).toMatchObject({ name: "10.0.0.2", protocol: "Telnet", host: "10.0.0.2", port: 23 });
  });

  it("keeps the name of a tab that was renamed", () => {
    expect(sessionFromQuick(QUICK, "Core switch").name).toBe("Core switch");
  });

  it("uses Default Settings when no profile was chosen", () => {
    expect(sessionFromQuick({ ...QUICK, profile: "" }, "10.0.0.1").profile).toBe("Default Settings");
  });
});
