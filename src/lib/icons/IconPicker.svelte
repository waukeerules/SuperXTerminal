<!--
  Choose Icon, as SuperXPuTTY's: a search box, the icons by section (Devices, General, Logos, Custom), a colour
  (the icon's default, a palette colour or any other), and a preview with the icon's name and colour. Gives back a
  key such as "switch#1E63B4" (no colour part when it's the icon's default).
-->
<script lang="ts">
  import { untrack } from "svelte";
  import Icon from "./Icon.svelte";
  import { logos } from "./brands";
  import { templates, templateOrder } from "./templates";
  import { parseKey, CUSTOM_PREFIX } from "./library";
  import { PALETTE } from "./palette";
  import type { CustomIcon } from "./custom";

  interface Props {
    /** The key chosen so far */
    value: string;
    /** The user's custom icons, for the Custom section */
    customs: ReadonlyMap<string, CustomIcon>;
    /** OK: the key chosen */
    onChoose: (key: string) => void;
    /** Cancel, or Escape: nothing changes */
    onCancel: () => void;
  }

  let { value, customs, onChoose, onCancel }: Props = $props();

  /** the window starts from the key it's opened with; later changes are its own */
  const start = untrack(() => parseKey(value));
  let name = $state((start?.name ?? "workstation").toLowerCase());
  let color = $state<string | null>(start?.color ?? null);
  let search = $state("");

  /** Each icon: its key name, title and section */
  const all = $derived([
    ...templateOrder.filter((k) => !templates[k].general).map((k) => ({ key: k, title: templates[k].title, section: "Devices" })),
    ...templateOrder.filter((k) => templates[k].general).map((k) => ({ key: k, title: templates[k].title, section: "General" })),
    ...Object.keys(logos).map((k) => ({ key: k, title: logos[k].title, section: "Logos" })),
    ...[...customs.entries()].map(([k, icon]) => ({ key: CUSTOM_PREFIX + k, title: icon.title, section: "Custom" })),
  ]);
  /** The sections, in Choose Icon's order */
  const sections = ["Devices", "General", "Logos", "Custom"];
  /** The icons whose title has the search's text */
  const shown = $derived(all.filter((i) => !search.trim() || i.title.toLowerCase().includes(search.trim().toLowerCase())));

  /** The icon's own colour, used when no other is chosen */
  const defaultColor = $derived(templates[name]?.color ?? logos[name]?.color ?? customs.get(name.replace(CUSTOM_PREFIX, ""))?.color ?? "");
  /** The chosen icon's title, for the preview */
  const title = $derived(all.find((i) => i.key === name)?.title ?? name);
  /** The key given back: the name, and the colour when it isn't the icon's own */
  const key = $derived(color && color.toUpperCase() !== defaultColor.toUpperCase() ? `${name}#${color.slice(1).toUpperCase()}` : name);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Choose Icon">
    <header>Choose Icon</header>
    <input class="search" placeholder="Search icons" bind:value={search} />
    <div class="grid-area">
      {#each sections as section}
        {@const items = shown.filter((i) => i.section === section)}
        {#if items.length}
          <h3>{section}</h3>
          <div class="grid">
            {#each items as item (item.key)}
              <button class="choice" class:selected={item.key === name} onclick={() => (name = item.key)}
                ondblclick={() => onChoose(key)} title={item.title}>
                <!-- in the colour chosen, as the preview, so each icon shows how it would look -->
                <Icon key={color ? `${item.key}#${color.slice(1)}` : item.key} size={40} {customs} />
                <span>{item.title}</span>
              </button>
            {/each}
          </div>
        {/if}
      {/each}
    </div>
    <footer>
      <div class="colours">
        <span>Colour:</span>
        <button class:selected={color === null} onclick={() => (color = null)}>Default</button>
        {#each PALETTE as p}
          <button class="swatch" class:selected={color?.toUpperCase() === p.color} style:background={p.color}
            title={p.name} aria-label={p.name} onclick={() => (color = p.color)}></button>
        {/each}
        <label class="more" title="Any other colour">More…
          <input type="color" value={color ?? defaultColor ?? "#607D8B"} oninput={(e) => (color = e.currentTarget.value.toUpperCase())} />
        </label>
      </div>
      <div class="preview">
        <Icon key={key} size={56} {customs} />
        <span>{title} · {(color ?? defaultColor).toUpperCase()}</span>
      </div>
      <div class="buttons">
        <button disabled title="Adding custom icons comes later">Add icon…</button>
        <button disabled title="Removing custom icons comes later">Remove</button>
        <span class="spacer"></span>
        <button class="primary" onclick={() => onChoose(key)}>OK</button>
        <button onclick={onCancel}>Cancel</button>
      </div>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(860px, 94vw);
    height: min(680px, 92vh);
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    font-weight: 600;
  }

  input,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
  }

  .grid-area {
    flex: 1;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 10px;
    background: var(--bg-pane);
  }

  h3 {
    margin: 10px 0 4px;
    font-size: 13px;
    color: var(--accent);
    font-weight: 500;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(112px, 1fr));
    gap: 4px;
  }

  .choice {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    background: none;
    border-color: transparent;
    padding: 6px 2px;
    font-size: 12px;
    text-align: center;
  }

  .choice.selected,
  .colours .selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  footer {
    display: grid;
    gap: 8px;
  }

  .colours {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }

  .swatch {
    width: 26px;
    height: 26px;
    padding: 0;
  }

  .colours .swatch.selected {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .more {
    position: relative;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
    background: var(--bg-input);
    overflow: hidden;
  }

  .more input {
    position: absolute;
    inset: 0;
    opacity: 0;
  }

  .preview {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .buttons {
    display: flex;
    gap: 8px;
  }

  .spacer {
    flex: 1;
  }

  button:disabled {
    opacity: 0.45;
  }
</style>
