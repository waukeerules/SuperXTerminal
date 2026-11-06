<!--
  A document tab's label: the session's icon, its name and a close button. Its environment colour fills the tab
  when it's selected and tints it, with a stripe along the top, when it isn't, as SuperXPuTTY's TabIcons. With no
  environment colour, its profile's tab colour does the same, but leaves the icon without a circle: the circle marks
  an environment, as in the Sessions tree.
-->
<script lang="ts">
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";
  import { running } from "../commands/sending.svelte";
  import { tabStates } from "./tabStates.svelte";

  interface Props {
    /** The tab's title */
    title: string;
    /** The session's icon key */
    icon: string;
    /** The environment colour, for the circle behind the icon; null for none */
    environment: string | null;
    /** The size the icon is drawn at */
    iconSize: number;
    /** The user's custom icons, for a session using one */
    customs: ReadonlyMap<string, CustomIcon>;
    /** The tab's id: a spinner shows in place of the icon while a script runs in it, as SuperXPuTTY's */
    tabId: string;
    /** The tab's ✕ */
    onClose: () => void;
  }

  let { title, icon, environment, iconSize, customs, tabId, onClose }: Props = $props();
  /** Whether a script is running in the tab, which shows a spinner in place of the icon */
  const scripting = $derived(running.runs.some((r) => r.tabId === tabId));
  /** The session is down: SuperXPuTTY's disconnected badge on the icon */
  const disconnected = $derived(tabStates[tabId]?.ended ?? false);
  /** Still reaching the far end: a small yellow dot, as the Sessions tree shows */
  const connecting = $derived(tabStates[tabId]?.connecting ?? false);
  /**
   * Connected, the far end having answered: a small green dot, as the Sessions tree shows. Only a terminal tab says
   * (its page host reports connecting); a Remote Desktop or SFTP tab has no dot
   */
  const connected = $derived(tabStates[tabId]?.connecting === false && !disconnected);
  /** Watch for Activity's orange dot, or Watch for Silence's grey pause */
  const alert = $derived(tabStates[tabId]?.alert ?? null);
  /** The tab's colour: the environment's, else its profile's (Settings > Profiles > Tab colour); null for none */
  const colour = $derived(environment ?? tabStates[tabId]?.tabColor ?? null);
</script>

<span class="tab" class:coloured={!!colour} style:--env={colour ?? "transparent"}>
  {#if scripting}
    <!-- the icon's cell, so the label doesn't move -->
    <span class="cell" style:width="{Math.round(Math.round(iconSize * 0.75) * 1.36)}px" style:height="{Math.round(Math.round(iconSize * 0.75) * 1.36)}px">
      <span class="spinner" title="A script is running" style:width="{Math.round(iconSize * 0.75)}px" style:height="{Math.round(iconSize * 0.75)}px"></span>
    </span>
  {:else}
    <span class="icon" class:down={disconnected} title={disconnected ? "Disconnected" : undefined}>
      <!-- on its environment circle, outlined, as in the Sessions tree -->
      <Icon key={icon} size={Math.round(iconSize * 0.75)} {environment} {customs} />
      {#if disconnected}<span class="badge" aria-label="Disconnected">✕</span>
      {:else if connecting}<span class="badge connecting" title="Connecting" aria-label="Connecting"></span>
      {:else if alert === "activity"}<span class="badge activity" title="New output" aria-label="New output"></span>
      {:else if alert === "silence"}<span class="badge silence" title="Quiet" aria-label="Quiet">❚❚</span>
      {:else if connected}<span class="badge connected" title="Connected" aria-label="Connected"></span>{/if}
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

  /*
   * The Sessions tree's dots, smaller than the cross (they hold no sign): yellow until the far end answers, green
   * while connected. Watch for Activity's and Silence's alerts take the green's place while they show
   */
  .badge.connecting,
  .badge.connected {
    width: 7px;
    height: 7px;
    right: 0;
    bottom: 0;
    box-shadow: 0 0 0 1.5px var(--bg-pane);
  }

  .badge.connecting {
    background: #e3b341;
  }

  .badge.connected {
    background: #2e9e44;
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

  /* A coloured tab's colour fills the whole tab, meeting the coloured frame below: dockview's own padding round the
     label (which left a ring of the tab bar's colour) moves inside it */
  :global(.dv-tab:has(.coloured)) {
    padding: 0;
  }

  /* the label's wrapper (TabArea's TabLabel) fills the tab, so the colour reaches its bottom edge */
  :global(.dv-tab:has(.coloured) .session-tab) {
    height: 100%;
  }

  .tab.coloured {
    padding: 0 14px 0 12px;
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
