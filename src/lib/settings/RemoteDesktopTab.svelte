<!--
  Settings > Remote Desktop, SuperXPuTTY's Options > RDP for the Mac: whether RDP sessions open in a tab (the built-in
  client) or in Windows App, and the options they connect with unless a session has its own (Edit Session > RDP
  options…). Saved as SuperXPuTTY's RdpOptions text, only what differs from its defaults.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { saveSettings, settings } from "./store.svelte";
  import RdpOptionsForm from "../sessions/RdpOptionsForm.svelte";
  import { BUILT_IN_DEFAULTS, formatRdpOptions, userDefaults, type RdpOptions } from "../sessions/rdpOptions";

  let options = $state<RdpOptions>(untrack(() => userDefaults(settings.rdpDefaults)));

  /** Save what differs from SuperXPuTTY's defaults */
  function save(): void {
    const changed: Record<string, unknown> = {};
    for (const [key, v] of Object.entries(options)) {
      if (v !== BUILT_IN_DEFAULTS[key as keyof RdpOptions]) changed[key] = v;
    }
    void saveSettings({ rdpDefaults: formatRdpOptions(changed as RdpOptions) });
  }

  function reset(): void {
    options = { ...BUILT_IN_DEFAULTS };
    save();
  }
</script>

<div class="remote">
  <fieldset>
    <legend>Remote Desktop sessions</legend>
    <label class="check"><input type="checkbox" checked={settings.rdpBuiltIn}
      onchange={(e) => void saveSettings({ rdpBuiltIn: e.currentTarget.checked })} /> Open RDP sessions in a tab (the built-in client)</label>
    <p class="hint">As SuperXPuTTY's built-in client. Untick to open them in Microsoft's Windows App instead; Connect
      external, in a session's menu, opens one there either way. The options below are the defaults for every RDP
      session; a session can have its own in Edit Session &gt; RDP options….</p>
  </fieldset>

  {#key options}
    <RdpOptionsForm bind:value={options} onChange={save} />
  {/key}

  <div class="actions">
    <button onclick={reset} title="SuperXPuTTY's defaults">Reset to Defaults</button>
  </div>
</div>

<style>
  .remote {
    display: grid;
    gap: 10px;
    align-content: start;
    height: 100%;
    overflow: auto;
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    display: grid;
    gap: 8px;
    padding: 8px 12px 12px;
  }

  .check {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .hint {
    margin: 0;
    color: var(--text-muted);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }
</style>
