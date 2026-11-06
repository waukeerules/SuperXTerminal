<!--
  A session icon at a size, as SuperXPuTTY's IconLibrary draws it: a template, logo or custom icon in its key's
  colour. With an environment colour it sits on a circle of that colour, outlined so it stays readable on it
  (IconLibrary.DrawOnCircle); `open` adds the small green dot of a session with an open tab, `connecting` a yellow
  one while its tab is still reaching the far end, and `down` the red cross
  of one whose tabs' sessions have all dropped (the tab's own disconnected badge, SessionTab.svelte), the icon dimmed.
-->
<script lang="ts" module>
  /** Gives each drawn icon its own prefix for SVG ids (gradients), since many share one page */
  let next = 0;
</script>

<script lang="ts">
  import { logos } from "./brands";
  import { templates } from "./templates";
  import { resolveIcon } from "./library";
  import type { CustomIcon } from "./custom";

  interface Props {
    /** The icon's key, e.g. "switch#1E63B4" (a colour after # when it isn't the icon's own) */
    key: string;
    /** The icon's size in pixels (Settings: icon size); the cell around it is larger by SuperXPuTTY's padding */
    size: number;
    /** The environment colour, for the circle behind it; none for no circle */
    environment?: string | null;
    /** Whether the session has an open tab: the small green dot */
    open?: boolean;
    /** Whether its open tabs' sessions have all dropped: the red cross, as its tab shows, in place of the dot */
    down?: boolean;
    /** Whether its tab is still connecting (the far end hasn't answered yet): the dot yellow, as its tab shows */
    connecting?: boolean;
    /** The user's custom icons, for a custom icon's key */
    customs?: ReadonlyMap<string, CustomIcon>;
    /** A toolbar glyph ("check", "clear", ...) rather than a session icon */
    glyph?: boolean;
  }

  let { key, size, environment = null, open = false, down = false, connecting = false, customs = new Map(), glyph = false }: Props = $props();

  /** This icon's prefix for its SVG ids */
  const uid = `i${next++}`;
  /** What the key draws: a template, logo, custom icon or glyph, and its colour */
  const icon = $derived(resolveIcon(key, new Set(customs.keys()), glyph));
  /** The custom icon, when it's one */
  const custom = $derived(icon.kind === "custom" ? customs.get(icon.name) : undefined);
  /** IconLibrary pads each icon by 18% on every side, which leaves room for the environment circle */
  const cell = $derived(Math.round(size * 1.36));
  /** The outline's width round the icon on a circle */
  const outline = $derived(Math.max(1, size / 20));
</script>

<span class="cell" style:width="{cell}px" style:height="{cell}px">
  {#if environment}
    <span class="circle" style:background={environment}></span>
  {/if}
  <span class="icon" class:outlined={!!environment} style:width="{size}px" style:height="{size}px" style:--outline="{outline}px">
    {#if icon.kind === "template"}
      <svg viewBox="0 0 1 1" width={size} height={size} aria-hidden="true">{@html templates[icon.name].draw(icon.color, uid)}</svg>
    {:else if icon.kind === "logo"}
      <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true"><path fill={icon.color} d={logos[icon.name].path} /></svg>
    {:else if custom}
      <span class="custom" style:-webkit-mask-image="url({custom.dataUrl})" style:mask-image="url({custom.dataUrl})"
        style:background={icon.color ?? custom.color}></span>
    {/if}
  </span>
  {#if down}
    <span class="dropped" role="img" aria-label="Disconnected" title="Disconnected" style:width="{Math.round(size * 0.46)}px"
      style:height="{Math.round(size * 0.46)}px" style:font-size="{Math.max(6, Math.round(size * 0.3))}px">✕</span>
  {:else if connecting}
    <span class="dot connecting" role="img" aria-label="Connecting" title="Connecting"
      style:width="{Math.round(size * 0.42)}px" style:height="{Math.round(size * 0.42)}px"></span>
  {:else if open}
    <span class="dot open" style:width="{Math.round(size * 0.42)}px" style:height="{Math.round(size * 0.42)}px"></span>
  {/if}
</span>

<style>
  .cell {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }

  .circle {
    position: absolute;
    inset: 0;
    border-radius: 50%;
  }

  .icon {
    position: relative;
    display: inline-flex;
  }

  /* a dark, slightly transparent outline in 8 directions, as IconLibrary.DrawOutlined */
  .outlined {
    filter: drop-shadow(var(--outline) 0 0 rgb(20 20 20 / 0.85)) drop-shadow(calc(-1 * var(--outline)) 0 0 rgb(20 20 20 / 0.85))
      drop-shadow(0 var(--outline) 0 rgb(20 20 20 / 0.85)) drop-shadow(0 calc(-1 * var(--outline)) 0 rgb(20 20 20 / 0.85));
  }

  .custom {
    width: 100%;
    height: 100%;
    -webkit-mask-size: contain;
    mask-size: contain;
    -webkit-mask-repeat: no-repeat;
    mask-repeat: no-repeat;
  }

  /* the tab's disconnected badge (SessionTab.svelte): a red cross over the corner, the icon under it dimmed */
  .dropped {
    position: absolute;
    right: -1px;
    bottom: -1px;
    border-radius: 50%;
    background: #d32f2f;
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
    box-shadow: 0 0 0 1.5px var(--bg-pane);
  }

  .cell:has(.dropped) :global(svg),
  .cell:has(.dropped) :global(img),
  .cell:has(.dropped) .custom {
    opacity: 0.55;
  }

  .dot {
    position: absolute;
    right: 0;
    bottom: 0;
    border-radius: 50%;
    box-shadow: 0 0 0 1.5px var(--bg-pane);
  }

  /* open and connected */
  .open {
    background: #2e9e44;
  }

  /* still connecting: the tab's yellow dot (SessionTab.svelte) */
  .connecting {
    background: #e3b341;
  }
</style>
