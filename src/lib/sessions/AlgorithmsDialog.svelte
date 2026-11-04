<!--
  SSH algorithms…, from Settings > Profiles and Edit Session: a profile's or a session's own key exchange, host key,
  cipher and MAC lists, as PuTTY's Kex, Host keys and Cipher panels. Each kind is either left to what comes before it
  (the session's profile, Default Settings, or ssh's own list with the older algorithms) or given as an ordered list
  of the algorithms ticked, from what this computer's ssh has. OK hands back the text ("kex=a,b;mac=c",
  src-tauri/src/algorithms.rs), which the profile or session saves. `inline` shows the lists without the window, as
  Edit Session's SSH tab does: each change is handed back at once (onChange), and there's no OK or Cancel.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  interface Props {
    /** The window's title, naming the session or profile */
    title: string;
    /** The lists as stored ("" for none) */
    text: string;
    /** What a kind left alone uses, e.g. "its profile's list" or "ssh's own list" */
    otherwise: string;
    /** OK: the lists as they're now stored ("" for none) */
    onDone?: (text: string) => void;
    /** Cancel, or Escape: nothing changes */
    onCancel?: () => void;
    /** Shown in a tab rather than its own window: no title, OK or Cancel, and every change goes to `onChange` */
    inline?: boolean;
    /**
     * Inline: the lists after a change, as they'd be stored, and why they can't be ("" when they can): a kind with
     * its own list must have one ticked, or ssh couldn't connect
     */
    onChange?: (text: string, problem: string) => void;
  }

  let { title, text, otherwise, onDone = () => {}, onCancel = () => {}, inline = false, onChange = () => {} }: Props = $props();

  /** One kind as algorithms.rs's Available gives it */
  interface Available {
    key: string;
    label: string;
    supported: string[];
    defaults: string[];
  }

  /** One kind being edited: whether it has its own list, and every algorithm with whether it's ticked */
  interface Kind {
    key: string;
    label: string;
    own: boolean;
    rows: { name: string; on: boolean; known: boolean }[];
  }

  /** Each kind as the "Use its own list of …" box names it */
  const PHRASES: Record<string, string> = { kex: "key exchange algorithms", hostkey: "host key algorithms", cipher: "ciphers", mac: "MACs" };

  let kinds = $state<Kind[]>([]);
  let shown = $state("kex");
  let error = $state("");

  /**
   * The lists a stored text sets, by kind key
   * @param value "kex=a,b;mac=c"
   * @returns Each kind's names, in order
   */
  function parse(value: string): Record<string, string[]> {
    const out: Record<string, string[]> = {};
    for (const part of value.split(";")) {
      const at = part.indexOf("=");
      if (at < 0) continue;
      const names = part.slice(at + 1).split(",").map((n) => n.trim()).filter(Boolean);
      if (names.length) out[part.slice(0, at).trim().toLowerCase()] = [...new Set(names)];
    }
    return out;
  }

  onMount(async () => {
    try {
      const available = await invoke<Available[]>("ssh_algorithms_available");
      const stored = parse(untrack(() => text));
      kinds = available.map((a) => {
        const own = stored[a.key];
        // its own list first, in its order; otherwise ssh's default order; then everything else ssh has, unticked
        const first = own ?? a.defaults.filter((d) => a.supported.includes(d));
        const rows = first.map((name) => ({ name, on: true, known: a.supported.length === 0 || a.supported.includes(name) }));
        for (const name of a.supported) if (!first.includes(name)) rows.push({ name, on: false, known: true });
        return { key: a.key, label: a.label, own: !!own, rows };
      });
    } catch (e) {
      error = `Couldn't ask ssh which algorithms it has: ${e}`;
    }
  });

  /** The kind whose list is shown (Key exchange, Host key, Cipher, MAC) */
  const current = $derived(kinds.find((k) => k.key === shown));

  /**
   * Move an algorithm up or down its list, as PuTTY's Up and Down
   * @param kind The list
   * @param index Where the algorithm is
   * @param by -1 up, 1 down
   */
  function move(kind: Kind, index: number, by: number): void {
    const to = index + by;
    if (to < 0 || to >= kind.rows.length) return;
    const rows = [...kind.rows];
    [rows[index], rows[to]] = [rows[to], rows[index]];
    kind.rows = rows;
  }

  /** The kinds with their own list but nothing ticked, which ssh couldn't connect with */
  const empty = $derived(kinds.filter((k) => k.own && !k.rows.some((r) => r.on)).map((k) => k.label));

  /** The text for what's chosen: each kind with its own list, its ticked algorithms in order */
  function result(): string {
    return kinds
      .filter((k) => k.own)
      .map((k) => `${k.key}=${k.rows.filter((r) => r.on).map((r) => r.name).join(",")}`)
      .join(";");
  }

  /** The lists as first shown, written as result() writes them, to tell a change from how they were stored */
  let opened: string | null = null;

  // inline: hand back each change; the lists as they were opened are handed back as the text they came as, so
  // writing them another way isn't taken for a change
  $effect(() => {
    if (!inline || kinds.length === 0) return;
    const now = result();
    const problem = empty.length ? `Tick at least one in ${empty.join(", ")}` : "";
    if (opened === null) {
      opened = now;
      return;
    }
    onChange(now === opened ? untrack(() => text) : now, problem);
  });
