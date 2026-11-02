<!--
  The Remote Desktop options, as SuperXPuTTY's Options > RDP page and its RDP Session Options dialog: Settings > Remote
  Desktop shows the defaults (every option set); Edit Session's RDP options… shows a session's own, where every option
  can stay at Default (a list's "Default (…)" choice, a filled tick box, an empty field). Options marked * are kept and
  passed on to SuperXPuTTY and Windows App (Connect external), but the built-in client doesn't have them yet.
-->
<script lang="ts">
  import type { FlagKey, NumberKey, RdpOptions } from "./rdpOptions";

  interface Props {
    /** The options shown; changed in place */
    value: RdpOptions;
    /** A session's: what Default stands for; absent for Settings' defaults */
    defaults?: Required<RdpOptions>;
    onChange: () => void;
  }

  let { value = $bindable(), defaults, onChange }: Props = $props();

  const session = $derived(!!defaults);

  const COLOUR_DEPTHS: [number, string][] = [[15, "15-bit (High Colour)"], [16, "16-bit (High Colour)"], [24, "24-bit (True Colour)"], [32, "32-bit (Highest Quality)"]];
  const AUDIO = ["Play on this computer", "Play on the remote computer", "Don't play"];
  const KEYBOARD = ["On this computer", "On the remote computer", "Only when using the full screen"];
  const AUTHENTICATION = ["Connect and don't warn me", "Don't connect", "Warn me"];

  /** A tick box's state, as shown: a session's unset one shows its default, filled */
  function ticked(key: FlagKey): boolean {
    return value[key] ?? defaults?.[key] ?? false;
  }

  /** A click on a tick box: on, off and (a session's) back to Default, as SuperXPuTTY's three-state boxes */
  function flip(key: FlagKey, box: HTMLInputElement): void {
    if (!session) value[key] = box.checked;
    else if (value[key] === undefined) value[key] = true;
    else if (value[key] === true) value[key] = false;
    else value[key] = undefined;
    box.checked = ticked(key);
    box.indeterminate = session && value[key] === undefined;
    onChange();
  }

  function flagTitle(key: FlagKey): string {
    if (!session) return "";
    return `Filled: Default (${defaults![key] ? "on" : "off"}). Ticked: on. Empty: off.`;
  }

  /** A list's choice: a number, or "" for a session's Default */
  function choose(key: NumberKey, text: string): void {
    value[key] = text === "" ? undefined : Number(text);
    onChange();
  }

  function sizeChanged(key: "width" | "height", text: string): void {
    const n = Number(text);
    value[key] = text.trim() === "" || !Number.isFinite(n) || n <= 0 ? (session ? undefined : value[key]) : Math.min(8192, Math.max(200, Math.round(n)));
    onChange();
  }

  const fixed = $derived((value.display ?? defaults?.display) === "Fixed");
</script>

{#snippet flag(key: FlagKey, label: string, unsupported = false)}
  <label class="check" title={flagTitle(key)}>
    <input type="checkbox" checked={ticked(key)} indeterminate={session && value[key] === undefined}
      onchange={(e) => flip(key, e.currentTarget)} />
    {label}{#if unsupported}<span class="star" title="Not in the built-in client yet">*</span>{/if}
  </label>
{/snippet}

{#snippet list(key: NumberKey, choices: [number, string][], unsupported = false)}
  <select value={value[key] === undefined ? "" : String(value[key])} onchange={(e) => choose(key, e.currentTarget.value)}>
    {#if session}<option value="">Default ({choices.find(([n]) => n === defaults![key])?.[1] ?? defaults![key]})</option>{/if}
    {#each choices as [n, label]}<option value={String(n)}>{label}</option>{/each}
  </select>{#if unsupported}<span class="star" title="Not in the built-in client yet">*</span>{/if}
{/snippet}

<div class="rdp-options">
  <fieldset>
    <legend>Display</legend>
    <div class="row">
      <label>Desktop size:
        <select value={value.display ?? ""} onchange={(e) => { value.display = (e.currentTarget.value || undefined) as RdpOptions["display"]; onChange(); }}>
          {#if session}<option value="">Default ({defaults!.display === "Fixed" ? `${defaults!.width} × ${defaults!.height}` : "Fit the tab"})</option>{/if}
          <option value="FitTab">Fit the tab</option>
          <option value="Fixed">Fixed size</option>
        </select>
      </label>
      <input class="size" type="number" min="200" max="8192" disabled={!fixed} value={value.width ?? ""} placeholder={String(defaults?.width ?? "")}
        aria-label="Width" onchange={(e) => sizeChanged("width", e.currentTarget.value)} />
      ×
      <input class="size" type="number" min="200" max="8192" disabled={!fixed} value={value.height ?? ""} placeholder={String(defaults?.height ?? "")}
        aria-label="Height" onchange={(e) => sizeChanged("height", e.currentTarget.value)} />
    </div>
    <div class="row">
      <label>Colour depth: {@render list("colorDepth", COLOUR_DEPTHS)}</label>
    </div>
    {@render flag("smartSizing", "Smart sizing: scale a fixed-size desktop to fit the tab")}
    {@render flag("retina", "Full Retina resolution (sharper text, more to send; the desktop's text is smaller)")}
  </fieldset>

  <fieldset>
    <legend>Local resources</legend>
    <div class="grid">
      {@render flag("clipboard", "Clipboard (text)")}
      {@render flag("drives", "Drives", true)}
      {@render flag("printers", "Printers", true)}
      {@render flag("smartCards", "Smart cards", true)}
      {@render flag("microphone", "Microphone", true)}
    </div>
    <div class="row"><label>Remote audio: {@render list("audio", AUDIO.map((t, i) => [i, t]))}</label></div>
    <div class="row"><label>Windows key combinations: {@render list("keyboard", KEYBOARD.map((t, i) => [i, t]), true)}</label></div>
  </fieldset>

  <fieldset>
    <legend>Connection</legend>
    <div class="row"><label>If server authentication fails: {@render list("authentication", AUTHENTICATION.map((t, i) => [i, t]), true)}</label></div>
    <div class="row">
      <label class="grow">RD Gateway:
        <input value={value.gateway ?? ""} placeholder={session ? `Default (${defaults!.gateway || "no gateway"})` : "none; e.g. rdg.example.com"}
          onchange={(e) => { const g = e.currentTarget.value.trim(); value.gateway = g === "" && session ? undefined : g; onChange(); }} />
      </label>
    </div>
    {@render flag("console", "Connect to the admin (console) session", true)}
  </fieldset>

  <p class="hint">* Kept for SuperXPuTTY and Windows App (Connect external); the built-in client doesn't have it yet.
    ⌘ keys stay with SuperXTerminal; Control and Option are Windows' Ctrl and Alt.</p>
</div>

<style>
  .rdp-options {
    display: grid;
    gap: 10px;
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    display: grid;
    gap: 8px;
    padding: 8px 12px 12px;
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .grow {
    flex: 1;
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .grow input {
    flex: 1;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(3, auto);
    justify-content: start;
    gap: 6px 24px;
  }

  .check {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .size {
    width: 72px;
  }

  .star {
    color: var(--text-muted);
    margin-left: 2px;
  }

  .hint {
    margin: 0;
    color: var(--text-muted);
  }
</style>
