<!--
  Edit a saved script in place, from the script panel's Saved scripts tab (Edit…), over the dialog the panel is in
  (Edit Button, or the Script Editor): the script box with its language's colours, its problems under it as it's
  written, and Save (back under the same name, in the settings folder's scripts folder, where the team shares it)
  or Cancel. Escape cancels this alone, not the dialog under it.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import ScriptBox from "./ScriptBox.svelte";
  import ScriptProblems from "./ScriptProblems.svelte";
  import { kindName, loadScript, saveScript, type SavedScript } from "./library";

  interface Props {
    /** The saved script edited */
    entry: SavedScript;
    /** Save: the saved scripts afterwards; Cancel: null */
    onClose: (saved: SavedScript[] | null) => void;
  }

  let { entry, onClose }: Props = $props();

  /** The script's text; null while it's being read */
  let text = $state<string | null>(null);
  let error = $state("");
  let box: ScriptBox | undefined = $state();
  /** The script box's language: a script's colours, or plain lines */
  const language = untrack(() => (entry.kind === "spsl" || entry.kind === "rhai" ? entry.kind : "plain"));

  void loadScript(untrack(() => entry)).then(
    (loaded) => (text = loaded),
    (e: unknown) => (error = `Couldn't read it: ${String(e)}`),
  );

  /** Save it back under its name */
  async function save(): Promise<void> {
    if (text === null) return;
    try {
      onClose(await saveScript(entry, text));
    } catch (e) {
      error = String(e);
    }
  }

  // Escape is this window's, caught before the dialog under it (Edit Button closes on Escape too)
  $effect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      onClose(null);
    };
    window.addEventListener("keydown", escape, true);
    return () => window.removeEventListener("keydown", escape, true);
  });
</script>

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Edit saved script">
    <header>Edit saved script: <b>{entry.name}</b> <span>· {kindName(entry.kind)}</span></header>
    {#if text === null}
      <p class="muted">{error || "Reading…"}</p>
    {:else}
      <div class="script">
        <ScriptBox bind:this={box} bind:value={text} {language} label="Saved script" />
        {#if language !== "plain"}
          <ScriptProblems script={text} {language} onGoTo={(line) => box?.goToLine(line)} />
        {/if}
      </div>
    {/if}
    <footer>
      <span class="error">{text === null ? "" : error}</span>
      <button class="primary" disabled={text === null} onclick={() => void save()}>Save</button>
      <button onclick={() => onClose(null)}>Cancel</button>
    </footer>
  </div>
</div>

<style>
  /* over Edit Button (130) and the Script Editor (120), under a name prompt (160) */
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
    width: min(1000px, 92vw);
    height: min(720px, 86vh);
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: 10px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.45);
  }

  header,
  header span,
  .muted {
    color: var(--text-muted);
  }

  header b {
    color: var(--text);
  }

  .script {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .error {
    flex: 1;
    color: #e5534b;
  }

  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 10px;
  }

  button:disabled {
    opacity: 0.5;
  }
</style>
