<!--
  Quick connect's list of hosts: a small panel under the Host box with a box of several lines, one host on each, for
  typing a handful of hosts or pasting a column of them from a spreadsheet. What's in it is the Host box's own text
  the other way up: the hosts there, one to a line, and every change goes straight back to the Host box as its usual
  list with commas between (quickHosts.ts), so closing the panel loses nothing. It closes on Escape, or once the
  keyboard is somewhere else in the window (a click on a tab, another box); not when another app comes in front, as
  the hosts are often copied from one.
-->
<script lang="ts">
  import { onMount, tick } from "svelte";
  import { MOD } from "../platform";
  import { splitHosts } from "./quickHosts";

  interface Props {
    /** The Host box's text as the panel opens: its hosts are the lines to begin with */
    hosts: string;
    /** What the panel belongs to (the Host box and the button that opens it): it goes under it, as wide, and a click there doesn't close it */
    anchor: HTMLElement;
    /** The hosts changed: the Host box's new text, with commas between */
    onChange: (hosts: string) => void;
    /** Close the panel; `refocus` when the keyboard should go back to the Host box (Escape), not where it was put */
    onClose: (refocus: boolean) => void;
    /** Connect to the hosts listed (Cmd or Ctrl with Enter; Enter alone starts a new line) */
    onConnect: () => void;
  }

  let { hosts, anchor, onChange, onClose, onConnect }: Props = $props();

  /** The lines as typed: read from the Host box once, as the panel opens, so typing isn't rewritten under the cursor */
  // svelte-ignore state_referenced_locally
  let text = $state(splitHosts(hosts).join("\n"));
  let box: HTMLTextAreaElement;
  /** Where the panel goes: under the Host box, on top of the tabs (fixed, so the toolbar doesn't cut it off) */
  let place = $state({ left: 0, top: 0, width: 0 });
  /** How many hosts the lines hold, each counted once, as they'd open */
  const count = $derived(splitHosts(text).length);

  onMount(() => {
    const around = anchor.getBoundingClientRect();
    place = { left: around.left, top: around.bottom + 2, width: Math.max(around.width, 260) };
    void tick().then(() => box.focus());
  });

  /** A line changed: the Host box follows */
  function changed(): void {
    onChange(splitHosts(text).join(", "));
  }

  /** Escape closes the panel (and nothing else); Cmd or Ctrl with Enter connects */
  function keydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose(true);
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      onClose(true);
      onConnect();
    }
  }

  /**
   * The keyboard left the box: close, once it's somewhere else in this window. Checked a moment later, when the
   * window knows where it went: into a tab's page counts (the tab's frame has it), but another app in front doesn't
   * (the window has no keyboard at all then, and the box has it again on coming back).
   */
  function left(): void {
    setTimeout(() => {
      if (document.hasFocus() && !anchor.contains(document.activeElement)) onClose(false);
    });
  }
</script>

<div class="panel" style:left="{place.left}px" style:top="{place.top}px" style:width="{place.width}px">
  <textarea bind:this={box} bind:value={text} rows="8" aria-label="One host on each line" placeholder={"192.168.0.5\nsw1.example.com\nadmin@10.0.0.7:2222"}
    oninput={changed} onkeydown={keydown} onfocusout={left}></textarea>
  <div class="note">
    <span>One host on each line: each opens in a tab of its own.</span>
    <span class="count">{count} {count === 1 ? "host" : "hosts"} · {MOD}+Enter connects</span>
  </div>
</div>

<style>
  .panel {
    position: fixed;
    z-index: 300;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px;
    background: var(--bg-bar);
    border: 1px solid var(--border);
    border-radius: 4px;
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.35);
  }

  textarea {
    resize: vertical;
    min-height: 60px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 2px;
    padding: 3px 4px;
    font: inherit;
    color: inherit;
    /* a long host stays on its line, as a line is a host */
    white-space: pre;
  }

  .note {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 2px 12px;
    font-size: 0.85em;
    color: var(--text-muted);
  }
</style>
