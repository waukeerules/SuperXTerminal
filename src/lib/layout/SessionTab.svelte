<!--
  A document tab's label: the session's icon, its name and a close button. Its environment colour fills the tab
  when it's selected and tints it, with a stripe along the top, when it isn't, as SuperXPuTTY's TabIcons.
-->
<script lang="ts">
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";

  interface Props {
    title: string;
    icon: string;
    environment: string | null;
    iconSize: number;
    customs: ReadonlyMap<string, CustomIcon>;
    onClose: () => void;
  }

  let { title, icon, environment, iconSize, customs, onClose }: Props = $props();
</script>

<span class="tab" class:coloured={!!environment} style:--env={environment ?? "transparent"}>
  <Icon key={icon} size={Math.round(iconSize * 0.75)} {customs} />
  <span class="title">{title}</span>
  <button class="close" title="Close" aria-label="Close {title}"
    onpointerdown={(e) => e.stopPropagation()} onclick={(e) => { e.stopPropagation(); onClose(); }}>✕</button>
</span>

<style>
  .tab {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 100%;
    padding: 0 6px 0 4px;
    max-width: 30ch;
  }

  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close {
    border: 0;
    background: none;
    padding: 0 2px;
    font-size: 12px;
    opacity: 0.8;
    cursor: default;
  }

  .close:hover {
    opacity: 1;
  }

  /* SuperXPuTTY's TabIcons: the selected tab filled with the colour, the others tinted with a 3 px top stripe */
  :global(.dv-inactive-tab) .coloured {
    background: linear-gradient(var(--env), var(--env)) top / 100% 3px no-repeat,
      color-mix(in srgb, var(--env) 27%, transparent);
  }

  :global(.dv-active-tab) .coloured {
    background: var(--env);
    color: #fff;
  }

  :global(.dv-active-tab) .coloured .close {
    color: #fff;
  }
</style>
