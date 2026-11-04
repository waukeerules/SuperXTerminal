<!--
  The Paste window, as SuperXPuTTY's dlgPasteCheck: "Paste n lines into session?", the text in an editable box, and
  Paste (Cmd+Enter) and Cancel (Esc). What's in the box when Paste is pressed is what's typed.
-->
<script lang="ts">
  import { MOD } from "../platform";
  import { pasteLineCount, pasting } from "./pasting.svelte";

  let text = $state("");
  let box: HTMLTextAreaElement | undefined = $state();

  // a new paste: its text, with the caret at the start and nothing selected
  $effect(() => {
    const current = pasting.current;
    if (!current) return;
    text = current.text.replace(/\r\n?/g, "\n");
    queueMicrotask(() => {
      box?.focus();
      box?.setSelectionRange(0, 0);
      if (box) box.scrollTop = 0;
    });
  });

  /** How many lines the paste has now, for the question */
  const lines = $derived(pasteLineCount(text));

  /** Close the window, giving back the text to type (with CR LF line breaks) or null for Cancel */
  function finish(paste: boolean): void {
    const current = pasting.current;
    if (!current) return;
    pasting.current = null;
    // CR LF line breaks, as the window gives them back on Windows; the page types them as Enter
    current.resolve(paste && text ? text.replace(/\n/g, "\r\n") : null);
  }

  /** Escape cancels, Cmd+Enter (Ctrl+Enter on Windows) pastes */
  function keydown(e: KeyboardEvent): void {
    if (!pasting.current) return;
    if (e.key === "Escape") finish(false);
    else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      finish(true);
    }
  }
</script>

<svelte:window onkeydown={keydown} />

{#if pasting.current}
  <div class="backdrop" role="presentation">
    <div class="dialog" role="dialog" aria-modal="true" aria-label="Paste">
      <header>Paste {lines} line{lines === 1 ? "" : "s"} into {pasting.current.session}? Check the text, and change it here if needed.</header>
      <textarea bind:this={box} bind:value={text} wrap="off" spellcheck="false"
        title="Change the text here before pasting it; Return adds a line, {MOD}+Return pastes"></textarea>
      <footer>
        <button class="primary" disabled={!text} title="Type the text as it is in the box ({MOD}+Return)" onclick={() => finish(true)}>Paste</button>
        <button onclick={() => finish(false)}>Cancel</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 150;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(680px, 94vw);
    height: min(360px, 90vh);
    min-height: 220px;
    display: grid;
    grid-template-rows: auto 1fr auto;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    color: var(--text-muted);
  }

  textarea {
    resize: none;
    overflow: auto;
    font-family: Menlo, "Courier New", monospace;
    font-size: 12px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 6px;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  button {
    min-width: 80px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 10px;
  }

  button:disabled {
    opacity: 0.5;
  }
</style>
