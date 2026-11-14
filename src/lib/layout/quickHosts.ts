/**
 * Quick connect's Host field as a list: several hosts typed or pasted there open a tab each, with the same Login,
 * Password and Profile, for a handful of servers to run a command on and then forget (nothing is saved). The hosts
 * are separated by commas, semicolons, spaces or new lines, none of which a host name, an address, "user@host:port"
 * or "ssh://host" can hold.
 */

/** More hosts than this are asked about before they're opened, as the Sessions pane's Connect all is past ten */
export const MANY_HOSTS = 10;

/**
 * The hosts in what's typed in Quick connect's Host field, in the order typed, each once (a host typed twice, in
 * either case, opens one tab).
 * @returns none when the field is empty
 */
export function splitHosts(text: string): string[] {
  const seen = new Set<string>();
  return text.split(/[\s,;]+/).filter((host) => {
    const key = host.toLowerCase();
    if (!host || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

/**
 * Pasted text as it goes into the Host field. A one-line box drops the line breaks of what's pasted, which would
 * run a column of hosts together ("10.0.0.110.0.0.2"), so a paste of several lines is put in with commas between.
 * @returns null for text of one line, which the box takes as it is
 */
export function pastedHosts(text: string): string | null {
  return /[\r\n]/.test(text) ? splitHosts(text).join(", ") : null;
}
