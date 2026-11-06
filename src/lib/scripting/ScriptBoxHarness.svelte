<!--
  For ScriptBox.test.ts only: a ScriptBox with its text bound, as Edit Button binds it, so a test can read what the
  box has made of the text and change it from outside (as Revert does).
-->
<script lang="ts">
  import ScriptBox from "./ScriptBox.svelte";

  let { start = "", language = "spsl" }: { /** The text to start with */ start?: string; /** The box's language */ language?: "spsl" | "rhai" | "plain" } = $props();

  // the first text only: after that the box and the test change it
  // svelte-ignore state_referenced_locally
  let value = $state(start);
  let box: ScriptBox | undefined = $state();

  /** The text the box has now */
  export function current(): string {
    return value;
  }

  /** Change the text from outside the box */
  export function setValue(text: string): void {
    value = text;
  }

  /** The box's insert, as the SPSL panel calls it */
  export function insert(text: string): void {
    box?.insert(text);
  }
</script>

<ScriptBox bind:this={box} bind:value {language} label="Script" />
