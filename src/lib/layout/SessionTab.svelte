<!--
  A document tab's label: the session's icon, its name, a tunnel icon when its ssh opened tunnels (a click shows
  them, TunnelsPanel), and a close button. New output in a watched tab that isn't being looked at (Watch for
  Activity) pulses the tab in its colour. Its environment colour fills the tab
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
    /** The tunnel icon, clicked at this point in the window */
    onTunnels?: (x: number, y: number) => void;
  }

  let { title, icon, environment, iconSize, customs, tabId, onClose, onTunnels }: Props = $props();
  /** How many tunnels the session's ssh opened: the tunnel icon shows with any */
  const tunnels = $derived(tabStates[tabId]?.tunnels ?? 0);
  /** The profile's tunnels changed since: the icon is marked, as a restart is needed to use them */
  const tunnelsChanged = $derived(tabStates[tabId]?.tunnelsChanged ?? false);
  /** Whether a script is running in the tab, which shows a spinner in place of the icon */
  const scripting = $derived(running.runs.some((r) => r.tabId === tabId));
  /** The session is down: SuperXPuTTY's disconnected badge on the icon */
  const disconnected = $derived(tabStates[tabId]?.ended ?? false);
  /** Still reaching the far end: a small yellow dot, as the Sessions tree shows */
  const connecting = $derived(tabStates[tabId]?.connecting ?? false);
  /**
   * Connected, the far end having answered: a small green dot, as the Sessions tree shows. A terminal tab and a
   * Remote Desktop tab say (their hosts report connecting); an SFTP tab has no dot
   */
  const connected = $derived(tabStates[tabId]?.connecting === false && !disconnected);
  /** Watch for Activity's alert (the tab pulses) or Watch for Silence's (a grey pause on the icon) */
  const alert = $derived(tabStates[tabId]?.alert ?? null);
  /** New output in a watched tab not being looked at: the tab pulses in its colour until it's looked at */
  const pulsing = $derived(alert === "activity");
  /** The tab's colour: the environment's, else its profile's (Settings > Profiles > Tab colour); null for none */
  const colour = $derived(environment ?? tabStates[tabId]?.tabColor ?? null);
</script>

<!-- the pulse is in the tab's own colour, so it reads as that tab's; one with none pulses in the app's accent -->
<span class="tab" class:coloured={!!colour} class:pulsing style:--env={colour ?? "transparent"} style:--pulse={colour ?? "var(--accent)"}
  title={pulsing ? "New output" : undefined}>
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
      {:else if alert === "silence"}<span class="badge silence" title="Quiet" aria-label="Quiet">❚❚</span>
      {:else if connected}<span class="badge connected" title="Connected" aria-label="Connected"></span>{/if}
    </span>
  {/if}
  <!-- the whole name on hover, for one cut short (Settings > General > Tabs > Longest tab name) -->
  <span class="title" title={title}>{title}</span>
  {#if tunnels > 0 && !disconnected}
    <!-- a tunnel's arch: what the tunnels make reachable, with links and tests -->
    <button class="tunnels" class:changed={tunnelsChanged}
      title={tunnelsChanged ? "Tunnels: the profile's have changed, restart the session to use them" : `Tunnels (${tunnels}): what they open, and how to test them`}
      aria-label="Tunnels of {title}"
      onpointerdown={(e) => e.stopPropagation()} onclick={(e) => { e.stopPropagation(); onTunnels?.(e.clientX, e.clientY); }}>
      <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
        <path d="M2 14V8a6 6 0 0 1 12 0v6" fill="none" stroke="currentColor" stroke-width="1.6" />
        <path d="M5.5 14V8.5a2.5 2.5 0 0 1 5 0V14" fill="none" stroke="currentColor" stroke-width="1.6" />
        <path d="M1 14h14" stroke="currentColor" stroke-width="1.6" />
      </svg>
    </button>
  {/if}
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
  }

  /* the name as long as Settings > General > Tabs allows (App.svelte's --tab-name-max), then cut short with … */
  .title {
    max-width: var(--tab-name-max, 30ch);
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
   * while connected. Watch for Silence's alert takes the green's place while it shows
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

  .tunnels {
    display: flex;
    border: 0;
    background: none;
    padding: 0 1px;
    color: inherit;
    opacity: 0.8;
    cursor: default;
  }

  .tunnels:hover {
    opacity: 1;
  }

  /* the profile's tunnels changed: a restart is needed, marked in orange */
  .tunnels.changed {
    color: #f0883e;
    opacity: 1;
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

  /*
   * Watch for Activity: new output in a tab that isn't being looked at. The tab's colour (or the accent, for a tab
   * with none) comes and goes over the whole tab, behind its icon and name, where a dot on the icon was easy to
   * miss among many tabs. The tab fills its place as a coloured one does, the same padding moved inside it, so
   * nothing shifts when the pulse starts.
   */
  :global(.dv-tab:has(.pulsing)) {
    padding: 0;
  }

  :global(.dv-tab:has(.pulsing) .session-tab) {
    height: 100%;
  }

  .tab.pulsing {
    position: relative;
    isolation: isolate;
    padding: 0 14px 0 12px;
  }

  .tab.pulsing::before {
    content: "";
    position: absolute;
    inset: 0;
    z-index: -1;
    background: var(--pulse);
    animation: pulse 1.6s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.12;
    }

    50% {
      opacity: 0.75;
    }
  }

  /* the tab showing in a split, or in a window that isn't in front, is already filled with its colour, which more of
     the same wouldn't show on: it pulses lighter instead */
  :global(.dv-active-tab) .coloured.pulsing::before {
    background: #fff;
    animation-name: pulse-lighter;
  }

  @keyframes pulse-lighter {
    0%,
    100% {
      opacity: 0;
    }

    50% {
      opacity: 0.4;
    }
  }

  /* with Reduce Motion on (the system's setting): the colour held, without the movement */
  @media (prefers-reduced-motion: reduce) {
    .tab.pulsing::before {
      animation: none;
      opacity: 0.6;
    }

    :global(.dv-active-tab) .coloured.pulsing::before {
      opacity: 0.3;
    }
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
