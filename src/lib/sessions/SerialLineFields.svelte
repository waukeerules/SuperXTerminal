<!--
  PuTTY's Serial settings: speed (a list of the usual ones, or typed), data bits, stop bits, parity and flow
  control. Used in Edit Session and in an open serial tab's Serial Settings.
-->
<script lang="ts">
  import { DATA_BITS, FLOWS, PARITIES, SPEEDS, STOP_BITS, type SerialLine } from "./serial";

  interface Props {
    /** The line's settings shown */
    line: SerialLine;
    /** Called with the settings after any change */
    onChange: (line: SerialLine) => void;
    /** Greyed, as for a field locked in the database */
    disabled?: boolean;
  }

  let { line, onChange, disabled = false }: Props = $props();

  /** Change some of the line's settings, keeping the rest */
  const set = (changes: Partial<SerialLine>) => onChange({ ...line, ...changes });
</script>

<div class="line">
  <label>Speed (baud):
    <input class="speed" list="serial-speeds" inputmode="numeric" value={line.speed} {disabled}
      onchange={(e) => { const speed = Number(e.currentTarget.value); if (speed > 0) set({ speed }); else e.currentTarget.value = String(line.speed); }} />
    <datalist id="serial-speeds">{#each SPEEDS as s}<option value={s}></option>{/each}</datalist>
  </label>
  <label>Data bits:
    <select value={line.dataBits} {disabled} onchange={(e) => set({ dataBits: Number(e.currentTarget.value) })}>
      {#each DATA_BITS as b}<option value={b}>{b}</option>{/each}
    </select>
  </label>
  <label>Stop bits:
    <select value={line.stopBits} {disabled} onchange={(e) => set({ stopBits: e.currentTarget.value })}>
      {#each STOP_BITS as b}<option value={b}>{b}</option>{/each}
    </select>
  </label>
  <label>Parity:
    <select value={line.parity} {disabled} onchange={(e) => set({ parity: e.currentTarget.value })}>
      {#each PARITIES as [value, name]}<option {value}>{name}</option>{/each}
    </select>
  </label>
  <label>Flow control:
    <select value={line.flow} {disabled} onchange={(e) => set({ flow: e.currentTarget.value })}>
      {#each FLOWS as [value, name]}<option {value}>{name}</option>{/each}
    </select>
  </label>
</div>

<style>
  .line {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 12px;
    align-items: center;
  }
  label {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  input,
  select {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }
  input:disabled,
  select:disabled {
    opacity: 0.45;
  }
  .speed {
    width: 7em;
  }
</style>
