<!--
  Asks for a name (New folder, Rename), as SuperXPuTTY's dlgRenameItem: the name is checked as it's typed, and OK
  is only possible when it's valid.
-->
<script lang="ts">
  import { untrack } from "svelte";

  interface Props {
    title: string;
    value: string;
    /** Why a name isn't allowed, or "" when it is */
    validate: (name: string) => string;
    onOk: (name: string) => void;
    onCancel: () => void;
  }

  let { title, value, validate, onOk, onCancel }: Props = $props();

  let name = $state(untrack(() => value));
  const problem = $derived(validate(name.trim()));

  /** Select the text when the field appears, ready to type over */
  function focus(input: HTMLInputElement): void {
    input.focus();
    input.select();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onCancel()} />

<div class="backdrop" role="presentation">
  <form class="dialog" aria-label={title} onsubmit={(e) => { e.preventDefault(); if (!problem) onOk(name.trim()); }}>
    <header>{title}</header>
    <input bind:value={name} use:focus />
    <p class="problem">{problem}</p>
    <footer>
      <button type="submit" class="primary" disabled={!!problem}>OK</button>
      <button type="button" onclick={onCancel}>Cancel</button>
    </footer>
  </form>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 160;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: 380px;
    display: grid;
    gap: 8px;
    padding: 14px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    font-weight: 600;
  }

  input,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
  }

  .problem {
    min-height: 1.2em;
    margin: 0;
    color: #e5534b;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  button:disabled {
    opacity: 0.45;
  }
</style>
