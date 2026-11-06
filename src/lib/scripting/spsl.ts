/**
 * SPSL, SuperXPuTTY's script language, as its Scripting folder, with SuperXTerminal's own additions: a script is lines
 * of COMMAND argument, run in one tab; blank lines and lines starting with # (including #!/bin/spsl) are comments.
 * docs/SPSL.md is the whole language; this file is its parser, compiler, runner and checker.
 *
 * SuperXPuTTY's commands (SENDLINE, SENDCHAR, SENDKEY, WAITFOR, SLEEP, PROMPT, PWDPROMPT, OPENSESSION, SENDBREAK) run
 * as ctlPuttyPanel.RunScriptLines did: a WAITFOR that times out, a line that fails or a cancelled PWDPROMPT stops the
 * script, so nothing is typed into the wrong prompt, and the status bar says why.
 *
 * SuperXTerminal's own, for scripts that must cope with a device that doesn't always ask the same things:
 * - WAITFOR's options before the text: /i (any case), /re (a regular expression), /t:seconds, and /else:continue or
 *   /else:label for what a timeout does instead of stopping.
 * - EXPECT … CASE … END, waiting for whichever of several things comes first, as expect(1); AGAIN waits again.
 * - Labels (a line ":name"), GOTO name, STOP [message].
 * - Variables: SET name value, PROMPT /var:name and PWDPROMPT /var:name, and what a wait matched ({0}, a regular
 *   expression's groups {1}…{9} and (?<name>…) groups), all written {name} and filled in as each line runs.
 * - FAILON text: from then on, any wait that sees it stops the script (a device rejecting a command).
 * - WAITPROMPT (the device's prompt, without a regular expression) and WAITQUIET ms (output has stopped).
 * - ECHO text (the status bar), SAVE path (what the last command printed, to a file), CALL name (a saved script).
 * - Stepping: a run can pause before each line until Next line (the status bar).
 * - REPEAT n … END ({repeat} is the round), LIMIT 10m (the whole script's time), and a loop that jumps 10,000 times
 *   without waiting stops, so nothing types forever.
 * - The tab's session's values ({host_address}, {session_name}…: SESSION_VARIABLES), given when the script starts.
 * - CHECK name [/not] pattern, watched in all the output from its line on, and REQUIRE [names], which stops the script
 *   (or with /else: goes elsewhere) when one isn't met: prerequisites defined early, judged after the show commands.
 * - PAGER pattern [/key:NAME]: from its line on, a wait that sees a pager prompt (--More--) presses Space (or the key)
 *   and keeps waiting, once per prompt.
 * - /idle:seconds on a wait: it ends when the output stops changing for that long (a stalled copy), whatever /t: says.
 * - COLLECT name pattern (every match in the last command's output, as a list), FOREACH item IN list … END, and
 *   IF a == b / != / < / > / ~ /regex/ … ELSE … END.
 *
 * A script is compiled first (`compile`): structure that can't run (an EXPECT without END, a GOTO to no label) stops
 * it before anything is typed; a single line that can't run stops it when it's reached, as before. A script belongs to
 * the connection it started on: it stops when the session ends or reconnects.
 */
import { isKeyName, canTypeKey, keySequence, parseSendKey } from "./keys";

/** Where a wait's text was found, and what it saved: the whole match, its numbered groups and named groups */
export interface Match {
  at: number;
  length: number;
  /** The match ({0}), then a regular expression's groups ({1}, {2}…; "" for one that didn't take part) */
  groups: string[];
  /** A regular expression's (?<name>…) groups, by name */
  named: Record<string, string>;
}

/** Something a wait looks for: where it first appears in some output, and how the line wrote it */
export interface Matcher {
  /** Where it first appears in `text` (a match of no characters doesn't count), or null */
  find(text: string): Match | null;
  /** As the script wrote it, for the status bar and messages */
  shown: string;
}

/** What a wait does when its time runs out: stop the script (SPSL's own), go on with the next line, or go to a label */
export type OnTimeout = { kind: "stop" } | { kind: "continue" } | { kind: "label"; label: string };

/** A pattern as a line wrote it, made into a Matcher as the line runs (its {variables} filled in then) */
export interface PatternSpec {
  regex: boolean;
  source: string;
  anyCase: boolean;
}

/** One line, ready to run; text is as written, its {variables} filled in when it runs */
export type Step =
  | { kind: "type"; text: string; enter: boolean }
  | { kind: "keys"; text: string }
  | { kind: "waitfor"; spec: PatternSpec; timeoutMs: number; idleMs: number; onTimeout: OnTimeout }
  | { kind: "waitprompt"; timeoutMs: number; idleMs: number; onTimeout: OnTimeout }
  | { kind: "waitquiet"; ms: number; timeoutMs: number; onTimeout: OnTimeout }
  | { kind: "sleep"; ms: number }
  | { kind: "prompt"; message: string; variable: string | null }
  | { kind: "pwdprompt"; variable: string | null }
  | { kind: "open"; id: string }
  | { kind: "break" }
  | { kind: "goto"; label: string }
  | { kind: "stop"; message: string }
  | { kind: "set"; name: string; value: string }
  | { kind: "failon"; spec: PatternSpec | null }
  | { kind: "echo"; text: string }
  | { kind: "save"; path: string; append: boolean }
  | { kind: "call"; name: string }
  | { kind: "limit"; ms: number }
  | { kind: "check"; name: string; spec: PatternSpec; not: boolean }
  | { kind: "require"; names: string[]; onElse: OnTimeout }
  | { kind: "pager"; spec: PatternSpec | null; keys: string }
  | { kind: "collect"; name: string; spec: PatternSpec };

/** An IF's test: two sides and how they compare, or one side alone (true when it isn't empty) */
export interface Condition {
  left: string;
  /** "==", "!=", "<", ">", "<=", ">=", "~" (matches a regular expression), "!~", or "" for one side alone */
  op: string;
  right: string;
}

/** How long a wait waits when no /t: is given */
const DEFAULT_WAITFOR_MS = 30_000;
/** How often a wait looks at the output */
const WAITFOR_POLL_MS = 100;
/** SENDLINE's (and PROMPT's) pause after Enter */
export const LINE_DELAY_MS = 50;
/** How deep CALLs may go: a script calling itself stops there rather than forever */
const MAX_CALL_DEPTH = 8;
/** How many times a script may go back (GOTO, AGAIN, REPEAT) without waiting for anything before it's stopped as a runaway */
const MAX_JUMPS_WITHOUT_WAITING = 10_000;
/** The most rounds a REPEAT may have */
const MAX_REPEAT = 100_000;

/**
 * The tab's session's values a script can use as {variables}, given when it starts (SET can change them): its own
 * host and port (as {host_address} and {host_port} in a username), its name, id and folder, and the username, profile
 * and connection type it was saved with. Named session_… so they can't be taken for an Environment Variable of yours.
 */
export const SESSION_VARIABLES = [
  "host_address", "host_port", "session_name", "session_id", "session_folder", "session_username", "session_profile", "session_protocol",
];

/**
 * The prompt WAITPROMPT waits for, on the output's last line (the cursor's): a line starting with something other
 * than a space and ending in > # $ or %, and perhaps one space: sw1>, sw1#, sw1(config-if)#, user@host:~$, [a@b] >.
 * SET prompt /regex/ … (a variable named prompt) puts another in its place.
 */
export const PROMPT_LINE = /^\S.{0,120}?[>#$%]\s?$/;

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
 * Text to wait for, exactly or in any case
 * @param text What to find
 * @param anyCase Whether case doesn't matter (/i)
 */
export function literal(text: string, anyCase: boolean): Matcher {
  const wanted = anyCase ? text.toLowerCase() : text;
  return {
    shown: text,
    find(output) {
      const at = (anyCase ? output.toLowerCase() : output).indexOf(wanted);
      if (at < 0 || wanted.length === 0) return null;
      return { at, length: wanted.length, groups: [output.slice(at, at + wanted.length)], named: {} };
    },
  };
}

/**
 * A regular expression to wait for, ^ and $ at each line's start and end (the output is many lines)
 * @param source The expression, as written
 * @param anyCase Whether case doesn't matter (/i, or the i of /…/i)
 * @throws when it isn't a regular expression, saying why
 */
export function pattern(source: string, anyCase: boolean): Matcher {
  let expression: RegExp;
  try {
    expression = new RegExp(source, anyCase ? "im" : "m");
  } catch (error) {
    throw new Error(`"${source}" isn't a regular expression: ${error instanceof Error ? error.message.replace(/^Invalid regular expression: /, "") : String(error)}`);
  }
  return {
    shown: `/${source}/${anyCase ? "i" : ""}`,
    find(output) {
      // a match of no characters (x*) would match everywhere, so it's passed over
      const global = new RegExp(expression.source, expression.flags + "g");
      for (const match of output.matchAll(global)) {
        if (match[0].length === 0) continue;
        const named: Record<string, string> = {};
        for (const [name, value] of Object.entries(match.groups ?? {})) named[name] = value ?? "";
        return { at: match.index ?? 0, length: match[0].length, groups: match.map((g) => g ?? ""), named };
      }
      return null;
    },
  };
}

/**
 * The device's prompt on the output's last line (the cursor's), for WAITPROMPT
 * @param custom A regular expression in its place (SET prompt …), or "" for PROMPT_LINE
 */
export function promptMatcher(custom: string): Matcher {
  const line = custom ? new RegExp(custom, "m") : PROMPT_LINE;
  return {
    shown: "the prompt",
    find(output) {
      const start = output.lastIndexOf("\n") + 1;
      const last = output.slice(start);
      if (!last || !line.test(last)) return null;
      return { at: start, length: last.length, groups: [last], named: {} };
    },
  };
}

/** A variable's name: a letter or _ first, so {1} (a group) and a regular expression's {2,3} are never one */
const VARIABLE_NAME = /^[A-Za-z_]\w*$/;

/** Fill {name} in text with the variables' values; a name with no value stays as written */
export function expand(text: string, variables: ReadonlyMap<string, string>): string {
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => variables.get(name.toLowerCase()) ?? whole);
}

