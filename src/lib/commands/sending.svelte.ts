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
import { ScriptRun, runScript, type ScriptHost } from "../scripting/spsl";

/** The window's tab area and what scripts need from the window; App sets them once it has them */
export const context: { tabs: TabArea | null; status: (text: string) => void; openSession: (id: string) => boolean; editVariables: () => void } = {
  tabs: null,
  status: () => {},
  openSession: () => false,
  editVariables: () => {},
};

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

/** Type a button's send string into tabs, step by step, as ButtonBarStrip's SendString */
export async function sendString(text: string, targets: CommandTab[]): Promise<void> {
  const steps = await parseSendString(text, () => navigator.clipboard.readText().catch(() => ""));
  await Promise.all(
    targets.map(async (tab) => {
      for (const step of steps) {
        tab.host.markOutput(false);
        await tab.host.type(step.text);
        if (step.delayMs) await new Promise((r) => setTimeout(r, step.delayMs));
      }
    }),
  );
}

/** What scripts ask the window for: PROMPT and PWDPROMPT, OPENSESSION, the status bar */
const host: ScriptHost = {
  ask: askOne,
  openSession: (id) => context.openSession(id),
  status: (text) => context.status(text),
};

/**
 * Run an SPSL script in a tab, as ctlPuttyPanel.RunScript; `fromStart` for a session's own script (its first WAITFOR
 * sees everything since the session started). `source` is what started it (a button's label), for its spinner.
 */
export function runScriptIn(tab: CommandTab, script: string, name: string, fromStart = false, source = ""): void {
  const lines = script.split("\n").filter((l) => l.trim() && !l.trimStart().startsWith("#")).length;
  const run = new ScriptRun(name, tab.title, lines);
  const entry = { run, tab: tab.title, tabId: tab.id, started: Date.now(), source };
  running.runs = [...running.runs, entry];
  void runScript(tab.host, host, run, script, fromStart).finally(() => {
    // the list holds proxies of the entries, so compare the run (a class, which isn't proxied)
    running.runs = running.runs.filter((r) => r.run !== run);
  });
}
