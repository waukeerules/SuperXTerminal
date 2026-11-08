<!--
  The Settings window (Cmd+,), SuperXPuTTY's Tools > Options for the Mac: General, Profiles, Terminal, Remote Desktop and Shortcuts.

  Its search box finds a setting in any tab: the other tabs are drawn out of sight while searching, their section
  titles, labels and shortcut names read from them, and a result opens its tab with the setting scrolled to and
  flashed. The open tab stays drawn (hidden) behind the results, so a profile being edited keeps its changes.
-->
<script lang="ts" module>
  let shown = $state(false);

  /** Whether Settings is open, so a dialog it was opened over (Edit Session's Profiles…) leaves Escape to it */
  export function settingsShown(): boolean {
    return shown;
  }
</script>

<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import ProfilesTab from "./ProfilesTab.svelte";
  import GeneralTab from "./GeneralTab.svelte";
  import TerminalTab from "./TerminalTab.svelte";
  import ShortcutsTab from "./ShortcutsTab.svelte";
  import RemoteDesktopTab from "./RemoteDesktopTab.svelte";

  interface Props {
    /** Close, or Escape */
    onClose: () => void;
    /** The tab to open on */
    tab?: string;
    /** On the Profiles tab, the profile to show first */
    profile?: string | null;
    /** And a field of it to scroll to and light up (its name in Profile, e.g. "forwards") */
    field?: string | null;
  }

  let { onClose, tab = "General", profile = null, field = null }: Props = $props();

  let current = $state(untrack(() => tab));

  onMount(() => {
    shown = true;
    return () => (shown = false);
  });

  /** A setting found by the search: its tab, the section it's in, and its own text */
  interface Found {
    tab: string;
    section: string;
    text: string;
  }

  let query = $state("");
  let entries = $state<Found[]>([]);
  let searchBox = $state<HTMLInputElement>();
  let main = $state<HTMLElement>();
  let away = $state<HTMLElement>();

  /** What a search reads: section titles, labels and the shortcut names */
  const SEARCHED = "legend, label, h3, [role='cell'].label";

  /** An element's own words: a label's, without its drop-down's choices */
  function wordsOf(element: Element): string {
    const copy = element.cloneNode(true) as Element;
    copy.querySelectorAll("select, textarea, button, .sample").forEach((e) => e.remove());
    return (copy.textContent ?? "").replace(/\s+/g, " ").trim();
  }

  /** The section an element is in: its fieldset's title */
  function sectionOf(element: Element): string {
    const fieldset = element.closest("fieldset");
    const legend = fieldset?.querySelector(":scope > legend");
    return legend && legend !== element ? wordsOf(legend) : "";
  }

  /** The settings in a tab's drawing */
  function read(tab: string, root: Element): Found[] {
    return [...root.querySelectorAll(SEARCHED)].map((e) => ({ tab, section: sectionOf(e), text: wordsOf(e) })).filter((f) => f.text);
  }

  /** Read every tab again: the open one, and the others drawn out of sight */
  function readAll(): void {
    const found: Found[] = [];
    for (const t of TABS) {
      const root = t.name === current ? main?.querySelector(".tab") : away?.querySelector(`[data-tab="${t.name}"]`);
      if (root) found.push(...read(t.name, root));
    }
    entries = found;
  }

  // the tabs fill in as they load (profiles, shortcuts): read them again when they change
  $effect(() => {
    if (!query.trim() || !away) return;
    readAll();
    let timer = 0;
    const watcher = new MutationObserver(() => {
      window.clearTimeout(timer);
      timer = window.setTimeout(readAll, 50);
    });
    watcher.observe(away, { childList: true, subtree: true, characterData: true });
    return () => {
      watcher.disconnect();
      window.clearTimeout(timer);
    };
  });

  /** The settings matching every word typed, in any order, in their text, section or tab */
  const results = $derived.by(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (!words.length) return [];
    const seen = new Set<string>();
    return entries.filter((f) => {
      const all = `${f.tab} ${f.section} ${f.text}`.toLowerCase();
      const key = `${f.tab}|${f.section}|${f.text}`;
      if (!words.every((w) => all.includes(w)) || seen.has(key)) return false;
      seen.add(key);
      return true;
    }).slice(0, 200);
  });

  /** Open a result's tab, scroll to the setting and flash it */
  async function go(found: Found): Promise<void> {
    query = "";
    current = found.tab;
    // the tab may still be loading what it lists: look for the setting a little while
    for (let tries = 0; tries < 40; tries++) {
      await tick();
      const target = [...(main?.querySelectorAll(SEARCHED) ?? [])]
        .find((e) => wordsOf(e) === found.text && sectionOf(e) === found.section);
      if (target) {
        const row = (target.closest("[role='row']") ?? target) as HTMLElement;
        row.scrollIntoView({ block: "center" });
        row.classList.remove("found");
        void row.offsetWidth;
        row.classList.add("found");
        window.setTimeout(() => row.classList.remove("found"), 1600);
        return;
      }
      await new Promise((r) => window.setTimeout(r, 25));
    }
  }

  /** Cmd+F (Ctrl+F) goes to the search box; Escape clears it, then closes */
  function onKey(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      if (query) {
        query = "";
        e.stopPropagation();
      } else onClose();
    } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      searchBox?.focus();
      searchBox?.select();
    }
  }

  /** The tabs, with the milestone that brings each one still to come */
  const TABS: { name: string; later?: string }[] = [
    { name: "General" },
    { name: "Profiles" },
    { name: "Terminal" },
    { name: "Remote Desktop" },
    { name: "Shortcuts" },
  ];