/**
 * Fill {name} in a regular expression with the variables' values, each taken as plain text (its . or ( match
 * themselves); only names starting with a letter or _, so a quantifier ({2}, {2,3}) stays one
 */
function expandPattern(source: string, variables: ReadonlyMap<string, string>): string {
  return source.replace(/\{([A-Za-z_]\w*)\}/g, (whole, name: string) => {
    const value = variables.get(name.toLowerCase());
    return value === undefined ? whole : value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  });
}

/** A pattern made into its Matcher, with the variables as they are now */
function makeMatcher(spec: PatternSpec, variables: ReadonlyMap<string, string>): Matcher {
  return spec.regex ? pattern(expandPattern(spec.source, variables), spec.anyCase) : literal(expand(spec.source, variables), spec.anyCase);
}

/** Check a pattern can be made, before the script runs: a regular expression must be one, with its variables as "x" */
function checkPattern(spec: PatternSpec): void {
  if (spec.regex) pattern(spec.source.replace(/\{([A-Za-z_]\w*)\}/g, "x"), spec.anyCase);
}

/** The options a wait takes before its text, and the text after them */
interface WaitOptions {
  timeoutMs: number;
  anyCase: boolean;
  regex: boolean;
  append: boolean;
  variable: string | null;
  /** CHECK /not: met when the pattern does NOT appear */
  not: boolean;
  /** PAGER /key:NAME: what to press, as SENDKEY writes it */
  key: string | null;
  /** /idle:seconds: the wait ends when the output stops changing for this long (0: no such limit) */
  idleMs: number;
  onTimeout: OnTimeout;
  /** What follows the options */
  rest: string;
}

/**
 * Read a line's options from the start of its arguments: /t:seconds, /i, /re, /else:continue|stop|label, /var:name and
 * /append, in any order, each followed by a space (or the end). Something else starting with / is the text itself.
 * @throws for /t: without a number, /else: without what to do, or /var: without a name
 */
function waitOptions(args: string): WaitOptions {
  const options: WaitOptions = {
    timeoutMs: DEFAULT_WAITFOR_MS, anyCase: false, regex: false, append: false, variable: null, not: false, key: null, idleMs: 0,
    onTimeout: { kind: "stop" }, rest: args,
  };
  for (;;) {
    const rest = options.rest;
    const option = /^\/([a-z]+)(?::(\S*))?(?:\s+|$)/i.exec(rest);
    if (!option) return options;
    const name = option[1].toLowerCase();
    const value = option[2];
    if (name === "t") {
      if (!value || !/^\d+$/.test(value)) throw new Error("A timeout is /t: and a number of seconds, e.g. WAITFOR /t:10 Password:");
      options.timeoutMs = parseInt(value, 10) * 1000;
    } else if (name === "i" && value === undefined) {
      options.anyCase = true;
    } else if (name === "re" && value === undefined) {
      options.regex = true;
    } else if (name === "append" && value === undefined) {
      options.append = true;
    } else if (name === "else") {
      if (!value) throw new Error("/else: says what a timeout does: /else:continue, /else:stop or /else:a-label");
      const what = value.toLowerCase();
      options.onTimeout = what === "continue" ? { kind: "continue" } : what === "stop" ? { kind: "stop" } : { kind: "label", label: value };
    } else if (name === "idle") {
      if (!value || !/^\d+$/.test(value) || value === "0") throw new Error("/idle: is how many seconds without new output end the wait, e.g. WAITFOR /t:1800 /idle:60 bytes copied");
      options.idleMs = parseInt(value, 10) * 1000;
    } else if (name === "not" && value === undefined) {
      options.not = true;
    } else if (name === "key") {
      if (!value || checkKey(value) !== null) throw new Error("/key: is a key as SENDKEY writes it, e.g. PAGER /key:ENTER --More--");
      options.key = value;
    } else if (name === "var") {
      if (!value || !VARIABLE_NAME.test(value)) throw new Error("/var: names a variable: a letter or _, then letters, digits or _, e.g. PROMPT /var:vlan Which VLAN?");
      options.variable = value.toLowerCase();
    } else {
      // not an option (a path, say): the text starts here
      return options;
    }
    options.rest = rest.slice(option[0].length);
  }
}

/** A wait's pattern from its options and text */
function specOf(options: WaitOptions): PatternSpec {
  return { regex: options.regex, source: options.rest, anyCase: options.anyCase };
}

/**
 * Parse one line, as SPSL.TryParseScriptLine and its handlers: null for comments, unknown commands (skipped, as
 * SuperXPuTTY logs and skips them) and keys SENDKEY can't name. Throws for a line that can't run. EXPECT, CASE, AGAIN,
 * END and labels are blocks, which `compile` reads.
 */
