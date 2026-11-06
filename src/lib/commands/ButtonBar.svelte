<!--
  The button bar at the bottom of the window, as SuperXPuTTY's ButtonBarStrip: the bar picker, then the bar's buttons
  (a session icon or a colour circle, and the label). A click sends the button's string, or runs its script (SPSL or Rhai), in the
  active tab; Shift+click sends to the Send Commands selection. The bar shown is the one picked by hand, until the
  active tab changes; else the active tab's session's own (Edit Session > Terminal); else its profile's (Settings >
  Profiles > Button bar); else the first whose tab
  icons include the tab's icon; else Default Settings'; else the one last picked; else the first (barFor in bars.ts). Right-click for New, Edit, Delete and Move buttons, and
  for bars: New, Rename, Delete, Show for Tab Icons, Export, Import, Reload, and Manage Button Bars (ManageBars.svelte)
  for moving, copying and tidying buttons across bars. ButtonBars.xml is in the settings folder, shared by the team,
  and read again whenever the menu opens.
-->
<script lang="ts">
  import { ask, open, save } from "@tauri-apps/plugin-dialog";
  import Icon from "../icons/Icon.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import Prompt from "../ui/Prompt.svelte";
  import EditButton from "./EditButton.svelte";
  import ManageBars from "./ManageBars.svelte";
  import TabIcons from "./TabIcons.svelte";
  import { barFor, buttonColour, exportBar, importBars, loadBars, saveBars, type Bar, type BarButton } from "./bars";
  import { listProfiles, PROFILES_CHANGED } from "../settings/profiles";
  import { activeTarget, context, fillPlaceholders, fillScriptPlaceholders, rhaiEnvReady, runRhaiIn, runScriptIn, running, selectedTargets, sendString } from "./sending.svelte";
  import { resolveSnippet } from "../scripting/asking.svelte";
  import { saveSettings, settings } from "../settings/store.svelte";
  import type { CustomIcon } from "../icons/custom";

  let { customs }: { /** The user's custom icons, for buttons with one */ customs: ReadonlyMap<string, CustomIcon> } = $props();

  let bars = $state<Bar[]>([{ name: "Default", buttons: [], tabIcons: [] }]);
  /** The active tab's icon, which picks a bar that lists it */
  let tabIcon = $state("");
  /** The profile the active tab's session uses, which picks its bar; "" for none */
  let tabProfile = $state("");
  /** The active tab's session, "store:id", whose own bar wins over its profile's; "" for none */
  let tabSession = $state("");
  /** Each profile's button bar, read again whenever Settings > Profiles saves */
  let profiles = $state<{ name: string; buttonBar: string }[]>([]);
  /** A bar picked by hand wins over the profile's and the tab icon's until the active tab changes */
  let picked = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let editing = $state<{ index: number | null; button: BarButton | null } | null>(null);
  let prompt = $state<{ title: string; value: string; validate: (v: string) => string; onOk: (v: string) => void } | null>(null);
  let choosingIcons = $state(false);
  /** Whether Manage Button Bars is open */
  let managing = $state(false);

  /** The buttons' icon size: smaller than the tree's, as SuperXPuTTY's toolbars */
  const size = $derived(Math.round(settings.iconSize * 0.85));

  /** Which bar shows, as ButtonBarStrip.CurrentBar, with the profile's bar (barFor) */
  const current = $derived(
    barFor(bars, { icon: tabIcon, session: tabSession ? context.sessionBar(tabSession) : "", profile: tabProfile, profiles, chosen: settings.activeButtonBar, picked }),
  );

  // the profiles' bars, and again when Settings > Profiles saves one
  $effect(() => {
    const read = () => void listProfiles().then((list) => (profiles = list)).catch(() => {});
    read();
    window.addEventListener(PROFILES_CHANGED, read);
    return () => window.removeEventListener(PROFILES_CHANGED, read);
  });

  // read ButtonBars.xml once the settings are in, and again when the settings folder changes
  $effect(() => {
    if (!settings.loaded) return;
    void settings.settingsFolder;
    void reload();
  });

  // follow the active tab's icon and profile (known once its session starts)
  $effect(() => {
    const tabs = context.tabs;
    if (!tabs) return;
    const update = () => {
      const tab = tabs.commandTabs().find((t) => t.active);
      const icon = tab?.icon ?? "";
      const profile = tab?.profile ?? "";
      const session = tab?.session ?? "";
      if (icon !== tabIcon || profile !== tabProfile || session !== tabSession) picked = false;
      tabIcon = icon;
      tabProfile = profile;
      tabSession = session;
    };
    update();
    return tabs.onChange(update);
  });

  /** Read ButtonBars.xml again; a problem goes in the status bar */
  async function reload(): Promise<void> {
    try {
      bars = await loadBars();
    } catch (e) {
      context.status(String(e));
    }
  }

  /** Save, and show what's saved */
  async function store(next: Bar[]): Promise<void> {
    try {
      await saveBars(next);
      bars = next;
    } catch (e) {
      context.status(String(e));
      await ask(String(e), { title: "Button bar", kind: "error" });
    }
  }

  /** Change the shown bar and save */
  function changeBar(change: (bar: Bar) => Bar): Promise<void> {
    const name = current.name;
    return store(bars.map((b) => (b.name === name ? change(structuredClone($state.snapshot(b)) as Bar) : b)));
  }

  /** Pick a bar by hand */
  async function pick(name: string): Promise<void> {
    picked = true;
    await saveSettings({ activeButtonBar: name });
  }

  /** A button was clicked, as ButtonBarStrip.RunButton */
  async function run(button: BarButton, toSelected: boolean): Promise<void> {
    const targets = toSelected ? selectedTargets() : activeTarget();
    if (targets.length === 0) {
      context.status(toSelected ? "No selected sessions to send to" : "No active session to send to");
      return;
    }
    if (settings.confirmButtons && !button.skipConfirm && !(await confirmRun(button, targets.map((t) => t.title)))) return;
    // a Rhai script as written: its braces are its own, not {key} placeholders or {prompt:} snippets
    if (button.action === "RhaiScript") {
      if (!(await rhaiEnvReady(button.text))) return;
      for (const tab of targets) runRhaiIn(tab, button.text, button.label, false, button.label, buttonColour(button));
      context.status(`Sent "${button.label}" to ${targets.length} session${targets.length === 1 ? "" : "s"}`);
      context.tabs?.focusActive();
      return;
    }
    // a script's own {variables} are its to fill as it runs
    const filled = button.action === "SpslScript" ? await fillScriptPlaceholders(button.text) : await fillPlaceholders(button.text);
    if (filled === null) return;
    const text = await resolveSnippet(filled, `Button "${button.label}"`);
    if (text === null) return;
    if (button.action === "SpslScript") {
      // its lines numbered in the button's colour, as a send string's
      for (const tab of targets) runScriptIn(tab, text, button.label, false, button.label, false, buttonColour(button));
    } else {
      await sendString(text, targets, buttonColour(button));
    }
    context.status(`Sent "${button.label}" to ${targets.length} session${targets.length === 1 ? "" : "s"}`);
    context.tabs?.focusActive();
  }

  /**
   * Ask before a button runs (Settings > General > Button bar), so a click meant for something else doesn't type into a
   * device: which button, what it does and where. True to run it.
   */
  function confirmRun(button: BarButton, tabs: string[]): Promise<boolean> {
    const where = tabs.length === 1 ? tabs[0] : `${tabs.length} tabs (${tabs.join(", ")})`;
    // a command's trailing line break is its Enter, not more to show
    const command = button.text.replace(/(\\[rn]|[\r\n])+$/, "");
    const what = button.action === "SpslScript" || button.action === "RhaiScript"
      ? "its script"
      : `"${command.split("\n")[0].slice(0, 60)}${command.length > 60 || command.includes("\n") ? "…" : ""}"`;
    return ask(`Run "${button.label}" (${what}) in ${where}?\n\nUntick Ask before it runs in Edit Button to stop it for this button, or Settings > General > Button bar for every button.`, {
      title: "Button bar",
      kind: "info",
      okLabel: "Run",
      cancelLabel: "Cancel",
    });
  }

  /** Whether a button's script is running in some tab (it shows a spinner) */
  function isRunning(button: BarButton): boolean {
    return running.runs.some((r) => r.source === button.label);
  }

  /** A button's tooltip: its own, else its command, and how to send it to the selected tabs */
  function tooltip(button: BarButton): string {
    return `${button.tooltip || button.text}\n(Shift+click: send to selected tabs)`;
  }

  /** A bar name's problem: empty, or taken */
  function nameProblem(name: string, except = ""): string {
    if (!name.trim()) return "Give the bar a name";
    if (name.trim().toLowerCase() !== except.toLowerCase() && bars.some((b) => b.name.toLowerCase() === name.trim().toLowerCase())) return "There's already a bar with that name";
    return "";
  }

  /** The right-click menu, on a button or the bar; ButtonBars.xml is read again first */
  async function showMenu(event: MouseEvent, index: number | null): Promise<void> {
    event.preventDefault();
    event.stopPropagation();
    const { clientX: x, clientY: y } = event;
    await reload();
    const count = current.buttons.length;
    const onButton = index !== null && index < count;
    menu = {
      x,
      y,
      items: [
        { label: "New Button…", action: () => (editing = { index: null, button: null }) },
        ...(onButton
          ? ([
              { label: "Edit Button…", action: () => (editing = { index, button: { ...current.buttons[index!] } }) },
              { label: "Delete Button", action: () => void deleteButton(index!) },
              { label: "Move Left", disabled: index === 0, action: () => void moveButton(index!, -1) },
              { label: "Move Right", disabled: index === count - 1, action: () => void moveButton(index!, 1) },
            ] as MenuItem[])
          : []),
        { separator: true },
        { label: "New Button Bar…", action: newBar },
        { label: "Rename Button Bar…", action: renameBar },
        { label: "Delete Button Bar", disabled: bars.length < 2 && "The only bar can't be deleted", action: () => void deleteBar() },
        { label: "Show for Tab Icons…", action: () => (choosingIcons = true) },
        { label: "Manage Button Bars…", action: () => (managing = true) },
        { separator: true },
        { label: "Export Button Bar…", action: () => void exportCurrent() },
        { label: "Import Button Bar…", action: () => void importFile() },
        { separator: true },
        { label: "Reload", action: () => void reload() },
      ],
    };
  }

  /** Delete a button, after asking */
  async function deleteButton(index: number): Promise<void> {
    const label = current.buttons[index].label;
    if (!(await ask(`Delete the button "${label}"?`, { title: "Delete Button", kind: "warning", okLabel: "Delete" }))) return;
    await changeBar((bar) => ({ ...bar, buttons: bar.buttons.filter((_, i) => i !== index) }));
  }

  /** Move a button left or right on its bar */
  async function moveButton(index: number, by: number): Promise<void> {
    await changeBar((bar) => {
      const buttons = [...bar.buttons];
      [buttons[index], buttons[index + by]] = [buttons[index + by], buttons[index]];
      return { ...bar, buttons };
    });
  }

  /** Ask for a name, then add an empty bar and show it */
  function newBar(): void {
    prompt = {
      title: "New Button Bar",
      value: "",
      validate: (name) => nameProblem(name),
      onOk: async (name) => {
        prompt = null;
        await store([...bars, { name: name.trim(), buttons: [], tabIcons: [] }]);
        await pick(name.trim());
      },
    };
  }

  /** Ask for a new name for the bar shown */
  function renameBar(): void {
    const old = current.name;
    prompt = {
      title: "Rename Button Bar",
      value: old,
      validate: (name) => nameProblem(name, old),
      onOk: async (name) => {
        prompt = null;
        await store(bars.map((b) => (b.name === old ? { ...b, name: name.trim() } : b)));
        await pick(name.trim());
      },
    };
  }

  /** Delete the bar shown and its buttons, after asking (it's gone for everyone sharing the settings folder) */
  async function deleteBar(): Promise<void> {
    const name = current.name;
    const sure = await ask(`Delete the button bar "${name}" and its ${current.buttons.length} buttons?\n\nButtonBars.xml is in the settings folder, so it goes for everyone who shares it.`,
      { title: "Delete Button Bar", kind: "warning", okLabel: "Delete" });
    if (!sure) return;
    const next = bars.filter((b) => b.name !== name);
    await store(next);
    await pick(next[0].name);
  }

  /** Save the bar shown as SuperXPuTTY's export JSON */
  async function exportCurrent(): Promise<void> {
    const path = await save({ defaultPath: `${current.name} button bar.json`, filters: [{ name: "Button bar", extensions: ["json"] }] });
    if (!path) return;
    try {
      await exportBar($state.snapshot(current) as Bar, path);
      context.status(`Exported the button bar "${current.name}"`);
    } catch (e) {
      context.status(String(e));
    }
  }

  /** Add a bar from SuperXPuTTY's export JSON, and show it */
  async function importFile(): Promise<void> {
    const path = await open({ filters: [{ name: "Button bar", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      const [next, last] = await importBars(path);
      bars = next;
      await pick(last);
      context.status(`Imported the button bar "${last}"`);
    } catch (e) {
      await ask(String(e), { title: "Import Button Bar", kind: "error" });
    }
  }

  /** Save what Manage Button Bars changed, and show the bar chosen there last */
  async function saveManaged(next: Bar[], shown: string): Promise<void> {
    managing = false;
    await store(next);
    await pick(shown);
  }

  /** Add the button edited, or put it in place of the one it was */
  async function saveButton(button: BarButton): Promise<void> {
    const index = editing?.index ?? null;
    editing = null;
    await changeBar((bar) => ({
      ...bar,
      buttons: index === null ? [...bar.buttons, button] : bar.buttons.map((b, i) => (i === index ? button : b)),
    }));
  }
</script>

<div class="buttonbar" role="toolbar" aria-label="Button bar" tabindex="-1" oncontextmenu={(e) => void showMenu(e, null)}>
  <select title="Button bar" value={current.name} onchange={(e) => void pick((e.currentTarget as HTMLSelectElement).value)}>
    {#each bars as bar (bar.name)}<option value={bar.name}>{bar.name}</option>{/each}
  </select>
  <span class="separator"></span>
  {#each current.buttons as button, i (i)}
    <button class="bar-button" title={tooltip(button)} onclick={(e) => void run(button, e.shiftKey)}
      oncontextmenu={(e) => void showMenu(e, i)}>
      {#if isRunning(button)}
        <span class="spinner" style:width="{size}px" style:height="{size}px"></span>
      {:else if button.icon}
        <Icon key={button.icon} {size} {customs} />
      {:else if button.color}
        <span class="circle" style:background={button.color} style:width="{Math.round(size * 14 / 16)}px" style:height="{Math.round(size * 14 / 16)}px"></span>
      {/if}
      <span>{button.label}</span>
    </button>
  {:else}
    <span class="empty">Right-click to add a button</span>
  {/each}
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

{#if editing}
  <EditButton button={editing.button} {customs} onCancel={() => (editing = null)} onSave={(b) => void saveButton(b)} />
{/if}

{#if prompt}
  <Prompt title={prompt.title} value={prompt.value} validate={prompt.validate} onOk={prompt.onOk} onCancel={() => (prompt = null)} />
{/if}

{#if managing}
  <ManageBars {bars} shown={current.name} {customs} onCancel={() => (managing = false)} onSave={(b, shown) => void saveManaged(b, shown)} />
{/if}

{#if choosingIcons}
  <TabIcons bar={current} {customs} onClose={() => (choosingIcons = false)}
    onChange={(icons) => void changeBar((bar) => ({ ...bar, tabIcons: icons }))} />
{/if}

<svelte:window onclick={() => (menu = null)} />

<style>
  .buttonbar {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 3px 8px;
    background: var(--bg-bar);
    border-top: 1px solid var(--border);
    overflow-x: auto;
  }

  select {
    width: 140px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 2px;
    padding: 2px 4px;
  }

  .separator {
    width: 1px;
    align-self: stretch;
    margin: 2px 6px;
    background: var(--border);
  }

  .bar-button {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
    padding: 2px 6px;
    background: none;
    border: 1px solid transparent;
    border-radius: 3px;
  }

  .bar-button:hover {
    border-color: var(--border);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .circle {
    display: inline-block;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.1);
  }

  .spinner {
    display: inline-block;
    box-sizing: border-box;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .empty {
    color: var(--text-muted);
  }
</style>
