<!--
  Settings > General > Look's Edit Themes…: the window's own themes (uiThemes.svelte.ts), each a name, the built-in
  theme it starts from (Light or Dark) and its colours: the window's background, bars, panes, fields, borders, text,
  muted text, selected rows and highlights, and checkboxes. A colour left alone is its base's, and Reset gives one
  back to it. The theme chosen in the list is drawn on the whole app while the window is open, so a change shows as
  it's made; Cancel puts the app back as it was. Save writes UiThemes.json in the user's folder; Use This Theme saves
  and makes it Settings' theme. A theme in use that's renamed stays in use, and one deleted goes back to Follow the
  system.
-->
<script lang="ts" module>
  let shown = $state(false);

  /** Whether Edit Themes is open, so Settings, which it opens over, leaves Escape to it */
  export function themesEditorShown(): boolean {
    return shown;
  }
</script>

<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { SYSTEM } from "../platform";
  import { applyTheme, saveSettings, settings } from "./store.svelte";
  import { applyUiTheme, BASE_COLOURS, COLOUR_TOKENS, saveUiThemes, uiThemes, type UiTheme } from "./uiThemes.svelte";

  interface Props {
    /** Saved, Cancel or Escape: the window closes */
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  /** The themes being edited, each with the name it was saved under ("" for a new one), to follow a rename */
  let drafts = $state<{ theme: UiTheme; was: string }[]>(
    untrack(() => uiThemes.list.map((t) => ({ theme: { ...t, colors: { ...t.colors } }, was: t.name }))),
  );
  /** The theme shown, by its place in the list */
  let chosen = $state(untrack(() => Math.max(0, uiThemes.list.findIndex((t) => t.name === settings.theme))));
  let message = $state("");

  onMount(() => {
    shown = true;
    return () => (shown = false);
  });

  /** The theme shown, or undefined with none */
  const current = $derived(drafts[chosen]?.theme);

  // the theme shown is drawn on the whole app, so a change shows as it's made
  $effect(() => {
    if (!current) return;
    document.documentElement.dataset.theme = current.base;
    applyUiTheme($state.snapshot(current) as UiTheme);
  });

  /** A colour as the theme shown has it, else its base's */
  const colourOf = (key: string) => current?.colors[key] ?? BASE_COLOURS[current?.base ?? "dark"][key];

  /**
   * A new theme, from the base the app is drawn in now, with no colours of its own yet (its base's, to change); or a
   * copy of one (Duplicate)
   * @param from A theme to copy (Duplicate), or none
   */
  function add(from?: UiTheme): void {
    const base = from?.base ?? (document.documentElement.dataset.theme === "light" ? "light" : "dark");
    const taken = (name: string) => drafts.some((d) => d.theme.name.toLowerCase() === name.toLowerCase());
    let name = from ? `${from.name} copy` : "My theme";
    for (let n = 2; taken(name); n++) name = from ? `${from.name} copy ${n}` : `My theme ${n}`;
    drafts = [...drafts, { theme: { name, base, colors: { ...(from?.colors ?? {}) } }, was: "" }];
    chosen = drafts.length - 1;
  }

  /** Take the theme shown out of the list (saved by Save) */
  function remove(): void {
    drafts = drafts.filter((_, i) => i !== chosen);
    chosen = Math.max(0, Math.min(chosen, drafts.length - 1));
  }

  /** Change the theme shown's base: the colours it has stay, the rest are the new base's */
  function setBase(base: "light" | "dark"): void {
    if (current) current.base = base;
  }

  /**
   * Save every theme, keep a renamed theme in use, and leave a deleted one for Follow the system
   * @param use Make the theme shown Settings' theme too (Use This Theme)
   */
  async function save(use = false): Promise<void> {
    message = "";
    try {
      const saved = await saveUiThemes(drafts.map((d) => $state.snapshot(d.theme) as UiTheme));
      const inUse = drafts.find((d) => d.was && d.was === settings.theme);
      const shown = saved[chosen];
      const theme = use && shown ? shown.name
        : inUse ? saved[drafts.indexOf(inUse)].name
        : uiThemes.list.some((t) => t.name === settings.theme) || ["system", "light", "dark"].includes(settings.theme) ? settings.theme : "system";
      if (theme !== settings.theme) await saveSettings({ theme });
      else applyTheme();
      onClose();
    } catch (e) {
      message = String(e);
    }
  }

  /** Cancel: the app drawn in Settings' theme again, as before the window opened */
  function cancel(): void {
    applyTheme();
    onClose();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && cancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Edit Themes">
    <header>Edit Themes</header>
    <p class="hint">The window's own colours, beside Light and Dark. The theme chosen here is drawn on the whole app while
      this is open. They're kept on this computer, in your own folder.</p>
    <div class="body">
      <div class="side">
        <ul role="listbox" aria-label="Themes">
          {#each drafts as draft, i (i)}
            <li role="option" aria-selected={i === chosen} class:selected={i === chosen} tabindex="0"
              onclick={() => (chosen = i)} onkeydown={(e) => e.key === "Enter" && (chosen = i)}>
              {draft.theme.name || "(no name)"}{draft.was && draft.was === settings.theme ? " (in use)" : ""}
            </li>
          {:else}
            <li class="empty">None yet: New makes one from the theme the app has now.</li>
          {/each}
        </ul>
        <div class="row">
          <button onclick={() => add()}>New</button>
          <button onclick={() => current && add($state.snapshot(current) as UiTheme)} disabled={!current}>Duplicate</button>
          <button onclick={remove} disabled={!current}>Delete</button>
        </div>
      </div>

      {#if current}
        <div class="form">
          <label>Name <input bind:value={current.name} aria-label="Name" /></label>
          <label>Based on
            <select value={current.base} onchange={(e) => setBase(e.currentTarget.value as "light" | "dark")} aria-label="Based on">
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select></label>
          {#each COLOUR_TOKENS as token (token.key)}
            <div class="colour">
              <span>{token.label}</span>
              <input type="color" value={colourOf(token.key)} aria-label={token.label}
                oninput={(e) => (current.colors[token.key] = e.currentTarget.value)} />
              <code>{colourOf(token.key)}</code>
              {#if current.colors[token.key] && current.colors[token.key] !== BASE_COLOURS[current.base][token.key]}
                <button class="link" onclick={() => delete current.colors[token.key]} title="Back to {current.base === 'dark' ? 'Dark' : 'Light'}'s">Reset</button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <footer>
      <span class="message">{message}</span>
      <button onclick={cancel}>Cancel</button>
      <button onclick={() => void save()}>Save</button>
      <button class="primary" onclick={() => void save(true)} disabled={!current}
        title="Save, and draw the app in this theme (Settings > General > Look > Theme)">Use This Theme</button>
    </footer>
    <p class="hint">Follow {SYSTEM}, Light and Dark stay as they are; a theme deleted that was in use goes back to Follow {SYSTEM}.</p>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    /* over Settings (125), as its other windows are */
    z-index: 140;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(680px, 94vw);
    max-height: 90vh;
    display: flex;
    flex-direction: column;
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

  .hint {
    margin: 0;
    color: var(--text-muted);
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    gap: 14px;
    overflow: auto;
  }

  .side {
    width: 190px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  ul {
    flex: 1;
    min-height: 160px;
    margin: 0;
    padding: 4px;
    list-style: none;
    border: 1px solid var(--border);
    background: var(--bg-pane);
    border-radius: 4px;
  }

  li {
    padding: 3px 6px;
    border-radius: 3px;
  }

  li.selected {
    background: var(--accent);
    color: #fff;
  }

  li.empty {
    color: var(--text-muted);
  }

  .row {
    display: flex;
    gap: 6px;
  }

  .form {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  label {
    display: grid;
    grid-template-columns: 90px 1fr;
    align-items: center;
    gap: 8px;
  }

  .colour {
    display: grid;
    grid-template-columns: 1fr 44px 70px 50px;
    align-items: center;
    gap: 8px;
  }

  .colour input {
    width: 44px;
    height: 24px;
    padding: 0;
  }

  code {
    color: var(--text-muted);
    font-size: 12px;
  }

  button.link {
    border: none;
    background: none;
    padding: 0;
    color: var(--accent);
    text-decoration: underline;
  }

  input,
  select,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  button:disabled {
    opacity: 0.45;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .message {
    flex: 1;
    color: #e5534b;
  }
</style>