export function parseLine(raw: string): Step | null {
  const line = raw.trimStart();
  if (!line || line.startsWith("#")) return null;
  const space = line.indexOf(" ");
  const command = (space > 0 ? line.slice(0, space) : line.trimEnd()).toUpperCase();
  const args = space > 0 ? line.slice(space + 1).trimEnd() : "";
  switch (command) {
    case "SENDLINE":
      return { kind: "type", text: args, enter: true };
    case "SENDCHAR":
      return { kind: "type", text: args, enter: false };
    case "SENDKEY": {
      const parsed = parseSendKey(args);
      return parsed ? { kind: "keys", text: keySequence(parsed.key).repeat(parsed.repeat) } : null;
    }
    case "SLEEP": {
      const ms = /^\d+$/.test(args.trim()) ? parseInt(args, 10) : 0;
      if (ms <= 0) throw new Error("The argument passed to SLEEP is not valid. Must be a positive whole number.");
      return { kind: "sleep", ms };
    }
    case "WAITFOR": {
      const options = waitOptions(args);
      if (!options.rest) throw new Error("WAITFOR needs the text to wait for, e.g. WAITFOR Password:");
      const spec = specOf(options);
      checkPattern(spec);
      return { kind: "waitfor", spec, timeoutMs: options.timeoutMs, idleMs: options.idleMs, onTimeout: options.onTimeout };
    }
    case "WAITPROMPT": {
      const options = waitOptions(args);
      if (options.rest) throw new Error("WAITPROMPT takes only /t: and /else:; to wait for a prompt of your own, SET prompt to a regular expression first");
      return { kind: "waitprompt", timeoutMs: options.timeoutMs, idleMs: options.idleMs, onTimeout: options.onTimeout };
    }
    case "WAITQUIET": {
      const options = waitOptions(args);
      const ms = /^\d+$/.test(options.rest) ? parseInt(options.rest, 10) : 0;
      if (ms <= 0) throw new Error("WAITQUIET needs how long the output must stay quiet, in milliseconds, e.g. WAITQUIET 1500");
      return { kind: "waitquiet", ms, timeoutMs: options.timeoutMs, onTimeout: options.onTimeout };
    }
    case "PROMPT": {
      const options = waitOptions(args);
      return { kind: "prompt", message: options.rest, variable: options.variable };
    }
    case "PWDPROMPT": {
      const options = waitOptions(args);
      return { kind: "pwdprompt", variable: options.variable };
    }
    case "OPENSESSION":
      return { kind: "open", id: args.trim() };
    case "SENDBREAK":
      return { kind: "break" };
    case "GOTO":
      if (!args.trim()) throw new Error("GOTO needs a label, e.g. GOTO login, for a line :login");
      return { kind: "goto", label: args.trim() };
    case "STOP":
      return { kind: "stop", message: args.trim() };
    case "SET": {
      const set = /^(\S+)(?:\s(.*))?$/.exec(args.trim());
      if (!set || !VARIABLE_NAME.test(set[1])) throw new Error("SET names a variable, then its value: SET vlan 20 (a name starts with a letter or _)");
      return { kind: "set", name: set[1].toLowerCase(), value: set[2] ?? "" };
    }
    case "FAILON": {
      if (args.trim().toUpperCase() === "OFF") return { kind: "failon", spec: null };
      const options = waitOptions(args);
      if (!options.rest) throw new Error('FAILON needs what the device says when it refuses, e.g. FAILON % Invalid input; FAILON OFF forgets them');
      const spec = specOf(options);
      checkPattern(spec);
      return { kind: "failon", spec };
    }
    case "ECHO":
      return { kind: "echo", text: args };
    case "SAVE": {
      const options = waitOptions(args);
      if (!options.rest) throw new Error("SAVE needs a file: SAVE show-run.txt (in Documents/SuperXTerminal/output) or a full path");
      return { kind: "save", path: options.rest, append: options.append };
    }
    case "CALL":
      if (!args.trim()) throw new Error("CALL needs a saved script's name, e.g. CALL login");
      return { kind: "call", name: args.trim() };
    case "LIMIT": {
      const limit = /^(\d+)\s*(s|m|h)?$/i.exec(args.trim());
      if (!limit || parseInt(limit[1], 10) <= 0) throw new Error("LIMIT is how long the whole script may run: LIMIT 90s, LIMIT 10m, LIMIT 1h (a number alone is seconds)");
      const unit = (limit[2] ?? "s").toLowerCase();
      return { kind: "limit", ms: parseInt(limit[1], 10) * (unit === "h" ? 3_600_000 : unit === "m" ? 60_000 : 1000) };
    }
    case "CHECK": {
      const named = /^([A-Za-z_]\w*)\s+(.+)$/.exec(args.trim());
      if (!named) throw new Error("CHECK names a prerequisite, then what to look for: CHECK ios /re Version 17\\. (REQUIRE ios judges it)");
      const options = waitOptions(named[2]);
      if (!options.rest) throw new Error(`CHECK ${named[1]} needs what to look for after its options`);
      const spec = specOf(options);
      checkPattern(spec);
      return { kind: "check", name: named[1].toLowerCase(), spec, not: options.not };
    }
    case "REQUIRE": {
      const options = waitOptions(args);
      const names = options.rest.split(/[\s,]+/).filter(Boolean).map((n) => n.toLowerCase());
      const bad = names.find((n) => !VARIABLE_NAME.test(n));
      if (bad) throw new Error(`REQUIRE ${bad}: a CHECK's name starts with a letter or _`);
      return { kind: "require", names, onElse: options.onTimeout };
    }
    case "PAGER": {
      if (args.trim().toUpperCase() === "OFF") return { kind: "pager", spec: null, keys: "" };
      const options = waitOptions(args);
      if (!options.rest) throw new Error("PAGER needs the pager's prompt, e.g. PAGER --More--; PAGER OFF forgets them");
      const spec = specOf(options);
      checkPattern(spec);
      const key = parseSendKey(options.key ?? "SPACE")!;
      return { kind: "pager", spec, keys: keySequence(key.key).repeat(key.repeat) };
    }
    case "COLLECT": {
      const named = /^([A-Za-z_]\w*)\s+(.+)$/.exec(args.trim());
      if (!named) throw new Error("COLLECT names a list, then what to find in the last command's output: COLLECT bins /re (\\S+\\.bin)$");
      const options = waitOptions(named[2]);
      if (!options.rest) throw new Error(`COLLECT ${named[1]} needs what to find after its options`);
      const spec = specOf(options);
      checkPattern(spec);
      return { kind: "collect", name: named[1].toLowerCase(), spec };
    }
    case "CLOSESESSION":
      throw new Error("CLOSESESSION isn't implemented");
    default:
      return null;
  }
}

/**
 * An IF's test: `left op right`, the operator with a space each side, or one side alone
 * @throws for nothing to test, or a ~ / !~ whose regular expression isn't one
 */
export function parseCondition(text: string): Condition {
  const written = text.trim();
  if (!written) throw new Error("IF needs a test: IF {count} > 0, IF {file} != {boot}, IF {version} ~ /^17\\./");
  const compared = /^(.*?)\s+(==|!=|<=|>=|<|>|!~|~)\s+(.*)$/.exec(written);
  if (!compared) return { left: written, op: "", right: "" };
  const condition = { left: compared[1], op: compared[2], right: compared[3] };
  if (condition.op.endsWith("~")) conditionRegex(condition.right.replace(/\{([A-Za-z_]\w*)\}/g, "x"));
  return condition;
}

/** The regular expression after ~ or !~: /expression/ (i after it for any case), or the text as one */
function conditionRegex(written: string): RegExp {
  const slashed = /^\/(.*)\/(i?)$/.exec(written.trim());
  const [source, flags] = slashed ? [slashed[1], slashed[2]] : [written.trim(), ""];
  try {
    return new RegExp(source, flags);
  } catch (error) {
    throw new Error(`"${source}" isn't a regular expression: ${message(error).replace(/^Invalid regular expression: /, "")}`);
  }
}

/**
 * Whether an IF's test holds with the variables as they are: {variables} filled in (one with no value is ""), sides
 * compared as numbers when both are, else as text
 */
export function holds(condition: Condition, variables: ReadonlyMap<string, string>): boolean {
  const value = (text: string) => expand(text, variables).replace(/\{\w+\}/g, "").trim();
  const left = value(condition.left);
  if (!condition.op) return left !== "";
  if (condition.op.endsWith("~")) {
    const matched = conditionRegex(expandPattern(condition.right, variables)).test(left);
    return condition.op === "~" ? matched : !matched;
  }
  const right = value(condition.right);
  const numbers = /^-?\d+(\.\d+)?$/.test(left) && /^-?\d+(\.\d+)?$/.test(right);
  const [a, b]: (string | number)[] = numbers ? [parseFloat(left), parseFloat(right)] : [left, right];
  switch (condition.op) {
    case "==": return a === b;
    case "!=": return a !== b;
    case "<": return a < b;
    case ">": return a > b;
    case "<=": return a <= b;
    default: return a >= b;
  }
}

/**
 * A CASE's pattern: "text" (quoted, \" and \\ inside), /regular expression/ (i after it for any case), or the rest of
 * the line as text
 * @param text The CASE's argument
 * @param anyCase EXPECT's /i, which every case takes
 * @throws for a CASE with nothing to wait for, an unclosed quote or a bad regular expression
 */
export function casePattern(text: string, anyCase: boolean): PatternSpec {
  const written = text.trim();
  if (!written) throw new Error('CASE needs what to wait for: "text", /regular expression/, or text to the end of the line');
  if (written.startsWith('"')) {
    const quoted = /^"((?:[^"\\]|\\.)*)"$/.exec(written);
    if (!quoted) throw new Error(`CASE ${written}: a quoted text ends with " and nothing after it (\\" for a quote inside)`);
    return { regex: false, source: quoted[1].replace(/\\(.)/g, "$1"), anyCase };
  }
  const regex = /^\/(.*)\/(i?)$/.exec(written);
  const spec = regex ? { regex: true, source: regex[1], anyCase: anyCase || regex[2] === "i" } : { regex: false, source: written, anyCase };
  checkPattern(spec);
  return spec;
}

/** A pattern as it's shown in messages */
function shownSpec(spec: PatternSpec): string {
  return spec.regex ? `/${spec.source}/${spec.anyCase ? "i" : ""}` : spec.source;
}

/** What one case of a wait looks for: a pattern, or the prompt */
type CaseLook = { pattern: PatternSpec } | { prompt: true };

/** One instruction of a compiled script, with the script line it came from (1-based, as the editor numbers them) */
export type Instruction =
  | { op: "step"; step: Exclude<Step, { kind: "waitfor" | "waitprompt" | "waitquiet" | "goto" | "stop" | "require" }>; line: number; shown: string }
  | { op: "wait"; cases: { look: CaseLook; to: number }[]; timeoutMs: number; idleMs: number; onTimeout: OnTimeout; next: number; line: number; shown: string }
  | { op: "quiet"; ms: number; timeoutMs: number; onTimeout: OnTimeout; next: number; line: number; shown: string }
  | { op: "jump"; to: number; label?: string; line: number; forward?: boolean }
  | { op: "if"; condition: Condition; otherwise: number; line: number; shown: string }
  | { op: "foreach"; variable: string; source: string; end: number; line: number; shown: string }
  | { op: "next"; start: number; line: number }
  | { op: "repeat"; count: number; line: number; shown: string }
  | { op: "loop"; start: number; line: number }
  | { op: "require"; names: string[]; onElse: OnTimeout; next: number; line: number; shown: string }
  | { op: "stop"; message: string; line: number }
  | { op: "fail"; message: string; line: number };

/** A script compiled: its instructions, labels, and what can't run (`issues`, which the checker shows too) */
export interface Program {
  instructions: Instruction[];
  /** Each label's instruction, by lower-case name */
  labels: Map<string, number>;
  /** Problems in the script's structure and lines: errors stop it before it starts */
  issues: Issue[];
}

/** An EXPECT being read: its wait, the jumps that end its cases (pointed past END once it's found) */
interface OpenExpect {
  kind: "expect";
  wait: Extract<Instruction, { op: "wait" }>;
  ends: Extract<Instruction, { op: "jump" }>[];
  line: number;
  anyCase: boolean;
}

