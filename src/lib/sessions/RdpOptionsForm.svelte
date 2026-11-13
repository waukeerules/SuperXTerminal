<!--
  The Remote Desktop options, as SuperXPuTTY's Options > RDP page and its RDP Session Options dialog: Settings > Remote
  Desktop shows the defaults (every option set); Edit Session's RDP options… shows a session's own, where every option
  can stay at Default (a list's "Default (…)" choice, a filled tick box, an empty field). Options marked * are kept and
  passed on to SuperXPuTTY and Windows App (Connect external), but the built-in client doesn't have them yet.
-->
<script lang="ts">
  import type { FlagKey, NumberKey, RdpOptions } from "./rdpOptions";
  import { isMac } from "../shortcuts";
  import { isWindows } from "../platform";

  interface Props {
    /** The options shown; changed in place */
    value: RdpOptions;
    /** A session's: what Default stands for; absent for Settings' defaults */
    defaults?: Required<RdpOptions>;
    /** Called after any change to `value` */
    onChange: () => void;
  }

  let { value = $bindable(), defaults, onChange }: Props = $props();

  /** Whether it's a session's options (with Default for each), rather than Settings' defaults */
  const session = $derived(!!defaults);

  /** The colour depths offered, as Remote Desktop Connection's */
  const COLOUR_DEPTHS: [number, string][] = [[15, "15-bit (High Colour)"], [16, "16-bit (High Colour)"], [24, "24-bit (True Colour)"], [32, "32-bit (Highest Quality)"]];
  /** Where the remote computer's sound plays, as Remote Desktop Connection's */
  const AUDIO = ["Play on this computer", "Play on the remote computer", "Don't play"];
  /** Where Windows key combinations go, as Remote Desktop Connection's */
  const KEYBOARD = ["On this computer", "On the remote computer", "Only when using the full screen"];
  /** What the built-in client's clipboard passes on this system: everything on Windows, text elsewhere so far */
  const CLIPBOARD = isWindows ? "Clipboard (text, pictures, formatted text, files)" : "Clipboard (text)";
  /** What to do when the server's identity can't be checked, as Remote Desktop Connection's */
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

  /** A session's tick box's tooltip, saying what Default is; none for Settings' defaults */
  function flagTitle(key: FlagKey): string {
    if (!session) return "";
    return `Filled: Default (${defaults![key] ? "on" : "off"}). Ticked: on. Empty: off.`;
  }

  /** A list's choice: a number, or "" for a session's Default */
  function choose(key: NumberKey, text: string): void {
    value[key] = text === "" ? undefined : Number(text);
    onChange();
  }

  /** Take a width or height typed: 200 to 8192 pixels; empty or nonsense is Default for a session */
  function sizeChanged(key: "width" | "height", text: string): void {
    const n = Number(text);
    value[key] = text.trim() === "" || !Number.isFinite(n) || n <= 0 ? (session ? undefined : value[key]) : Math.min(8192, Math.max(200, Math.round(n)));
    onChange();
  }

  /** Whether the size is fixed (rather than fitting the tab), which shows the width and height */
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
      {@render flag("clipboard", CLIPBOARD)}
      {@render flag("drives", "Drives")}
      {@render flag("printers", "Printers", true)}
      {@render flag("smartCards", "Smart cards", true)}
      {@render flag("microphone", "Microphone", true)}
    </div>
    <div class="row"><label>Remote audio: {@render list("audio", AUDIO.map((t, i) => [i, t]))}</label></div>
    <div class="row"><label>Windows key combinations{isMac ? " (⌘ and a key)" : ""}: {@render list("keyboard", KEYBOARD.map((t, i) => [i, t]))}</label></div>
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

  {#if isMac}
    <p class="hint">* Kept for SuperXPuTTY and Windows App (Connect external); the built-in client doesn't have it yet.
      Keys: Control and Option are Windows' Ctrl and Alt; ⌘C, ⌘V, ⌘X, ⌘A and ⌘Z are its Ctrl ones. Where Windows
      key combinations are on, ⌘ tapped alone is the Windows key (Start) and ⌘ and another key is the Windows key
      and it (⌘L locks, ⌘R is Run), except ⌘W, ⌘T, ⌘Q,
      ⌘, ⌘`, ⌘1–9 and ⌃⌘F, which stay SuperXTerminal's. Fn+Control+Option+Delete is Ctrl+Alt+Delete; the tab's menu has
      the rest.</p>
  {:else}
    <p class="hint">* Kept for SuperXPuTTY and Remote Desktop Connection (Connect external); the built-in client doesn't
      have it yet. Keys go to the remote computer as typed, except SuperXTerminal's own shortcuts. Windows keeps
      Ctrl+Alt+Delete and some Windows key combinations (Win+L) for itself: Ctrl+Alt+End is Ctrl+Alt+Delete, and the
      tab's menu has the rest.</p>
  {/if}
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
