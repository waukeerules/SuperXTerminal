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
 * What to say of a tunnel whose port another tab already holds: ssh still connects, without it, saying only
 * "Address already in use", so the tab says which tab has it and what to do
 * @param tunnel The tunnel this tab's ssh couldn't open
 * @param holder The title of the tab that has its port
 */
export function takenText(tunnel: Forward, holder: string): string {
  const { port } = listenAddress(tunnel.listen.trim());
  return `Port ${port} on ${THIS_COMPUTER} is already the tunnel of the tab "${holder}", so this tab's isn't open: use that tab's, or close it and restart this session.`;
}

/**
 * The lines at the top of a tab whose session opened tunnels: one for each, its link clickable in the terminal, a
 * warning for one whose port another tab already holds, and where to see them all again
 * @param tunnels The tunnels ssh was given
 * @param taken Those whose port another of this window's tabs holds: their listen, and that tab's title
 */
export function tunnelNotices(tunnels: readonly Forward[], taken: ReadonlyMap<string, string> = new Map()): { text: string; kind: "info" | "warn" }[] {
  if (tunnels.length === 0) return [];
  return [
    ...tunnels.map((t) => {
      const info = describeTunnel(t);
      const holder = taken.get(t.listen);
      return holder === undefined
        ? { text: `Tunnel (${info.kind}): ${info.text}`, kind: "info" as const }
        : { text: `Tunnel (${info.kind}): ${takenText(t, holder)}`, kind: "warn" as const };
    }),
    { text: "The tunnel icon on the tab, or Tunnels… in its menu, shows them with a way to test each.", kind: "info" },
  ];
}

/**
 * The ports on this computer that this window's tabs' tunnels hold (Local and SOCKS ones; a Remote one's port is on
 * the server), by port: the tab's id and title. Only one program can listen on a port, so a second tab of the same
 * profile can't open the first's tunnels. A tab in another window isn't known here; its ssh still says the port is in
 * use, in its own words
 */
const held = new Map<string, { tab: string; title: string }>();

/**
 * A tab's tunnels have opened: hold their ports for it, and say which another tab holds already
 * @param tab The tab's id
 * @param title Its title, for the other tab's warning
 * @param tunnels The tunnels its ssh was given
 * @returns The listen of each tunnel whose port another tab holds, with that tab's title
 */
export function holdTunnels(tab: string, title: string, tunnels: readonly Forward[]): Map<string, string> {
  releaseTunnels(tab);
  const taken = new Map<string, string>();
  for (const t of tunnels) {
    if (t.kind === "remote") continue;
    const { port } = listenAddress(t.listen.trim());
    const holder = held.get(port);
    if (holder && holder.tab !== tab) taken.set(t.listen, holder.title);
    else held.set(port, { tab, title });
  }
  return taken;
}

/**
 * A tab's session has ended, or the tab closed: its ports are free for another
 * @param tab The tab's id
 */
export function releaseTunnels(tab: string): void {
  for (const [port, holder] of held) if (holder.tab === tab) held.delete(port);
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

/** A common use for a tunnel, offered by Settings > Profiles > Tunnel Samples… to start from */
export interface TunnelSample {
  /** What it's for, as the menu lists it */
  label: string;
  tunnel: Forward;
}

/**
 * The common uses, each a tunnel to adjust: the addresses 10.0.0.5 and 10.0.0.10 stand for a device and a PC behind
 * the server. Ports here go above 10000 where the usual one may already be taken on this computer (a database or
 * Remote Desktop of its own)
 */
export const TUNNEL_SAMPLES: TunnelSample[] = [
  { label: "The server's own web page (HTTP)", tunnel: { kind: "local", listen: "8080", to: "localhost:80" } },
  { label: "The server's own web page (HTTPS)", tunnel: { kind: "local", listen: "8443", to: "localhost:443" } },
  { label: "A device's web page that only the server reaches (HTTPS)", tunnel: { kind: "local", listen: "9443", to: "10.0.0.5:443" } },
  { label: "Remote Desktop to a PC that only the server reaches", tunnel: { kind: "local", listen: "13389", to: "10.0.0.10:3389" } },
  { label: "PostgreSQL that only listens on the server", tunnel: { kind: "local", listen: "15432", to: "localhost:5432" } },
  { label: "MySQL or MariaDB that only listens on the server", tunnel: { kind: "local", listen: "13306", to: "localhost:3306" } },
  { label: "SOCKS proxy: browse the server's whole network", tunnel: { kind: "dynamic", listen: "1080", to: "" } },
  { label: "Let the server reach a web server on this computer", tunnel: { kind: "remote", listen: "9090", to: "localhost:8000" } },
];

/**
 * A sample's tunnel for a profile that has these already: its port moved up to the next free one, as two tunnels
 * can't listen on the same port
 * @param sample The sample picked
 * @param have The profile's tunnels
 */
export function sampleFor(sample: TunnelSample, have: readonly Forward[]): Forward {
  // a remote tunnel's port is on the server, a local or SOCKS one's here: they don't clash with each other
  const side = (kind: string) => (kind === "remote" ? "server" : "here");
  const used = new Set(have.filter((f) => side(f.kind) === side(sample.tunnel.kind)).map((f) => listenAddress(f.listen.trim()).port));
  let port = Number(sample.tunnel.listen);
  while (used.has(String(port))) port += 1;
  return { ...sample.tunnel, listen: String(port) };
}
