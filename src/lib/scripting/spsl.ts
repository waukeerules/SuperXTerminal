/**
 * SPSL, SuperXPuTTY's script language, as its Scripting folder: a script is lines of COMMAND argument, run in order in
 * one tab; blank lines and lines starting with # (including #!/bin/spsl) are comments. The runner is
 * ctlPuttyPanel.RunScriptLines: a WAITFOR that times out, a line that fails or a cancelled PWDPROMPT stops the script,
 * so nothing is typed into the wrong prompt, and the status bar says why.
 */
import { isKeyName, canTypeKey, keySequence, parseSendKey } from "./keys";

/** One line, ready to run */
export type Step =
  | { kind: "type"; text: string; delayMs: number }
  | { kind: "waitfor"; text: string; timeoutMs: number }
  | { kind: "sleep"; ms: number }
  | { kind: "prompt"; message: string }
  | { kind: "pwdprompt" }
  | { kind: "open"; id: string };

/** How long WAITFOR waits when no /t: is given */
const DEFAULT_WAITFOR_MS = 30_000;
/** How often WAITFOR looks at the output */
const WAITFOR_POLL_MS = 100;
/** SENDLINE's (and PROMPT's) pause after Enter */
const LINE_DELAY_MS = 50;

/** Each line feed (a button's \n, a line break in Send Commands) as Enter (CR); CR LF is one Enter */
export function lineFeedsAsEnter(text: string): string {
  return text.replace(/\r\n/g, "\r").replace(/\n/g, "\r");
}

/** A script's command lines: no blank or comment lines, CR dropped */
export function commandLines(script: string): string[] {
  return script
    .split("\n")
    .map((l) => l.replace(/\r$/, ""))
    .filter((l) => l.trim() !== "" && !l.trimStart().startsWith("#"));
}

/**
 * Parse one line, as SPSL.TryParseScriptLine and its handlers: null for comments, unknown commands (skipped, as
 * SuperXPuTTY logs and skips them) and keys SENDKEY can't name. Throws for a line that can't run.
 */
export function parseLine(raw: string): Step | null {
  const line = raw.trimStart();
  if (!line || line.startsWith("#")) return null;
  const space = line.indexOf(" ");
  const command = (space > 0 ? line.slice(0, space) : line.trimEnd()).toUpperCase();
  const args = space > 0 ? line.slice(space + 1).trimEnd() : "";
  switch (command) {
    case "SENDLINE":
      return { kind: "type", text: lineFeedsAsEnter(args) + "\r", delayMs: LINE_DELAY_MS };
    case "SENDCHAR":
      return { kind: "type", text: lineFeedsAsEnter(args), delayMs: 0 };
    case "SENDKEY": {
      const parsed = parseSendKey(args);
      return parsed ? { kind: "type", text: keySequence(parsed.key).repeat(parsed.repeat), delayMs: 0 } : null;
    }
    case "SLEEP": {
      const ms = /^\d+$/.test(args.trim()) ? parseInt(args, 10) : 0;
      if (ms <= 0) throw new Error("The argument passed to SLEEP is not valid. Must be a positive whole number.");
      return { kind: "sleep", ms };
    }
    case "WAITFOR": {
      const timed = /^\/t:(\d+)\s+(.*)$/i.exec(args);
      const text = timed ? timed[2] : args;
      if (!text) throw new Error("WAITFOR needs the text to wait for, e.g. WAITFOR Password:");
      return { kind: "waitfor", text, timeoutMs: timed ? parseInt(timed[1], 10) * 1000 : DEFAULT_WAITFOR_MS };
    }
    case "PROMPT":
      return { kind: "prompt", message: args };
    case "PWDPROMPT":
      return { kind: "pwdprompt" };
    case "OPENSESSION":
      return { kind: "open", id: args.trim() };
    case "CLOSESESSION":
      throw new Error("CLOSESESSION isn't implemented");
    default:
      return null;
  }
}

/** A tab a script runs in: SuperXTerminal's PageHost */
export interface ScriptTarget {
  /** Type text and key sequences into the session */
  type(text: string): Promise<void>;
  /** Make WAITFOR look only at output after now, or from the start of the cursor's line */
  markOutput(atLineStart: boolean): void;
  /** The output since the WAITFOR mark: finished lines, then the cursor's line */
  outputSinceMark(): string;
  /** Move the mark past a match */
  advanceMark(characters: number): void;
}

/** What a script needs from the window */
export interface ScriptHost {
  /** PROMPT and PWDPROMPT's question; null when cancelled */
  ask(message: string, secret: boolean): Promise<string | null>;
  /** OPENSESSION; false if there's no such session */
  openSession(id: string): boolean;
  /** A line for the status bar */
  status(text: string): void;
}

/** A running script: its progress for the status bar, and its Stop */
export class ScriptRun {
  line = 0;
  /** The current command word, or WAITFOR's whole line; never what's typed (it may be a secret) */
  step = "";
  private stopped = false;
  private wake: (() => void) | null = null;

