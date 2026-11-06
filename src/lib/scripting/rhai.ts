/**
 * Rhai scripts in a tab: the page's half. The script itself runs in the core (src-tauri/src/rhai_scripts.rs), on a
 * thread of its own, since Rhai can't wait for a Promise; each thing it asks of the tab comes here as a call on the
 * run's Channel, and is answered with `rhai_reply`. The calls are done with what SPSL already uses (spsl.ts): a send
 * marks the output first, so the next wait looks only at what follows; a wait is SPSL's `waitForAny`, so it sees
 * the cursor's line, moves the mark past what it found, and ends when the session drops or reconnects.
 *
 * The calls (`args` as the core sends them):
 * - `type` { text, enter }: type text into the session, Enter after it if `enter`, its lines numbered in the
 *   button's colour (run.color); answers null.
 * - `wait` { patterns: [{ source, regex, anyCase } | { prompt: true }], timeoutMs | null }: answers
 *   { index, groups } for the pattern found first, or null when the time ran out.
 * - `ask` { message, secret }: PROMPT's question; answers the text, or null when cancelled.
 * Status events go to the status bar, and Done ends the run with a message as SPSL's.
 */
import { Channel, invoke } from "@tauri-apps/api/core";
import { LINE_DELAY_MS, ScriptRun, lineFeedsAsEnter, literal, pattern, promptMatcher, waitForAny, type Matcher, type ScriptHost, type ScriptTarget } from "./spsl";

/** What the core sends on a run's Channel (rhai_scripts.rs RhaiEvent) */
export type RhaiEvent =
  | { kind: "call"; id: number; line: number; call: string; args: Record<string, unknown> }
  | { kind: "status"; line: number; text: string }
  | { kind: "done"; stopped: boolean; error: string | null; line: number };

/** A pattern a `wait` call looks for, as the core sends it */
type WaitPattern = { source: string; regex: boolean; anyCase: boolean } | { prompt: true };

/** The way to the core: start a run, answer its calls, stop it. Tests stand in for it; the app's is `tauriCore`. */
export interface RhaiCore {
  /** Start a script; its events go to `onEvent`. Its run id. */
  start(source: string, session: Record<string, string>, onEvent: (event: RhaiEvent) => void): Promise<number>;
  /** Answer a call: its value, or why it couldn't be done */
  reply(run: number, id: number, outcome: { value: unknown } | { error: string }): void;
  /** Stop a run */
  stop(run: number): void;
}

/** The core, through Tauri: a Channel for the events, and the rhai_* commands */
export const tauriCore: RhaiCore = {
  start(source, session, onEvent) {
    const channel = new Channel<RhaiEvent>();
    channel.onmessage = onEvent;
    return invoke<number>("rhai_run", { source, session, onEvent: channel });
  },
  reply(run, id, outcome) {
    void invoke("rhai_reply", { run, id, value: "value" in outcome ? outcome.value : null, error: "error" in outcome ? outcome.error : null });
  },
  stop(run) {
    void invoke("rhai_stop", { run });
  },
};

/**
 * The Environment Variables a script reads, `env.name` or `env["name"]`, in lower case as Environment Variables keeps
 * them, each once with the line it's first on; for asking to define a missing one before it runs, and the problems
 * list. A name the script works out as it runs (`env[name]`) can't be seen here: the core names it if it's missing.
 */
export function envNames(source: string): { name: string; line: number }[] {
  const found = new Map<string, number>();
  source.split("\n").forEach((text, i) => {
    // not in a // comment
    const code = text.replace(/\/\/.*$/, "");
    for (const m of code.matchAll(/\benv\s*(?:\.\s*([A-Za-z_]\w*)|\[\s*"(\w+)"\s*\])/g)) {
      const name = (m[1] ?? m[2]).toLowerCase();
      // env.has("x") is a question, not a use
      if (m[1] === "has") continue;
      if (!found.has(name)) found.set(name, i + 1);
    }
  });
  return [...found].map(([name, line]) => ({ name, line }));
}

/** A script's syntax errors (its first), for the Script Editor: each with its line */
export function checkRhai(source: string): Promise<{ line: number; message: string }[]> {
  return invoke("rhai_check", { source });
}

/** A wait's pattern as a Matcher: the device's prompt, a regular expression, or text */
function matcherOf(look: WaitPattern): Matcher {
  if ("prompt" in look) return promptMatcher("");
  return look.regex ? pattern(look.source, look.anyCase) : literal(look.source, look.anyCase);
}

