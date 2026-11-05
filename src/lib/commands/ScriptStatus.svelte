<!--
  The running scripts in the status bar, as SuperXPuTTY's: "▶ Name (tab): line i/n · STEP · Ns", or "▶ N scripts
  running"; click for Stop per script, and Stop all. A script being stepped through (the Script Editor's Step) shows
  Next line while it's paused before a line; the menu has it too. Hidden when none runs.
-->
<script lang="ts">
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { running } from "./sending.svelte";

  let now = $state(Date.now());
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  // tick while something runs, so the seconds and the line move
  $effect(() => {
    if (running.runs.length === 0) return;
    const timer = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(timer);
  });

  /** A run's line in the status bar: its name, tab, line, step and seconds so far */
  function describe(entry: (typeof running.runs)[number]): string {
    const { run } = entry;
    return `${run.name} (${entry.tab}): line ${run.line}/${run.lineCount} · ${run.step} · ${Math.round((now - entry.started) / 1000)}s`;
  }

  /** Open the menu above the status bar: Stop for each script, and Stop all */
  function showMenu(event: MouseEvent): void {
    event.stopPropagation();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: rect.left,
      y: rect.top - 8,
      items: [
        ...running.runs.filter((entry) => entry.run.paused).map((entry) => ({ label: `Next line  ${entry.run.name} (${entry.tab})`, action: () => entry.run.next() })),
        ...running.runs.map((entry) => ({ label: `Stop  ${entry.run.name} (${entry.tab})`, action: () => entry.run.stop() })),
        ...(running.runs.length > 1 ? [{ separator: true } as MenuItem, { label: "Stop all", action: () => running.runs.forEach((e) => e.run.stop()) }] : []),
      ],
    };
  }
</script>

{#if running.runs.length}
  <button class="scripts" title="Click to stop" onclick={showMenu}>
    ▶ {running.runs.length === 1 ? describe(running.runs[0]) : `${running.runs.length} scripts running`}
  </button>
  <!-- read again at each tick: a run's pause isn't reactive state -->
  {#each running.runs.filter((entry) => (void now, entry.run.paused)) as entry (entry)}
    <button class="next" title="Run line {entry.run.line} of {entry.run.name} ({entry.tab})" onclick={() => entry.run.next()}>Next line ▸</button>
  {/each}
{/if}

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

<svelte:window onclick={() => (menu = null)} />

<style>
  .scripts {
    border: 0;
    background: none;
    padding: 0 6px;
    color: inherit;
    font-size: 12px;
  }

  .scripts:hover {
    text-decoration: underline;
  }

  .next {
    font-size: 12px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-input);
  }
</style>
