<!--
  A place to drop a file from Finder or File Explorer, or click to choose one, as websites' upload boxes: for a
  dialog that needs a file (File > Import Sessions). The windows are made with Tauri's own drop handling off (the
  page's own drag and drop needs that on Windows), so a dropped file arrives as the browser's drop event: its name and
  contents, not its path (lib/ui/fileDrops.ts). The zone hands the dialog the File, and the dialog has the core save it
  where it can be read. It's meant for a dialog that covers the window: a drop anywhere on the window counts, and the
  zone lights up while files are dragged over it. A drop that isn't what's wanted (another kind of file, several for
  one, a folder, which the browser can't read this way) is turned down with why, in the zone.
-->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { FILE_MANAGER } from "../platform";
  import { carriesFiles } from "./fileDrops";

  interface Props {
    /** What's chosen so far, shown in the zone: paths or file names, or none */
    paths: string[];
    /** The file extensions wanted, without the dot ("xml"); none for any */
    extensions?: string[];
    /** Whether several files may be dropped at once */
    multiple?: boolean;
    /** Whether it takes a folder (chosen by clicking only: a dropped folder can't be read) */
    folder?: boolean;
    /** What it takes, in the zone's words: "a SuperPuTTY .xml file" */
    what: string;
    /** Click, Enter or Space: open the system's file picker */
    onChoose: () => void;
    /** Files dropped that are what's wanted */
    onDrop: (files: File[]) => void;
  }

  let { paths, extensions = [], multiple = false, folder = false, what, onChoose, onDrop }: Props = $props();

  /** How many elements a drag of files is over (enter and leave come for each), so the zone lights up while it's > 0 */
  let over = $state(0);
  /** Why the last drop was turned down; "" when it wasn't */
  let refused = $state("");

  /** A path's file name */
  const nameOf = (path: string) => path.split(/[\\/]/).pop() ?? path;

  /** Whether dropped files are what's wanted, else why not */
  function problemWith(files: File[], folders: boolean): string {
    if (folder) return `Choose ${what} by clicking: a dropped folder can't be read here.`;
    if (folders) return `Drop a file, not a folder: this takes ${what}.`;
    if (files.length > 1 && !multiple) return `Drop one file: ${files.length} were dropped.`;
    if (extensions.length === 0) return "";
    const wrong = files.find((f) => !extensions.some((e) => f.name.toLowerCase().endsWith(`.${e.toLowerCase()}`)));
    if (!wrong) return "";
    return `${wrong.name} isn't ${what}: it takes ${extensions.map((e) => `.${e}`).join(" or ")}${multiple ? " files" : ""}.`;
  }

  /** A drag of files coming over an element of the window */
  function entered(event: DragEvent): void {
    if (carriesFiles(event)) over++;
  }

  /** A drag of files leaving an element of the window */
  function left(event: DragEvent): void {
    if (carriesFiles(event)) over = Math.max(0, over - 1);
  }

  /** A drag of files over the window: say a copy is wanted (the drop then comes here, not to the browser) */
  function hovering(event: DragEvent): void {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
  }

  /** Files dropped on the window: take them if they're what's wanted */
  function dropped(event: DragEvent): void {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    over = 0;
    const files = Array.from(event.dataTransfer?.files ?? []);
    // a folder comes as a File too: only its entry says what it is
    const folders = Array.from(event.dataTransfer?.items ?? []).some((item) => item.webkitGetAsEntry?.()?.isDirectory);
    refused = problemWith(files, folders);
    if (!refused && files.length) onDrop(files);
  }

  onMount(() => {
    window.addEventListener("dragenter", entered);
    window.addEventListener("dragleave", left);
    window.addEventListener("dragover", hovering);
    window.addEventListener("drop", dropped);
  });

  onDestroy(() => {
    window.removeEventListener("dragenter", entered);
    window.removeEventListener("dragleave", left);
    window.removeEventListener("dragover", hovering);
    window.removeEventListener("drop", dropped);
  });

  /** Open the picker, forgetting a drop turned down */
  function choose(): void {
    refused = "";
    onChoose();
  }
</script>

<div class="zone" class:dragging={over > 0} class:chosen={paths.length > 0} role="button" tabindex="0"
  aria-label={folder ? `Choose ${what} in ${FILE_MANAGER}` : `Drop ${what} here, or choose one in ${FILE_MANAGER}`}
  onclick={choose} onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), choose())}>
  {#if over > 0}
    <span class="big">Drop to use it</span>
  {:else if paths.length}
    <span class="big" title={paths.join("\n")}>{paths.length === 1 ? nameOf(paths[0]) : `${paths.length} files: ${paths.map(nameOf).join(", ")}`}</span>
    <span class="small">{folder ? "Click to choose another" : "Drop another, or click to choose another"}</span>
  {:else if folder}
    <span class="big">Choose {what}</span>
    <span class="small">click to find it in {FILE_MANAGER}</span>
  {:else}
    <span class="big">Drop {what} here</span>
    <span class="small">or click to choose one in {FILE_MANAGER}</span>
  {/if}
  {#if refused}<span class="refused" role="alert">{refused}</span>{/if}
</div>

<style>
  .zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    min-height: 96px;
    padding: 12px;
    border: 2px dashed var(--border);
    border-radius: 8px;
    background: var(--bg-input);
    text-align: center;
    cursor: pointer;
  }

  .zone:hover,
  .zone:focus-visible {
    border-color: var(--accent);
    outline: none;
  }

  .zone.dragging {
    border-color: var(--accent);
    border-style: solid;
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-input));
  }

  .big {
    font-weight: 600;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .small {
    color: var(--text-muted);
    font-size: 12px;
  }

  .refused {
    color: #e5534b;
    font-size: 12px;
  }
</style>