/** What the status bar shows a call as: never what's typed, which may be a password */
function shownCall(call: string, args: Record<string, unknown>): string {
  if (call === "type") return args.enter ? "send_line" : "send";
  if (call === "wait") {
    const looks = (args.patterns as WaitPattern[]) ?? [];
    if (looks.length === 1) return "prompt" in looks[0] ? "wait_prompt" : `wait_for "${looks[0].source}"`;
    return "wait_for_any";
  }
  return call === "ask" ? (args.secret ? "password" : "prompt") : call;
}

/**
 * Run a Rhai script in a tab, as runScript runs an SPSL one: `fromStart` (a session's own script) lets the first wait
 * see everything since the session started, otherwise it sees the cursor's line and after. `session` is the tab's
 * session's values (`session.host_address`...). Resolves when the script ends; `run` can stop it.
 */
export async function runRhai(
  target: ScriptTarget,
  host: ScriptHost,
  run: ScriptRun,
  source: string,
  fromStart: boolean,
  session: Record<string, string> = {},
  core: RhaiCore = tauriCore,
): Promise<void> {
  const where = `"${run.name}" in ${run.where}`;
  const connection = target.connection();
  if (connection === null) {
    host.status(`Script ${where} didn't start: the session isn't connected`);
    return;
  }
  /** Why the script can't go on in this tab, as SPSL's: dropped, or reconnected; null if it can */
  const lost = (): string | null => {
    const now = target.connection();
    if (now === connection) return null;
    return now === null ? "the session ended" : "the session reconnected, so the script stopped rather than type into the new connection";
  };

  if (!fromStart) target.markOutput(true);
  let ended: () => void = () => {};
  const finished = new Promise<void>((resolve) => (ended = resolve));
  /**
   * The run's id, once the core has said it. Its first call can come before then (even while `core.start` is still
   * running), so answers and Stop wait for this, made before the run starts.
   */
  let haveId: (id: number) => void = () => {};
  let noId: (error: unknown) => void = () => {};
  const started = new Promise<number>((resolve, reject) => ((haveId = resolve), (noId = reject)));
  // Stop pressed while it's starting stops it once the core has said which run it is
  run.onStop = () => void started.then((id) => core.stop(id), () => {});

  /** Do one call in the tab; its answer, or throws why it couldn't */
  const perform = async (call: string, args: Record<string, unknown>): Promise<unknown> => {
    const gone = lost();
    if (gone) throw new Error(gone);
    switch (call) {
      case "type": {
        const text = lineFeedsAsEnter(String(args.text ?? ""));
        target.markOutput(false);
        // its lines numbered in its button's colour (Line Numbers), as a send string's
        target.markCommand?.(run.color);
        await target.type(args.enter ? text + "\r" : text);
        if (args.enter) await run.pause(LINE_DELAY_MS);
        return null;
      }
      case "wait": {
        const looks = ((args.patterns as WaitPattern[]) ?? []).map(matcherOf);
        const timeoutMs = typeof args.timeoutMs === "number" ? args.timeoutMs : Infinity;
        const found = await waitForAny(target, run, looks, [], timeoutMs, lost);
        if (found && "index" in found) return { index: found.index, groups: found.match.groups };
        const went = lost();
        if (went) throw new Error(went);
        return null;
      }
      case "ask":
        return host.ask(String(args.message ?? ""), Boolean(args.secret));
      default:
        throw new Error(`the tab doesn't know "${call}"`);
    }
  };

  /** The script's last status() line, kept in the one it finishes with, which would otherwise cover it at once */
  let said = "";
  const onEvent = (event: RhaiEvent) => {
    if (event.kind === "status") {
      if (event.line) run.line = event.line;
      said = event.text;
      host.status(`Script ${where}: ${event.text}`);
    } else if (event.kind === "call") {
      run.line = event.line;
      run.step = shownCall(event.call, event.args);
      const answer = perform(event.call, event.args).then(
        (value) => ({ value }),
        (error: unknown) => ({ error: error instanceof Error ? error.message : String(error) }),
      );
      void Promise.all([started, answer]).then(([id, outcome]) => core.reply(id, event.id, outcome));
    } else {
      if (event.stopped) host.status(`Script ${where} stopped by you${event.line ? ` at line ${event.line} of ${run.lineCount}` : ""}`);
      else if (event.error) host.status(`Script ${where} stopped${event.line ? ` at line ${event.line}` : ""}: ${event.error}`);
      else host.status(`Script ${where} finished${said ? `: ${said}` : ""}`);
      ended();
    }
  };

  core.start(source, session, onEvent).then(haveId, noId);
  try {
    await started;
  } catch (error) {
    host.status(`Script ${where} didn't start: ${error instanceof Error ? error.message : String(error)}`);
    return;
  }
  await finished;
}
