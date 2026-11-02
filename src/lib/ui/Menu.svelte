<!--
  A right-click menu, drawn in the app's theme like the terminal's own menu: items, separators, greyed items with
  the reason as their tooltip, and one level of submenus. Closes on choosing, Escape or a click elsewhere.
-->
<script lang="ts" module>
  /** One menu entry */
  export type MenuItem =
    | { separator: true }
    | { label: string; action?: () => void; disabled?: string | boolean; bold?: boolean; items?: MenuItem[]; swatch?: string };
</script>

<script lang="ts">
  import Menu from "./Menu.svelte";

  interface Props {
    items: MenuItem[];
    x: number;
    y: number;
    onClose: () => void;
    /** A submenu: placed beside its item instead of at x, y */
    nested?: boolean;
  }

  let { items, x, y, onClose, nested = false }: Props = $props();

  let element: HTMLDivElement;
  let open = $state<number | null>(null);
  let left = $state(0);
  let top = $state(0);

  /** A submenu's shift upwards, so one near the bottom of the window stays inside it */
  let lift = $state(0);

  // keep the menu inside the window
  $effect(() => {
    if (nested) {
      const rect = element.getBoundingClientRect();
      lift = Math.max(0, rect.bottom - window.innerHeight + 4);
      return;
    }
    const rect = element.getBoundingClientRect();
    left = Math.max(4, Math.min(x, window.innerWidth - rect.width - 4));
    top = Math.max(4, Math.min(y, window.innerHeight - rect.height - 4));
  });

  /** Run an item's action and close */
  function choose(item: MenuItem): void {
    if ("separator" in item || item.disabled || item.items) return;
    onClose();
    item.action?.();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="menu" class:nested role="menu" tabindex="-1" bind:this={element} onclick={(e) => e.stopPropagation()}
  onkeydown={() => {}} style:margin-top={nested && lift ? `-${lift}px` : null} style:left={nested ? null : `${left}px`} style:top={nested ? null : `${top}px`}>
  {#each items as item, index}
    {#if "separator" in item}
      <div class="separator" role="separator"></div>
    {:else}
      <div class="item" class:bold={item.bold} class:disabled={!!item.disabled} role="menuitem" tabindex="-1"
        aria-disabled={!!item.disabled} title={typeof item.disabled === "string" ? item.disabled : ""}
        onmouseenter={() => (open = item.items ? index : null)} onclick={() => choose(item)}
        onkeydown={(e) => e.key === "Enter" && choose(item)}>
        {#if item.swatch !== undefined}<span class="swatch" style:background={item.swatch || "transparent"}></span>{/if}
        <span class="label">{item.label}</span>
        {#if item.items}<span class="arrow">▸</span>{/if}
        {#if item.items && open === index}
          <div class="sub">
            <Menu items={item.items} x={0} y={0} {onClose} nested />
          </div>
        {/if}
      </div>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 200;
    min-width: 210px;
    padding: 4px 0;
    background: var(--bg-pane);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.35);
    font-size: 13px;
  }

  .nested {
    position: absolute;
    left: calc(100% - 4px);
    top: -5px;
  }

  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 14px 4px 22px;
  }

  .item:hover:not(.disabled) {
    background: var(--accent);
    color: #fff;
  }

  .bold {
    font-weight: 600;
  }

  .disabled {
    opacity: 0.45;
  }

  .label {
    flex: 1;
    white-space: nowrap;
  }

  .arrow {
    font-size: 10px;
  }

  .swatch {
    width: 12px;
    height: 12px;
    border-radius: 2px;
    border: 1px solid var(--border);
  }

  .separator {
    height: 1px;
    margin: 4px 0;
    background: var(--border);
  }
</style>
