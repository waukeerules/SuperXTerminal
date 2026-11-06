<!--
  Manage Button Bars: every bar on the left, the chosen bar's buttons on the right, for tidying up what the right-click
  menu does one button at a time. Buttons can be moved or copied to another bar (the Move to and Copy to menus, or
  dragged onto a bar, Alt/Option to copy), put in order, edited and deleted; bars can be added, renamed, copied, put in
  order, merged into another and deleted. A bar imported under a name already taken arrives as "Name (2)", so a bar
  that looks like another's copy gets a Merge hint, a button the same as one before it is marked Duplicate (Remove
  Duplicates takes those off), and buttons sharing a label with another command are marked so the user can choose.

  Nothing is written until Save, which hands the bars back to the button bar to write ButtonBars.xml; Cancel or
  Escape leaves the file as it was. The changes themselves are in `manage.ts`.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import Icon from "../icons/Icon.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import Prompt from "../ui/Prompt.svelte";
  import EditButton from "./EditButton.svelte";
  import TabIcons from "./TabIcons.svelte";
  import type { CustomIcon } from "../icons/custom";
  import type { Bar, BarButton } from "./bars";
  import { clashes, copies, duplicateBar, duplicates, freeName, merge, reorder, transfer } from "./manage";

  interface Props {
    /** The bars as ButtonBars.xml has them, read just before the window opened */
    bars: Bar[];
    /** The bar chosen first: the one the button bar shows */
    shown: string;
    /** The user's custom icons, for buttons with one */
    customs: ReadonlyMap<string, CustomIcon>;
    /** Save: the bars as changed, and the bar chosen last, which the button bar then shows */
    onSave: (bars: Bar[], shown: string) => void;
    /** Cancel, or Escape: nothing changes */
    onCancel: () => void;
  }

  let { bars, shown, customs, onSave, onCancel }: Props = $props();

  /** The bars as changed so far */
  let draft = $state<Bar[]>(untrack(() => structuredClone($state.snapshot(bars)) as Bar[]));
  /** The bar whose buttons show on the right, by name */
  let chosen = $state(untrack(() => (bars.some((b) => b.name === shown) ? shown : bars[0]?.name ?? "")));
  /** The buttons ticked, by index on the chosen bar */
  let ticked = $state<Set<number>>(new Set());
  /** What the last change did, at the foot of the window */
  let said = $state("");
  /** Whether anything has changed, so Cancel can tell */
  let changed = $state(false);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let editing = $state<{ index: number | null; button: BarButton | null } | null>(null);
  let prompt = $state<{ title: string; value: string; validate: (v: string) => string; onOk: (v: string) => void } | null>(null);
  let choosingIcons = $state(false);
  /** What's being dragged: a bar (to put in order) or ticked buttons (to put in order, or onto a bar) */
  let dragging = $state<{ kind: "bar"; index: number } | { kind: "buttons"; indexes: number[] } | null>(null);
  /** Where a drag would drop, for the line or highlight it shows */
  let over = $state<{ kind: "bar" | "button"; index: number } | null>(null);

  /** The chosen bar */
  const bar = $derived(draft.find((b) => b.name === chosen) ?? draft[0]);
  /** Its buttons the same as one before them */
  const repeated = $derived(bar ? duplicates(bar) : new Set<number>());
  /** Its buttons sharing a label with another command */
  const clashing = $derived(bar ? clashes(bar) : new Set<number>());
  /** Bars that look like copies of another, for the Merge hints */
  const hints = $derived(copies(draft));
  /** The ticked buttons, in order */
  const selection = $derived([...ticked].sort((a, b) => a - b));

  /** Put changed bars in place, say what was done, and keep the chosen bar if it's still there */
  function apply(next: Bar[], message: string, choose = chosen): void {
    draft = next;
    chosen = next.some((b) => b.name === choose) ? choose : next[0].name;
    said = message;
    changed = true;
  }

  /** Choose a bar, which clears the ticks */
  function choose(name: string): void {
    chosen = name;
    ticked = new Set();
  }

  /** Tick or untick a button */
  function tick(index: number): void {
    const next = new Set(ticked);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    ticked = next;
  }

  /** "1 button" or "3 buttons" */
  function count(n: number): string {
    return `${n} button${n === 1 ? "" : "s"}`;
  }

  /** A bar name's problem: empty, or taken */
  function nameProblem(name: string, except = ""): string {
    if (!name.trim()) return "Give the bar a name";
    const taken = draft.some((b) => b.name.toLowerCase() === name.trim().toLowerCase());
    if (name.trim().toLowerCase() !== except.toLowerCase() && taken) return "There's already a bar with that name";
    return "";
  }

  /** A menu of the other bars, under the control clicked */
  function barMenu(event: MouseEvent, run: (name: string) => void): void {
    event.stopPropagation();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const others = draft.filter((b) => b.name !== chosen);
    menu = { x: rect.left, y: rect.bottom + 2, items: others.map((b) => ({ label: b.name, action: () => run(b.name) })) };
  }

  /** Move or copy the ticked buttons (or those given) to another bar */
  function send(to: string, copy: boolean, indexes = selection): void {
    if (indexes.length === 0) return;
    const { bars: next, skipped } = transfer(draft, chosen, indexes, to, copy);
    const done = indexes.length - skipped;
    const verb = copy ? "Copied" : "Moved";
    const left = skipped ? `; ${count(skipped)} already there${copy ? "" : ", taken off here"}` : "";
    ticked = new Set();
    apply(next, `${verb} ${count(done)} to "${to}"${left}`);
  }

  /** Delete the ticked buttons */
  function deleteButtons(): void {
    const gone = new Set(selection);
    const n = gone.size;
    ticked = new Set();
    apply(draft.map((b) => (b.name === chosen ? { ...b, buttons: b.buttons.filter((_, i) => !gone.has(i)) } : b)), `Deleted ${count(n)}`);
  }

  /** Take off the buttons the same as one before them */
  function removeDuplicates(): void {
    const n = repeated.size;
    const gone = repeated;
    ticked = new Set();
    apply(draft.map((b) => (b.name === chosen ? { ...b, buttons: b.buttons.filter((_, i) => !gone.has(i)) } : b)), `Removed ${count(n)} the same as another`);
  }

  /** Move the one ticked button up or down */
  function moveButton(by: number): void {
    const [index] = selection;
    const to = index + by;
    apply(draft.map((b) => (b.name === chosen ? { ...b, buttons: reorder(b.buttons, index, to) } : b)), said);
    ticked = new Set([to]);
  }

  /** Move the chosen bar up or down the list (the button bar's picker follows this order) */
  function moveBar(by: number): void {
    const index = draft.findIndex((b) => b.name === chosen);
    apply(reorder(draft, index, index + by), said);
  }

  /** Add the button edited, or put it in place of the one it was */
  function saveButton(button: BarButton): void {
    const index = editing?.index ?? null;
    editing = null;
    apply(draft.map((b) => (b.name === chosen
      ? { ...b, buttons: index === null ? [...b.buttons, button] : b.buttons.map((x, i) => (i === index ? button : x)) }
      : b)), index === null ? `Added "${button.label}"` : `Changed "${button.label}"`);
  }

  /** Ask for a name, then add an empty bar and choose it */
  function newBar(): void {
    prompt = {
      title: "New Button Bar",
      value: "",
      validate: (name) => nameProblem(name),
      onOk: (name) => {
        prompt = null;
        ticked = new Set();
        apply([...draft, { name, buttons: [], tabIcons: [] }], `Added the bar "${name}"`, name);
      },
    };
  }

  /** Ask for a new name for the chosen bar */
  function renameBar(): void {
    const old = chosen;
    prompt = {
      title: "Rename Button Bar",
      value: old,
      validate: (name) => nameProblem(name, old),
      onOk: (name) => {
        prompt = null;
        apply(draft.map((b) => (b.name === old ? { ...b, name } : b)), `Renamed "${old}" to "${name}"`, name);
      },
    };
  }

  /** Copy the chosen bar, and choose the copy */
  function copyBar(): void {
    const next = duplicateBar(draft, chosen);
    const index = next.findIndex((b) => b.name === chosen) + 1;
    ticked = new Set();
    apply(next, `Copied "${chosen}" as "${next[index].name}"`, next[index].name);
  }

  /** Merge one bar into another, and choose the one merged into */
  function mergeBar(from: string, into: string): void {
    const result = merge(draft, from, into);
    const parts = [`Merged "${from}" into "${into}": ${count(result.added)} added`];
    if (result.skipped) parts.push(`${result.skipped} already there`);
    if (result.clashing) parts.push(`${result.clashing} with a label already there and another command, marked`);
    ticked = new Set();
    apply(result.bars, parts.join(", "), into);
  }

  /** Delete the chosen bar (it's only gone from the file at Save) */
  function deleteBar(): void {
    const name = chosen;
    const n = bar.buttons.length;
    ticked = new Set();
    apply(draft.filter((b) => b.name !== name), `Deleted the bar "${name}" and its ${count(n)}`);
  }

  /** Cancel, after asking when there are changes */
  async function cancel(): Promise<void> {
    if (changed && !(await ask("Leave without saving your changes to the button bars?", { title: "Manage Button Bars", kind: "warning", okLabel: "Leave" }))) return;
    onCancel();
  }

  /** Whether one of this window's own dialogs is open, which take Escape first */
  const inner = $derived(!!(editing || prompt || choosingIcons || menu));

  /** Start dragging a button: the ticked ones if it's ticked, else it alone */
  function dragButton(event: DragEvent, index: number): void {
    const indexes = ticked.has(index) ? selection : [index];
    dragging = { kind: "buttons", indexes };
    event.dataTransfer?.setData("text/plain", bar.buttons[index].label);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "copyMove";
  }

  /** Drop on a bar: buttons go to it (Alt/Option copies), or a bar dragged takes its place */
  function dropOnBar(event: DragEvent, index: number): void {
    event.preventDefault();
    const drag = dragging;
    dragging = null;
    over = null;
    if (!drag) return;
    if (drag.kind === "bar") {
      apply(reorder(draft, drag.index, index), said);
    } else if (draft[index].name !== chosen) {
      send(draft[index].name, event.altKey, drag.indexes);
    }
  }

  /** Drop on a button: one button dragged takes its place */
  function dropOnButton(event: DragEvent, index: number): void {
    event.preventDefault();
    const drag = dragging;
    dragging = null;
    over = null;
    if (drag?.kind !== "buttons" || drag.indexes.length !== 1) return;
    apply(draft.map((b) => (b.name === chosen ? { ...b, buttons: reorder(b.buttons, drag.indexes[0], index) } : b)), said);
    ticked = new Set([index]);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !inner && void cancel()} onclick={() => (menu = null)} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Manage Button Bars">
    <header>Manage Button Bars</header>

    {#each hints as hint (hint.copy)}
      <p class="hint">
        "{hint.copy}" looks like a copy of "{hint.of}".
        <button onclick={() => mergeBar(hint.copy, hint.of)}>Merge into "{hint.of}"</button>
      </p>
    {/each}

    <div class="columns">
      <section aria-label="Bars">
        <h3>Bars</h3>
        <div class="list" role="listbox" aria-label="Bars">
          {#each draft as b, i (b.name)}
            <div class="row" class:chosen={b.name === chosen} class:over={over?.kind === "bar" && over.index === i}
              role="option" aria-selected={b.name === chosen} tabindex="0" draggable="true"
              onclick={() => choose(b.name)} onkeydown={(e) => e.key === "Enter" && choose(b.name)}
              ondragstart={() => (dragging = { kind: "bar", index: i })}
              ondragover={(e) => { e.preventDefault(); over = { kind: "bar", index: i }; }}
              ondragleave={() => (over = null)} ondrop={(e) => dropOnBar(e, i)}>
              <span class="name">{b.name}</span>
              <span class="muted">{b.buttons.length}</span>
            </div>
          {/each}
        </div>
        <div class="tools">
          <button onclick={newBar}>New…</button>
          <button onclick={renameBar}>Rename…</button>
          <button onclick={copyBar}>Copy</button>
          <button disabled={draft.length < 2} onclick={(e) => barMenu(e, (into) => mergeBar(chosen, into))}>Merge into ▾</button>
          <button onclick={() => (choosingIcons = true)}>Tab Icons…</button>
          <button aria-label="Delete bar" disabled={draft.length < 2} title={draft.length < 2 ? "The only bar can't be deleted" : ""} onclick={deleteBar}>Delete</button>
          <button aria-label="Move bar up" disabled={draft[0]?.name === chosen} onclick={() => moveBar(-1)}>▲</button>
          <button aria-label="Move bar down" disabled={draft.at(-1)?.name === chosen} onclick={() => moveBar(1)}>▼</button>
        </div>
      </section>

      <section aria-label="Buttons">
        <h3>Buttons on "{bar?.name}"</h3>
        <div class="list" role="list" aria-label="Buttons">
          {#each bar?.buttons ?? [] as button, i (i)}
            <div class="row button" role="listitem" class:ticked={ticked.has(i)} class:over={over?.kind === "button" && over.index === i}
              draggable="true" ondragstart={(e) => dragButton(e, i)}
              ondragover={(e) => { e.preventDefault(); over = { kind: "button", index: i }; }}
              ondragleave={() => (over = null)} ondrop={(e) => dropOnButton(e, i)}
              ondblclick={() => (editing = { index: i, button: { ...button } })}>
              <input type="checkbox" aria-label={`Tick "${button.label}"`} checked={ticked.has(i)} onchange={() => tick(i)} />
              <span class="icon">
                {#if button.icon}<Icon key={button.icon} size={16} {customs} />
                {:else if button.color}<span class="circle" style:background={button.color}></span>{/if}
              </span>
              <span class="name">{button.label}</span>
              <span class="text muted" title={button.text}>{button.action === "SpslScript" || button.action === "RhaiScript" ? "Script" : button.text.split("\n")[0]}</span>
              {#if repeated.has(i)}<span class="mark" title="The same as a button before it">Duplicate</span>
              {:else if clashing.has(i)}<span class="mark" title="Another button has this label and another command">Same label</span>{/if}
            </div>
          {:else}
            <p class="muted empty">No buttons</p>
          {/each}
        </div>
        <div class="tools">
          <button onclick={() => (editing = { index: null, button: null })}>New…</button>
          <button disabled={selection.length !== 1} onclick={() => (editing = { index: selection[0], button: { ...bar.buttons[selection[0]] } })}>Edit…</button>
          <button disabled={selection.length === 0 || draft.length < 2} onclick={(e) => barMenu(e, (to) => send(to, false))}>Move to ▾</button>
          <button disabled={selection.length === 0 || draft.length < 2} onclick={(e) => barMenu(e, (to) => send(to, true))}>Copy to ▾</button>
          <button aria-label="Delete buttons" disabled={selection.length === 0} onclick={deleteButtons}>Delete</button>
          <button disabled={repeated.size === 0} onclick={removeDuplicates}>Remove Duplicates</button>
          <button aria-label="Move button up" disabled={selection.length !== 1 || selection[0] === 0} onclick={() => moveButton(-1)}>▲</button>
          <button aria-label="Move button down" disabled={selection.length !== 1 || selection[0] === (bar?.buttons.length ?? 0) - 1} onclick={() => moveButton(1)}>▼</button>
        </div>
      </section>
    </div>

    <p class="help muted">Tick buttons to move, copy or delete them. Drag buttons onto a bar to move them there (hold Alt/Option to copy), or drag them and bars to put them in order. Double-click a button to edit it.</p>

    <footer>
      <span class="said" role="status">{said}</span>
      <button onclick={() => void cancel()}>Cancel</button>
      <button class="primary" onclick={() => onSave($state.snapshot(draft) as Bar[], chosen)}>Save</button>
    </footer>
  </div>
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

{#if editing}
  <EditButton button={editing.button} {customs} onCancel={() => (editing = null)} onSave={saveButton} />
{/if}

{#if prompt}
  <Prompt title={prompt.title} value={prompt.value} validate={prompt.validate} onOk={prompt.onOk} onCancel={() => (prompt = null)} />
{/if}

{#if choosingIcons && bar}
  <TabIcons {bar} {customs} onClose={() => (choosingIcons = false)}
    onChange={(icons) => apply(draft.map((b) => (b.name === chosen ? { ...b, tabIcons: icons } : b)), `Changed the tab icons of "${chosen}"`)} />
{/if}

<style>
  /* under Edit Button, Show for Tab Icons and the name prompt, which open over it */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 125;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(860px, 96vw);
    max-height: 92vh;
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    font-weight: 600;
  }

  h3 {
    margin: 0 0 4px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .hint {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .columns {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) 2.4fr;
    gap: 12px;
    min-height: 0;
  }

  section {
    display: grid;
    grid-template-rows: auto 1fr auto;
    min-width: 0;
  }

  .list {
    height: 340px;
    overflow: auto;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 3px;
    border-top: 2px solid transparent;
    cursor: default;
  }

  .row.chosen {
    background: var(--accent);
    color: #fff;
  }

  .row.chosen .muted {
    color: inherit;
  }

  .row.ticked {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .row.over {
    border-top-color: var(--accent);
  }

  .name {
    flex: none;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .list[role="listbox"] .name {
    flex: 1;
  }

  .text {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .icon {
    display: inline-flex;
    width: 16px;
    justify-content: center;
  }

  .circle {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }

  .mark {
    flex: none;
    font-size: 11px;
    padding: 0 5px;
    border-radius: 8px;
    color: #fff;
    background: #d08a1c;
  }

  .muted {
    color: var(--text-muted);
  }

  .empty {
    margin: 8px;
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 6px;
  }

  .help {
    margin: 0;
    font-size: 12px;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .said {
    flex: 1;
    color: var(--text-muted);
  }

  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 9px;
  }

  button:disabled {
    opacity: 0.45;
  }

  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
</style>
