// What tunnels.ts says a session's tunnels make reachable: the words, links and test commands for the lines at the
// top of the tab and its Tunnels panel, and which of a profile's tunnels ssh is given. Plain functions; this test's
// jsdom isn't a Mac, so the words are Windows' (this PC, curl.exe).
import { describe, expect, it } from "vitest";
import { completeTunnels, describeTunnel, profileTunnels, holdTunnels, listenAddress, releaseTunnels, sameTunnels, sampleFor, TUNNEL_SAMPLES, tunnelNotices } from "./tunnels";
import { isMac } from "../shortcuts";
import type { Forward } from "../settings/profiles";

const tunnel = (kind: Forward["kind"], listen: string, to = ""): Forward => ({ kind, listen, to });

describe("a tunnel, explained", () => {
  it("is Windows' here", () => {
    expect(isMac).toBe(false);
  });

  it("gives a local tunnel to a web port a link to click, https for 443, and a curl to test it", () => {
    expect(describeTunnel(tunnel("local", "8080", "localhost:80"))).toEqual({
      kind: "Local",
      text: "http://localhost:8080 on this PC opens localhost:80, as the server sees it.",
      link: "http://localhost:8080",
      test: { command: "curl.exe -I http://localhost:8080", where: "here" },
    });
    const https = describeTunnel(tunnel("local", "8443", "10.0.0.5:443"));
    expect(https.link).toBe("https://localhost:8443");
    // a device's own certificate isn't for localhost: -k, so the test shows the tunnel rather than a certificate error
    expect(https.test.command).toBe("curl.exe -Ik https://localhost:8443");
  });

  it("gives a local tunnel to anything else no link, and a port check to test it", () => {
    const db = describeTunnel(tunnel("local", "5432", "localhost:5432"));
    expect(db.link).toBeNull();
    expect(db.text).toBe("localhost:5432 on this PC opens localhost:5432, as the server sees it.");
    expect(db.test.command).toBe("Test-NetConnection localhost -Port 5432");
  });

  it("says a SOCKS proxy goes out through the server, tested with curl through it", () => {
    const socks = describeTunnel(tunnel("dynamic", "1080"));
    expect(socks.kind).toBe("SOCKS proxy");
    expect(socks.text).toMatch(/^A SOCKS proxy at localhost:1080 on this PC: set a browser's SOCKS proxy to it/);
    expect(socks.test).toEqual({ command: "curl.exe --socks5-hostname localhost:1080 -I http://localhost", where: "here" });
  });

  it("says a remote tunnel reaches this computer from the server, tested in the tab", () => {
    expect(describeTunnel(tunnel("remote", "9090", "localhost:8000"))).toEqual({
      kind: "Remote",
      text: "The server's localhost:9090 reaches localhost:8000 on this PC.",
      link: null,
      test: { command: "curl -I http://localhost:9090", where: "server" },
    });
  });

  it("reads where a tunnel listens: a port alone is localhost, every address is reached as localhost here", () => {
    expect(listenAddress("8080")).toEqual({ host: "localhost", port: "8080" });
    expect(listenAddress("127.0.0.1:8080")).toEqual({ host: "127.0.0.1", port: "8080" });
    expect(listenAddress("[::1]:8080")).toEqual({ host: "[::1]", port: "8080" });
    expect(listenAddress("0.0.0.0:8080")).toEqual({ host: "localhost", port: "8080" });
  });
});

describe("a tab's tunnels", () => {
  it("are listed at its top, one line each, then where to see them again; none says nothing", () => {
    const lines = tunnelNotices([tunnel("local", "8080", "localhost:80"), tunnel("dynamic", "1080")]);
    expect(lines.map((l) => l.kind)).toEqual(["info", "info", "info"]);
    expect(lines[0].text).toBe("Tunnel (Local): http://localhost:8080 on this PC opens localhost:80, as the server sees it.");
    expect(lines[1].text).toMatch(/^Tunnel \(SOCKS proxy\): A SOCKS proxy at localhost:1080/);
    expect(lines[2].text).toMatch(/tunnel icon on the tab, or Tunnels… in its menu/);
    expect(tunnelNotices([])).toEqual([]);
  });

  it("warn of one whose port another tab already holds, naming that tab", () => {
    const lines = tunnelNotices([tunnel("local", "8080", "localhost:80")], new Map([["8080", "Web"]]));
    expect(lines[0]).toEqual({
      text: `Tunnel (Local): Port 8080 on this PC is already the tunnel of the tab "Web", so this tab's isn't open: use that tab's, or close it and restart this session.`,
      kind: "warn",
    });
  });

  it("hold their ports on this computer for the first tab only, until its session ends", () => {
    const web = [tunnel("local", "8080", "localhost:80"), tunnel("dynamic", "1080"), tunnel("remote", "9090", "localhost:8000")];
    expect(holdTunnels("t1", "Web", web)).toEqual(new Map());
    // the second tab of the same profile: its Local and SOCKS ports are the first's; a Remote one's port is on the server
    expect(holdTunnels("t2", "Web (2)", web)).toEqual(new Map([["8080", "Web"], ["1080", "Web"]]));
    // the first reconnecting holds them still
    expect(holdTunnels("t1", "Web", web)).toEqual(new Map());
    releaseTunnels("t1");
    expect(holdTunnels("t3", "Web (3)", web)).toEqual(new Map());
    releaseTunnels("t3");
    releaseTunnels("t2");
  });

  it("are the profile's complete ones, as ssh is given them, compared in order", () => {
    const profile = [tunnel("local", " 8080 ", "localhost:80"), tunnel("local", "8081", ""), tunnel("dynamic", "1080"), tunnel("remote", "90 90", "x:1")];
    const given = completeTunnels(profile);
    expect(given).toEqual([tunnel("local", "8080", "localhost:80"), tunnel("dynamic", "1080")]);
    expect(sameTunnels(given, [tunnel("local", "8080", "localhost:80"), tunnel("dynamic", "1080")])).toBe(true);
    expect(sameTunnels(given, [tunnel("local", "8080", "localhost:443"), tunnel("dynamic", "1080")])).toBe(false);
    expect(sameTunnels(given, given.slice(1))).toBe(false);
  });
});

describe("Tunnel Samples", () => {
  it("each start from a tunnel ssh would be given, with what it's for", () => {
    for (const sample of TUNNEL_SAMPLES) {
      expect(sample.label).not.toBe("");
      expect(completeTunnels([sample.tunnel])).toEqual([sample.tunnel]);
    }
  });

  it("move a sample's port up past the profile's own on the same side, not past a Remote one's on the server", () => {
    const web = TUNNEL_SAMPLES.find((s) => s.label === "The server's own web page (HTTP)")!;
    expect(sampleFor(web, [])).toEqual(tunnel("local", "8080", "localhost:80"));
    expect(sampleFor(web, [tunnel("local", "8080", "localhost:80"), tunnel("dynamic", "8081")])).toEqual(tunnel("local", "8082", "localhost:80"));
    expect(sampleFor(web, [tunnel("remote", "8080", "localhost:80")]).listen).toBe("8080");
  });
});

describe("a profile's tunnels", () => {
  it("are none while turned off, its list kept", () => {
    const forwards = [tunnel("local", "8080", "localhost:80"), tunnel("local", "8081", "")];
    expect(profileTunnels({ forwards, tunnelsOff: null })).toEqual([tunnel("local", "8080", "localhost:80")]);
    expect(profileTunnels({ forwards, tunnelsOff: true })).toEqual([]);
    expect(profileTunnels(undefined)).toEqual([]);
  });
});
