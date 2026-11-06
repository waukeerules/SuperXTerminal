<!--
  A script's problems, under its box in Edit Button and the Script Editor, checked as it's written: a moment after
  each change (CHECK_DELAY_MS), so a mistake shows while it's being made rather than when OK or Send is pressed.
  An SPSL script is checked as SpslChecker does (spsl.ts checkScript: commands, labels, sessions, saved scripts and
  Environment Variables); a Rhai script in the core (rhai_scripts.rs: syntax, and variables never set), with a
  warning for each Environment Variable it reads (env.name) that has no value, and an error for a session value read
  as one (env.host_address, which is session.host_address). Clicking a
  problem puts the cursor on its line. OK and Send ask `check()` for the script as it is that moment.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { checkScript, SESSION_VARIABLES, type Issue } from "./spsl";
  import { checkRhai, envNames } from "./rhai";
  import { loadSessions } from "../sessions/api";

  interface Props {
    /** The script being edited */
    script: string;
    /** Its language */
    language: "spsl" | "rhai";
    /** Go to a problem's line in the script box */
    onGoTo?: (line: number) => void;
  }

  let { script, language, onGoTo }: Props = $props();

  /** How long after a change the script is checked: a pause in typing, so it isn't checked at every key */
  const CHECK_DELAY_MS = 350;

  /** The problems the last check found; null before the first */
  let issues = $state<Issue[] | null>(null);
  /** Which check is the latest, so an older one finishing late doesn't show its stale problems */
  let latest = 0;

  /**
   * What an SPSL check looks at besides the script: the sessions OPENSESSION can open and the saved SPSL scripts CALL
   * can run. Read once when the list first needs them, not at every key; Environment Variables are read at each check,
   * since which {key}s are missing depends on the text.
   */
  let known: Promise<{ sessions: Set<string>; scripts: Set<string> }> | null = null;

  /** The sessions' ids and saved SPSL scripts' names, in lower case, read once */
  function knownNames(): Promise<{ sessions: Set<string>; scripts: Set<string> }> {
    known ??= Promise.all([loadSessions(), invoke<{ name: string; kind: string }[]>("scripts_list")]).then(([loaded, saved]) => ({
      sessions: new Set(loaded.sessions.map((s) => s.id.toLowerCase())),
      scripts: new Set(saved.filter((s) => s.kind === "spsl").map((s) => s.name.toLowerCase())),
    }));
    return known;
  }

  /** The problems in a script of a language */
  async function problemsIn(text: string, lang: "spsl" | "rhai"): Promise<Issue[]> {
    if (lang === "rhai") {
      const errors = (await checkRhai(text)).map((i) => ({ ...i, error: true }));
      const all = envNames(text);
      // env.host_address is the session's: it can't be an Environment Variable (they won't take its name)
      const misplaced = all
        .filter((u) => SESSION_VARIABLES.includes(u.name))
        .map((u) => ({ line: u.line, error: true, message: `env.${u.name}: ${u.name} is the tab's session's, not an Environment Variable: write session.${u.name}` }));
      const used = all.filter((u) => !SESSION_VARIABLES.includes(u.name));
      let unset: Issue[] = [];
      if (used.length) {
        const missing = new Set((await invoke<{ missing: string[] }>("variables_fill", { text: used.map((u) => `{${u.name}}`).join(" ") })).missing.map((m) => m.toLowerCase()));
        unset = used
          .filter((u) => missing.has(`{${u.name}}`))
          .map((u) => ({ line: u.line, error: false, message: `env.${u.name} has no value in Environment Variables: define it, or the script stops there` }));
      }
      return [...errors, ...misplaced, ...unset].sort((a, b) => a.line - b.line);
    }
    const { sessions, scripts } = await knownNames();
    const missing = new Set((await invoke<{ missing: string[] }>("variables_fill", { text })).missing);
    return checkScript(text, (id) => sessions.has(id.toLowerCase()), (line) => (line.match(/\{\w+\}/g) ?? []).filter((m) => missing.has(m)),
      (name) => scripts.has(name.toLowerCase()));
  }

  /** Check the script as it is now and show what's found (OK and Send, which mustn't wait for the pause) */
  export async function check(): Promise<Issue[]> {
    const mine = ++latest;
    const found = await problemsIn(script, language);
    if (mine === latest) issues = found;
    return found;
  }

  // a moment after each change of the script or its language
  $effect(() => {
    const text = script;
    const lang = language;
    const mine = ++latest;
    const timer = setTimeout(() => {
      problemsIn(text, lang).then(
        (found) => mine === latest && (issues = found),
        // a check that can't run (the core busy, a session file being written) leaves the last result showing
        () => {},
      );
    }, CHECK_DELAY_MS);
    return () => clearTimeout(timer);
  });

  const errors = $derived(issues?.filter((i) => i.error).length ?? 0);
  const warnings = $derived((issues?.length ?? 0) - errors);
</script>

<div class="problems" role="region" aria-label="Problems" aria-live="polite">
  {#if issues === null}
    <p class="muted">Checking…</p>
  {:else if issues.length === 0}
    <p class="ok">No problems found.</p>
  {:else}
    <p class="summary">
      {#if errors}<span class="error">{errors} problem{errors === 1 ? "" : "s"}</span>{/if}
      {#if errors && warnings}·{/if}
      {#if warnings}<span class="warning">{warnings} warning{warnings === 1 ? "" : "s"}</span>{/if}
      <span class="muted">— click one to go to its line</span>
    </p>
    <ul>
      {#each issues as issue}
        <li>
          <button class:error={issue.error} class:warning={!issue.error} onclick={() => onGoTo?.(issue.line)}>
            Line {issue.line}: {issue.message}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .problems {
    max-height: 104px;
    overflow: auto;
    padding: 3px 6px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-input);
    font-size: 12px;
  }

  p {
    margin: 2px 0;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  li button {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    padding: 1px 2px;
    border-radius: 3px;
    font: inherit;
  }

  li button:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .muted {
    color: var(--text-muted);
  }

  .error {
    color: #e5534b;
  }

  .warning {
    color: #e3b341;
  }

  .ok {
    color: #3fb950;
  }
</style>