  constructor(
    readonly name: string,
    readonly where: string,
    readonly lineCount: number,
  ) {}

  get isStopped(): boolean {
    return this.stopped;
  }

  /** Stop: a SLEEP or WAITFOR ends at once */
  stop(): void {
    this.stopped = true;
    this.wake?.();
  }

  /** Wait, returning early when stopped */
  pause(ms: number): Promise<void> {
    return new Promise((resolve) => {
      const timer = setTimeout(done, ms);
      this.wake = done;
      function done() {
        clearTimeout(timer);
        resolve();
      }
    });
  }
}

/**
 * Run a script in a tab, as ctlPuttyPanel.RunScript: `fromStart` (the session's own script) lets the first WAITFOR
 * see everything since the session started; otherwise (a button) it sees the cursor's line and after, so a prompt
 * already on screen matches but older output doesn't. Resolves when it ends; `run` can stop it.
 */
export async function runScript(target: ScriptTarget, host: ScriptHost, run: ScriptRun, script: string, fromStart: boolean): Promise<void> {
  const lines = commandLines(script);
  const where = `"${run.name}" in ${run.where}`;
  if (!fromStart) target.markOutput(true);
  for (let i = 0; i < lines.length; i++) {
    if (run.isStopped) break;
    const line = lines[i];
    run.line = i + 1;
    const keyword = line.trimStart().split(" ")[0].toUpperCase();
    run.step = keyword === "WAITFOR" ? line.trim() : keyword;
    try {
      const step = parseLine(line);
      if (!step) continue;
      switch (step.kind) {
        case "sleep":
          await run.pause(step.ms);
          break;
        case "waitfor": {
          if (!(await waitFor(target, run, step.text, step.timeoutMs))) {
            if (run.isStopped) break;
            host.status(`Script ${where} stopped: "${step.text}" didn't appear within ${Math.round(step.timeoutMs / 1000)} s`);
            return;
          }
          break;
        }
        case "type":
          await send(target, step.text, step.delayMs);
          break;
        case "prompt": {
          // Cancel sends an empty line, as SuperXPuTTY's InputBox did
          const answer = (await host.ask(step.message, false)) ?? "";
          await send(target, lineFeedsAsEnter(answer) + "\r", LINE_DELAY_MS);
          break;
        }
        case "pwdprompt": {
          const answer = await host.ask("Insert your password", true);
          if (answer === null) {
            host.status(`Script ${where} stopped at line ${i + 1}: you cancelled the password prompt`);
            return;
          }
          await send(target, lineFeedsAsEnter(answer) + "\r", LINE_DELAY_MS);
          break;
        }
        case "open":
          host.openSession(step.id);
          break;
      }
    } catch (error) {
      host.status(`Script ${where} stopped at line ${i + 1}: ${error instanceof Error ? error.message : String(error)}`);
      return;
    }
  }
  host.status(run.isStopped ? `Script ${where} stopped by you at line ${run.line} of ${run.lineCount}` : `Script ${where} finished`);

  /** Type, marking the output first so a following WAITFOR looks at what comes after */
  async function send(t: ScriptTarget, text: string, delayMs: number): Promise<void> {
    t.markOutput(false);
    await t.type(text);
    if (delayMs > 0) await run.pause(delayMs);
  }
}

/** Wait until text appears in the output since the mark (exactly, case-sensitive); false on timeout or Stop */
async function waitFor(target: ScriptTarget, run: ScriptRun, text: string, timeoutMs: number): Promise<boolean> {
  const giveUp = Date.now() + timeoutMs;
  while (Date.now() < giveUp && !run.isStopped) {
    const shown = target.outputSinceMark();
    const at = shown.indexOf(text);
    if (at >= 0) {
      target.advanceMark(at + text.length);
      return true;
    }
    await run.pause(WAITFOR_POLL_MS);
  }
  return false;
}

/** One problem Check script finds */
export interface Issue {
  line: number;
  error: boolean;
  message: string;
}

/** The commands, for the checker's suggestions; SENDBREAK and CLOSESESSION are listed but can't run */
export const COMMANDS = ["SENDLINE", "SENDCHAR", "SENDKEY", "SLEEP", "WAITFOR", "PROMPT", "PWDPROMPT", "OPENSESSION"];
const UNAVAILABLE = ["SENDBREAK", "CLOSESESSION"];

/**
 * Check a script without running it, as SpslChecker: unknown commands (with a suggestion for a near miss), missing
 * or malformed arguments, unknown SENDKEY keys, sessions that don't exist, and {key} placeholders with no value
 */
