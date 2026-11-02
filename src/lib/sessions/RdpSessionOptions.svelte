<!--
  RDP options…, from Edit Session: a session's own Remote Desktop options, as SuperXPuTTY's RDP Session Options. Every
  option starts at Default (Settings > Remote Desktop's); OK hands back the ones set, as its RdpOptions text, which
  Edit Session saves with the rest.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { settings } from "../settings/store.svelte";
  import RdpOptionsForm from "./RdpOptionsForm.svelte";
  import { formatRdpOptions, parseRdpOptions, userDefaults, type RdpOptions } from "./rdpOptions";

  interface Props {
    name: string;
    /** The session's RdpOptions text */
    text: string;
    onDone: (text: string) => void;
    onCancel: () => void;
  }

  let { name, text, onDone, onCancel }: Props = $props();

  let options = $state<RdpOptions>(untrack(() => parseRdpOptions(text)));
  const defaults = $derived(userDefaults(settings.rdpDefaults));
  const title = $derived(`RDP options: ${name || "new session"}`);
</script>

<svelte:window onkeydown={(e) => { if (e.key === "Escape") { e.stopPropagation(); onCancel(); } }} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
    <header>{title}</header>
    <p class="note">This session's own options. Left at Default, an option follows Settings &gt; Remote Desktop.</p>
    {#key options}
      <RdpOptionsForm bind:value={options} {defaults} onChange={() => {}} />
    {/key}
    <footer>
      <button onclick={() => (options = {})} title="Every option back to Default">Use Defaults</button>
      <span class="grow"></span>
      <button class="primary" onclick={() => onDone(formatRdpOptions(options))}>OK</button>
      <button onclick={onCancel}>Cancel</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 130;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(680px, 94vw);
    max-height: 94vh;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    font-weight: 600;
  }

  .note {
    margin: 0;
    color: var(--text-muted);
  }

  footer {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .grow {
    flex: 1;
  }
</style>
