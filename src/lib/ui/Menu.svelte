<!--
  A right-click menu, drawn in the app's theme like the terminal's own menu: items, separators, greyed items with
  the reason as their tooltip, their keys, and one level of submenus. Closes on choosing, Escape, a click elsewhere
  (its owner's window click), or a click in a terminal, which takes the window's focus. Taller than the window, it
  scrolls; a submenu opens beside its item, over the window, so the scrolling doesn't cut it off.
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
        /** A heading, not an item: bold, nothing happens on it (a terminal's menu names its tab so) */
        heading?: boolean;
        /** Its key, shown at the right ("⌘C"), as the terminal's own menu shows them */
        key?: string;
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
    /**
     * A submenu: x and y are then its item's right edge and top, and `back` its left edge, where the submenu goes
     * instead when there's no room for it on the right
     */
    nested?: boolean;
    /** A submenu's item's left edge (nested) */
    back?: number;
    /**
     * Take the keyboard focus as it opens, for a menu opened from a terminal (a tab's page, in an iframe): Escape then
     * reaches it, and a click back in a terminal takes the focus from the window, which closes it
     */
    takeFocus?: boolean;
    /** Custom icons, for items with a session icon */
    customs?: ReadonlyMap<string, CustomIcon>;
  }

  let { items, x, y, onClose, nested = false, back = 0, takeFocus = false, customs = new Map() }: Props = $props();

  let element: HTMLDivElement;
  let open = $state<number | null>(null);
  /** Where the open submenu's item is in the window, which the submenu opens beside */
  let anchor = $state<DOMRect | null>(null);
  /**
   * A list (no submenus), as the tab list (▾) with many tabs open: when taller than the window, the ticked item (the
   * tab in front) is scrolled into view. Every menu scrolls when it's too tall (the CSS); this is only for that
   */
  const scrolls = $derived(!nested && !items.some((item) => "items" in item && item.items));
  let left = $state(0);
  let top = $state(0);

  // keep the menu inside the window: a submenu on its item's right, else its left, and as low as its item if it fits
  $effect(() => {
    const rect = element.getBoundingClientRect();
    if (nested) {
      left = Math.max(4, x + rect.width <= window.innerWidth - 4 ? x : back - rect.width);
      top = Math.max(4, Math.min(y, window.innerHeight - rect.height - 4));
      return;
    }
    left = Math.max(4, Math.min(x, window.innerWidth - rect.width - 4));
    top = Math.max(4, Math.min(y, window.innerHeight - rect.height - 4));
    if (takeFocus) element.focus({ preventScroll: true });
    // the ticked item (the tab in front) in view, in one that scrolls (jsdom, in the tests, has no scrolling)
    if (scrolls) element.querySelector("[data-checked]")?.scrollIntoView?.({ block: "nearest" });
  });

  /** Run an item's action and close */
  function choose(item: MenuItem): void {
    if ("separator" in item || item.disabled || item.items || item.heading) return;
    onClose();
    item.action?.();
  }
</script>

<!-- a click in a terminal (its own page, in an iframe) never reaches this window as a click, but takes the focus from
  it: the menu closes then too, as for a click elsewhere, and when the app goes to the background -->
<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} onblur={() => !nested && onClose()} />

<div class="menu" class:scrolls role="menu" tabindex="-1" bind:this={element} onclick={(e) => e.stopPropagation()}
  onkeydown={() => {}} onscroll={() => (open = null)} style:left={`${left}px`} style:top={`${top}px`}>
  {#each items as item, index}
    {#if "separator" in item}
      <div class="separator" role="separator"></div>
    {:else if item.heading}
      <div class="heading" role="presentation" title={item.label}>{item.label}</div>
    {:else}
      <div class="item" class:bold={item.bold} class:disabled={!!item.disabled} role="menuitem" tabindex="-1"
        data-checked={item.checked ? "" : undefined}
        aria-disabled={!!item.disabled} title={typeof item.disabled === "string" ? item.disabled : ""}
        onmouseenter={(e) => {
          anchor = item.items ? e.currentTarget.getBoundingClientRect() : null;
          open = item.items ? index : null;
        }} onclick={() => choose(item)}
        onkeydown={(e) => e.key === "Enter" && choose(item)}>
        {#if item.swatch !== undefined}<span class="swatch" style:background={item.swatch || "transparent"}></span>{/if}
        {#if item.checked !== undefined}<span class="check">{item.checked ? "✓" : ""}</span>{/if}
        {#if item.icon}<Icon key={item.icon} size={18} environment={item.environment ?? null} {customs} />{/if}
        <span class="label">{item.label}</span>
        {#if item.key}<span class="key">{item.key}</span>{/if}
        {#if item.items}<span class="arrow">▸</span>{/if}
        {#if item.items && open === index && anchor}
          <div class="sub">
            <Menu items={item.items} x={anchor.right - 4} y={anchor.top - 5} back={anchor.left + 4} {onClose} {customs} nested />
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

  /*
   * taller than the window: it scrolls within it, a margin left at the bottom. Every menu does; its submenus are
   * fixed beside their item, so the scrolling doesn't cut them off
   */
  .menu {
    max-height: calc(100vh - 8px);
    overflow-y: auto;
  }

  .menu:focus {
    outline: none;
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

  /* the tab a terminal's menu is for: bold, in line with the labels, cut short with … when long */
  .heading {
    padding: 4px 14px 4px 22px;
    font-weight: 600;
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  /* an item's key, at the right and greyed, as the terminal's own menu shows them */
  .key {
    margin-left: 24px;
    opacity: 0.6;
    white-space: nowrap;
  }

  .item:hover:not(.disabled) .key {
    opacity: 0.85;
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
