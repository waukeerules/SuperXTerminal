<!--
  A right-click menu, drawn in the app's theme like the terminal's own menu: items, separators, greyed items with
  the reason as their tooltip, and one level of submenus. Closes on choosing, Escape, a click elsewhere (its owner's
  window click), or a click in a terminal, which takes the window's focus.
-->
<script lang="ts" module>
  /** One menu entry */
  export type MenuItem =
    | { separator: true }
    | {
        label: string;
        action?: () => void;
        disabled?: string | boolean;
        bold?: boolean;
        items?: MenuItem[];
        swatch?: string;
        checked?: boolean;
        /** A session icon key, drawn before the label */
        icon?: string;
        environment?: string | null;
      };
</script>

<script lang="ts">
  import Menu from "./Menu.svelte";
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";

  interface Props {
    /** The items, separators and submenus */
    items: MenuItem[];
    /** Where the menu opens: its left edge, in the window's pixels (moved in when it would run off the window) */
    x: number;
    /** Where the menu opens: its top edge, in the window's pixels (moved up when it would run off the window) */
    y: number;
    /** An item was chosen, Escape, or a click outside */
    onClose: () => void;
    /** A submenu: placed beside its item instead of at x, y */
    nested?: boolean;
    /** Custom icons, for items with a session icon */
    customs?: ReadonlyMap<string, CustomIcon>;
  }

  let { items, x, y, onClose, nested = false, customs = new Map() }: Props = $props();

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

<!-- a click in a terminal (its own page, in an iframe) never reaches this window as a click, but takes the focus from
  it: the menu closes then too, as for a click elsewhere, and when the app goes to the background -->
<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} onblur={() => !nested && onClose()} />

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
        {#if item.checked !== undefined}<span class="check">{item.checked ? "✓" : ""}</span>{/if}
        {#if item.icon}<Icon key={item.icon} size={18} environment={item.environment ?? null} {customs} />{/if}
        <span class="label">{item.label}</span>
        {#if item.items}<span class="arrow">▸</span>{/if}
        {#if item.items && open === index}
          <div class="sub">
            <Menu items={item.items} x={0} y={0} {onClose} {customs} nested />
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
    /* its own, so a submenu doesn't take the highlighted row's white */
    color: var(--text);
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

  /* in the left margin, as Windows menus draw it, so labels stay lined up */
  .check {
    position: absolute;
    left: 7px;
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
