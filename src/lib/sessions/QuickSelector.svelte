<!--
  The Quick Selector, as SuperXPuTTY's QuickSelector: File > Open Session… (every session) and Window > Switch
  Session… (the open tabs), and a Jump host's ▾ (every session, with the list each is in as a column of its own).
  Type to filter: every word (split at spaces, dots and *) must be in the name, the detail or that list, ignoring
  case. Enter takes the highlighted one (the first, until the arrows move it), Esc or its ✕ closes, a
  click takes one (SuperXPuTTY's needs a double-click; one click only highlighted it, which looked broken, and pickers
  such as VS Code's take a click), and Option+Delete removes the last "/" part of what's typed.
-->
<script lang="ts" module>
  /** One entry: what it's called, its detail (a session id), and its icon */
  export interface QuickItem {
    key: string;
    name: string;
    detail: string;
    icon: string;
    environment: string | null;
    /** Ticked: the active tab, for Switch */
    current?: boolean;
    /**
     * Which list it's in, a column of its own (Saved, Private, Database), for a picker where the same path can be in
     * more than one (a Jump host's ▾); matched by the filter too
     */
    group?: string;
  }
</script>

<script lang="ts">
  import { tick, untrack } from "svelte";
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";

  interface Props {
    /** "Open Session" or "Switch Session", shown with the count */
    title: string;
    /** What can be chosen: sessions, or open tabs */
    items: QuickItem[];
    /** The user's custom icons, for the list */
    customs: ReadonlyMap<string, CustomIcon>;
    /** Enter or a click on a row: the item chosen */
    onChoose: (item: QuickItem) => void;
    /** Escape, or a click outside the window */
    onClose: () => void;
  }

  let { title, items, customs, onChoose, onClose }: Props = $props();

  let text = $state("");
  let index = $state(0);
  let list: HTMLDivElement;

  /** The words typed, as SuperXPuTTY's FormatFilterString: split at spaces, dots and *, with * [ , dropped */
  const words = $derived(text.split(/[ .*]/).map((w) => w.replace(/[*[,]/g, "").toLowerCase()).filter(Boolean));
  /** The items with every word in their name or detail, in detail order, as SuperXPuTTY's list */
  const shown = $derived(
    items
      .filter((item) => words.every((w) => [item.name, item.detail, item.group ?? ""].some((t) => t.toLowerCase().includes(w))))
      .sort((a, b) => a.detail.localeCompare(b.detail, undefined, { sensitivity: "base", numeric: true })),
  );

  // a new filter starts again at the top, on the first that isn't the active tab, so Enter switches
  $effect(() => {
    void words;
    index = Math.max(0, untrack(() => shown).findIndex((item) => !item.current));
  });

  /** Move the highlight up or down, keeping it in view */
  async function move(by: number): Promise<void> {
    if (shown.length === 0) return;
    index = Math.max(0, Math.min(shown.length - 1, index + by));
    await tick();
    list?.querySelector(".row.current")?.scrollIntoView({ block: "nearest" });
  }

  /** Choose an item, if there is one */
  function choose(item: QuickItem | undefined): void {
    if (item) onChoose(item);
  }

  /** The search box's keys: Escape closes, Enter chooses, the arrows and Page keys move the highlight, Ctrl+Backspace goes back to the last "/" */
  function key(event: KeyboardEvent): void {
    // kept from the window, so a dialog it was opened over (Edit Session, for a Jump host) doesn't close too
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
    }
    else if (event.key === "Enter") choose(shown[index]);
    else if (event.key === "ArrowDown") void move(1);
    else if (event.key === "ArrowUp") void move(-1);
    else if (event.key === "PageDown") void move(10);
    else if (event.key === "PageUp") void move(-10);
    else if (event.key === "Backspace" && (event.altKey || event.ctrlKey)) {
      // back to the last "/", as SuperXPuTTY's Ctrl+Backspace
      const at = text.lastIndexOf("/");
      text = at >= 0 ? text.slice(0, at) : "";
    } else return;
    event.preventDefault();
  }

  /** Focus the box when the window opens */
  function focus(element: HTMLInputElement): void {
    element.focus();
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
    <header><span>{title} [{shown.length}]</span>
      <!-- closes it without choosing, as Escape and a click outside do -->
      <button class="close" onclick={onClose} aria-label="Close" title="Close without choosing">✕</button></header>
    <input use:focus bind:value={text} onkeydown={key} placeholder="Type to filter: words in the name or the folder path"
      aria-label="Filter" spellcheck="false" autocomplete="off" />
    <div class="list" role="listbox" bind:this={list}>
      {#each shown as item, i (item.key)}
        <div class="row" class:current={i === index} role="option" aria-selected={i === index} tabindex="-1"
          onmousemove={() => (index = i)} onclick={() => choose(item)} onkeydown={() => {}}>
          <span class="tick">{item.current ? "✓" : ""}</span>
          <Icon key={item.icon} size={18} environment={item.environment} {customs} />
          <span class="name">{item.name}</span>
          <span class="detail">{item.detail}</span>
          {#if item.group}<span class="group">{item.group}</span>{/if}
        </div>
      {:else}
        <p class="empty">Nothing matches.</p>
      {/each}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    background: rgb(0 0 0 / 0.25);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
  }

  .dialog {
    width: min(640px, 94vw);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px 12px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.45);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-weight: 600;
    color: var(--text-muted);
    font-size: 12px;
  }

  .close {
    border: 0;
    background: none;
    padding: 0 2px;
    color: var(--text-muted);
    font-size: 14px;
  }

  .close:hover {
    color: var(--text);
  }

  input {
    font-size: 15px;
    padding: 6px 8px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 6px;
  }

  .list {
    max-height: 50vh;
    overflow: auto;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 8px 3px 4px;
    border-radius: 5px;
    white-space: nowrap;
    cursor: default;
  }

  .row.current {
    background: var(--accent);
    color: #fff;
  }

  .row.current .detail {
    color: rgb(255 255 255 / 0.8);
  }

  .tick {
    width: 12px;
    text-align: center;
  }

  .name {
    flex: none;
  }

  /* the list it's in, after its path, in a column of the same width on every row */
  .group {
    flex: none;
    width: 64px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .row.current .group {
    color: rgb(255 255 255 / 0.8);
  }

  .detail {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: right;
    color: var(--text-muted);
    font-size: 12px;
  }

  .empty {
    margin: 8px;
    color: var(--text-muted);
  }
</style>
