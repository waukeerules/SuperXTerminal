// SPSL scripts run end to end in a stand-in tab: SuperXPuTTY's commands as before, WAITFOR's /i, /re and /else:,
// EXPECT (a login whose prompts come in any order), labels, GOTO and STOP, a session that drops or reconnects
// mid-script, variables and what a wait matched, FAILON, WAITPROMPT and WAITQUIET, ECHO, SAVE, CALL, stepping a line
// at a time, and Check script on them. Saved scripts (CALL) and saved files (SAVE) are stood in for by `library` and
// `files`. The tab is
// stood in for by `Device`, whose output is a string with WAITFOR's mark in it, and which answers what's typed as a
// switch would; the window (PROMPT, the status bar) by a host that records the status lines.
import { describe, expect, it } from "vitest";
import { checkScript, runScript, ScriptRun, type ScriptHost, type ScriptTarget } from "./spsl";

/** A stand-in switch: what it prints after each thing typed, in turn, and what it has been sent */
class Device implements ScriptTarget {
  output = "";
  mark = 0;
  typed: string[] = [];
  /** The connection's id (a new one after a reconnect), or null while the session is down */
  connected: number | null = 1;

  /** @param answers What the device prints after each thing typed, in order (none: it prints nothing) */
  constructor(private readonly answers: string[] = []) {}

  /** Print something, as the session's output */
  print(text: string): void {
    this.output += text;
  }

  async type(text: string): Promise<void> {
    this.typed.push(text);
    const answer = this.answers.shift();
    if (answer !== undefined) setTimeout(() => this.print(answer), 5);
  }

  markOutput(atLineStart: boolean): void {
    this.mark = atLineStart ? this.output.lastIndexOf("\n") + 1 : this.output.length;
  }

  outputSinceMark(): string {
    return this.output.slice(this.mark);
  }

  advanceMark(characters: number): void {
    this.mark += characters;
  }

  async sendBreak(): Promise<void> {
    throw new Error("no Break in an SSH tab");
  }

  connection(): number | null {
    return this.connected;
  }

  position(): number {
    return this.output.length;
  }

  outputFrom(position: number): string {
    return this.output.slice(position);
  }
}

/** The saved scripts CALL loads, by name */
let library: Record<string, string> = {};
/** What SAVE wrote, by path */
let files: Record<string, string> = {};
/** What PROMPT and PWDPROMPT answer */
let answer = "secret";

/** Run a script in a device (as a session's own script, so it sees the output from the start); its status lines */
async function run(script: string, device: Device, run = new ScriptRun("Login", "sw1", script.split("\n").length)): Promise<string[]> {
  const status: string[] = [];
  const host: ScriptHost = {
    ask: async () => answer,
    openSession: () => true,
    status: (text) => status.push(text),
    save: async (path, text, append) => {
      files[path] = (append ? files[path] ?? "" : "") + text;
      return `/out/${path}`;
    },
    loadScript: async (name) => {
      if (!(name in library)) throw new Error(`there's no saved script "${name}"`);
      return library[name];
    },
  };
  await runScript(device, host, run, script, true);
  return status;
}

