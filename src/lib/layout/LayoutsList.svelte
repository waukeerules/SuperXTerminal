<!--
  The Layouts pane (View > Layouts), as SuperXPuTTY's LayoutsList: the saved layouts, "<Auto Restore>" first and the
  default one marked; a double-click opens one, and its menu has Load, Load in New Window, Rename, Delete and Set as
  default layout.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { AUTO_RESTORE, LAYOUTS_CHANGED, deleteLayout, listLayouts } from "./layouts";
  import { saveSettings, settings } from "../settings/store.svelte";
  import { ask } from "@tauri-apps/plugin-dialog";

  interface Props {
    /** The layout open now, shown in bold */
    current: string | null;
    onLoad: (name: string) => void;
    onLoadInNewWindow: (name: string) => void;
    onRename: (name: string) => void;
    onClose: () => void;
  }

  let { current, onLoad, onLoadInNewWindow, onRename, onClose }: Props = $props();

  let names = $state<string[]>([]);
  let selected = $state<string | null>(null);
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  async function refresh(): Promise<void> {
    names = await listLayouts().catch(() => []);
  }

  onMount(() => {
    void refresh();
    const changed = () => void refresh();
    window.addEventListener(LAYOUTS_CHANGED, changed);
    return () => window.removeEventListener(LAYOUTS_CHANGED, changed);
  });

  async function remove(name: string): Promise<void> {
    if (name === settings.defaultLayoutName) {
      await ask("The default layout can't be deleted: set another as the default first.", { title: "Delete layout", kind: "warning" });
      return;
    }
    if (!(await ask(`Are you sure you want to delete layout "${name}"?`, { title: "Delete layout", kind: "warning", okLabel: "Delete" }))) return;
    await deleteLayout(name).catch(() => {});
    await refresh();
  }

  function showMenu(event: MouseEvent, name: string): void {
    event.preventDefault();
    selected = name;
    const automatic = name === AUTO_RESTORE;
    const isDefault = name === settings.defaultLayoutName;
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: "Load", bold: true, action: () => onLoad(name) },
        { label: "Load in New Window", action: () => onLoadInNewWindow(name) },
        { separator: true },
        { label: "Rename…", disabled: automatic && "The automatic layout keeps its name", action: () => onRename(name) },
        { label: "Delete", disabled: automatic && "The automatic layout can't be deleted", action: () => void remove(name) },
        { separator: true },
        isDefault
          ? { label: "No default layout (a Local Shell at start)", action: () => void saveSettings({ defaultLayoutName: "" }) }
          : { label: "Set as default layout", action: () => void saveSettings({ defaultLayoutName: name }) },
      ],
    };
  }
</script>

<div class="pane">
  <div class="title">
    <span>Layouts</span>
    <button class="close" title="Hide the Layouts pane (View > Layouts)" aria-label="Hide the Layouts pane" onclick={onClose}>×</button>
  </div>
  <div class="list" role="listbox" aria-label="Layouts">
    {#each names as name (name)}
      <div class="item" class:selected={selected === name} class:current={current === name} role="option" aria-selected={selected === name}
        tabindex="-1" title={name === AUTO_RESTORE ? "The tabs as they were last, saved as they change; set it as the default to have them back at start" : ""}
        onclick={() => (selected = name)} ondblclick={() => onLoad(name)} oncontextmenu={(e) => showMenu(e, name)}
        onkeydown={(e) => e.key === "Enter" && onLoad(name)}>
        {name}{name === settings.defaultLayoutName ? " (default)" : ""}
      </div>
    {/each}
  </div>
</div>

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}

<svelte:window onclick={() => (menu = null)} />

<style>
  .pane {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
    min-height: 0;
  }

  .title {
    display: flex;
    align-items: center;
    padding: 4px 8px;
    color: var(--text-muted);
    font-size: 13px;
  }

  .title span {
    flex: 1;
  }

  .close {
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font-size: 15px;
    line-height: 1;
    padding: 0 3px;
  }

  .close:hover {
    color: var(--text);
  }

  .list {
    max-height: 140px;
    overflow: auto;
    padding: 0 4px 6px;
  }

  .item {
    padding: 2px 8px;
    border-radius: 3px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: default;
  }

  .item.current {
    font-weight: 600;
  }

  .item.selected {
    background: color-mix(in srgb, var(--accent) 25%, transparent);
  }
</style>