export function checkScript(script: string, sessionExists: (id: string) => boolean, missingPlaceholders: (text: string) => string[]): Issue[] {
  const issues: Issue[] = [];
  const lines = script.replace(/\r/g, "").split("\n");
  lines.forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#")) return;
    const space = line.indexOf(" ");
    const command = (space > 0 ? line.slice(0, space) : line).toUpperCase();
    const args = space > 0 ? line.slice(space + 1).trim() : "";
    const add = (message: string, error = true) => issues.push({ line: i + 1, error, message });
    if (!COMMANDS.includes(command) && !UNAVAILABLE.includes(command)) {
      const suggestion = suggest(command);
      // shell text pasted into a script is the usual cause, so show how to type it
      add(`"${line.slice(0, space > 0 ? space : undefined)}" isn't an SPSL command` +
        (suggestion ? `; did you mean ${suggestion}?` : `. Lines start with a command; to type this line, write SENDLINE ${line}`));
      return;
    }
    const problem = checkArguments(command, args, sessionExists);
    if (problem) add(problem.replace(/^!/, ""), !problem.startsWith("!"));
    // SENDKEY's braces are key names, not placeholders
    if (command !== "SENDKEY") {
      for (const missing of missingPlaceholders(args)) add(`${missing} has no value in Tools > Environment Variables, so it's typed as it is.`, false);
    }
  });
  return issues;
}

/** A command's arguments: null if fine, an error, or a warning starting with "!" */
function checkArguments(command: string, args: string, sessionExists: (id: string) => boolean): string | null {
  switch (command) {
    case "SENDCHAR":
      return args ? null : "SENDCHAR needs the text to type, e.g. SENDCHAR y";
    case "SENDKEY":
      return checkKey(args);
    case "SLEEP": {
      const ms = /^\d+$/.test(args) ? parseInt(args, 10) : 0;
      if (ms <= 0) return "SLEEP needs a number of milliseconds, e.g. SLEEP 500";
      return ms > 60000 ? `!SLEEP ${ms} waits ${Math.floor(ms / 1000)} seconds.` : null;
    }
    case "WAITFOR": {
      let text = args;
      if (/^\/t:/i.test(args)) {
        const timed = /^\/t:(\d+)\s+(.*)$/i.exec(args);
        if (!timed) return "WAITFOR's timeout is /t: and a number of seconds, then the text, e.g. WAITFOR /t:10 Password:";
        text = timed[2];
      }
      return text.trim() ? null : "WAITFOR needs the text to wait for, e.g. WAITFOR Password:";
    }
    case "OPENSESSION":
      if (!args) return "OPENSESSION needs a session ID, e.g. OPENSESSION Lab/core-sw1";
      return sessionExists(args) ? null : `!There's no saved session with the ID "${args}"; the line will do nothing.`;
    case "SENDBREAK":
      return "SENDBREAK isn't available: a Break isn't a key, so a script can't send one; this line would do nothing.";
    case "CLOSESESSION":
      return "CLOSESESSION isn't available: SPSL lists it but doesn't implement it, so the script would stop here.";
    default:
      return null;
  }
}

/** A SENDKEY argument: one key name or character, with optional ^ % + */
function checkKey(args: string): string | null {
  if (!args) return "SENDKEY needs a key, e.g. SENDKEY ENTER or SENDKEY ^C";
  const key = args.replace(/^[\^%+]+/, "");
  if (key === "{{}" || key === "{}}") return null;
  if (key === "{" || key === "}") return `SENDKEY ${args}: braces are SENDKEY's own syntax; to press one, write {{} or {}}`;
  const braced = /^\{([^}\s]+)(\s+\d+)?\}$/.exec(key);
  const name = braced ? braced[1] : key;
  if (!braced && key.length > 1 && key.startsWith("{")) return `SENDKEY ${args}: a key in braces is written {NAME} or {NAME count}, e.g. {TAB 3}`;
  if (!name) return `SENDKEY ${args}: ^, % and + are Ctrl, Alt and Shift, so a key must follow; to type the character itself, use SENDCHAR ${args}`;
  if (name.length === 1) return canTypeKey(name) ? null : `SENDKEY ${args}: no key on this keyboard types "${name}"; use SENDCHAR to type it.`;
  if (isKeyName(name)) return null;
  return `SENDKEY presses one key; "${name}" isn't a key name. Use ENTER, TAB, ESC, UP, DOWN, F1-F16 and so on (see "Keys for SENDKEY"), or a single character. To type text, use SENDCHAR or SENDLINE.`;
}

/** The command closest to a misspelt one, if close enough */
function suggest(word: string): string | null {
  let best: { name: string; distance: number } | null = null;
  for (const name of COMMANDS) {
    const distance = levenshtein(word, name);
    if (distance <= Math.max(2, Math.floor(name.length / 3)) && (!best || distance < best.distance)) best = { name, distance };
  }
  return best?.name ?? null;
}

function levenshtein(a: string, b: string): number {
  const d = Array.from({ length: a.length + 1 }, (_, i) => [i, ...Array(b.length).fill(0)]);
  for (let j = 0; j <= b.length; j++) d[0][j] = j;
  for (let i = 1; i <= a.length; i++)
    for (let j = 1; j <= b.length; j++)
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
  return d[a.length][b.length];
}
