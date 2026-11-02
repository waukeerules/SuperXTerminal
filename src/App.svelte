<!--
  The main window, laid out as SuperXPuTTY's: the Quick connect and Send Commands toolbars, the Sessions pane on the
  left, the document tabs, then the button bar and the status bar. M0 has the frame only; the tree, toolbars and
  button bar are filled in M1 and M3.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { TabArea } from "./lib/layout/TabArea";

  let tabElement: HTMLDivElement;
  let tabs: TabArea | null = null;
  let status = $state("Ready");

  onMount(() => {
    tabs = new TabArea(tabElement);
    tabs.openLocalShell();
    status = "Opened session: Local Shell";
    return () => tabs?.dispose();
  });

  /** Open another Local Shell tab (toolbar button, for now) */
  function newShell(): void {
    tabs?.openLocalShell();
    status = "Opened session: Local Shell";
  }
</script>

<div class="window">
  <div class="toolbar" aria-label="Quick connect" title="Quick connect comes in M1">
    <label>Protocol <select disabled><option>SSH</option></select></label>
    <label>Host <input disabled placeholder="e.g. 192.168.0.5" /></label>
    <label>Login <input disabled placeholder="e.g. admin" /></label>
    <label>Password <input disabled type="password" /></label>
    <label>Session <select disabled><option></option></select></label>
    <button onclick={newShell} title="Open a Local Shell tab (for now, the only way to open a tab)">New Local Shell</button>
  </div>
  <div class="toolbar" aria-label="Send Commands" title="Send Commands comes in M3">
    <label class="grow">Command <input disabled class="grow" /></label>
  </div>

  <div class="main">
    <aside class="sessions">
      <div class="pane-title">Sessions</div>
      <input class="search" disabled placeholder="Type to search..." title="Search comes in M1" />
      <div class="tree">
        <p class="empty">No sessions yet. The sessions tree comes in M1.</p>
      </div>
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

  .search {
    margin: 0 8px 6px;
  }

  .tree {
    flex: 1;
    overflow: auto;
    padding: 4px 8px;
  }

  .empty {
    color: var(--text-muted);
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
