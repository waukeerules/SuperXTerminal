// The page's half of a Rhai script (rhai.ts), run end to end in a stand-in tab: sends, waits that see what the device
// prints after them (and not what came before), what a regular expression matched, a wait that times out, a session
// that drops mid-wait, Stop reaching the core, and an answer to a call that comes before the core has said its run id.
// The core is stood in for by `FakeCore`, which plays a script as the Rust thread would: one call at a time, each
// waiting for the page's answer. The tab is a small stand-in switch, as in spsl.test.ts; the window, a host that
// records the status lines.
import { describe, expect, it } from "vitest";
import { ScriptRun, type ScriptHost, type ScriptTarget } from "./spsl";
import { runRhai, type RhaiCore, type RhaiEvent } from "./rhai";

/** A stand-in switch: what it prints after each thing typed, in turn, and what it has been sent */
class Device implements ScriptTarget {
  output = "";
  mark = 0;
  typed: string[] = [];
  connected: number | null = 1;

  /** @param answers What the device prints after each thing typed, in order */
  constructor(private readonly answers: string[] = []) {}

  async type(text: string): Promise<void> {
    this.typed.push(text);
    const answer = this.answers.shift();
    if (answer !== undefined) setTimeout(() => (this.output += answer), 5);
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
  async sendBreak(): Promise<void> {}
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

/** What a stand-in script does: make calls (each answered by the page) and say things; it returns or throws */
type Play = (call: (name: string, args: Record<string, unknown>, line?: number) => Promise<unknown>, say: (text: string) => void) => Promise<void>;

/** The core as the Rust thread behaves: a script's calls one at a time, Done when it ends */
class FakeCore implements RhaiCore {
  stopped: number[] = [];
  private waiting = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();

  /** @param play The script @param startDelayMs How long `rhai_run` takes to answer with the run id */
  constructor(private readonly play: Play, private readonly startDelayMs = 0) {}

  async start(_source: string, _session: Record<string, string>, onEvent: (event: RhaiEvent) => void): Promise<number> {
    let next = 0;
    const call = (name: string, args: Record<string, unknown>, line = 1) =>
      new Promise<unknown>((resolve, reject) => {
        const id = ++next;
        this.waiting.set(id, { resolve, reject });
        onEvent({ kind: "call", id, line, call: name, args });
      });
    const say = (text: string) => onEvent({ kind: "status", line: 0, text });
    void this.play(call, say).then(
      () => onEvent({ kind: "done", stopped: false, error: null, line: 0 }),
      (e: Error) => onEvent(e.message === "stopped" ? { kind: "done", stopped: true, error: null, line: 1 } : { kind: "done", stopped: false, error: e.message, line: 1 }),
    );
    await new Promise((r) => setTimeout(r, this.startDelayMs));
    return 7;
  }

  reply(run: number, id: number, outcome: { value: unknown } | { error: string }): void {
    expect(run).toBe(7);
    const waiting = this.waiting.get(id)!;
    this.waiting.delete(id);
    if ("error" in outcome) waiting.reject(new Error(outcome.error));
    else waiting.resolve(outcome.value);
  }

  stop(run: number): void {
    this.stopped.push(run);
    for (const w of this.waiting.values()) w.reject(new Error("stopped"));
  }
}

/** Run a stand-in script in a device; its status lines */
async function run(core: FakeCore, device: Device, script = new ScriptRun("Script", "sw1", 3)): Promise<string[]> {
  const status: string[] = [];
  const host: ScriptHost = { ask: async () => "answer", openSession: () => true, status: (t) => status.push(t), save: async () => "", loadScript: async () => "" };
  await runRhai(device, host, script, "(the script)", false, {}, core);
  return status;
}

/** A wait for text, as the core sends it */
const waitFor = (source: string, timeoutMs: number | null = 1000, regex = false) => ({ patterns: [{ source, regex, anyCase: false }], timeoutMs });

describe("Rhai scripts in a tab", () => {
  it("types a line and waits for what the device prints after it, not the prompt already there", async () => {
    const device = new Device(["\r\nCisco IOS, uptime is 5 weeks\r\nsw1#"]);
    device.output = "old output sw1#";
    let found: unknown;
    const core = new FakeCore(async (call, say) => {
      await call("type", { text: "show version", enter: true });
      found = await call("wait", waitFor("uptime is (\\d+) weeks", 1000, true));
      say("done");
    });
    const status = await run(core, device);
    expect(device.typed).toEqual(["show version\r"]);
    expect(found).toEqual({ index: 0, groups: ["uptime is 5 weeks", "5"] });
    expect(status).toEqual(['Script "Script" in sw1: done', 'Script "Script" in sw1 finished']);
  });

  it("answers null when a wait's time runs out, so the script can choose what to do", async () => {
    let found: unknown = "unset";
    const core = new FakeCore(async (call) => {
      found = await call("wait", waitFor("never", 50));
    });
    await run(core, new Device());
    expect(found).toBeNull();
  });

  it("stops the script at its line when the session drops during a wait", async () => {
    const device = new Device();
    setTimeout(() => (device.connected = null), 30);
    const core = new FakeCore(async (call) => {
      await call("wait", waitFor("#", null));
    });
    expect(await run(core, device)).toEqual(['Script "Script" in sw1 stopped at line 1: the session ended']);
  });

  it("tells the core when Stop is pressed, which ends a wait that would go on for ever", async () => {
    const script = new ScriptRun("Script", "sw1", 3);
    const core = new FakeCore(async (call) => {
      await call("wait", waitFor("#", null));
    });
    setTimeout(() => script.stop(), 30);
    const status = await run(core, new Device(), script);
    expect(core.stopped).toEqual([7]);
    expect(status).toEqual(['Script "Script" in sw1 stopped by you at line 1 of 3']);
  });

  it("answers a call that comes before the core has said which run it is", async () => {
    const device = new Device();
    const core = new FakeCore(async (call) => {
      await call("type", { text: "a", enter: false });
      await call("type", { text: "b", enter: false });
    }, 30);
    expect(await run(core, device)).toEqual(['Script "Script" in sw1 finished']);
    expect(device.typed).toEqual(["a", "b"]);
  });
});

describe("a button's colour", () => {
  it("numbers each line a Rhai script types in its button's colour, marked just before it's typed", async () => {
    const device = new Device();
    const log: string[] = [];
    Object.assign(device, {
      markCommand: (color: string) => log.push(`mark ${color}`),
      type: async (text: string) => void log.push(`type ${text}`),
    });
    const core = new FakeCore(async (call) => {
      await call("type", { text: "show clock", enter: true });
      await call("type", { text: "q", enter: false });
    });
    const script = new ScriptRun("Show", "sw1", 2);
    script.color = "#E5534B";
    await run(core, device, script);
    expect(log).toEqual(["mark #E5534B", "type show clock\r", "mark #E5534B", "type q"]);
  });
});