/** A REPEAT being read: its instruction's place, which its END loops back to */
interface OpenRepeat {
  kind: "repeat";
  start: number;
  line: number;
}

/** A FOREACH being read: its instruction, which its END loops back to and which skips past END for an empty list */
interface OpenForeach {
  kind: "foreach";
  start: Extract<Instruction, { op: "foreach" }>;
  at: number;
  line: number;
}

/** An IF being read: its instruction (where a false test goes is set at ELSE or END), and ELSE's jump past END */
interface OpenIf {
  kind: "if";
  test: Extract<Instruction, { op: "if" }>;
  otherwise: Extract<Instruction, { op: "jump" }> | null;
  line: number;
}

/** The names blocks have, for messages */
const BLOCK_NAMES = { expect: "EXPECT", repeat: "REPEAT", foreach: "FOREACH", if: "IF" } as const;

/**
 * Compile a script: each line into instructions, EXPECT's cases into one wait with a jump to each, AGAIN into a jump
 * back to its EXPECT, the end of each case into a jump past END, and labels and GOTOs joined up
 */
export function compile(script: string): Program {
  const instructions: Instruction[] = [];
  const labels = new Map<string, number>();
  const issues: Issue[] = [];
  const open: (OpenExpect | OpenRepeat | OpenForeach | OpenIf)[] = [];
  /** The innermost open EXPECT, which CASE and AGAIN belong to */
  const innermostExpect = () => [...open].reverse().find((b): b is OpenExpect => b.kind === "expect");
  /** GOTOs and /else: labels, to join up once every label is known */
  const pending: { line: number; label: string; set: (to: number) => void }[] = [];
  const add = (line: number, message: string, error = true) => issues.push({ line, error, message });
  /** Whether a line can go here: not between an EXPECT and its first CASE (where it would have nowhere to go) */
  const inCase = () => {
    const block = open[open.length - 1];
    return !block || block.kind !== "expect" || block.wait.cases.length > 0;
  };
  const needLabel = (line: number, timeout: OnTimeout) => {
    if (timeout.kind === "label") pending.push({ line, label: timeout.label, set: () => {} });
  };

  script.replace(/\r/g, "").split("\n").forEach((raw, index) => {
    const line = index + 1;
    const text = raw.trim();
    if (!text || text.startsWith("#")) return;
    if (text.startsWith(":")) {
      const name = text.slice(1).trim();
      if (!name || /\s/.test(name)) return add(line, `A label is a colon and one word, e.g. :login`);
      if (labels.has(name.toLowerCase())) return add(line, `The label :${name} is used twice; GOTO couldn't tell which`);
      labels.set(name.toLowerCase(), instructions.length);
      return;
    }
    const space = text.indexOf(" ");
    const command = (space > 0 ? text.slice(0, space) : text).toUpperCase();
    const args = space > 0 ? text.slice(space + 1).trim() : "";
    if (command === "EXPECT") {
      if (!inCase()) return add(line, "EXPECT inside an EXPECT goes under one of its CASEs");
      try {
        const options = waitOptions(args);
        if (options.rest) return add(line, `EXPECT takes only /t:, /i and /else: on its own line; what it waits for goes on CASE lines below it`);
        const wait: OpenExpect["wait"] = { op: "wait", cases: [], timeoutMs: options.timeoutMs, idleMs: options.idleMs, onTimeout: options.onTimeout, next: -1, line, shown: text };
        instructions.push(wait);
        open.push({ kind: "expect", wait, ends: [], line, anyCase: options.anyCase });
        needLabel(line, options.onTimeout);
      } catch (error) {
        add(line, message(error));
      }
      return;
    }
    if (command === "REPEAT") {
      if (!inCase()) return add(line, `Lines inside an EXPECT go under a CASE, so it's clear when they run`);
      const count = /^\d+$/.test(args) ? parseInt(args, 10) : 0;
      if (count < 1 || count > MAX_REPEAT) return add(line, `REPEAT is how many times to run the lines up to its END: REPEAT 5 (1 to ${MAX_REPEAT.toLocaleString("en")})`);
      open.push({ kind: "repeat", start: instructions.length, line });
      instructions.push({ op: "repeat", count, line, shown: text });
      return;
    }
    if (command === "FOREACH") {
      if (!inCase()) return add(line, `Lines inside an EXPECT go under a CASE, so it's clear when they run`);
      const each = /^([A-Za-z_]\w*)\s+IN\s+(.+)$/i.exec(args);
      if (!each) return add(line, "FOREACH names a variable, then IN and a list: FOREACH file IN bins (a COLLECT's list), or FOREACH vlan IN 10, 20, 30");
      const start: OpenForeach["start"] = { op: "foreach", variable: each[1].toLowerCase(), source: each[2].trim(), end: -1, line, shown: text };
      open.push({ kind: "foreach", start, at: instructions.length, line });
      instructions.push(start);
      return;
    }
    if (command === "IF") {
      if (!inCase()) return add(line, `Lines inside an EXPECT go under a CASE, so it's clear when they run`);
      try {
        const test: OpenIf["test"] = { op: "if", condition: parseCondition(args), otherwise: -1, line, shown: text };
        open.push({ kind: "if", test, otherwise: null, line });
        instructions.push(test);
      } catch (error) {
        add(line, message(error));
      }
      return;
    }
    if (command === "ELSE") {
      const block = open[open.length - 1];
      if (!block || block.kind !== "if") return add(line, "ELSE goes inside an IF … END");
      if (block.otherwise) return add(line, "This IF has an ELSE already");
      if (args) return add(line, "ELSE takes nothing after it (an IF inside the ELSE tests something else)");
      const past: OpenIf["otherwise"] & object = { op: "jump", to: -1, line, forward: true };
      instructions.push(past);
      block.otherwise = past;
      block.test.otherwise = instructions.length;
      return;
    }
    if (command === "CASE") {
      const block = open[open.length - 1];
      if (!block || block.kind !== "expect") return add(line, block ? `CASE goes straight inside an EXPECT, not inside a ${BLOCK_NAMES[block.kind]} in it` : "CASE goes inside an EXPECT … END");
      try {
        const look: CaseLook = args.trim().toUpperCase() === "PROMPT" ? { prompt: true } : { pattern: casePattern(args, block.anyCase) };
        // the case before this one ends here: it goes on after END
        if (block.wait.cases.length > 0) {
          const end: OpenExpect["ends"][number] = { op: "jump", to: -1, line };
          instructions.push(end);
          block.ends.push(end);
        }
        block.wait.cases.push({ look, to: instructions.length });
      } catch (error) {
        add(line, message(error));
      }
      return;
    }
    if (command === "AGAIN") {
      const block = innermostExpect();
      if (!block) return add(line, "AGAIN goes inside an EXPECT's CASE: it waits again");
      if (!inCase()) return add(line, "AGAIN goes under a CASE");
      if (args) return add(line, "AGAIN takes nothing after it");
      instructions.push({ op: "jump", to: instructions.indexOf(block.wait), line });
      return;
    }
    if (command === "END") {
      const block = open.pop();
      if (!block) return add(line, "END closes an EXPECT, REPEAT, FOREACH or IF, and there's none open");
      if (block.kind === "repeat") {
        instructions.push({ op: "loop", start: block.start, line });
        return;
      }
      if (block.kind === "foreach") {
        instructions.push({ op: "next", start: block.at, line });
        block.start.end = instructions.length;
        return;
      }
      if (block.kind === "if") {
        if (block.otherwise) block.otherwise.to = instructions.length;
        else block.test.otherwise = instructions.length;
        return;
      }
      if (block.wait.cases.length === 0) add(block.line, "EXPECT needs at least one CASE: what to wait for");
      const after = instructions.length;
      block.wait.next = after;
      for (const end of block.ends) end.to = after;
      return;
    }
    if (!inCase()) return add(line, `Lines inside an EXPECT go under a CASE, so it's clear when they run`);
    let step: Step | null;
    try {
      step = parseLine(text);
    } catch (error) {
      // a line that can't run stops the script when it's reached, as SPSL does
      add(line, message(error));
      instructions.push({ op: "fail", message: message(error), line });
      return;
    }
    if (!step) return;
    const next = instructions.length + 1;
    if (step.kind === "waitfor") {
      instructions.push({ op: "wait", cases: [{ look: { pattern: step.spec }, to: next }], timeoutMs: step.timeoutMs, idleMs: step.idleMs, onTimeout: step.onTimeout, next, line, shown: text });
      needLabel(line, step.onTimeout);
    } else if (step.kind === "waitprompt") {
      instructions.push({ op: "wait", cases: [{ look: { prompt: true }, to: next }], timeoutMs: step.timeoutMs, idleMs: step.idleMs, onTimeout: step.onTimeout, next, line, shown: text });
      needLabel(line, step.onTimeout);
    } else if (step.kind === "waitquiet") {
      instructions.push({ op: "quiet", ms: step.ms, timeoutMs: step.timeoutMs, onTimeout: step.onTimeout, next, line, shown: text });
      needLabel(line, step.onTimeout);
    } else if (step.kind === "require") {
      instructions.push({ op: "require", names: step.names, onElse: step.onElse, next, line, shown: text });
      needLabel(line, step.onElse);
    } else if (step.kind === "goto") {
      const jump: Extract<Instruction, { op: "jump" }> = { op: "jump", to: -1, label: step.label, line };
      instructions.push(jump);
      pending.push({ line, label: step.label, set: (to) => (jump.to = to) });
    } else if (step.kind === "stop") {
      instructions.push({ op: "stop", message: step.message, line });
    } else {
      instructions.push({ op: "step", step, line, shown: command });
    }
  });
  for (const block of open) add(block.line, `This ${BLOCK_NAMES[block.kind]} has no END`);
  for (const { line, label, set } of pending) {
    const to = labels.get(label.toLowerCase());
    if (to === undefined) add(line, `There's no label :${label} to go to`);
    else set(to);
  }
  return { instructions, labels, issues };
}

