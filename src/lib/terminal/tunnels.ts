/**
 * What a session's tunnels (Settings > Profiles > Tunnels, ssh's -L, -R and -D) make reachable, in plain words, for
 * the lines at the top of its tab, its tunnel icon's panel (TunnelsPanel.svelte) and the tab's Tunnels… menu item.
 * Each says where to go on this computer or the server, gives a link to open when it carries a web page (the
 * terminal's links open in the browser), and a command to check it works. The tunnels themselves come from the core
 * with the tab's look (launch.rs: the ones ssh was given, after any left out as incomplete).
 */
import type { Forward } from "../settings/profiles";
import { THIS_COMPUTER } from "../platform";
import { isMac } from "../shortcuts";

/** One tunnel, explained */
export interface TunnelInfo {
  /** "Local", "Remote" or "SOCKS proxy" */
  kind: string;
  /** What it does, one sentence */
  text: string;
  /** A link to open on this computer: a web page through a local tunnel; null where there's none to click */
  link: string | null;
  /** A command that shows it working, and where it's typed */
  test: { command: string; where: "here" | "server" };
}

/** Target ports taken for HTTPS, and for plain HTTP, when making a local tunnel's link */
const HTTPS_PORTS = ["443", "8443", "9443"];
const HTTP_PORTS = ["80", "8000", "8008", "8080", "8888", "3000", "5000", "9000", "9090"];

/**
 * A listen address split into host and port: "8080" (localhost, as ssh's default), "127.0.0.1:8080", "[::1]:8080"
 * @param listen As the tunnel names it
 */
export function listenAddress(listen: string): { host: string; port: string } {
  const bracketed = /^\[([^\]]+)\]:(\d+)$/.exec(listen);
  if (bracketed) return { host: `[${bracketed[1]}]`, port: bracketed[2] };
  const at = listen.lastIndexOf(":");
  if (at < 0) return { host: "localhost", port: listen };
  const host = listen.slice(0, at);
  // ssh's "*" or "0.0.0.0" listens on every address: this computer still reaches it as localhost
  return { host: host === "*" || host === "0.0.0.0" || host === "" ? "localhost" : host, port: listen.slice(at + 1) };
}

/** The port a tunnel goes to: "localhost:80" gives "80" */
function targetPort(to: string): string {
  return to.slice(to.lastIndexOf(":") + 1);
}

/** curl as typed on this computer: in PowerShell plain curl is another command, so curl.exe on Windows */
const CURL = isMac ? "curl" : "curl.exe";

/**
 * A tunnel in plain words, with its link and a test
 * @param tunnel As the profile gives it
 */
export function describeTunnel(tunnel: Forward): TunnelInfo {
  const { host, port } = listenAddress(tunnel.listen.trim());
  const here = `${host}:${port}`;
  const to = tunnel.to.trim();
  if (tunnel.kind === "dynamic") {
    return {
      kind: "SOCKS proxy",
      text: `A SOCKS proxy at ${here} on ${THIS_COMPUTER}: set a browser's SOCKS proxy to it, and what it opens goes out through the server, as if from there.`,
      link: null,
      test: { command: `${CURL} --socks5-hostname ${here} -I http://localhost`, where: "here" },
    };
  }
  if (tunnel.kind === "remote") {
    const there = /:/.test(tunnel.listen) ? tunnel.listen.trim() : `localhost:${port}`;
    return {
      kind: "Remote",
      text: `The server's ${there} reaches ${to} on ${THIS_COMPUTER}.`,
      link: null,
      test: { command: `curl -I http://${there}`, where: "server" },
    };
  }
  const goes = targetPort(to);
  const scheme = HTTPS_PORTS.includes(goes) ? "https" : HTTP_PORTS.includes(goes) ? "http" : null;
  const link = scheme ? `${scheme}://${here}` : null;
  return {
    kind: "Local",
    text: `${link ?? here} on ${THIS_COMPUTER} opens ${to}, as the server sees it.`,
    link,
    test: link
      ? { command: `${CURL} -I${scheme === "https" ? "k" : ""} ${link}`, where: "here" }
      : { command: isMac ? `nc -vz ${host} ${port}` : `Test-NetConnection ${host} -Port ${port}`, where: "here" },
  };
}

/**
 * The lines at the top of a tab whose session opened tunnels: one for each, its link clickable in the terminal, and
 * where to see them all again
 * @param tunnels The tunnels ssh was given
 */
export function tunnelNotices(tunnels: readonly Forward[]): string[] {
  if (tunnels.length === 0) return [];
  return [
    ...tunnels.map((t) => {
      const info = describeTunnel(t);
      return `Tunnel (${info.kind}): ${info.text}`;
    }),
    "The tunnel icon on the tab, or Tunnels… in its menu, shows them with a way to test each.",
  ];
}

/**
 * The tunnels ssh is given from a profile's list: as launch.rs's `forwarding`, one missing its port or where it goes,
 * or with a space in either, is left out
 * @param forwards The profile's tunnels
 */
export function completeTunnels(forwards: readonly Forward[]): Forward[] {
  const word = (s: string) => s !== "" && !/\s/.test(s);
  return forwards
    .map((f) => ({ kind: f.kind, listen: f.listen.trim(), to: f.to.trim() }))
    .filter((f) => (f.kind === "dynamic" ? word(f.listen) : (f.kind === "local" || f.kind === "remote") && word(f.listen) && word(f.to)));
}

/**
 * Whether two lists of tunnels are the same, in order: a tab whose profile's tunnels changed after it opened says
 * to restart it, as ssh can't add or drop a tunnel on a connection it has made
 * @param a One list
 * @param b The other
 */
export function sameTunnels(a: readonly Forward[], b: readonly Forward[]): boolean {
  const key = (t: Forward) => `${t.kind} ${t.listen.trim()} ${t.to.trim()}`;
  return a.length === b.length && a.every((t, i) => key(t) === key(b[i]));
}
