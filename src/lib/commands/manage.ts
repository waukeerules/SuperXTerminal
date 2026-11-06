/**
 * The changes Manage Button Bars makes to the bars, kept apart from its window so they can be tested alone: moving
 * and copying buttons between bars, merging one bar into another, and finding the clutter an import leaves behind.
 * An import of a bar whose name is taken arrives as "Name (2)" (`buttonbars::import`, as SuperXPuTTY's
 * ButtonBarFile.Import), so the same bar often ends up twice with a few buttons different; these say which buttons
 * are the same, which only share a label, and which bars look like copies of another.
 *
 * Duplicates of a bar or a button are named as the Sessions tree names a copy: "Name-1", "Name-2"...
 *
 * Every function returns new bars and leaves the ones given alone, so the window can keep its changes until Save.
 */
import type { Bar, BarButton } from "./bars";

/** A label as compared: trimmed, any case */
function labelKey(label: string): string {
  return label.trim().toLowerCase();
}

/** Whether two buttons are the same in every way that shows or sends: label, tooltip, colour, icon, action and text */
export function sameButton(a: BarButton, b: BarButton): boolean {
  return labelKey(a.label) === labelKey(b.label) && a.tooltip === b.tooltip && a.color.toLowerCase() === b.color.toLowerCase()
    && a.icon.toLowerCase() === b.icon.toLowerCase() && a.action === b.action && a.text === b.text;
}

/** The buttons on a bar that are the same as one before them, by index: what Remove Duplicates takes off */
export function duplicates(bar: Bar): Set<number> {
  const found = new Set<number>();
  bar.buttons.forEach((button, i) => {
    if (bar.buttons.slice(0, i).some((b) => sameButton(b, button))) found.add(i);
  });
  return found;
}

/**
 * The buttons on a bar that share their label with another button there that isn't the same (another command under
 * the same name, as two imports of one bar leave), by index: shown marked, so the user can pick which to keep
 */
export function clashes(bar: Bar): Set<number> {
  const found = new Set<number>();
  bar.buttons.forEach((button, i) => {
    const other = bar.buttons.some((b, j) => j !== i && labelKey(b.label) === labelKey(button.label) && !sameButton(b, button));
    if (other) found.add(i);
  });
  return found;
}

/** A bar's name without the " (2)" an import adds when the name is taken */
export function baseName(name: string): string {
  return name.trim().replace(/\s+\(\d+\)$/, "");
}

/** Bars that look like another's copy: "Name (2)" beside "Name", as `{ copy, of }` names, for a Merge hint */
export function copies(bars: Bar[]): { copy: string; of: string }[] {
  return bars.flatMap((bar) => {
    const base = baseName(bar.name);
    if (base === bar.name.trim()) return [];
    const original = bars.find((b) => b !== bar && b.name.trim().toLowerCase() === base.toLowerCase());
    return original ? [{ copy: bar.name, of: original.name }] : [];
  });
}

/** A bar's index by name (any case), or -1 */
function find(bars: Bar[], name: string): number {
  return bars.findIndex((b) => b.name.toLowerCase() === name.toLowerCase());
}

/** A deep copy of the bars, so a change never reaches the ones given */
function copyOf(bars: Bar[]): Bar[] {
  return bars.map((b) => ({ ...b, buttons: b.buttons.map((x) => ({ ...x })), tabIcons: [...b.tabIcons] }));
}

/** What moving or copying buttons did: the bars after, and how many were left out as already on the other bar */
export interface Moved {
  bars: Bar[];
  /** Buttons not added because the bar they went to has the same one already (a move still takes them off) */
  skipped: number;
}

/**
 * Move or copy buttons from one bar to the end of another. A button the other bar already has the same of isn't
 * added twice; a move takes it off the first bar all the same, as it's on the other.
 * @param indexes the buttons, by index on `from`
 */
export function transfer(bars: Bar[], from: string, indexes: number[], to: string, copy: boolean): Moved {
  const next = copyOf(bars);
  const source = next[find(next, from)];
  const target = next[find(next, to)];
  if (!source || !target || source === target) return { bars: next, skipped: 0 };
  const chosen = new Set(indexes);
  let skipped = 0;
  for (const button of source.buttons.filter((_, i) => chosen.has(i))) {
    if (target.buttons.some((b) => sameButton(b, button))) skipped++;
    else target.buttons.push({ ...button });
  }
  if (!copy) source.buttons = source.buttons.filter((_, i) => !chosen.has(i));
  return { bars: next, skipped };
}

