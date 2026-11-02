<!--
  The main window, laid out as SuperXPuTTY's: the Quick connect and Send Commands toolbars, the Sessions pane on the
  left, the document tabs, then the button bar and the status bar. M0 has the frame only; the tree, toolbars and
  button bar are filled in M1 and M3.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { TabArea } from "./lib/layout/TabArea";
  import SessionTree from "./lib/sessions/SessionTree.svelte";
  import Settings from "./lib/settings/Settings.svelte";
  import EnvironmentVariables from "./lib/settings/EnvironmentVariables.svelte";
  import QuickConnect from "./lib/layout/QuickConnect.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { opensAsLocalShell, type SessionView } from "./lib/sessions/api";

  let tabElement: HTMLDivElement;
  let tabs: TabArea | null = null;
  let status = $state("Ready");
  let openSessions = $state<ReadonlySet<string>>(new Set());
  let settingsOpen = $state(false);
  let variablesOpen = $state(false);

  // the menu bar's items (src-tauri/src/lib.rs) arrive as "menu" events with the item's id
  void listen<string>("menu", (event) => {
    if (event.payload === "settings") settingsOpen = true;
    else if (event.payload === "newLocalShell") newShell();
    else if (event.payload === "environmentVariables") variablesOpen = true;
  });

  onMount(() => {
    tabs = new TabArea(tabElement, (open) => (openSessions = open));
    tabs.openLocalShell();
    status = "Opened session: Local Shell";
    return () => tabs?.dispose();
  });

  /** Open a session from the tree in a new tab */
  function openSession(session: SessionView, environment: string | null): void {
    tabs?.openSession(session.store, session.id, session.name, opensAsLocalShell(session.protocol), session.icon, environment);
    status = `Opened session: ${session.name} [${session.protocol || "SSH"}]`;
  }

  /** Open another Local Shell tab (File > New Local Shell, Cmd+T) */
  function newShell(): void {
    tabs?.openLocalShell();
    status = "Opened session: Local Shell";
  }
</script>

{#if variablesOpen}
  <EnvironmentVariables onClose={() => (variablesOpen = false)} />
{/if}

{#if settingsOpen}
  <Settings onClose={() => (settingsOpen = false)} />
{/if}

<div class="window">
  <QuickConnect onProblem={(text) => (status = text)} onConnect={(quick, title, local) => {
    tabs?.openQuickConnect(quick, title, local);
    status = `Opened session: ${title} [${quick.protocol}]`;
  }} />
  <div class="toolbar" aria-label="Send Commands" title="Send Commands comes in M3">
    <label class="grow">Command <input disabled class="grow" /></label>
  </div>

  <div class="main">
    <aside class="sessions">
      <div class="pane-title">Sessions</div>
      <SessionTree {openSessions} onOpen={openSession} onOpenProfiles={() => (settingsOpen = true)} onLook={(size, customs) => {
        if (tabs) {
          tabs.iconSize = size;
          tabs.customs = customs;
        }
      }} />
    </aside>
    <div class="tabs" bind:this={tabElement}></div>
  </div>

  <div class="buttonbar" aria-label="Button bar" title="The button bar comes in M3">
    <select disabled><option>Default</option></select>
  </div>
  <div class="statusbar">
    <span>{status}</span>
    <span class="version">0.1.0</span>
  </div>
</div>

<style>
  .window {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .toolbar,
  .buttonbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 4px 8px;
    background: var(--bg-bar);
    border-bottom: 1px solid var(--border);
  }

  label {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  input:disabled,
  select:disabled {
    opacity: 0.6;
  }

  input,
  select {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 2px;
    padding: 2px 4px;
    min-width: 0;
  }

  .grow {
    flex: 1;
  }

  .main {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .sessions {
    width: 280px;
    display: flex;
    flex-direction: column;
    background: var(--bg-pane);
    border-right: 1px solid var(--border);
  }

  .pane-title {
    padding: 6px 8px;
    color: var(--text-muted);
    font-size: 14px;
  }

  .tabs {
    flex: 1;
    min-width: 0;
  }

  .tabs :global(.terminal-panel),
  .tabs :global(.terminal-frame) {
    width: 100%;
    height: 100%;
    border: 0;
    display: block;
  }

  .buttonbar {
    border-top: 1px solid var(--border);
  }

  .statusbar {
    display: flex;
    justify-content: space-between;
    padding: 3px 8px;
    background: var(--bg-bar);
    color: var(--text-muted);
    font-size: 12px;
  }
</style>
