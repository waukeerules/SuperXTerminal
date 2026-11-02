<!--
  A document tab's label: the session's icon, its name and a close button. Its environment colour fills the tab
  when it's selected and tints it, with a stripe along the top, when it isn't, as SuperXPuTTY's TabIcons.
-->
<script lang="ts">
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";
  import { running } from "../commands/sending.svelte";
  import { tabStates } from "./tabStates.svelte";

  interface Props {
    title: string;
    icon: string;
    environment: string | null;
    iconSize: number;
    customs: ReadonlyMap<string, CustomIcon>;
    /** The tab's id: a spinner shows in place of the icon while a script runs in it, as SuperXPuTTY's */
    tabId: string;
    onClose: () => void;
  }

  let { title, icon, environment, iconSize, customs, tabId, onClose }: Props = $props();
  const scripting = $derived(running.runs.some((r) => r.tabId === tabId));
  /** The session is down: SuperXPuTTY's disconnected badge on the icon */
  const disconnected = $derived(tabStates[tabId]?.ended ?? false);
  /** Watch for Activity's orange dot, or Watch for Silence's grey pause */
  const alert = $derived(tabStates[tabId]?.alert ?? null);
</script>

<span class="tab" class:coloured={!!environment} style:--env={environment ?? "transparent"}>
  {#if scripting}
    <!-- the icon's cell, so the label doesn't move -->
    <span class="cell" style:width="{Math.round(Math.round(iconSize * 0.75) * 1.36)}px" style:height="{Math.round(Math.round(iconSize * 0.75) * 1.36)}px">
      <span class="spinner" title="A script is running" style:width="{Math.round(iconSize * 0.75)}px" style:height="{Math.round(iconSize * 0.75)}px"></span>
    </span>
  {:else}
    <span class="icon" class:down={disconnected} title={disconnected ? "Disconnected" : undefined}>
      <Icon key={icon} size={Math.round(iconSize * 0.75)} {customs} />
      {#if disconnected}<span class="badge" aria-label="Disconnected">✕</span>
      {:else if alert === "activity"}<span class="badge activity" title="New output" aria-label="New output"></span>
      {:else if alert === "silence"}<span class="badge silence" title="Quiet" aria-label="Quiet">❚❚</span>{/if}
    </span>
  {/if}
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

  .icon {
    position: relative;
    display: flex;
  }

  .icon.down :global(svg),
  .icon.down :global(img) {
    opacity: 0.55;
  }

  /* SuperXPuTTY's disconnected badge: a small red cross over the icon's corner */
  .badge {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: #d32f2f;
    color: #fff;
    font-size: 7px;
    line-height: 11px;
    text-align: center;
  }

  .badge.activity {
    background: #f0883e;
    box-shadow: 0 0 0 1.5px #fff;
  }

  .badge.silence {
    background: #6e7681;
    box-shadow: 0 0 0 1.5px #fff;
    font-size: 5px;
    letter-spacing: -1px;
  }

  .cell {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: center;
  }

  .spinner {
    box-sizing: border-box;
    flex: none;
    border: 2px solid color-mix(in srgb, currentColor 30%, transparent);
    border-top-color: currentColor;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
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
