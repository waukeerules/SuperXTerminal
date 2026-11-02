<!--
  The Settings window (Cmd+,), SuperXPuTTY's Tools > Options for the Mac: General, Profiles, Terminal, Remote Desktop and Shortcuts.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import ProfilesTab from "./ProfilesTab.svelte";
  import GeneralTab from "./GeneralTab.svelte";
  import TerminalTab from "./TerminalTab.svelte";
  import ShortcutsTab from "./ShortcutsTab.svelte";
  import RemoteDesktopTab from "./RemoteDesktopTab.svelte";

  interface Props {
    onClose: () => void;
    /** The tab to open on */
    tab?: string;
  }

  let { onClose, tab = "General" }: Props = $props();

  let current = $state(untrack(() => tab));

  /** The tabs, with the milestone that brings each one still to come */
  const TABS: { name: string; later?: string }[] = [
    { name: "General" },
    { name: "Profiles" },
    { name: "Terminal" },
    { name: "Remote Desktop" },
    { name: "Shortcuts" },
  ];
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Settings">
    <header>
      <nav>
        {#each TABS as tab}
          <button class:active={tab.name === current} disabled={!!tab.later} title={tab.later ? `${tab.name} ${tab.later}` : ""}
            onclick={() => (current = tab.name)}>
            {tab.name}
          </button>
        {/each}
      </nav>
      <button class="close" onclick={onClose} aria-label="Close">✕</button>
    </header>
    <main>
      {#if current === "General"}<GeneralTab />{:else if current === "Terminal"}<TerminalTab />{:else if current === "Remote Desktop"}<RemoteDesktopTab />{:else if current === "Shortcuts"}<ShortcutsTab />{:else}<ProfilesTab />{/if}
    </main>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .dialog {
    width: min(860px, 94vw);
    height: min(720px, 92vh);
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
    display: flex;
    flex-direction: column;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px 0;
    border-bottom: 1px solid var(--border);
  }

  nav {
    display: flex;
    gap: 2px;
  }

  nav button {
    border: 1px solid transparent;
    border-bottom: 0;
    border-radius: 6px 6px 0 0;
    background: none;
    padding: 6px 14px;
  }

  nav button.active {
    background: var(--bg-pane);
    border-color: var(--border);
  }

  nav button:disabled {
    opacity: 0.45;
  }

  .close {
    border: 0;
    background: none;
  }

  main {
    flex: 1;
    min-height: 0;
    padding: 12px 16px 16px;
    background: var(--bg-pane);
  }
</style>
