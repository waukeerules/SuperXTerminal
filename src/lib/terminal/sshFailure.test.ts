// How an ssh session ended, read from ssh's last words, and whether reconnecting by itself can help.
import { describe, expect, it } from "vitest";
import { reconnectDelay, retries, sshFailure } from "./sshFailure";

describe("sshFailure", () => {
  it("reads a refused login, and doesn't reconnect by itself", () => {
    const failure = sshFailure("(admin@192.168.0.50) Password:\r\nadmin@192.168.0.50: Permission denied (publickey,keyboard-interactive,password).\r\n");
    expect(failure).toEqual({ kind: "login" });
    expect(retries(failure)).toBe(false);
  });

  it("doesn't take the session's own Permission denied for a refused login", () => {
    const output = "lab$ cat /etc/shadow\r\ncat: /etc/shadow: Permission denied\r\nlab$ \r\nclient_loop: send disconnect: Broken pipe\r\n";
    expect(sshFailure(output)).toEqual({ kind: "other" });
    // nor one long before the end
    const earlier = "Permission denied (publickey).\r\n" + "line\r\n".repeat(20) + "Connection reset\r\n";
    expect(sshFailure(earlier).kind).toBe("other");
  });

  it("reads an unknown host", () => {
    const failure = sshFailure("ssh: Could not resolve hostname test.localdomain: nodename nor servname provided, or not known\r\n");
    expect(failure).toEqual({ kind: "unknownHost", host: "test.localdomain" });
    expect(retries(failure)).toBe(false);
    // Windows' wording
    expect(sshFailure("ssh: Could not resolve hostname sw9: No such host is known. \r\n").kind).toBe("unknownHost");
  });

  it("reads which kind of algorithm didn't match, and what the device offers", () => {
    const failure = sshFailure("Unable to negotiate with 10.0.0.9 port 22: no matching key exchange method found. Their offer: diffie-hellman-group1-sha1,diffie-hellman-group14-sha1\r\n");
    expect(failure).toEqual({ kind: "algorithms", what: "key exchange method", offer: "diffie-hellman-group1-sha1,diffie-hellman-group14-sha1" });
    expect(retries(failure)).toBe(false);
    expect(sshFailure("Unable to negotiate with 10.0.0.9 port 22: no matching host key type found. Their offer: ssh-rsa\r\n")).toMatchObject({ what: "host key type", offer: "ssh-rsa" });
  });

  it("reads too many attempts before a Permission denied the device sends with it", () => {
    const failure = sshFailure("Received disconnect from 10.0.0.9 port 22:2: Too many authentication failures\r\nDisconnected from 10.0.0.9 port 22\r\n");
    expect(failure).toEqual({ kind: "tooManyAttempts" });
    expect(retries(failure)).toBe(false);
  });

  it("reconnects by itself after a refusal, no route, a timeout or a lost connection", () => {
    expect(sshFailure("ssh: connect to host 10.0.0.9 port 2222: Connection refused\r\n")).toEqual({ kind: "refused", port: "2222" });
    expect(sshFailure("ssh: connect to host 10.0.0.9 port 22: No route to host\r\n").kind).toBe("unreachable");
    for (const output of ["ssh: connect to host 10.0.0.9 port 22: Connection timed out\r\n", "client_loop: send disconnect: Connection reset\r\n", ""]) {
      expect(retries(sshFailure(output))).toBe(true);
    }
  });
});

describe("sshFailure with Windows' ConPTY output", () => {
  it("reads ssh's words through a window title, cursor hiding and cursor moves for line breaks", () => {
    const conpty = "\x1b[?25l\x1b[2J\x1b[m\x1b[H\x1b]0;C:\\WINDOWS\\System32\\OpenSSH\\ssh.exe\x07\x1b[?25h"
      + "(admin@192.168.0.50) Password:\x1b[2;1H\x1b[?25l\x1b[3;1H"
      + "admin@192.168.0.50: Permission denied (publickey,keyboard-interactive,password).\x1b[?25h\x1b[4;1H";
    expect(sshFailure(conpty)).toEqual({ kind: "login" });
  });

  it("keeps a cursor move apart from the line after it, so an earlier line isn't read as ssh's", () => {
    // the session's own output, then a lost connection, the rows written by cursor moves
    const conpty = "cat: /etc/shadow: Permission denied (os error 13)\x1b[5;1H" + "lab$ \x1b[6;1H".repeat(8) + "client_loop: send disconnect: Connection reset\x1b[14;1H";
    expect(sshFailure(conpty).kind).toBe("other");
  });

  it("reads the device's offer without the escape codes after it", () => {
    const conpty = "Unable to negotiate with 10.0.0.9 port 22: no matching host key type found. Their offer: ssh-rsa,ssh-dss\x1b[?25h\x1b[3;1H";
    expect(sshFailure(conpty)).toMatchObject({ kind: "algorithms", offer: "ssh-rsa,ssh-dss" });
  });
});

describe("reconnectDelay", () => {
  it("doubles from the delay chosen up to two minutes, riding out a reload", () => {
    expect([0, 1, 2, 3, 4, 5].map((n) => reconnectDelay(10, n))).toEqual([10, 20, 40, 80, 120, 120]);
    // a delay chosen above two minutes stays as chosen
    expect(reconnectDelay(300, 3)).toBe(300);
  });
});
