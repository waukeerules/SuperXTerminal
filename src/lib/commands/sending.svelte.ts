/**
 * Sending to tabs, shared by the Send Commands toolbar, the button bar, the Script Editor and sessions' own scripts,
 * as SuperXPuTTY's frmSuperPutty and ButtonBarStrip: which tabs get it (the active tab, or the Send Commands
 * selection), {key} placeholders filled from Environment Variables, {prompt:} and {secret:} values asked for, then
 * the text typed or the script run in each tab. Running scripts are listed for the status bar and its Stop.
 */
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import type { CommandTab, TabArea } from "../layout/TabArea";
import { settings } from "../settings/store.svelte";
import { resolveSnippet, askOne } from "../scripting/asking.svelte";
import { parseSendString } from "../scripting/sendString";
import { clipboardText } from "../clipboard";
import { ScriptRun, runScript, scriptVariables, SESSION_VARIABLES, type ScriptHost } from "../scripting/spsl";
import { envNames, runRhai } from "../scripting/rhai";
import type { SessionView } from "../sessions/api";

/**
 * The window's tab area and what scripts need from the window; App sets them once it has them. Reactive: App sets
 * them in its onMount, which runs after its children's effects, so the button bar's effect first sees no tabs and
 * must run again when they come, or it never follows the active tab. The tab area itself isn't made reactive (a
 * class instance stays as it is), only which one is set.
 */
export const context: {
  tabs: TabArea | null;
  status: (text: string) => void;
  openSession: (id: string) => boolean;
  editVariables: () => void;
  /** A session's own button bar by its "store:id" (the tree's, so it's read again when Edit Session saves); "" for none */
  sessionBar: (key: string) => string;
  /** A session by its "store:id", as the Sessions tree has it; undefined for none (a Local Shell, Quick connect) */
  sessionOf: (key: string) => SessionView | undefined;
} = $state({
  tabs: null,
  sessionBar: () => "",
  sessionOf: () => undefined,
  status: () => {},
  openSession: () => false,
  editVariables: () => {},
});

/** Send Commands' icon group: only tabs with this session icon; "" for every tab */
export const selection = $state({ iconGroup: "" });

/** The scripts running now, for the status bar */
export const running = $state<{ runs: { run: ScriptRun; tab: string; tabId: string; started: number; source: string }[] }>({ runs: [] });

/** Tabs that can be typed into */
function receiving(tabs: CommandTab[]): CommandTab[] {
  return tabs.filter((t) => t.host.canReceiveCommands);
}

/** The active tab, if it can be typed into */
export function activeTarget(): CommandTab[] {
  return receiving((context.tabs?.commandTabs() ?? []).filter((t) => t.active));
}

/**
 * The Send Commands selection, as GetSelectedPuttyPanels: tabs in the order they show, only the icon group's if one
 * is chosen, each either visible ("Send to visible tabs only") or with Accept commands on
 */
export function selectedTargets(): CommandTab[] {
  return receiving(
    (context.tabs?.commandTabs() ?? []).filter(
      (t) =>
        (!selection.iconGroup || t.icon.split("#")[0].toLowerCase() === selection.iconGroup.toLowerCase()) &&
        (settings.sendToVisibleOnly ? t.visible : t.acceptCommands),
    ),
  );
}

/**
 * Fill {key} placeholders from Environment Variables, as SessionVariables: an undefined one offers to open Environment
 * Variables, and declining sends nothing (null)
 */
export async function fillPlaceholders(text: string): Promise<string | null> {
  const filled = await invoke<{ text: string; missing: string[] }>("variables_fill", { text });
  if (filled.missing.length === 0) return filled.text;
  const define = await ask(
    `${filled.missing.join(", ")} ${filled.missing.length === 1 ? "has" : "have"} no value. Define ${filled.missing.length === 1 ? "it" : "them"} in Environment Variables?`,
    { title: "Environment Variables", kind: "warning", okLabel: "Open Environment Variables", cancelLabel: "Cancel" },
  );
  if (define) context.editVariables();
  return null;
}

/**
 * Type a button's send string into tabs, step by step, as ButtonBarStrip's SendString; `color` is the button's, which
 * the numbers of the lines it brings take (Line Numbers)
 */
export async function sendString(text: string, targets: CommandTab[], color = ""): Promise<void> {
  const steps = await parseSendString(text, clipboardText);
  await Promise.all(
    targets.map(async (tab) => {
      tab.host.markCommand?.(color);
      for (const step of steps) {
        tab.host.markOutput(false);
        await tab.host.type(step.text);
        if (step.delayMs) await new Promise((r) => setTimeout(r, step.delayMs));
      }
    }),
  );
}

/** What scripts ask the window for: PROMPT and PWDPROMPT, OPENSESSION, the status bar, SAVE's file and CALL's script */
const host: ScriptHost = {
  ask: askOne,
  openSession: (id) => context.openSession(id),
  status: (text) => context.status(text),
  save: (path, text, append) => invoke<string>("script_output_save", { path, text, append }),
  loadScript: (name) => invoke<string>("script_load", { name, kind: "spsl" }),
};