/** What merging did: the bars after, and what happened to the buttons of the bar merged */
export interface Merged {
  bars: Bar[];
  /** Buttons added to the bar merged into */
  added: number;
  /** Buttons left out as that bar has the same already */
  skipped: number;
  /** Of those added, the ones whose label is already there with another command: both are kept, marked */
  clashing: number;
}

/**
 * Merge one bar into another: its buttons go to the end of the other, but for ones the other has already, its tab
 * icons join the other's, and the bar goes. A button with a label already there and another command is kept, so
 * nothing is lost; the window marks both.
 */
export function merge(bars: Bar[], from: string, into: string): Merged {
  const next = copyOf(bars);
  const source = next[find(next, from)];
  const target = next[find(next, into)];
  if (!source || !target || source === target) return { bars: next, added: 0, skipped: 0, clashing: 0 };
  let added = 0;
  let skipped = 0;
  let clashing = 0;
  for (const button of source.buttons) {
    if (target.buttons.some((b) => sameButton(b, button))) {
      skipped++;
      continue;
    }
    if (target.buttons.some((b) => labelKey(b.label) === labelKey(button.label))) clashing++;
    target.buttons.push({ ...button });
    added++;
  }
  for (const icon of source.tabIcons) {
    if (!target.tabIcons.some((t) => t.toLowerCase() === icon.toLowerCase())) target.tabIcons.push(icon);
  }
  return { bars: next.filter((b) => b !== source), added, skipped, clashing };
}

/** A name not yet taken: `name`, else "name (2)", "name (3)"... as an import names a clashing bar */
export function freeName(bars: Bar[], name: string): string {
  let free = name;
  for (let n = 2; find(bars, free) >= 0; n++) free = `${name} (${n})`;
  return free;
}

/**
 * A copy's name: `name` with -1 added, else -2, -3… the first not `taken`, as the Sessions tree names a copy (and
 * SuperXPuTTY's UniqueName). Never the name itself: a copy beside its original always reads as one.
 */
export function copyName(name: string, taken: (candidate: string) => boolean): string {
  let n = 1;
  while (taken(`${name}-${n}`)) n++;
  return `${name}-${n}`;
}

/** Copy a bar, its buttons and tab icons, placed after it as "Name-1" (or the next number free) */
export function duplicateBar(bars: Bar[], name: string): Bar[] {
  const next = copyOf(bars);
  const at = find(next, name);
  if (at < 0) return next;
  const copy = { ...copyOf([next[at]])[0], name: copyName(next[at].name, (n) => find(next, n) >= 0) };
  next.splice(at + 1, 0, copy);
  return next;
}

/** What duplicating buttons did: the bars after, and where the copies are on their bar, by index */
export interface Duplicated {
  bars: Bar[];
  copies: number[];
}

/**
 * Duplicate buttons on a bar: each copy goes just after its button, labelled "Label-1" (or the next number not taken
 * on that bar, in any case), with everything else the same, ready to change into a button of its own.
 * @param indexes the buttons, by index on the bar
 */
export function duplicateButtons(bars: Bar[], name: string, indexes: number[]): Duplicated {
  const next = copyOf(bars);
  const bar = next[find(next, name)];
  if (!bar) return { bars: next, copies: [] };
  const chosen = new Set(indexes);
  const buttons: BarButton[] = [];
  const copies: number[] = [];
  const taken = (label: string) => [...bar.buttons, ...buttons].some((b) => labelKey(b.label) === labelKey(label));
  bar.buttons.forEach((button, i) => {
    buttons.push(button);
    if (!chosen.has(i)) return;
    copies.push(buttons.length);
    buttons.push({ ...button, label: copyName(button.label.trim(), taken) });
  });
  bar.buttons = buttons;
  return { bars: next, copies };
}

/** Move an item in a list from one place to another: a bar among bars, a button on its bar */
export function reorder<T>(items: T[], from: number, to: number): T[] {
  const next = [...items];
  if (from < 0 || from >= next.length || to < 0 || to >= next.length) return next;
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}
