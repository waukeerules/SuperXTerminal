<!--
  Show for Tab Icons, as SuperXPuTTY's: every session icon with a tick box; the bar shows by itself for tabs whose
  session has a ticked icon. Each tick is saved at once, and the window stays open while ticking.
-->
<script lang="ts">
  import Icon from "../icons/Icon.svelte";
  import { templates, templateOrder } from "../icons/templates";
  import { logos } from "../icons/brands";
  import { CUSTOM_PREFIX } from "../icons/library";
  import type { CustomIcon } from "../icons/custom";
  import type { Bar } from "./bars";

  interface Props {
    bar: Bar;
    customs: ReadonlyMap<string, CustomIcon>;
    onChange: (icons: string[]) => void;
    onClose: () => void;
  }

  let { bar, customs, onChange, onClose }: Props = $props();

  const icons = $derived([
    ...templateOrder.map((k) => ({ key: k, title: templates[k].title })),
    ...Object.keys(logos).map((k) => ({ key: k, title: logos[k].title })),
    ...[...customs.entries()].map(([k, icon]) => ({ key: CUSTOM_PREFIX + k, title: icon.title })),
  ]);

  function toggle(key: string): void {
    const has = bar.tabIcons.some((t) => t.toLowerCase() === key.toLowerCase());
    onChange(has ? bar.tabIcons.filter((t) => t.toLowerCase() !== key.toLowerCase()) : [...bar.tabIcons, key]);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Show for Tab Icons">
    <header>Show "{bar.name}" for tabs with these icons</header>
    <div class="list">
      {#each icons as icon (icon.key)}
        <label>
          <input type="checkbox" checked={bar.tabIcons.some((t) => t.toLowerCase() === icon.key.toLowerCase())} onchange={() => toggle(icon.key)} />
          <Icon key={icon.key} size={18} {customs} />
          {icon.title}
        </label>
      {/each}
    </div>
    <footer><button class="primary" onclick={onClose}>Close</button></footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 130;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(420px, 94vw);
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    color: var(--text-muted);
  }

  .list {
    height: 360px;
    overflow: auto;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px;
  }

  label {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  footer {
    display: flex;
    justify-content: flex-end;
  }

  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 10px;
  }
</style>