/**
 * Fill a script's Environment Variables in before it runs, as fillPlaceholders, leaving the script's own {variables}
 * (SET, /var:, a match's {0}…{9} and named groups) for it to fill as it runs: they aren't Environment Variables, and
 * asking to define them would stop the script. Null when one is missing (the window offers Environment Variables).
 */
export async function fillScriptPlaceholders(script: string): Promise<string | null> {
  const own = scriptVariables(script);
  // kept aside as text no Environment Variable can match, then put back
  const hidden = script.replace(/\{(\w+)\}/g, (whole, name: string) => (own.has(name.toLowerCase()) ? `\u0000${name}\u0001` : whole));
  const filled = await fillPlaceholders(hidden);
  return filled === null ? null : filled.replace(/\u0000(\w+)\u0001/g, "{$1}");
}

/**
 * The session's values a script in its tab starts with (SESSION_VARIABLES), as the Sessions tree has it; none for a
 * tab without a session (a Local Shell, Quick connect)
 */
export function sessionValues(key: string): Record<string, string> {
  const session = key ? context.sessionOf(key) : undefined;
  if (!session) return {};
  return {
    host_address: session.host,
    // a session saved without a port connects on its protocol's usual one
    host_port: session.port ? String(session.port) : ({ SSH: "22", TELNET: "23", RDP: "3389" } as Record<string, string>)[session.protocol.toUpperCase() || "SSH"] ?? "",
    session_name: session.name,
    session_id: session.id,
    session_folder: session.folder,
    session_username: session.username,
    session_profile: session.profile,
    session_protocol: session.protocol,
  };
}

/**
 * The session values a script in a tab starts with: its saved session's, or for a Quick connect tab the host and
 * login typed (TabArea.setScriptValues), so a jump server profile's script goes on to the host typed
 */
export function tabValues(tab: { session: string; values?: Record<string, string> }): Record<string, string> {
  return { ...sessionValues(tab.session), ...tab.values };
}

/**
 * Run an SPSL script in a tab, as ctlPuttyPanel.RunScript; `fromStart` for a session's own script (its first WAITFOR
 * sees everything since the session started). `source` is what started it (a button's label), for its spinner.
 * `stepping` (the Script Editor's Step) pauses before each line until Next line in the status bar. `color` is the
 * button's that started it: the lines it types are numbered in it (Line Numbers), as a send string's.
 */
export function runScriptIn(tab: CommandTab, script: string, name: string, fromStart = false, source = "", stepping = false, color = ""): void {
  const run = new ScriptRun(name, tab.title, lineCount(script), stepping);
  run.color = color;
  track(run, tab, source, runScript(tab.host, host, run, script, fromStart, tabValues(tab)));
}

/**
 * Whether the Environment Variables a Rhai script reads (`env.name`) all have values; for one that hasn't, offers to
 * open Environment Variables as a missing {key} does for SPSL (fillPlaceholders), and the script doesn't run. A
 * session value read as one (`env.host_address`) stops it too, saying to write `session.host_address`. Its
 * values themselves are filled in the core when it runs, so a secret never comes to the window.
 */
export async function rhaiEnvReady(script: string): Promise<boolean> {
  const all = envNames(script);
  // a session value read as an Environment Variable can't be defined there: say what to write instead
  const misplaced = all.find((n) => SESSION_VARIABLES.includes(n.name));
  if (misplaced) {
    context.status(`The Rhai script didn't run: line ${misplaced.line}: env.${misplaced.name} is the tab's session's: write session.${misplaced.name}`);
    return false;
  }
  const names = all;
  if (names.length === 0) return true;
  return (await fillPlaceholders(names.map((n) => `{${n.name}}`).join(" "))) !== null;
}

/**
 * Run a Rhai script in a tab (rhai.ts), as runScriptIn runs an SPSL one: the tab's session's values are its `session`
 * map and Environment Variables its `env` (rhaiEnvReady first); `color` as runScriptIn's. No Step yet, and no {key}
 * placeholders filled, since braces are Rhai's own
 */
export function runRhaiIn(tab: CommandTab, script: string, name: string, fromStart = false, source = "", color = ""): void {
  const run = new ScriptRun(name, tab.title, lineCount(script));
  run.color = color;
  track(run, tab, source, runRhai(tab.host, host, run, script, fromStart, tabValues(tab)));
}

/** A script's own lines, comments included, as the editor numbers them and a run reports them */
function lineCount(script: string): number {
  return script.replace(/\s+$/, "").split("\n").length;
}

/** List a run for the status bar (its line and Stop) until it ends */
function track(run: ScriptRun, tab: CommandTab, source: string, ending: Promise<void>): void {
  const entry = { run, tab: tab.title, tabId: tab.id, started: Date.now(), source };
  running.runs = [...running.runs, entry];
  void ending.finally(() => {
    // the list holds proxies of the entries, so compare the run (a class, which isn't proxied)
    running.runs = running.runs.filter((r) => r.run !== run);
  });
}
