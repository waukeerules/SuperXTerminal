<!--
  A small window asking for values: a snippet's {prompt:...} and {secret:...} fields (SuperXPuTTY's SnippetPrompts
  form, with Send and Cancel), or SPSL's PROMPT and PWDPROMPT (frmPrivatePrompt: one box, masked for a password).
  Opened through ask() in ./asking.svelte.ts, which any part of the window can await.
-->
<script lang="ts">
  import { asking } from "./asking.svelte";

  let boxes: HTMLInputElement[] = $state([]);

  $effect(() => {
    if (asking.current) queueMicrotask(() => boxes[0]?.select());
  });

  /** Close the window, giving back the values typed (OK, Send) or null (Cancel, Escape) */
  function finish(ok: boolean): void {
    const current = asking.current;
    if (!current) return;
    asking.current = null;
    current.resolve(ok ? current.fields.map((f) => f.value) : null);
  }
</script>

<svelte:window onkeydown={(e) => asking.current && e.key === "Escape" && finish(false)} />

{#if asking.current}
  {@const current = asking.current}
  <div class="backdrop" role="presentation">
    <form class="dialog" aria-label={current.title}
      onsubmit={(e) => { e.preventDefault(); finish(true); }}>
      <header>{current.title}</header>
      {#each current.fields as field, i}
        <label>
          <span>{field.label}</span>
          {#if field.choices}
            <select bind:value={field.value}>
              {#each field.choices as choice}<option value={choice}>{choice}</option>{/each}
            </select>
          {:else}
            <input bind:this={boxes[i]} bind:value={field.value} type={field.secret ? "password" : "text"}
              autocomplete="off" />
          {/if}
        </label>
      {/each}
      <footer>
        <button type="submit" class="primary">{current.okLabel}</button>
        <button type="button" onclick={() => finish(false)}>Cancel</button>
      </footer>
    </form>
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
    min-width: 380px;
    display: grid;
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

  label {
    display: grid;
    grid-template-columns: max-content 260px;
    align-items: center;
    gap: 8px;
  }

  input,
  select,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  button {
    min-width: 80px;
  }
</style>