</script>

<!-- Escape cancels the window; inline, it's left to the window around the lists -->
<svelte:window onkeydown={(e) => { if (!inline && e.key === "Escape") { e.stopPropagation(); onCancel(); } }} />

{#if inline}
  <div class="inline">
    {@render lists()}
    <footer>
      <button onclick={() => kinds.forEach((k) => (k.own = false))} title="Every kind back to {otherwise}">Use Defaults</button>
      <span class="error">{empty.length ? `Tick at least one in ${empty.join(", ")}` : ""}</span>
    </footer>
  </div>
{:else}

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
    <header>{title}</header>
    {@render lists()}

    <footer>
      <button onclick={() => kinds.forEach((k) => (k.own = false))} title="Every kind back to {otherwise}">Use Defaults</button>
      <span class="error">{empty.length ? `Tick at least one in ${empty.join(", ")}` : ""}</span>
      <span class="grow"></span>
      <button class="primary" disabled={empty.length > 0 || !!error} onclick={() => onDone(result())}>OK</button>
      <button onclick={onCancel}>Cancel</button>
    </footer>
  </div>
</div>
{/if}

<!-- the note, the kinds and the list of the one shown: the window's, or an Edit Session tab's -->
{#snippet lists()}
    <p class="note">For devices that need particular algorithms. A kind left alone uses {otherwise}; given its own list,
      ssh offers only the ticked algorithms, in this order.</p>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="tabs" role="tablist">
      {#each kinds as kind (kind.key)}
        <button role="tab" class:active={shown === kind.key} aria-selected={shown === kind.key} onclick={() => (shown = kind.key)}>
          {kind.label}{kind.own ? " ●" : ""}</button>
      {/each}
    </div>

    {#if current}
      <label class="own"><input type="checkbox" bind:checked={current.own} /> Use its own list of {PHRASES[current.key] ?? current.label}</label>
      <div class="list" class:off={!current.own} role="list">
        {#each current.rows as row, i (row.name)}
          <div class="row" role="listitem">
            <input type="checkbox" bind:checked={row.on} disabled={!current.own} aria-label={row.name} />
            <span class="name" class:unknown={!row.known} title={row.known ? "" : "This computer's ssh doesn't have it: ssh won't start with it in a list"}>{row.name}</span>
            <button disabled={!current.own || i === 0} onclick={() => move(current, i, -1)} aria-label="Up">↑</button>
            <button disabled={!current.own || i === current.rows.length - 1} onclick={() => move(current, i, 1)} aria-label="Down">↓</button>
          </div>
        {/each}
      </div>
    {/if}
{/snippet}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 140;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(560px, 94vw);
    max-height: 94vh;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    font-weight: 600;
  }

  .inline {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .note {
    margin: 0;
    color: var(--text-muted);
  }

  .error {
    color: var(--danger, #e05555);
  }

  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border);
  }

  .tabs button {
    background: none;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    padding: 4px 10px;
  }

  .tabs button.active {
    background: var(--bg-input);
    border-color: var(--border);
  }

  .list {
    height: 300px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
  }

  .list.off {
    opacity: 0.55;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 6px;
  }

  .name {
    flex: 1;
    font-family: ui-monospace, "Cascadia Mono", Menlo, monospace;
    font-size: 0.92em;
  }

  .name.unknown {
    color: var(--danger, #e05555);
    text-decoration: line-through;
  }

  .row button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 6px;
  }

  footer {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .grow {
    flex: 1;
  }
</style>
