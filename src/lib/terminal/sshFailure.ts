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
  /**
   * The jump server (the Jump host field, ssh -J) refused to pass the connection on: its sshd doesn't allow TCP
   * forwarding (AllowTcpForwarding no, DisableForwarding, or a PermitOpen without this host). OpenSSH says
   * "channel 0: open failed: administratively prohibited"
   */
  | { kind: "jumpForwarding" }
  /** The jump server tried, and couldn't reach the host itself: "open failed: connect failed: No route to host" */
  | { kind: "jumpConnect"; reason: string }
  /** Anything else: a lost connection, a timeout, the device closing it */
  | { kind: "other" };

/** Whether reconnecting by itself can help: not when the device would only answer the same again */
export function retries(failure: SshFailure): boolean {
  return !["login", "tooManyAttempts", "unknownHost", "algorithms", "jumpForwarding"].includes(failure.kind);
}

/** Lines at the end of the output read for ssh's last words: earlier ones are the session's own (a remote "Permission denied") */
const LAST_LINES = 6;

/**
 * The output's lines as text: escape sequences taken out, and a move of the cursor to a row's start taken as a new
 * line, as Windows' ConPTY often writes one in place of a line break (with a window title and cursor hiding around)
 */
function plainLines(output: string): string[] {
  return output
    // a window title or other OSC string, ended by BEL or ST
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "")
    // the cursor to a row (and its first column): a new line
    .replace(/\x1b\[\d*(?:;1)?H/g, "\n")
    .replace(/\x1b\[[0-9;?]*[ -/]*[@-~]/g, "")
    .replace(/\x1b[()][0-9A-Za-z]|\x1b[=>78]/g, "")
    .split(/\r?\n|\r/)
    .filter((line) => line.trim());
}

/** What ssh's output, since the session last started, says about how it ended */
export function sshFailure(all: string): SshFailure {
  const output = plainLines(all).slice(-LAST_LINES).join("\n");
  // checked before "Permission denied", which a device may also send as it gives up
  if (/Too many authentication failures/i.test(output)) return { kind: "tooManyAttempts" };
  const negotiate = /Unable to negotiate with \S+ port \d+: no matching (key exchange method|host key type|cipher|MAC) found\. Their offer: (\S+)/i.exec(output);
  if (negotiate) return { kind: "algorithms", what: negotiate[1], offer: negotiate[2] };
  // ssh's own words: "user@host: Permission denied (publickey,password)." or "Permission denied, please try again."
  if (/Permission denied \([\w,-]+\)|Permission denied, please try again/i.test(output)) return { kind: "login" };
  // the jump server's own answers, before the target's: its "connect failed: No route to host" isn't this computer's
  if (/open failed: administratively prohibited/i.test(output)) return { kind: "jumpForwarding" };
  const jump = /open failed: connect failed: (.+?)\s*$/im.exec(output);
  if (jump) return { kind: "jumpConnect", reason: jump[1] };
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