</script>

<svelte:window onkeydown={onKey} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Settings">
    <header>
      <nav>
        {#each TABS as tab}
          <button class:active={tab.name === current} disabled={!!tab.later} title={tab.later ? `${tab.name} ${tab.later}` : ""}
            onclick={() => (current = tab.name)}>
            {tab.name}
          </button>
        {/each}
      </nav>
      <input class="search" type="search" bind:this={searchBox} bind:value={query} placeholder="Search settings"
        aria-label="Search settings" onkeydown={(e) => e.key === "Enter" && results[0] && void go(results[0])} />
      <button class="close" onclick={onClose} aria-label="Close">✕</button>
    </header>
    <main bind:this={main}>
      <div class="tab" hidden={!!query.trim()}>
        {@render tabContent(current, true)}
      </div>
      {#if query.trim()}
        <ul class="results" aria-label="Settings found">
          {#each results as found}
            <li><button onclick={() => void go(found)}>
              <span class="where">{found.tab}{found.section ? ` › ${found.section}` : ""}</span>
              <span>{found.text}</span>
            </button></li>
          {:else}
            <li class="none">No settings match “{query.trim()}”.</li>
          {/each}
        </ul>
        <!-- the other tabs, out of sight, for the search to read -->
        <div class="away" bind:this={away} aria-hidden="true" inert>
          {#each TABS.filter((t) => t.name !== current) as t (t.name)}
            <div data-tab={t.name}>{@render tabContent(t.name, false)}</div>
          {/each}
        </div>
      {/if}
    </main>
  </div>
</div>

{#snippet tabContent(name: string, shown: boolean)}
  {#if name === "General"}<GeneralTab />{:else if name === "Terminal"}<TerminalTab />{:else if name === "Remote Desktop"}<RemoteDesktopTab />{:else if name === "Shortcuts"}<ShortcutsTab />{:else}<ProfilesTab initial={shown ? profile : null} field={shown ? field : null} onLeave={onClose} />{/if}
{/snippet}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    /* above Edit Session (120), which opens it from Profiles…; below the dialogs Settings opens itself (140) */
    z-index: 125;
  }

  .dialog {
    width: min(860px, 94vw);
    height: min(720px, 92vh);
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
    display: flex;
    flex-direction: column;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px 0;
    border-bottom: 1px solid var(--border);
  }

  nav {
    display: flex;
    gap: 2px;
  }

  nav button {
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 6px 6px 0 0;
    background: none;
    padding: 6px 14px;
  }

  nav button.active {
    background: var(--bg-pane);
    border-color: var(--border);
  }

  nav button:disabled {
    opacity: 0.45;
  }

  .search {
    margin-left: auto;
    margin-right: 8px;
    width: 220px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 8px;
  }

  .tab {
    height: 100%;
  }

  .results {
    height: 100%;
    overflow: auto;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .results button {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 4px;
    padding: 5px 8px;
  }

  .results button:hover,
  .results button:focus-visible {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .where {
    color: var(--text-muted);
    font-size: 12px;
  }

  .none {
    padding: 8px;
    color: var(--text-muted);
  }

  /* drawn so their words can be read, but never seen or reached */
  .away {
    position: fixed;
    left: -10000px;
    top: 0;
    width: 800px;
    visibility: hidden;
  }

  /* the setting a result went to, flashed */
  main :global(.found) {
    animation: found 1.6s ease-out;
    border-radius: 4px;
  }

  @keyframes found {
    0%, 40% { background: color-mix(in srgb, var(--accent) 35%, transparent); }
    100% { background: transparent; }
  }

  .close {
    border: 0;
    background: none;
  }

  main {
    flex: 1;
    min-height: 0;
    padding: 12px 16px 16px;
    background: var(--bg-pane);
  }
</style>
