<!--
  A session icon at a size, as SuperXPuTTY's IconLibrary draws it: a template, logo or custom icon in its key's
  colour. With an environment colour it sits on a circle of that colour, outlined so it stays readable on it
  (IconLibrary.DrawOnCircle); `open` adds the small green dot of a session with an open tab.
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
    key: string;
    /** The icon's size in pixels (Settings: icon size); the cell around it is larger by SuperXPuTTY's padding */
    size: number;
    environment?: string | null;
    open?: boolean;
    customs?: ReadonlyMap<string, CustomIcon>;
  }

  let { key, size, environment = null, open = false, customs = new Map() }: Props = $props();

  const uid = `i${next++}`;
  const icon = $derived(resolveIcon(key, new Set(customs.keys())));
  const custom = $derived(icon.kind === "custom" ? customs.get(icon.name) : undefined);
  // IconLibrary pads each icon by 18% on every side, which leaves room for the environment circle
  const cell = $derived(Math.round(size * 1.36));
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
  {#if open}
    <span class="open" style:width="{Math.round(size * 0.42)}px" style:height="{Math.round(size * 0.42)}px"></span>
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

  .open {
    position: absolute;
    right: 0;
    bottom: 0;
    border-radius: 50%;
    background: #2e9e44;
    box-shadow: 0 0 0 1.5px var(--bg-pane);
  }
</style>
