<!--
  Search Output, as SuperXPuTTY's dlgSearchOutput: find text in everything a tab has printed (its session log), with
  Match case and Regex; up to 5,000 matching lines with their line numbers, double-click one to copy it, and Open log
  to see the whole file. Floats over the window without blocking it; one per tab.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  interface Props {
    title: string;
    logPath: string | null;
    onClose: () => void;
  }

  let { title, logPath, onClose }: Props = $props();

  const LIMIT = 5000;

  let query = $state("");
  let matchCase = $state(false);
  let regex = $state(false);
  let results = $state<{ line: number; text: string }[]>([]);
  let summary = $state("Type text to find in what this tab has printed, then press Return.");
  let list: HTMLDivElement | undefined = $state();
  let at = $state({ x: 160, y: 120 });
  let box: HTMLInputElement | undefined = $state();

  // ready to type in
  $effect(() => box?.focus());

  /** The log as lines of plain text, as SessionLog.CleanForDisplay: escape sequences and control characters gone */
  function clean(text: string): string[] {
    return text
      .replace(/\x1b?\[[0-9;?]*[A-Za-z]/g, "")
      .replace(/\x1b?\][0-9]+;[^\x07\x1b\n]*(\x07|\x1b\\)/g, "")
      .replace(/[\x00-\x08\x0b-\x1f\x7f]/g, "")
      .split("\n");
  }

  async function find(): Promise<void> {
    if (!logPath) {
      summary = "This tab has no log yet.";
      return;
    }
    let test: (line: string) => boolean;
    if (regex) {
      try {
        const pattern = new RegExp(query, matchCase ? "" : "i");
        test = (line) => pattern.test(line);
      } catch (e) {
        summary = `Not a valid regular expression: ${e instanceof Error ? e.message : e}`;
        return;
      }
    } else {
      const needle = matchCase ? query : query.toLowerCase();
      test = (line) => (matchCase ? line : line.toLowerCase()).includes(needle);
    }
    const [text] = await invoke<[string, number]>("log_read", { path: logPath, offset: 0 });
    const lines = clean(text);
    const found: { line: number; text: string }[] = [];
    let capped = false;
    for (let i = 0; i < lines.length; i++) {
      if (!test(lines[i])) continue;
      if (found.length === LIMIT) {
        capped = true;
        break;
      }
      found.push({ line: i + 1, text: lines[i] });
    }
    results = found;
    summary = `${found.length}${capped ? "+" : ""} matching line${found.length === 1 ? "" : "s"} in ${lines.length} line${lines.length === 1 ? "" : "s"} of output. Double-click a line to copy it.`;
    // the newest match in view
    queueMicrotask(() => list?.scrollTo({ top: list.scrollHeight }));
  }

  async function copy(result: { line: number; text: string }): Promise<void> {
    await navigator.clipboard.writeText(result.text);
    summary = `Copied line ${result.line} to the clipboard.`;
  }

  /** Drag the window by its title */
  function drag(event: PointerEvent): void {
    const start = { x: event.clientX - at.x, y: event.clientY - at.y };
    const move = (e: PointerEvent) => (at = { x: Math.max(0, e.clientX - start.x), y: Math.max(0, e.clientY - start.y) });
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
</script>

<div class="search" role="dialog" aria-label="Search Output: {title}" style:left="{at.x}px" style:top="{at.y}px">
  <header role="toolbar" tabindex="-1" aria-label="Move the window" onpointerdown={drag}>
    <span>Search Output: {title}</span>
    <button class="close" aria-label="Close" onclick={onClose}>×</button>
  </header>
  <div class="row">
    <input bind:this={box} bind:value={query} placeholder="Find" aria-label="Find" onkeydown={(e) => e.key === "Enter" && void find()} />
    <label><input type="checkbox" bind:checked={matchCase} /> Match case</label>
    <label><input type="checkbox" bind:checked={regex} /> Regex</label>
    <button class="primary" onclick={() => void find()}>Find</button>
  </div>
  <div class="list" bind:this={list} role="listbox" aria-label="Matching lines">
    <div class="head"><span>Line</span><span>Text</span></div>
    {#each results as result (result.line)}
      <div class="result" role="option" aria-selected="false" tabindex="-1" ondblclick={() => void copy(result)}
        onkeydown={(e) => e.key === "Enter" && void copy(result)}>
        <span class="number">{result.line}</span><span class="text">{result.text}</span>
      </div>
    {/each}
  </div>
  <footer>
    <span class="summary">{summary}</span>
    <button disabled={!logPath} onclick={() => logPath && void invoke("open_path", { path: logPath })}>Open log</button>
  </footer>
</div>

<style>
  .search {
    position: fixed;
    z-index: 120;
    width: min(752px, 96vw);
    height: 472px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0 12px 12px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding-top: 8px;
    color: var(--text-muted);
    cursor: move;
    outline: none;
  }

  .close {
    border: 0;
    background: none;
    font-size: 18px;
  }

  .row,
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .row input:not([type]) {
    flex: 1;
  }

  .list {
    flex: 1;
    overflow: auto;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-family: Menlo, "Courier New", monospace;
    font-size: 12px;
  }

  .head,
  .result {
    display: grid;
    grid-template-columns: 70px 1fr;
    gap: 8px;
    padding: 1px 6px;
  }

  .head {
    position: sticky;
    top: 0;
    background: var(--bg-pane);
    color: var(--text-muted);
    font-family: inherit;
  }

  .result:hover {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }

  .number {
    color: var(--text-muted);
    text-align: right;
  }

  .text {
    white-space: pre;
  }

  .summary {
    flex: 1;
    color: var(--text-muted);
  }

  input,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
  }

  button:disabled {
    opacity: 0.5;
  }
</style>