describe("SuperXPuTTY's commands", () => {
  it("types a line, waits for the prompt, and stops when it doesn't come", async () => {
    const device = new Device(["\r\nsw1#"]);
    device.print("sw1>");
    expect(await run("WAITFOR >\nSENDLINE enable\nWAITFOR #", device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual(["enable\r"]);
    // the line numbers are the script's own, comments counted
    expect(await run("# show the clock\nWAITFOR /t:1 Password:\nSENDLINE x", new Device())).toEqual([
      'Script "Login" in sw1 stopped at line 2: "Password:" didn\'t appear within 1 s',
    ]);
  });
});

describe("WAITFOR's options", () => {
  it("/i matches whatever the case, and without it the case must match", async () => {
    const device = new Device();
    device.print("PASSWORD:");
    expect(await run("WAITFOR /i password:\nSENDLINE x", device)).toEqual(['Script "Login" in sw1 finished']);
    expect(await run("WAITFOR /t:1 password:", device)).toEqual(['Script "Login" in sw1 stopped at line 1: "password:" didn\'t appear within 1 s']);
  });

  it("/re waits for a regular expression, ^ and $ at each line's ends", async () => {
    const device = new Device();
    // a # in the banner isn't the prompt; the prompt is a line ending in # or >
    device.print("Welcome to switch #4\r\nsw1#");
    const script = "WAITFOR /re ^\\S+[>#]$\nSENDLINE show clock";
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual(["show clock\r"]);
  });

  it("/else:continue goes on, and /else:label goes to the label, after a timeout", async () => {
    const device = new Device();
    expect(await run("WAITFOR /t:1 /else:continue Password:\nSENDLINE next", device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual(["next\r"]);
    const jumped = new Device();
    const script = "WAITFOR /t:1 /else:nobody Password:\nSENDLINE skipped\n:nobody\nSENDLINE here\nSTOP no password prompt";
    expect(await run(script, jumped)).toEqual(['Script "Login" in sw1 stopped at line 5: no password prompt']);
    expect(jumped.typed).toEqual(["here\r"]);
  });
});

describe("EXPECT", () => {
  /** A login that copes with a username asked or not, and a new host's key question */
  const LOGIN = [
    "EXPECT /t:5",
    '  CASE "(yes/no"',
    "    SENDLINE yes",
    "    AGAIN",
    "  CASE /username:/i",
    "    SENDLINE admin",
    "    AGAIN",
    "  CASE assword:",
    "    SENDLINE secret",
    "    AGAIN",
    "  CASE /^\\S+#$/",
    "END",
    "SENDLINE show clock",
  ].join("\n");

  it("answers the prompts in whatever order they come, until the prompt", async () => {
    const asksAll = new Device(["\r\nUsername: ", "\r\nPassword: ", "\r\nsw1#"]);
    asksAll.print("Are you sure you want to continue connecting (yes/no/[fingerprint])? ");
    expect(await run(LOGIN, asksAll)).toEqual(['Script "Login" in sw1 finished']);
    expect(asksAll.typed).toEqual(["yes\r", "admin\r", "secret\r", "show clock\r"]);

    const passwordOnly = new Device(["\r\nsw1#"]);
    passwordOnly.print("Password: ");
    expect(await run(LOGIN, passwordOnly)).toEqual(['Script "Login" in sw1 finished']);
    expect(passwordOnly.typed).toEqual(["secret\r", "show clock\r"]);
  });

  it("takes whichever case comes first in the output, and stops when none comes", async () => {
    const device = new Device();
    device.print("% Login invalid\r\nUsername: ");
    const script = 'EXPECT /t:1\n  CASE Username:\n    STOP asked again\n  CASE "% Login invalid"\n    STOP refused\nEND';
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 stopped at line 5: refused']);
    expect(await run("EXPECT /t:1\nCASE nothing\nCASE /never/\nEND", new Device())).toEqual([
      'Script "Login" in sw1 stopped at line 1: none of its cases appeared within 1 s',
    ]);
  });

  it("doesn't start when its structure can't run, and types nothing", async () => {
    const device = new Device();
    device.print("Password:");
    expect(await run("SENDLINE first\nEXPECT\n  CASE Password:\n    SENDLINE x", device)).toEqual([
      'Script "Login" in sw1 didn\'t start: line 2: This EXPECT has no END',
    ]);
    expect(await run("GOTO nowhere", device)).toEqual(['Script "Login" in sw1 didn\'t start: line 1: There\'s no label :nowhere to go to']);
    expect(device.typed).toEqual([]);
  });
});

describe("SENDKEY", () => {
  it("presses Space by name, for --More--, once or a number of times", async () => {
    const device = new Device();
    device.print(" --More-- ");
    const script = "EXPECT /t:2\n  CASE --More--\n    SENDKEY SPACE\nEND\nSENDKEY {SPACE 3}";
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual([" ", "   "]);
    expect(checkScript("SENDKEY SPACE\nSENDKEY {SPACE 2}", () => true, () => [])).toEqual([]);
  });
});

describe("a session that drops or reconnects", () => {
  it("stops at once when the session ends during a wait, without waiting out its time", async () => {
    const device = new Device();
    // well inside the wait (SENDLINE itself takes 50 ms)
    setTimeout(() => (device.connected = null), 200);
    const started = Date.now();
    const status = await run("SENDLINE show tech\nWAITFOR /t:20 /else:continue #\nSENDLINE next", device);
    expect(status).toEqual(['Script "Login" in sw1 stopped at line 2: the session ended']);
    expect(Date.now() - started).toBeLessThan(2000);
    // /else: is for the device not answering, not for the session going: nothing more is typed
    expect(device.typed).toEqual(["show tech\r"]);
  });

  it("doesn't type into a reconnected session, which may be at a login prompt", async () => {
    const device = new Device();
    // during the SLEEP, after SENDLINE's own 50 ms
    setTimeout(() => {
      device.connected = 2;
      device.print("Username: ");
    }, 200);
    const status = await run("SENDLINE configure terminal\nSLEEP 500\nSENDLINE interface Gi1/0/1", device);
    expect(status).toEqual(['Script "Login" in sw1 stopped at line 3: the session reconnected, so the script stopped rather than type into the new connection']);
    expect(device.typed).toEqual(["configure terminal\r"]);
  });

  it("doesn't start in a tab whose session is down", async () => {
    const device = new Device();
    device.connected = null;
    expect(await run("SENDLINE x", device)).toEqual(['Script "Login" in sw1 didn\'t start: the session isn\'t connected']);
    expect(device.typed).toEqual([]);
  });
});

describe("GOTO and Stop", () => {
  it("lets Stop end a loop with nothing to wait for", async () => {
    const device = new Device();
    const loop = new ScriptRun("Loop", "sw1", 3);
    setTimeout(() => loop.stop(), 50);
    expect(await run(":again\nSENDCHAR .\nGOTO again", device, loop)).toEqual(['Script "Loop" in sw1 stopped by you at line 3 of 3']);
    expect(device.typed.length).toBeGreaterThan(1);
  });
});

describe("Check script", () => {
  const check = (script: string) => checkScript(script, () => true, () => []).map((i) => `${i.line}: ${i.message}`);

  it("knows the new commands, and finds their mistakes", async () => {
    expect(check("EXPECT /t:5 /i /else:retry\n  CASE /[>#]$/\n  AGAIN\nEND\n:retry\nGOTO retry")).toEqual([]);
    expect(check("CASE x\nAGAIN\nEND")).toEqual([
      "1: CASE goes inside an EXPECT … END",
      "2: AGAIN goes inside an EXPECT's CASE: it waits again",
      "3: END closes an EXPECT, REPEAT, FOREACH or IF, and there's none open",
    ]);
    expect(check("WAITFOR /re (unclosed")[0]).toMatch(/^1: "\(unclosed" isn't a regular expression/);
    expect(check("EXPECT\n  SENDLINE x\nEND")).toEqual([
      "1: EXPECT needs at least one CASE: what to wait for",
      "2: Lines inside an EXPECT go under a CASE, so it's clear when they run",
    ]);
    expect(check("WAITFOR /else:later x")).toEqual(["1: There's no label :later to go to"]);
  });
});

describe("variables", () => {
  it("SET, PROMPT /var: and what a wait matched fill {names} in later lines", async () => {
    answer = "20";
    const device = new Device(["\r\nsw1(config)#", "\r\nsw1(config-vlan)#"]);
    device.print("Cisco IOS Software, Version 17.9.4a\r\nsw1#");
    const script = [
      "WAITFOR /re Version (?<version>\\S+)",
      "WAITFOR /re ^([\\w-]+)#$",
      "SET host {1}",
      "PROMPT /var:vlan Which VLAN?",
      "SENDLINE ! {host} runs {version}",
      "SENDLINE vlan {vlan}",
      "WAITFOR {host}(config-vlan)#",
      "STOP vlan {vlan} on {host}",
    ].join("\n");
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 stopped at line 8: vlan 20 on sw1']);
    // the question isn't typed: its answer went into {vlan}
    expect(device.typed).toEqual(["! sw1 runs 17.9.4a\r", "vlan 20\r"]);
  });

  it("puts a variable into a regular expression as plain text, and leaves a quantifier alone", async () => {
    const device = new Device();
    device.print("host a.b up 42\r\n");
    expect(await run("SET name a.b\nWAITFOR /re ^host {name} up \\d{2}$\nSTOP {0}", device)).toEqual(['Script "Login" in sw1 stopped at line 3: host a.b up 42']);
    // the . in a.b is a dot, not any character
    const other = new Device();
    other.print("host axb up 42\r\n");
    expect(await run("SET name a.b\nWAITFOR /t:1 /re ^host {name} up", other)).toEqual(['Script "Login" in sw1 stopped at line 2: "/^host a\\.b up/" didn\'t appear within 1 s']);
  });
});

describe("FAILON", () => {
  it("stops the script at the first wait that sees it, naming the device's line", async () => {
    const device = new Device(["\r\n% Invalid input detected at '^' marker.\r\nsw1(config)#"]);
    device.print("sw1(config)#");
    const script = "FAILON % Invalid\nSENDLINE interfase Gi1/0/1\nWAITPROMPT\nSENDLINE description uplink";
    expect(await run(script, device)).toEqual([`Script "Login" in sw1 stopped at line 3: the device said "% Invalid input detected at '^' marker." (FAILON)`]);
    expect(device.typed).toEqual(["interfase Gi1/0/1\r"]);
    // FAILON OFF forgets it
    const again = new Device(["\r\n% Invalid input\r\nsw1#"]);
    again.print("sw1#");
    expect(await run("FAILON % Invalid\nFAILON OFF\nSENDLINE x\nWAITPROMPT", again)).toEqual(['Script "Login" in sw1 finished']);
  });
});

describe("WAITPROMPT and WAITQUIET", () => {
  it("WAITPROMPT waits for the prompt on the last line, not for one in the middle of the output", async () => {
    const device = new Device(["\r\nline 1 ends in #\r\nline 2\r\nsw1#"]);
    device.print("sw1#");
    expect(await run("SENDLINE show x\nWAITPROMPT\nSTOP {0}", device)).toEqual(['Script "Login" in sw1 stopped at line 3: sw1#']);
    // SET prompt puts another in its place
    const custom = new Device();
    custom.print("Router> ");
    expect(await run("SET prompt ^Router>\\s$\nWAITPROMPT /t:1\nSTOP got it", custom)).toEqual(['Script "Login" in sw1 stopped at line 3: got it']);
    expect(await run("WAITPROMPT /t:1", new Device())).toEqual(['Script "Login" in sw1 stopped at line 1: the prompt didn\'t appear within 1 s']);
  });

  it("WAITQUIET waits until the output has stopped changing", async () => {
    const device = new Device();
    let printed = 0;
    const timer = setInterval(() => printed++ < 4 && device.print("more\r\n"), 50);
    const started = Date.now();
    expect(await run("WAITQUIET 300", device)).toEqual(['Script "Login" in sw1 finished']);
    clearInterval(timer);
    // the four lines took about 200 ms, then 300 quiet
    expect(Date.now() - started).toBeGreaterThanOrEqual(450);
    expect(await run("SET x 1\nWAITQUIET /t:1 5000", device)).toEqual(['Script "Login" in sw1 stopped at line 2: the output didn\'t stay quiet for 5000 ms within 1 s']);
  });
});

describe("ECHO and SAVE", () => {
  it("ECHO speaks in the status bar, and SAVE writes what the last command printed, without its echo and prompt", async () => {
    files = {};
    const device = new Device(["show clock\r\n*10:01:02 UTC Mon Nov 4\r\nsw1#"]);
    device.print("sw1#");
    const script = "SET host sw1\nSENDLINE show clock\nWAITPROMPT\nSAVE clock-{host}.txt\nSAVE /append clock-{host}.txt\nECHO saved {host}'s clock";
    expect(await run(script, device)).toEqual(['Script "Login" in sw1: saved sw1\'s clock', 'Script "Login" in sw1 finished']);
    expect(files).toEqual({ "clock-sw1.txt": "*10:01:02 UTC Mon Nov 4\n*10:01:02 UTC Mon Nov 4\n" });
  });
});

describe("CALL", () => {
  it("runs a saved script here, with the same variables, and goes on after it", async () => {
    library = { login: "WAITFOR /i password:\nSENDLINE {pw}\nSET logged yes" };
    const device = new Device();
    device.print("Password: ");
    expect(await run("SET pw hunter2\nCALL login\nSTOP logged in: {logged}", device)).toEqual(['Script "Login" in sw1 stopped at line 3: logged in: yes']);
    expect(device.typed).toEqual(["hunter2\r"]);
  });

  it("says where it stopped in the called script, and stops a script that calls itself", async () => {
    library = { broken: "# a comment\nWAITFOR /t:1 never", loop: "CALL loop" };
    expect(await run("CALL broken", new Device())).toEqual(['Script "Login" in sw1 stopped at line 2 of "broken": "never" didn\'t appear within 1 s']);
    expect((await run("CALL loop", new Device()))[0]).toMatch(/^Script "Login" in sw1 stopped at line 1 of "loop": CALL loop: scripts calling scripts 8 deep/);
    expect(await run("CALL missing", new Device())).toEqual(['Script "Login" in sw1 stopped at line 1: CALL missing: there\'s no saved script "missing"']);
  });
});

describe("stepping", () => {
  it("pauses before each line until Next line, and Stop ends it there", async () => {
    const device = new Device();
    const stepping = new ScriptRun("Step", "sw1", 3, true);
    const done = run("SENDLINE one\nSENDLINE two\nSENDLINE three", device, stepping);
    await expect.poll(() => stepping.paused).toBe(true);
    expect(device.typed).toEqual([]);
    stepping.next();
    await expect.poll(() => device.typed).toEqual(["one\r"]);
    await expect.poll(() => stepping.paused && stepping.line === 2).toBe(true);
    stepping.stop();
    expect(await done).toEqual(['Script "Step" in sw1 stopped by you at line 2 of 3']);
    expect(device.typed).toEqual(["one\r"]);
  });
});

describe("Check script on the new commands", () => {
  const check = (script: string) => checkScript(script, () => true, (text) => text.match(/\{\w+\}/g) ?? [], (name) => name === "login").map((i) => `${i.line}: ${i.message}`);

  it("knows the script's own variables, and finds the new commands' mistakes", () => {
    expect(check("SET host sw1\nWAITFOR /re (?<ver>\\d+)\nSENDLINE {host} {ver} {1}\nCALL login\nFAILON % Invalid\nWAITQUIET 500\nWAITPROMPT /t:5\nSAVE /append x.txt\nECHO hi")).toEqual([]);
    expect(check("SENDLINE {nowhere}")).toEqual(["1: {nowhere} has no value in Tools > Environment Variables, and the script doesn't SET it, so it's typed as it is."]);
    expect(check("SET 1x y\nWAITQUIET soon\nFAILON\nSAVE\nCALL nothing\nWAITPROMPT sw1#")).toEqual([
      "1: SET names a variable, then its value: SET vlan 20 (a name starts with a letter or _)",
      "2: WAITQUIET needs how long the output must stay quiet, in milliseconds, e.g. WAITQUIET 1500",
      "3: FAILON needs what the device says when it refuses, e.g. FAILON % Invalid input; FAILON OFF forgets them",
      "4: SAVE needs a file: SAVE show-run.txt (in Documents/SuperXTerminal/output) or a full path",
      '5: There\'s no saved SPSL script "nothing" in the scripts folder; the script will stop here.',
      "6: WAITPROMPT takes only /t: and /else:; to wait for a prompt of your own, SET prompt to a regular expression first",
    ]);
  });
});

describe("REPEAT, LIMIT and loops that never wait", () => {
  it("REPEAT runs its lines that many times, {repeat} the round, an inner REPEAT's round its own", async () => {
    const device = new Device();
    const script = "REPEAT 2\n  SENDLINE outer {repeat}\n  REPEAT 2\n    SENDLINE inner {repeat}\n  END\n  SENDLINE after {repeat}\nEND";
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual([
      "outer 1\r", "inner 1\r", "inner 2\r", "after 1\r",
      "outer 2\r", "inner 1\r", "inner 2\r", "after 2\r",
    ]);
  });

  it("LIMIT ends the whole script when its time runs out, a wait included", async () => {
    const started = Date.now();
    expect(await run("LIMIT 1s\nWAITFOR /t:30 never", new Device())).toEqual(['Script "Login" in sw1 stopped at line 2: the script\'s LIMIT of 1 s ran out']);
    expect(Date.now() - started).toBeLessThan(3000);
    expect(await run("LIMIT 1\n:poll\nSLEEP 300\nGOTO poll", new Device())).toEqual(['Script "Login" in sw1 stopped at line 3: the script\'s LIMIT of 1 s ran out']);
  });

  it("stops a loop that goes round 10,000 times without waiting, in moments", async () => {
    const device = new Device();
    const started = Date.now();
    expect(await run(":again\nSENDCHAR x\nGOTO again", device)).toEqual([
      'Script "Login" in sw1 stopped at line 3: it went round 10,000 times without waiting for anything; a wait or SLEEP in the loop lets it go on',
    ]);
    expect(Date.now() - started).toBeLessThan(5000);
    // with a wait in it, a loop may go round as often as it likes
    const waiting = new Device();
    waiting.print("#".repeat(12_000));
    expect(await run("REPEAT 10001\n  WAITFOR #\nEND", waiting)).toEqual(['Script "Login" in sw1 finished']);
  });
});

describe("the session's values", () => {
  it("are there from the start as {host_address}, {session_name}…, and SET can change them", async () => {
    const device = new Device();
    const status: string[] = [];
    const host: ScriptHost = { ask: async () => "", openSession: () => true, status: (t) => status.push(t), save: async () => "", loadScript: async () => "" };
    const script = "SENDLINE ssh {host_address} -p {host_port} # {session_name}\nSET host_address 10.9.9.9\nSENDLINE {host_address}";
    await runScript(device, host, new ScriptRun("Hop", "sw1", 3), script, true, { host_address: "10.0.0.5", host_port: "2222", session_name: "core-sw1" });
    expect(device.typed).toEqual(["ssh 10.0.0.5 -p 2222 # core-sw1\r", "10.9.9.9\r"]);
  });

  it("aren't taken for Environment Variables, and the new commands' mistakes are found", () => {
    const check = (script: string) => checkScript(script, () => true, (text) => text.match(/\{\w+\}/g) ?? []).map((i) => `${i.line}: ${i.message}`);
    expect(check("SENDLINE {host_address} {session_username} {session_folder}\nREPEAT 3\nSENDLINE {repeat}\nEND\nLIMIT 10m")).toEqual([]);
    expect(check("REPEAT\nEND\nREPEAT 2\nLIMIT soon")).toEqual([
      "1: REPEAT is how many times to run the lines up to its END: REPEAT 5 (1 to 100,000)",
      "2: END closes an EXPECT, REPEAT, FOREACH or IF, and there's none open",
      "3: This REPEAT has no END",
      "4: LIMIT is how long the whole script may run: LIMIT 90s, LIMIT 10m, LIMIT 1h (a number alone is seconds)",
    ]);
  });
});

describe("CHECK and REQUIRE", () => {
  /** A switch that answers show version and show run's bgp section */
  const switchWith = (version: string, bgp: string) => {
    const device = new Device([`\r\nCisco IOS XE Software, Version ${version}\r\nsw1#`, `\r\n${bgp}sw1#`]);
    device.print("sw1#");
    return device;
  };
  const script = [
    "CHECK version /re Version (17\\.\\d+)",
    "CHECK nobgp /not /re ^router bgp",
    "SENDLINE show version",
    "WAITPROMPT",
    "SENDLINE show run | section bgp",
    "WAITPROMPT",
    "REQUIRE version nobgp",
    "SENDLINE ! ready on {version}",
  ].join("\n");

  it("goes on when the prerequisites are met, keeping what a CHECK matched", async () => {
    const device = switchWith("17.9.4a", "");
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed.at(-1)).toBe("! ready on 17.9\r");
  });

  it("stops at REQUIRE when one isn't met, saying which and why, and types nothing after it", async () => {
    const old = switchWith("16.12.4", "");
    expect(await run(script, old)).toEqual(['Script "Login" in sw1 stopped at line 7: prerequisite "version" isn\'t met: /Version (17\\.\\d+)/ didn\'t appear']);
    expect(old.typed).toEqual(["show version\r", "show run | section bgp\r"]);
    const bgp = switchWith("17.9.4a", "router bgp 65001\r\n");
    expect(await run(script, bgp)).toEqual(['Script "Login" in sw1 stopped at line 7: prerequisite "nobgp" isn\'t met: /^router bgp/ appeared ("router bgp 65001")']);
  });

  it("REQUIRE /else: goes elsewhere, and REQUIRE alone judges every CHECK so far", async () => {
    const device = switchWith("16.12.4", "");
    const branching = "CHECK new /re Version 17\nSENDLINE show version\nWAITPROMPT\nREQUIRE /else:old\nSTOP new\n:old\nSTOP old software";
    expect(await run(branching, device)).toEqual(['Script "Login" in sw1 stopped at line 7: old software']);
  });
});

describe("PAGER", () => {
  it("presses Space at each --More-- a wait sees, once each, until the prompt", async () => {
    const device = new Device([
      "\r\nline 1\r\nline 2\r\n --More-- ",
      "\r\nline 3\r\n --More-- ",
      "\r\nline 4\r\nsw1#",
    ]);
    device.print("sw1#");
    files = {};
    const script = "PAGER --More--\nSENDLINE show running-config\nWAITPROMPT /t:5\nSAVE run.txt";
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual(["show running-config\r", " ", " "]);
    expect(files["run.txt"]).toContain("line 4");
  });

  it("pages through less: Space at its : between pages, q at its (END), where it waits rather than ending", async () => {
    const device = new Device(["\r\n1\r\n2\r\n:", "\r\n3\r\n4\r\n(END)", "\r\nuser@omv:~$ "]);
    device.print("user@omv:~$ ");
    const script = "PAGER --More--\nPAGER /re ^:\\s*$\nPAGER /key:q (END)\nSENDLINE seq 1 4 | more\nWAITPROMPT /t:5";
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual(["seq 1 4 | more\r", " ", "q"]);
  });

  it("presses its own key, and PAGER OFF stops answering", async () => {
    const device = new Device(["\r\nmore\r\n<--- More --->", "\r\nsw1#"]);
    device.print("sw1#");
    expect(await run("PAGER /key:ENTER /re <-+ More -+>\nSENDLINE show log\nWAITPROMPT /t:5", device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual(["show log\r", "\r"]);
    const off = new Device(["\r\n --More-- "]);
    off.print("sw1#");
    expect(await run("PAGER --More--\nPAGER OFF\nSENDLINE show run\nWAITPROMPT /t:1", off)).toEqual(['Script "Login" in sw1 stopped at line 4: the prompt didn\'t appear within 1 s']);
    expect(off.typed).toEqual(["show run\r"]);
  });

  it("is found by Check script, as are CHECK and REQUIRE's mistakes", () => {
    const check = (script: string) => checkScript(script, () => true, (text) => text.match(/\{\w+\}/g) ?? []).map((i) => `${i.line}: ${i.message}`);
    expect(check("PAGER --More--\nCHECK ios /re Version (\\S+)\nREQUIRE ios\nSENDLINE {ios}\nPAGER /key:ENTER More\nPAGER OFF")).toEqual([]);
    expect(check("CHECK\nREQUIRE missing\nPAGER\nPAGER /key:NOPE x")).toEqual([
      "1: CHECK names a prerequisite, then what to look for: CHECK ios /re Version 17\\. (REQUIRE ios judges it)",
      "2: There's no CHECK missing in the script, so REQUIRE stops here",
      "3: PAGER needs the pager's prompt, e.g. PAGER --More--; PAGER OFF forgets them",
      "4: /key: is a key as SENDKEY writes it, e.g. PAGER /key:ENTER --More--",
    ]);
  });
});

describe("a copy with !!!! progress, and /idle:", () => {
  /** A switch copying a file: its questions, then `bangs` of progress, `stall` ms apart, then the result (if `finish`) */
  function copying(bangs: number, every: number, finish: boolean): Device {
    const device = new Device(["\r\nDestination filename [cat9k.bin]? "]);
    device.print("sw1#");
    const type = device.type.bind(device);
    device.type = async (text: string) => {
      await type(text);
      // Enter at the filename: the copy starts
      if (text === "\r" && device.typed.length === 2) {
        let sent = 0;
        const timer = setInterval(() => {
          if (sent++ < bangs) device.print("!!!!");
          else {
            clearInterval(timer);
            if (finish) device.print("\r\n12345678 bytes copied in 4.2 secs (2940000 bytes/sec)\r\nsw1#");
          }
        }, every);
      }
    };
    return device;
  }
  const script = [
    "FAILON %Error",
    "SENDLINE copy ftp://10.0.0.9/cat9k.bin flash:",
    "EXPECT /t:30 /idle:2",
    "  CASE /\\[[^\\]]*\\]\\?\\s*$/",
    "    SENDLINE",
    "    AGAIN",
    "  CASE /(\\d+) bytes copied in ([\\d.]+) secs/",
    "    STOP copied {1} bytes in {2} s",
    "END",
  ].join("\n");

  it("answers the filename question, rides out the progress, and reads the result", async () => {
    expect(await run(script, copying(8, 100, true))).toEqual(['Script "Login" in sw1 stopped at line 8: copied 12345678 bytes in 4.2 s']);
  });

  it("stops a copy whose progress has stalled after /idle:, not after its whole /t:", async () => {
    const started = Date.now();
    expect(await run(script, copying(3, 100, false))).toEqual(['Script "Login" in sw1 stopped at line 3: no new output for 2 s (/idle:): stalled?']);
    expect(Date.now() - started).toBeLessThan(6000);
  });
});

describe("COLLECT, FOREACH and IF", () => {
  const DIR = [
    "",
    "Directory of flash:/",
    "",
    "   16  -rw-  508371196  Mar 1 2024  cat9k_iosxe.17.06.05.SPA.bin",
    "   17  -rw-  611823810  Jun 2 2024  cat9k_iosxe.17.09.04a.SPA.bin",
    "   18  -rw-       2097  Jun 2 2024  packages.conf",
    "   19  -rw-  633112021  Nov 1 2024  cat9k_iosxe.17.12.04.SPA.bin",
    "",
    "11353194496 bytes total (8022446080 bytes free)",
    "sw1#",
  ].join("\r\n");

  it("collects every .bin in dir's output and deletes all but the one it boots from", async () => {
    answer = "yes";
    const device = new Device(["\r\nBOOT variable = flash:cat9k_iosxe.17.12.04.SPA.bin;\r\nsw1#", DIR, "\r\nsw1#", "\r\nsw1#"]);
    device.print("sw1#");
    const script = [
      "SENDLINE show boot | include BOOT",
      // {1} is the next wait's to replace: kept with SET at once
      "WAITFOR /re (\\S+\\.bin)",
      "SET boot {1}",
      "WAITPROMPT",
      "SENDLINE dir flash:",
      "WAITPROMPT",
      "COLLECT bins /re (\\S+\\.bin)$",
      "IF {bins_count} == 0",
      "  STOP nothing to clean up",
      "END",
      "PROMPT /var:ok Delete {bins_count} images except {boot}? Type yes",
      "IF {ok} != yes",
      "  STOP not deleted",
      "END",
      "FOREACH file IN bins",
      "  IF flash:{file} != {boot}",
      "    SENDLINE delete /force flash:{file}",
      "    WAITPROMPT",
      "  ELSE",
      "    ECHO keeping {file}",
      "  END",
      "END",
    ].join("\n");
    const status = await run(script, device);
    expect(status).toEqual(['Script "Login" in sw1: keeping cat9k_iosxe.17.12.04.SPA.bin', 'Script "Login" in sw1 finished']);
    expect(device.typed).toEqual([
      "show boot | include BOOT\r",
      "dir flash:\r",
      "delete /force flash:cat9k_iosxe.17.06.05.SPA.bin\r",
      "delete /force flash:cat9k_iosxe.17.09.04a.SPA.bin\r",
    ]);
  });

  it("FOREACH takes a written list too, skips an empty one, and goes inside another", async () => {
    const device = new Device();
    const script = "FOREACH vlan IN 10, 20\n  FOREACH port IN Gi1, Gi2\n    SENDLINE {vlan}-{port}\n  END\nEND\nFOREACH none IN {nothing}\n  SENDLINE never\nEND";
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual(["10-Gi1\r", "10-Gi2\r", "20-Gi1\r", "20-Gi2\r"]);
  });

  it("IF compares as numbers when both sides are, as text otherwise, and with ~ a regular expression", async () => {
    const device = new Device();
    const script = [
      "SET count 9",
      "IF {count} > 10",
      "  SENDLINE big",
      "ELSE",
      "  SENDLINE small",
      "END",
      "SET version 17.12.4",
      "IF {version} ~ /^17\\.1[0-9]\\./",
      "  SENDLINE new",
      "END",
      "IF {version} !~ /^16/",
      "  IF {missing}",
      "    SENDLINE has missing",
      "  ELSE",
      "    SENDLINE no missing",
      "  END",
      "END",
    ].join("\n");
    expect(await run(script, device)).toEqual(['Script "Login" in sw1 finished']);
    expect(device.typed).toEqual(["small\r", "new\r", "no missing\r"]);
  });

  it("is checked: blocks, conditions and lists", () => {
    const check = (script: string) => checkScript(script, () => true, (text) => text.match(/\{\w+\}/g) ?? []).map((i) => `${i.line}: ${i.message}`);
    expect(check("COLLECT bins /re (\\S+\\.bin)$\nFOREACH f IN bins\nIF {f} ~ /x/\nELSE\nEND\nEND\nSENDLINE {bins_count}")).toEqual([]);
    expect(check("ELSE\nIF\nFOREACH x\nIF x ~ /(/\nEND\nCOLLECT\nWAITFOR /idle:0 x")).toEqual([
      "1: ELSE goes inside an IF … END",
      "2: IF needs a test: IF {count} > 0, IF {file} != {boot}, IF {version} ~ /^17\\./",
      "3: FOREACH names a variable, then IN and a list: FOREACH file IN bins (a COLLECT's list), or FOREACH vlan IN 10, 20, 30",
      expect.stringMatching(/^4: "\(" isn't a regular expression/),
      "5: END closes an EXPECT, REPEAT, FOREACH or IF, and there's none open",
      "6: COLLECT names a list, then what to find in the last command's output: COLLECT bins /re (\\S+\\.bin)$",
      "7: /idle: is how many seconds without new output end the wait, e.g. WAITFOR /t:1800 /idle:60 bytes copied",
    ]);
  });
});

describe("the guide and the example scripts", () => {
  it("pass Check script: every example in docs/SPSL.md and docs/spsl-examples", () => {
    // read as text through Vite, which the tests run in
    const guide = Object.values(import.meta.glob<string>("/docs/SPSL.md", { query: "?raw", import: "default", eager: true }))[0];
    // the runnable examples: not the grammar, nor EXPECT's outline. Lines end in \r\n where Git checks the guide out
    // that way on Windows (core.autocrlf)
    const examples = [...guide.matchAll(/```\r?\n([\s\S]*?)```/g)].map((m) => m[1]).filter((b) => !b.includes("script     =") && !b.startsWith("EXPECT [/t:"));
    const tours = Object.values(import.meta.glob<string>("/docs/spsl-examples/*.spsl", { query: "?raw", import: "default", eager: true }));
    expect(examples.length).toBeGreaterThan(10);
    expect(tours.length).toBe(4);
    // {username} and {ftp_user} are Environment Variables the reader defines
    const problems = [...examples, ...tours].flatMap((script) =>
      checkScript(script, () => true, (text) => (text.match(/\{\w+\}/g) ?? []).filter((m) => !["{username}", "{ftp_user}"].includes(m)), () => true)
        .map((issue) => `${script.split("\n")[0]} … line ${issue.line}: ${issue.message}`),
    );
    expect(problems).toEqual([]);
  });
});

describe("a button's colour", () => {
  it("numbers each line a script types in its button's colour, marked just before it's typed", async () => {
    const device = new Device(["\nsw1#", "\nsw1#"]);
    const log: string[] = [];
    Object.assign(device, {
      markCommand: (color: string) => log.push(`mark ${color}`),
      type: async (text: string) => {
        log.push(`type ${text}`);
        Device.prototype.type.call(device, text);
      },
    });
    const script = "SENDLINE show clock\nWAITPROMPT\nSENDLINE show version";
    const runner = new ScriptRun("Show", "sw1", 3);
    runner.color = "#3FB950";
    await run(script, device, runner);
    expect(log).toEqual(["mark #3FB950", "type show clock\r", "mark #3FB950", "type show version\r"]);
  });
});