/**
 * The variables a script makes (SET, /var:, named groups), the groups {0}…{9}, {repeat}, and the session's values
 * (SESSION_VARIABLES): their {name}s are the script's, not Environment Variables, so they're left for the script to fill
 * as it runs. In lower case.
 */
export function scriptVariables(script: string): Set<string> {
  const names = new Set([...Array.from({ length: 10 }, (_, i) => String(i)), ...SESSION_VARIABLES, "repeat"]);
  for (const raw of script.replace(/\r/g, "").split("\n")) {
    const line = raw.trim();
    const set = /^SET\s+([A-Za-z_]\w*)/i.exec(line);
    if (set) names.add(set[1].toLowerCase());
    for (const v of line.matchAll(/\/var:([A-Za-z_]\w*)/gi)) names.add(v[1].toLowerCase());
    for (const g of line.matchAll(/\(\?<([A-Za-z_]\w*)>/g)) names.add(g[1].toLowerCase());
    const check = /^CHECK\s+([A-Za-z_]\w*)/i.exec(line);
    if (check) names.add(check[1].toLowerCase());
    const collect = /^COLLECT\s+([A-Za-z_]\w*)/i.exec(line);
    if (collect) names.add(collect[1].toLowerCase()).add(`${collect[1].toLowerCase()}_count`);
    const each = /^FOREACH\s+([A-Za-z_]\w*)\s+IN\b/i.exec(line);
    if (each) names.add(each[1].toLowerCase());
  }
  return names;
}

/** An error's message, for a status line or the checker */
function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** A tab a script runs in: SuperXTerminal's PageHost (or RdpHost, which has no output) */
export interface ScriptTarget {
  /** Type text and key sequences into the session */
  type(text: string): Promise<void>;
  /**
   * A command starts on the cursor's line: Line Numbers number its lines in this colour, a button's ("" for the usual,
   * which ends an earlier button's colour as a command typed does); none in a tab without line numbers (RdpHost)
   */
  markCommand?(color: string): void;
  /** Make a wait look only at output after now, or from the start of the cursor's line */
  markOutput(atLineStart: boolean): void;
  /** The output since the wait's mark: finished lines, then the cursor's line */
  outputSinceMark(): string;
  /** Move the mark past a match */
  advanceMark(characters: number): void;
  /** Where the output has got to now (a count of characters since the session started), for SAVE */
  position(): number;
  /** The output since a position, as far as it's kept */
  outputFrom(position: number): string;
  /** A Break (serial or Telnet); throws when the tab can't send one */
  sendBreak(): Promise<void>;
  /**
   * Which connection the tab has now (an id, new at each reconnect), or null when it's down: a script stops when it
   * differs from the one it started on, so nothing is typed into a dropped session or a reconnected one (which may be
   * at a login prompt, not where the script left it)
   */
  connection(): number | null;
}

/** What a script needs from the window */
export interface ScriptHost {
  /** PROMPT and PWDPROMPT's question; null when cancelled */
  ask(message: string, secret: boolean): Promise<string | null>;
  /** OPENSESSION; false if there's no such session */
  openSession(id: string): boolean;
  /** A line for the status bar */
  status(text: string): void;
  /** SAVE: write text to a file (a relative path goes in Documents/SuperXTerminal/output); its full path */
  save(path: string, text: string, append: boolean): Promise<string>;
  /** CALL: a saved SPSL script's text, by its name in the scripts folder */
  loadScript(name: string): Promise<string>;
}

/** A running script: its progress for the status bar, its Stop, and its Next line when stepping */
export class ScriptRun {
  /** The script line running now (1-based, as the editor numbers them) */
  line = 0;
  /** The current command word, or a wait's whole line; never what's typed (it may be a secret) */
  step = "";
  /** Whether it's paused before a line, waiting for Next line (when stepping) */
  paused = false;
  /**
   * The colour of the button that started it ("" for none: the Script Editor, a session's own script): each command
   * it types is numbered in it (Line Numbers), as a send string's lines are
   */
  color = "";
  /** Told when Stop is pressed: a Rhai script (rhai.ts) runs in the core, which must hear of it */
  onStop: (() => void) | null = null;
  private stopped = false;
  private wake: (() => void) | null = null;
  private resume: (() => void) | null = null;

  /**
   * @param name What's running (a button's label, "Script", "Login script")
   * @param where The tab it runs in, for the status bar
   * @param lineCount The script's lines, for "line 4/12"
   * @param stepping Whether it pauses before each line until Next line (the Script Editor's Step)
   */
  constructor(
    readonly name: string,
    readonly where: string,
    readonly lineCount: number,
    readonly stepping = false,
  ) {}

  /** Whether Stop has been pressed for this run */
  get isStopped(): boolean {
    return this.stopped;
  }

  /** Stop: a SLEEP, wait or pause before a line ends at once */
  stop(): void {
    this.stopped = true;
    this.onStop?.();
    this.wake?.();
    this.resume?.();
  }

  /** Next line: run the line it's paused before */
  next(): void {
    this.resume?.();
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

  /** Pause before a line until Next line or Stop */
  waitForNext(): Promise<void> {
    this.paused = true;
    return new Promise((resolve) => {
      this.resume = () => {
        this.paused = false;
        this.resume = null;
        resolve();
      };
    });
  }
}

/** What a wait found: a case's index, a FAILON that matched first (the line it's on), or nothing (time or Stop) */
export type Found = { index: number; match: Match } | { failed: string } | { idle: true } | null;

/**
 * Run a script in a tab, as ctlPuttyPanel.RunScript: `fromStart` (the session's own script) lets the first wait see
 * everything since the session started; otherwise (a button) it sees the cursor's line and after, so a prompt already
 * on screen matches but older output doesn't. A script whose structure can't run (an EXPECT without END, a GOTO to no
 * label) doesn't start. Resolves when it ends; `run` can stop it.
 */
export async function runScript(
  target: ScriptTarget,
  host: ScriptHost,
  run: ScriptRun,
  script: string,
  fromStart: boolean,
  /** Variables it starts with: the tab's session's values (SESSION_VARIABLES) */
  initial: Record<string, string> = {},
): Promise<void> {
  const where = `"${run.name}" in ${run.where}`;
  const program = compile(script);
  const broken = firstBroken(program);
  if (broken) {
    host.status(`Script ${where} didn't start: line ${broken.line}: ${broken.message}`);
    return;
  }
  const connection = target.connection();
  if (connection === null) {
    host.status(`Script ${where} didn't start: the session isn't connected`);
    return;
  }
  /** The script's variables, by lower-case name; shared with the scripts it CALLs */
  const variables = new Map<string, string>(Object.entries(initial).map(([name, value]) => [name.toLowerCase(), value]));
  /** When the whole script must have ended (LIMIT), and the LIMIT as written; none until a LIMIT runs */
  const started = Date.now();
  let deadline = Infinity;
  let limitShown = "";
  /** How many times it has gone back since it last waited for anything (a runaway loop stops at MAX_JUMPS_WITHOUT_WAITING) */
  let jumpsWithoutWaiting = 0;
  /** CHECK's prerequisites, by name: what to look for, whether it must NOT appear, and where the output was at CHECK */
  const checks = new Map<string, { spec: PatternSpec; not: boolean; from: number }>();
  /** PAGER's prompts and what each presses; and where the last one answered was (absolute), so each is answered once */
  let pagers: { spec: PatternSpec; keys: string }[] = [];
  let paged = -1;
  /** COLLECT's lists, by name, for FOREACH */
  const lists = new Map<string, string[]>();

  /**
   * Answer a pager prompt a wait sees: one after the last answered, before (or at) anything the wait itself found
   * (`before`, in `shown`). Its key is typed without moving the wait's mark, so the wait goes on seeing the rest of the answer.
   * True if one was answered.
   */
  const answerPager = async (shown: string, before: number): Promise<boolean> => {
    if (pagers.length === 0) return false;
    // where the wait's text starts in the whole output
    const base = target.position() - shown.length;
    const from = Math.max(0, paged + 1 - base);
    let first: { at: number; keys: string } | null = null;
    for (const pager of pagers) {
      const match = makeMatcher(pager.spec, variables).find(shown.slice(from));
      if (!match) continue;
      const at = from + match.at;
      // at the same place as what's waited for, the pager wins: "<--- More --->" ends in > and looks like a prompt too
      if (at <= before && (!first || at < first.at)) first = { at, keys: pager.keys };
    }
    if (!first) return false;
    paged = base + first.at;
    await target.type(first.keys);
    return true;
  };
  /** FAILON's patterns, in force from where they're run */
  let failOn: PatternSpec[] = [];
  /** Where the output was when the last thing was typed, for SAVE */
  let typedAt = target.position();

  /** Why the script can't go on in this tab: its session dropped, or reconnected (a new connection); null if it can */
  const lost = (): string | null => {
    const now = target.connection();
    if (now === connection) return null;
    return now === null ? "the session ended" : "the session reconnected, so the script stopped rather than type into the new connection";
  };

  if (!fromStart) target.markOutput(true);
  const outcome = await execute(program, "", 0);
  if (outcome === "done") host.status(run.isStopped ? `Script ${where} stopped by you at line ${run.line} of ${run.lineCount}` : `Script ${where} finished`);

  /**
   * Run a compiled script (the script itself, or one it CALLs, named `called`); "done" when it ran to its end (or
   * Stop), "ended" when it stopped and said why
   */
  async function execute(compiled: Program, called: string, depth: number): Promise<"done" | "ended"> {
    const { instructions } = compiled;
    /** Where a message says it stopped: the line, and the called script's name */
    const at = (line: number) => (called ? `line ${line} of "${called}"` : `line ${line}`);
    const end = (line: number, why: string): "ended" => {
      host.status(`Script ${where} stopped at ${at(line)}${why ? `: ${why}` : ""}`);
      return "ended";
    };
    let next = 0;
    /** The REPEATs running here, innermost last: where each starts, its rounds and how many are left */
    const repeats: { start: number; count: number; left: number }[] = [];
    /** The FOREACHs running here, innermost last: where each starts, its items and which one it's on */
    const foreaches: { start: number; items: string[]; index: number; variable: string }[] = [];
    /** A jump back: counted, so a loop that never waits stops; and now and then a pause, so Stop (and the window) get in */
    const back = async (line: number): Promise<"ended" | null> => {
      if (++jumpsWithoutWaiting > MAX_JUMPS_WITHOUT_WAITING) {
        return end(line, `it went round ${MAX_JUMPS_WITHOUT_WAITING.toLocaleString("en")} times without waiting for anything; a wait or SLEEP in the loop lets it go on`);
      }
      // every so often, so a runaway is caught in moments: a pause takes about a millisecond
      if (jumpsWithoutWaiting % 50 === 0) await run.pause(0);
      return null;
    };
    /** How long a wait may wait: its own time, or what's left of the LIMIT */
    const within = (ms: number) => Math.max(0, Math.min(ms, deadline - Date.now()));
    const pastLimit = () => Date.now() >= deadline;
    const limitEnded = (line: number) => end(line, `the script's LIMIT of ${limitShown} ran out`);
    while (next < instructions.length && !run.isStopped) {
      const instruction = instructions[next];
      run.line = instruction.line;
      const gone = lost();
      if (gone) return end(instruction.line, gone);
      if (pastLimit()) return limitEnded(instruction.line);
      // a jump back (GOTO, AGAIN, END of a REPEAT) isn't a line of its own to stop at
      if (run.stepping && instruction.op !== "jump" && instruction.op !== "loop" && instruction.op !== "next") {
        run.step = instruction.op === "step" || instruction.op === "fail" || instruction.op === "stop" ? `paused before line ${instruction.line}` : instruction.shown;
        await run.waitForNext();
        if (run.isStopped) break;
      }
      try {
        switch (instruction.op) {
          case "fail":
            throw new Error(instruction.message);
          case "stop":
            return end(instruction.line, expand(instruction.message, variables));
          case "jump": {
            run.step = "GOTO";
            const going = instruction.to;
            // ELSE's jump past END goes forward: it's no loop
            if (instruction.forward) {
              next = going;
              continue;
            }
            if ((await back(instruction.line)) === "ended") return "ended";
            next = going;
            continue;
          }
          case "repeat": {
            run.step = instruction.shown;
            // entering it again (a GOTO back to it) starts its rounds again
            const known = repeats.findIndex((r) => r.start === next);
            if (known >= 0) repeats.splice(known);
            repeats.push({ start: next, count: instruction.count, left: instruction.count });
            variables.set("repeat", "1");
            next++;
            continue;
          }
          case "if":
            run.step = instruction.shown;
            next = holds(instruction.condition, variables) ? next + 1 : instruction.otherwise;
            continue;
          case "foreach": {
            run.step = instruction.shown;
            const items = itemsOf(instruction.source);
            const known = foreaches.findIndex((f) => f.start === next);
            if (known >= 0) foreaches.splice(known);
            if (items.length === 0) {
              next = instruction.end;
              continue;
            }
            foreaches.push({ start: next, items, index: 0, variable: instruction.variable });
            variables.set(instruction.variable, items[0]);
            next++;
            continue;
          }
          case "next": {
            const at = foreaches.findIndex((f) => f.start === instruction.start);
            const each = at >= 0 ? foreaches[at] : null;
            if (each && each.index < each.items.length - 1) {
              each.index++;
              variables.set(each.variable, each.items[each.index]);
              if ((await back(instruction.line)) === "ended") return "ended";
              next = instruction.start + 1;
              continue;
            }
            if (at >= 0) foreaches.splice(at);
            next++;
            continue;
          }
          case "loop": {
            const at = repeats.findIndex((r) => r.start === instruction.start);
            const round = at >= 0 ? repeats[at] : null;
            if (round && round.left > 1) {
              round.left--;
              variables.set("repeat", String(round.count - round.left + 1));
              if ((await back(instruction.line)) === "ended") return "ended";
              next = instruction.start + 1;
              continue;
            }
            // done: an outer REPEAT's round is {repeat} again
            if (at >= 0) repeats.splice(at);
            const outer = repeats[repeats.length - 1];
            if (outer) variables.set("repeat", String(outer.count - outer.left + 1));
            else variables.delete("repeat");
            next++;
            continue;
          }
          case "wait": {
            run.step = instruction.shown;
            const looks = instruction.cases.map((c) => ("prompt" in c.look ? promptMatcher(variables.get("prompt") ?? "") : makeMatcher(c.look.pattern, variables)));
            jumpsWithoutWaiting = 0;
            const found = await waitForAny(target, run, looks, failOn.map((f) => makeMatcher(f, variables)), within(instruction.timeoutMs), lost, answerPager, instruction.idleMs);
            if (found && "failed" in found) return end(instruction.line, `the device said "${found.failed}" (FAILON)`);
            if (found && "idle" in found) {
              const jumped = timedOut(instruction.onTimeout, instruction.next, compiled);
              if (jumped === null) return end(instruction.line, `no new output for ${Math.round(instruction.idleMs / 1000)} s (/idle:): stalled?`);
              next = jumped;
              continue;
            }
            if (found) {
              remember(found.match);
              next = instruction.cases[found.index].to;
              continue;
            }
            if (run.isStopped) break;
            // the session went while waiting: say so, not that the text didn't come (and no /else:, which is for the device)
            const went = lost();
            if (went) return end(instruction.line, went);
            if (pastLimit()) return limitEnded(instruction.line);
            const what = looks.length === 1 ? `${looks[0].shown === "the prompt" ? "the prompt" : `"${looks[0].shown}"`} didn't appear` : "none of its cases appeared";
            const jumped = timedOut(instruction.onTimeout, instruction.next, compiled);
            if (jumped === null) return end(instruction.line, `${what} within ${Math.round(instruction.timeoutMs / 1000)} s`);
            next = jumped;
            continue;
          }
          case "quiet": {
            run.step = instruction.shown;
            jumpsWithoutWaiting = 0;
            const quiet = await waitQuiet(target, run, instruction.ms, failOn.map((f) => makeMatcher(f, variables)), within(instruction.timeoutMs), lost, answerPager);
            if (quiet !== true && quiet !== false) return end(instruction.line, `the device said "${quiet.failed}" (FAILON)`);
            if (quiet) {
              next++;
              continue;
            }
            if (run.isStopped) break;
            const went = lost();
            if (went) return end(instruction.line, went);
            if (pastLimit()) return limitEnded(instruction.line);
            const jumped = timedOut(instruction.onTimeout, instruction.next, compiled);
            if (jumped === null) return end(instruction.line, `the output didn't stay quiet for ${instruction.ms} ms within ${Math.round(instruction.timeoutMs / 1000)} s`);
            next = jumped;
            continue;
          }
          case "require": {
            run.step = instruction.shown;
            const unmet = requireChecks(instruction.names);
            if (!unmet) {
              next++;
              continue;
            }
            const jumped = timedOut(instruction.onElse, instruction.next, compiled);
            if (jumped === null) return end(instruction.line, unmet);
            next = jumped;
            continue;
          }
          case "step": {
            run.step = instruction.shown;
            const step = instruction.step;
            if (step.kind === "call") {
              const outcome = await call(expand(step.name, variables), depth);
              if (outcome === "ended") return "ended";
            } else {
              await runStep(step);
              // a SLEEP (or a PROMPT's answer) that outlasted the LIMIT ran out on this line
              if (pastLimit()) return limitEnded(instruction.line);
            }
            // a session that went meanwhile (a PROMPT's answer or a SLEEP can outlast it) stops the next line
            next++;
            continue;
          }
        }
      } catch (error) {
        return end(instruction.line, message(error));
      }
    }
    return "done";
  }

  /** Where a wait goes when its time runs out: the line after it, a label's, or null to stop */
  function timedOut(timeout: OnTimeout, after: number, compiled: Program): number | null {
    if (timeout.kind === "continue") return after;
    if (timeout.kind === "label") return compiled.labels.get(timeout.label.toLowerCase()) ?? compiled.instructions.length;
    return null;
  }

  /**
   * Judge prerequisites (these, or every CHECK so far): each looks at all the output since its CHECK line. A met CHECK
   * keeps what it matched as {name} (its first group, if it has one). Why the first unmet one isn't met, or null.
   * @throws for a name no CHECK has defined
   */
  function requireChecks(names: string[]): string | null {
    const judged = names.length ? names : [...checks.keys()];
    for (const name of judged) {
      const check = checks.get(name);
      if (!check) throw new Error(`REQUIRE ${name}: no CHECK ${name} has run before it`);
      const output = target.outputFrom(check.from);
      const match = makeMatcher(check.spec, variables).find(output);
      const shown = shownSpec(check.spec);
      if (check.not && match) return `prerequisite "${name}" isn't met: ${shown} appeared ("${lineAround(output, match.at)}")`;
      if (!check.not && !match) return `prerequisite "${name}" isn't met: ${shown} didn't appear`;
      if (match) variables.set(name, match.groups[1] ?? match.groups[0]);
    }
    return null;
  }

  /**
   * FOREACH's items: a COLLECT's list by its name, else the text (its {variables} filled in) split at commas
   */
  function itemsOf(source: string): string[] {
    const list = lists.get(source.toLowerCase());
    if (list) return list;
    // a {variable} with no value is no item, as it's no text in IF
    return expand(source, variables).replace(/\{\w+\}/g, "").split(",").map((item) => item.trim()).filter(Boolean);
  }

  /** What a wait matched, as {0}, {1}… and named groups; the groups of an earlier match go */
  function remember(match: Match): void {
    for (let i = 0; i < 10; i++) variables.delete(String(i));
    match.groups.slice(0, 10).forEach((value, i) => variables.set(String(i), value));
    for (const [name, value] of Object.entries(match.named)) variables.set(name.toLowerCase(), value);
  }

  /** CALL: a saved script, run here with the same variables and FAILONs; its labels are its own */
  async function call(name: string, depth: number): Promise<"done" | "ended"> {
    if (depth >= MAX_CALL_DEPTH) throw new Error(`CALL ${name}: scripts calling scripts ${MAX_CALL_DEPTH} deep (one calling itself?)`);
    let text: string;
    try {
      text = await host.loadScript(name);
    } catch (error) {
      throw new Error(`CALL ${name}: ${message(error)}`);
    }
    const called = compile(text.replace(/\r\n/g, "\n"));
    const broken = firstBroken(called);
    if (broken) throw new Error(`CALL ${name}: line ${broken.line}: ${broken.message}`);
    return execute(called, name, depth + 1);
  }

  /** Run one plain step; throws when it can't, or when a cancelled password should stop the script */
  async function runStep(step: Extract<Instruction, { op: "step" }>["step"]): Promise<void> {
    switch (step.kind) {
      case "sleep":
        jumpsWithoutWaiting = 0;
        // no further than the LIMIT, which the next line then says ran out
        await run.pause(Math.max(0, Math.min(step.ms, deadline - Date.now())));
        break;
      case "check":
        checks.set(step.name, { spec: step.spec, not: step.not, from: target.position() });
        break;
      case "pager":
        pagers = step.spec ? [...pagers, { spec: step.spec, keys: step.keys }] : [];
        break;
      case "collect": {
        const items = collect(step.spec, lastCommandsOutput(target.outputFrom(typedAt)), variables);
        lists.set(step.name, items);
        variables.set(step.name, items.join(", "));
        variables.set(`${step.name}_count`, String(items.length));
        break;
      }
      case "limit":
        deadline = started + step.ms;
        limitShown = step.ms % 3_600_000 === 0 ? `${step.ms / 3_600_000} h` : step.ms % 60_000 === 0 ? `${step.ms / 60_000} min` : `${step.ms / 1000} s`;
        break;
      case "type": {
        const text = lineFeedsAsEnter(expand(step.text, variables));
        await send(step.enter ? text + "\r" : text, step.enter ? LINE_DELAY_MS : 0);
        break;
      }
      case "keys":
        await send(step.text, 0);
        break;
      case "prompt": {
        // Cancel gives an empty answer, as SuperXPuTTY's InputBox did
        const answer = (await host.ask(expand(step.message, variables), false)) ?? "";
        if (step.variable) variables.set(step.variable, answer);
        else await send(lineFeedsAsEnter(answer) + "\r", LINE_DELAY_MS);
        break;
      }
      case "pwdprompt": {
        const answer = await host.ask("Insert your password", true);
        if (answer === null) throw new Error("you cancelled the password prompt");
        if (step.variable) variables.set(step.variable, answer);
        else await send(lineFeedsAsEnter(answer) + "\r", LINE_DELAY_MS);
        break;
      }
      case "open":
        host.openSession(expand(step.id, variables));
        break;
      case "break":
        target.markOutput(false);
        await target.sendBreak();
        break;
      case "set":
        if (step.value === "") variables.delete(step.name);
        else variables.set(step.name, expand(step.value, variables));
        break;
      case "failon":
        failOn = step.spec ? [...failOn, step.spec] : [];
        break;
      case "echo":
        host.status(`Script ${where}: ${expand(step.text, variables)}`);
        break;
      case "save": {
        const path = expand(step.path, variables);
        await host.save(path, lastCommandsOutput(target.outputFrom(typedAt)), step.append);
        break;
      }
    }
  }

  /**
   * Type, marking the output first so a following wait looks at what comes after, and the command for Line Numbers
   * (the button's colour, run.color)
   */
  async function send(text: string, delayMs: number): Promise<void> {
    target.markOutput(false);
    target.markCommand?.(run.color);
    typedAt = target.position();
    await target.type(text);
    if (delayMs > 0) await run.pause(delayMs);
  }
}

/**
 * What the last command printed, for SAVE: the output since it was typed, without its echo (the rest of the line it
 * was typed on) and without the prompt after it (a last line WAITPROMPT would take for one)
 */
export function lastCommandsOutput(output: string): string {
  // the tab's output has none, but a carriage return left in would double every line break on Windows
  const lines = output.replace(/\r/g, "").split("\n");
  if (lines.length > 1) lines.shift();
  if (lines.length > 0 && PROMPT_LINE.test(lines[lines.length - 1])) lines.pop();
  const text = lines.join("\n");
  return text && !text.endsWith("\n") ? text + "\n" : text;
}

/**
 * COLLECT: every match of a pattern in some output, as a list: a regular expression's first group (else its whole
 * match), or each place plain text appears
 */
export function collect(spec: PatternSpec, output: string, variables: ReadonlyMap<string, string>): string[] {
  if (!spec.regex) {
    const wanted = expand(spec.source, variables);
    const text = spec.anyCase ? output.toLowerCase() : output;
    const items: string[] = [];
    for (let at = text.indexOf(spec.anyCase ? wanted.toLowerCase() : wanted); at >= 0 && wanted; at = text.indexOf(spec.anyCase ? wanted.toLowerCase() : wanted, at + wanted.length)) {
      items.push(output.slice(at, at + wanted.length));
    }
    return items;
  }
  const expression = new RegExp(expandPattern(spec.source, variables), spec.anyCase ? "gim" : "gm");
  return [...output.matchAll(expression)].filter((m) => m[0].length > 0).map((m) => m[1] ?? m[0]);
}

/** A script's first error that stops it before it starts (a line that can't run stops it only when reached) */
function firstBroken(program: Program): Issue | undefined {
  return program.issues.find((i) => i.error && !program.instructions.some((s) => s.op === "fail" && s.line === i.line));
}

/** The whole line a match is on, for FAILON's message */
function lineAround(text: string, at: number): string {
  const start = text.lastIndexOf("\n", at) + 1;
  const end = text.indexOf("\n", at);
  return text.slice(start, end < 0 ? undefined : end).trim();
}

/**
 * Wait until one of the matchers appears in the output since the mark: the one appearing first in the output wins
 * (the earlier case on a tie), and the mark moves past it, so the next wait looks at what follows. A FAILON appearing
 * first (or at the same place) wins over them all. Null on timeout, Stop, or the session going (`lost` says why;
 * checked at every look, so a drop ends the wait at once).
 */
export async function waitForAny(
  target: ScriptTarget,
  run: ScriptRun,
  matchers: Matcher[],
  failOn: Matcher[],
  timeoutMs: number,
  lost: () => string | null,
  /** PAGER: answers a pager prompt before `before` in the text, if there's one to answer; true if it did */
  answerPager: (shown: string, before: number) => Promise<boolean> = async () => false,
  /** /idle: the wait ends (`{ idle: true }`) when the output hasn't changed for this long; 0 for no such limit */
  idleMs = 0,
): Promise<Found> {
  const giveUp = Date.now() + timeoutMs;
  let last = "";
  let changed = Date.now();
  while (Date.now() < giveUp && !run.isStopped && !lost()) {
    const shown = target.outputSinceMark();
    if (shown !== last) {
      last = shown;
      changed = Date.now();
    } else if (idleMs > 0 && Date.now() - changed >= idleMs) {
      return { idle: true };
    }
    let best: { index: number; match: Match } | null = null;
    matchers.forEach((matcher, index) => {
      const match = matcher.find(shown);
      if (match && (!best || match.at < best.match.at)) best = { index, match };
    });
    for (const fail of failOn) {
      const match = fail.find(shown);
      if (match && (!best || match.at <= (best as { match: Match }).match.at)) return { failed: lineAround(shown, match.at) };
    }
    // a pager prompt before what's waited for: answered, and the wait goes on (the rest of the answer comes)
    if (await answerPager(shown, best ? (best as { match: Match }).match.at : Infinity)) {
      await run.pause(WAITFOR_POLL_MS);
      continue;
    }
    if (best) {
      const found = best as { index: number; match: Match };
      target.advanceMark(found.match.at + found.match.length);
      return found;
    }
    await run.pause(WAITFOR_POLL_MS);
  }
  return null;
}

/**
 * Wait until the output since the mark has stayed the same for `ms` (the device has finished printing), then move the
 * mark to its end; true then, false on timeout, Stop or the session going, or the FAILON seen
 */
async function waitQuiet(
  target: ScriptTarget,
  run: ScriptRun,
  ms: number,
  failOn: Matcher[],
  timeoutMs: number,
  lost: () => string | null,
  /** PAGER: answers a pager prompt in the text, if there's one to answer; true if it did */
  answerPager: (shown: string, before: number) => Promise<boolean> = async () => false,
): Promise<boolean | { failed: string }> {
  const giveUp = Date.now() + timeoutMs;
  let last = target.outputSinceMark();
  let since = Date.now();
  while (Date.now() < giveUp && !run.isStopped && !lost()) {
    await run.pause(Math.min(WAITFOR_POLL_MS, ms));
    const now = target.outputSinceMark();
    for (const fail of failOn) {
      const match = fail.find(now);
      if (match) return { failed: lineAround(now, match.at) };
    }
    // a pager prompt isn't quiet: the answer goes on once it's answered
    if (await answerPager(now, Infinity)) {
      since = Date.now();
      continue;
    }
    if (now !== last) {
      last = now;
      since = Date.now();
    } else if (Date.now() - since >= ms) {
      target.advanceMark(now.length);
      return true;
    }
  }
  return false;
}

/** One problem Check script finds */
export interface Issue {
  line: number;
  error: boolean;
  message: string;
}

/** The commands, for the checker's suggestions; CLOSESESSION is listed but can't run */
export const COMMANDS = [
  "SENDLINE", "SENDCHAR", "SENDKEY", "SLEEP", "WAITFOR", "PROMPT", "PWDPROMPT", "OPENSESSION", "SENDBREAK",
  "EXPECT", "CASE", "AGAIN", "END", "GOTO", "STOP", "SET", "FAILON", "WAITPROMPT", "WAITQUIET", "ECHO", "SAVE", "CALL",
  "REPEAT", "LIMIT", "CHECK", "REQUIRE", "PAGER", "COLLECT", "FOREACH", "IF", "ELSE",
];
/** Commands SPSL has that this app can't run: the checker knows them, so it doesn't call them misspelt */
const UNAVAILABLE = ["CLOSESESSION"];

/**
 * Check a script without running it, as SpslChecker: what `compile` finds (EXPECT blocks, labels, bad options and
 * regular expressions), unknown commands (with a suggestion for a near miss), missing or malformed arguments, unknown
 * SENDKEY keys, sessions and saved scripts that don't exist, and {key} placeholders with no value (the script's own
 * variables aside)
 * @param scriptExists Whether CALL's saved script is there; leave it out to not check
 */
export function checkScript(
  script: string,
  sessionExists: (id: string) => boolean,
  missingPlaceholders: (text: string) => string[],
  scriptExists?: (name: string) => boolean,
): Issue[] {
  const issues: Issue[] = [...compile(script).issues];
  const own = scriptVariables(script);
  /** The CHECKs the script defines, for REQUIRE's names; a script that CALLs others may get some from them */
  const checks = new Set([...script.matchAll(/^\s*CHECK\s+([A-Za-z_]\w*)/gim)].map((m) => m[1].toLowerCase()));
  const calls = /^\s*CALL\s/im.test(script);
  const lines = script.replace(/\r/g, "").split("\n");
  lines.forEach((raw, i) => {
    const line = raw.trim();
    if (!line || line.startsWith("#") || line.startsWith(":")) return;
    const space = line.indexOf(" ");
    const command = (space > 0 ? line.slice(0, space) : line).toUpperCase();
    const args = space > 0 ? line.slice(space + 1).trim() : "";
    const add = (message: string, error = true) => {
      // compile may have said it already
      if (!issues.some((x) => x.line === i + 1 && x.message === message)) issues.push({ line: i + 1, error, message });
    };
    if (!COMMANDS.includes(command) && !UNAVAILABLE.includes(command)) {
      const suggestion = suggest(command);
      // shell text pasted into a script is the usual cause, so show how to type it
      add(`"${line.slice(0, space > 0 ? space : undefined)}" isn't an SPSL command` +
        (suggestion ? `; did you mean ${suggestion}?` : `. Lines start with a command; to type this line, write SENDLINE ${line}`));
      return;
    }
    if (command === "REQUIRE" && !calls) {
      for (const name of args.replace(/\/\S+/g, "").split(/[\s,]+/).filter(Boolean)) {
        if (!checks.has(name.toLowerCase())) add(`There's no CHECK ${name} in the script, so REQUIRE stops here`, false);
      }
    }
    const problem = checkArguments(command, args, sessionExists, scriptExists);
    if (problem) add(problem.replace(/^!/, ""), !problem.startsWith("!"));
    // SENDKEY's braces are key names, not placeholders
    if (command !== "SENDKEY") {
      for (const missing of missingPlaceholders(args)) {
        if (own.has(missing.replace(/[{}]/g, "").toLowerCase())) continue;
        add(`${missing} has no value in Tools > Environment Variables, and the script doesn't SET it, so it's typed as it is.`, false);
      }
    }
  });
  return issues.sort((a, b) => a.line - b.line);
}

/** A command's arguments: null if fine, an error, or a warning starting with "!" (the rest are compile's) */
function checkArguments(command: string, args: string, sessionExists: (id: string) => boolean, scriptExists?: (name: string) => boolean): string | null {
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
    case "OPENSESSION":
      if (!args) return "OPENSESSION needs a session ID, e.g. OPENSESSION Lab/core-sw1";
      return args.includes("{") || sessionExists(args) ? null : `!There's no saved session with the ID "${args}"; the line will do nothing.`;
    case "CALL":
      if (!args || !scriptExists || args.includes("{")) return null;
      return scriptExists(args) ? null : `!There's no saved SPSL script "${args}" in the scripts folder; the script will stop here.`;
    case "SENDBREAK":
      if (args) return "SENDBREAK takes no argument";
      return "!SENDBREAK works in Serial and Telnet tabs; an SSH tab stops the script here.";
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
  return `SENDKEY presses one key; "${name}" isn't a key name. Use ENTER, TAB, SPACE, ESC, UP, DOWN, F1-F16 and so on (see "Keys for SENDKEY"), or a single character. To type text, use SENDCHAR or SENDLINE.`;
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

/** How many single-letter changes turn `a` into `b` (Levenshtein distance), for the checker's suggestions */
function levenshtein(a: string, b: string): number {
  const d = Array.from({ length: a.length + 1 }, (_, i) => [i, ...Array(b.length).fill(0)]);
  for (let j = 0; j <= b.length; j++) d[0][j] = j;
  for (let i = 1; i <= a.length; i++)
    for (let j = 1; j <= b.length; j++)
      d[i][j] = Math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + (a[i - 1] === b[j - 1] ? 0 : 1));
  return d[a.length][b.length];
}
