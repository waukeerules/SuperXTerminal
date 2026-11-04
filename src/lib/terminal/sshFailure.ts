/**
 * Why an ssh session ended, from what ssh said last: OpenSSH exits with 255 for every failure of its own (a lost
 * connection, a wrong password, an unknown host), so its message is all there is to go by. Each kind says whether
 * reconnecting can help: a wrong password tried again by itself can lock the account (AAA lockouts, Cisco's
 * login block-for), and an unknown host or a device offering only older algorithms never answers differently.
 */

/** Why a session ended, and what the tab does about it */
export type SshFailure =
  /** Permission denied: the username, password or key was refused */
  | { kind: "login" }
  /** The device closed the connection after too many attempts: ssh offered its keys before the password */
  | { kind: "tooManyAttempts" }
  /** Could not resolve hostname */
  | { kind: "unknownHost"; host: string }
  /** Unable to negotiate: no algorithm of one kind in common ("key exchange method", "host key type", "cipher", "MAC") */
  | { kind: "algorithms"; what: string; offer: string }
  /** Connection refused: nothing listening (rebooting, SSH off, another port) */
  | { kind: "refused"; port: string }
  /** No route to host, Network is unreachable */
  | { kind: "unreachable" }
  /** Anything else: a lost connection, a timeout, the device closing it */
  | { kind: "other" };

/** Whether reconnecting by itself can help: not when the device would only answer the same again */
export function retries(failure: SshFailure): boolean {
  return !["login", "tooManyAttempts", "unknownHost", "algorithms"].includes(failure.kind);
}

/** Lines at the end of the output read for ssh's last words: earlier ones are the session's own (a remote "Permission denied") */
const LAST_LINES = 6;

/** What ssh's output, since the session last started, says about how it ended */
export function sshFailure(all: string): SshFailure {
  const output = all.replace(/\x1b\[[0-9;?]*[A-Za-z]/g, "").split(/\r?\n/).filter((l) => l.trim()).slice(-LAST_LINES).join("\n");
  // checked before "Permission denied", which a device may also send as it gives up
  if (/Too many authentication failures/i.test(output)) return { kind: "tooManyAttempts" };
  const negotiate = /Unable to negotiate with \S+ port \d+: no matching (key exchange method|host key type|cipher|MAC) found\. Their offer: (\S+)/i.exec(output);
  if (negotiate) return { kind: "algorithms", what: negotiate[1], offer: negotiate[2] };
  // ssh's own words: "user@host: Permission denied (publickey,password)." or "Permission denied, please try again."
  if (/Permission denied \([\w,-]+\)|Permission denied, please try again/i.test(output)) return { kind: "login" };
  const unknown = /Could not resolve hostname (\S+?):/i.exec(output);
  if (unknown) return { kind: "unknownHost", host: unknown[1] };
  const refused = /port (\d+): Connection refused/i.exec(output);
  if (refused) return { kind: "refused", port: refused[1] };
  if (/No route to host|Network is unreachable/i.test(output)) return { kind: "unreachable" };
  return { kind: "other" };
}

/** Seconds before an automatic reconnect: the delay at first, doubling each time up to two minutes, so a switch's reload is ridden out */
export function reconnectDelay(base: number, attempt: number): number {
  return Math.min(Math.max(base, 120), base * 2 ** attempt);
}
