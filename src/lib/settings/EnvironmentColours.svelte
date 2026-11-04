<!--
  Tools > Environment Colours…, as SuperXPuTTY's dlgEnvironmentColors: the name rules that colour sessions by their
  folders' or their own names, e.g. "Prod*" red (the environmentColorRules setting, SuperXPuTTY's
  EnvironmentColorRules). One row per rule; the first rule that matches wins, so they can be moved up and down.
  A colour set on a folder or a session, from the Sessions tree, comes first.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { normalizeColor, parseRules } from "../icons/environment";
  import { PALETTE } from "../icons/palette";
  import { saveSettings, settings } from "./store.svelte";

  interface Props {
    /** OK, Cancel or Escape: the window closes */
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  /** SuperXPuTTY's EnvironmentColors.DefaultRules */
  const DEFAULT_RULES = "Prod=#C62828;Production=#C62828;Staging=#EF6C00;UAT=#EF6C00";

  let key = 0;
  /** One rule as the list edits it, with a key of its own so rows keep their place as they move */
  type Rule = { key: number; pattern: string; color: string };
  /** Rules as the list edits them, each with a new key */
  const rulesOf = (text: string): Rule[] => parseRules(text).map((r) => ({ key: key++, pattern: r.pattern, color: r.color }));

  let rules = $state<Rule[]>(untrack(() => rulesOf(settings.environmentColorRules)));
  let current = $state<number | null>(null);
  let error = $state("");

  /** Black or white text, whichever reads on the colour */
  function textOn(color: string): string {
    const hex = normalizeColor(color);
    if (!hex) return "inherit";
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    return 0.2126 * r + 0.7152 * g + 0.0722 * b < 0.55 ? "#fff" : "#000";
  }

  /** Add a rule, in red to start with, and select it */
  function add(): void {
    const rule = { key: key++, pattern: "", color: "#C62828" };
    rules = [...rules, rule];
    current = rule.key;
    // start typing its pattern
    queueMicrotask(() => document.querySelector<HTMLInputElement>(`[data-rule="${rule.key}"]`)?.focus());
  }

  /** Remove the selected rule */
  function remove(): void {
    rules = rules.filter((r) => r.key !== current);
    current = null;
  }

  /** Move the selected rule up or down: the first rule that matches wins */
  function move(by: number): void {
    const from = rules.findIndex((r) => r.key === current);
    const to = from + by;
    if (from < 0 || to < 0 || to >= rules.length) return;
    const next = [...rules];
    [next[from], next[to]] = [next[to], next[from]];
    rules = next;
  }

  /** Save the rules, refusing a colour that isn't #RRGGBB */
  async function save(): Promise<void> {
    const kept = rules.filter((r) => r.pattern.trim());
    const bad = kept.find((r) => !normalizeColor(r.color));
    if (bad) {
      current = bad.key;
      error = `"${bad.color}" isn't a colour. Use #RRGGBB, e.g. #C62828, or choose one.`;
      return;
    }
    // stored as SuperXPuTTY's FormatRules: "pattern=#RRGGBB" joined with ";" (a ";" in a pattern can't be kept)
    const text = kept.map((r) => `${r.pattern.trim().replaceAll(";", "")}=${normalizeColor(r.color)}`).join(";");
    try {
      await saveSettings({ environmentColorRules: text });
      onClose();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Environment Colours">
    <header>Environment Colours</header>
    <p class="note">Sessions get the colour of the first rule matching their own name or one of their folders' names
      (nearest first). * and ? are wildcards; case doesn't matter. A colour set on a folder or a session (from its menu
      in the Sessions tree) comes first.</p>

    <div class="body">
      <div class="table" role="grid">
        <div class="head" role="row"><span role="columnheader">Folder or session name</span><span role="columnheader">Colour</span></div>
        {#each rules as rule (rule.key)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div class="rule" class:current={rule.key === current} role="row" tabindex="-1" onclick={() => (current = rule.key)}>
            <input class="pattern" data-rule={rule.key} bind:value={rule.pattern} placeholder="e.g. Prod*" onfocus={() => (current = rule.key)} aria-label="Name" />
            <span class="colour">
              <input type="color" value={normalizeColor(rule.color) ?? "#000000"} onfocus={() => (current = rule.key)}
                oninput={(e) => (rule.color = e.currentTarget.value.toUpperCase())} aria-label="Choose the colour" />
              <input class="hex" bind:value={rule.color} onfocus={() => (current = rule.key)} aria-label="Colour as #RRGGBB"
                style:background={normalizeColor(rule.color) ?? ""} style:color={textOn(rule.color)} />
            </span>
          </div>
        {:else}
          <p class="empty">No rules: sessions are coloured only by their own or their folders' colours.</p>
        {/each}
      </div>

      <div class="side">
        <button onclick={add}>Add</button>
        <button disabled={current === null} onclick={remove}>Remove</button>
        <button disabled={current === null} onclick={() => move(-1)}>Move up</button>
        <button disabled={current === null} onclick={() => move(1)}>Move down</button>
        <button onclick={() => { rules = rulesOf(DEFAULT_RULES); current = null; }}>Defaults</button>
        <div class="palette" title="The usual colours: sets the selected rule's">
          {#each PALETTE as p}
            <button class="swatch" style:background={p.color} aria-label={p.name} title={p.name} disabled={current === null}
              onclick={() => { const rule = rules.find((r) => r.key === current); if (rule) rule.color = p.color; }}></button>
          {/each}
        </div>
      </div>
    </div>

    <footer>
      <span class="error">{error}</span>
      <button class="primary" onclick={save}>OK</button>
      <button onclick={onClose}>Cancel</button>
    </footer>
  </div>
</div>

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
    width: min(600px, 94vw);
    max-height: 94vh;
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

  .note {
    margin: 0;
    color: var(--text-muted);
  }

  .body {
    display: flex;
    gap: 10px;
    min-height: 0;
  }

  .table {
    flex: 1;
    min-width: 0;
    height: 260px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: var(--bg-input);
  }

  .head,
  .rule {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 150px;
    gap: 6px;
    align-items: center;
    padding: 3px 6px;
  }

  .head {
    position: sticky;
    top: 0;
    background: var(--bg-pane);
    color: var(--text-muted);
    font-size: 12px;
    border-bottom: 1px solid var(--border);
  }

  .rule.current {
    background: var(--selection, rgb(80 120 200 / 0.3));
  }

  .colour {
    display: flex;
    gap: 4px;
    align-items: center;
  }

  input[type="color"] {
    width: 28px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 4px;
    background: none;
  }

  .hex {
    width: 100%;
    font-family: ui-monospace, monospace;
  }

  .empty {
    margin: 8px;
    color: var(--text-muted);
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 110px;
  }

  .palette {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 3px;
    margin-top: 6px;
  }

  .swatch {
    height: 18px;
    padding: 0;
    border: 1px solid var(--border);
  }

  input,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  :disabled {
    opacity: 0.45;
  }

  footer {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
  }

  .error {
    flex: 1;
    color: #e5534b;
  }
</style>
