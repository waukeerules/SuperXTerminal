<!--
  Settings > Terminal, as SuperXPuTTY's Options > Terminal: the terminal's preferences, shared by every tab and
  applied to open tabs at once. The same preferences change from a tab's own menu (font size, theme, copy on
  select...), and show here.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { allPrefs, drawnTheme, PREF_DEFAULTS, setPrefs } from "../terminal/prefs";
  import { highlighting } from "../terminal/highlighting";
  import { settings } from "./store.svelte";

  /** The colours a terminal theme sets, for showing what each colour box has when the user hasn't picked one */
  interface ThemeColours {
    background?: string;
    foreground?: string;
    selection?: string;
    cursor?: string;
  }

  let themes = $state<string[]>([]);
  /** Each theme's colours, by name, to show what "the theme's" is */
  let themeColours = $state<Record<string, ThemeColours>>({});
  let ruleSets = $state<{ id: string; name: string }[]>([]);

  /** re-read when a preference changes anywhere */
  const prefs = $derived.by(() => {
    void settings.terminalPrefs;
    return allPrefs();
  });

  onMount(async () => {
    const files = await highlighting();
    themes = files.themes.map((t) => String((t as { name?: string }).name ?? "")).filter(Boolean);
    themeColours = Object.fromEntries(files.themes.map((t) => {
      const theme = t as { name?: string; colors?: ThemeColours };
      return [String(theme.name ?? ""), theme.colors ?? {}];
    }));
    ruleSets = files.ruleSets.map((r) => {
      const set = r as { id?: string; name?: string };
      return { id: String(set.id ?? ""), name: String(set.name ?? set.id ?? "") };
    }).filter((r) => r.id);
  });

  /** The colours of the theme the terminal draws with: PuTTY's when none is chosen, Light or Dark when following the app */
  const shownTheme = $derived.by(() => {
    void prefs;
    return themeColours[drawnTheme()] ?? {};
  });

  /** The terminal's own colours: the preference, its theme's colour, and what it's for */
  const COLOURS: { pref: string; theme: keyof ThemeColours; label: string }[] = [
    { pref: "colorBackground", theme: "background", label: "Background" },
    { pref: "colorForeground", theme: "foreground", label: "Text" },
    { pref: "colorSelection", theme: "selection", label: "Selection" },
    { pref: "colorCursor", theme: "cursor", label: "Cursor colour" },
  ];

  /** A colour as it's drawn: the preference's, else the theme's */
  const colour = (c: (typeof COLOURS)[number]) =>
    prefs[c.pref] || shownTheme[c.theme] || (c.theme === "cursor" ? shownTheme.foreground : "") || "#000000";

  /** The fonts offered; any other can be typed */
  const FONTS = ["Menlo", "Monaco", "SF Mono", "Courier New", "Andale Mono", "PT Mono"];

  /** Set a terminal preference, which every tab takes at once */
  function set(name: string, value: string): void {
    void setPrefs({ [name]: value });
  }

  /** A number box's value, kept within its range */
  function number(name: string, value: string, min: number, max: number): void {
    const n = Math.round(Number(value));
    set(name, String(Number.isFinite(n) ? Math.min(max, Math.max(min, n)) : Number(PREF_DEFAULTS[name])));
  }
</script>

<div class="terminal">
  <fieldset>
    <legend>Look</legend>
    <label>Theme
      <select value={prefs.theme} onchange={(e) => set("theme", e.currentTarget.value)}>
        <option value="">PuTTY's (the profile's colours)</option>
        <option value="@look">Follow the app's look (Settings &gt; General &gt; Theme: Light or Dark)</option>
        {#each themes as theme}<option value={theme}>{theme}</option>{/each}
      </select>
    </label>
    <div class="colours" role="group" aria-label="Colours">
      <span>Colours</span>
      <div class="colour-rows">
        {#each COLOURS as c (c.pref)}
          <label class="colour">
            <input type="color" value={colour(c)} aria-label={c.label} onchange={(e) => set(c.pref, e.currentTarget.value)} />
            {c.label} <span class="hint">{prefs[c.pref] ? prefs[c.pref] : "the theme's"}</span>
            <button disabled={!prefs[c.pref]} onclick={(e) => { e.preventDefault(); void setPrefs({ [c.pref]: null }); }}
              title="Use the theme's colour again">Theme's</button>
          </label>
        {/each}
        <span class="sample" style:background={colour(COLOURS[0])} style:color={colour(COLOURS[1])}>
          admin@switch:~$ show version <span style:background={colour(COLOURS[2])}>selected text</span><span class="block" style:background={colour(COLOURS[3])}>&nbsp;</span>
        </span>
        <span class="hint">Over the theme's, in every tab; a profile's own colours (Settings &gt; Profiles &gt; Terminal) win. A session's or folder's environment colour still colours its cursor. Highlighting keeps the theme's colours, so pick a theme that suits the background.</span>
      </div>
    </div>
    <label>Font
      <select value={prefs.fontFamily} onchange={(e) => set("fontFamily", e.currentTarget.value)}>
        <option value="">The theme's</option>
        <option value="profile">The profile's</option>
        {#each FONTS as font}<option value={font}>{font}</option>{/each}
      </select>
    </label>
    <label>Font size
      <span class="row"><input type="number" min="0" max="72" value={prefs.fontSize}
        onchange={(e) => number("fontSize", e.currentTarget.value, 0, 72)} /> <span class="hint">pixels; 0: the theme's or profile's</span></span>
    </label>
    <label>Cursor
      <select value={prefs.cursorShape} onchange={(e) => set("cursorShape", e.currentTarget.value)}>
        <option value="profile">The profile's</option>
        <option value="block">Block</option>
        <option value="underline">Underline</option>
        <option value="bar">Bar</option>
      </select>
    </label>
    <label>Cursor blinks
      <select value={prefs.cursorBlink} onchange={(e) => set("cursorBlink", e.currentTarget.value)}>
        <option value="profile">As the profile says</option>
        <option value="on">Yes</option>
        <option value="off">No</option>
      </select>
    </label>
    <label>Bell
      <select value={prefs.bell} onchange={(e) => set("bell", e.currentTarget.value)}>
        <option value="profile">The profile's</option>
        <option value="visual">Flash the tab</option>
        <option value="sound">Sound</option>
        <option value="none">None</option>
      </select>
    </label>
    <label>Drawing
      <select value={prefs.renderer} onchange={(e) => set("renderer", e.currentTarget.value)}>
        <option value="webgl">WebGL (faster)</option>
        <option value="dom">DOM</option>
      </select>
    </label>
  </fieldset>

  <fieldset>
    <legend>Output</legend>
    <label>Highlighting
      <select value={prefs.ruleSet} onchange={(e) => set("ruleSet", e.currentTarget.value)}>
        <option value="auto">Automatic (from the output)</option>
        <option value="none">None</option>
        {#each ruleSets as set}<option value={set.id}>{set.name}</option>{/each}
      </select>
    </label>
    <label class="check"><input type="checkbox" checked={prefs.markMatches === "true"}
      onchange={(e) => set("markMatches", String(e.currentTarget.checked))} /> Mark matches of the selected text</label>
    <label class="check"><input type="checkbox" checked={prefs.copyOnSelect === "true"}
      onchange={(e) => set("copyOnSelect", String(e.currentTarget.checked))} /> Copy on select</label>
    <label class="check"><input type="checkbox" checked={prefs.wrap === "true"}
      onchange={(e) => set("wrap", String(e.currentTarget.checked))} /> Wrap lines
      <span class="hint">(off: lines up to 300 columns stay whole, with a scroll bar to pan along them)</span></label>
    <label class="check"><input type="checkbox" checked={prefs.lineNumbers === "true"}
      onchange={(e) => set("lineNumbers", String(e.currentTarget.checked))} /> Line numbers
      <span class="hint">(none while a full-screen program such as less or vim runs; less -N numbers its own)</span></label>
    <label>Scrollback
      <span class="row"><input type="number" min="0" max="1000000" step="1000" value={prefs.scrollback}
        onchange={(e) => number("scrollback", e.currentTarget.value, 0, 1000000)} /> <span class="hint">lines; 0: unlimited</span></span>
    </label>
    <label>Check pastes of
      <span class="row"><input type="number" min="0" max="10000" value={prefs.pasteLines}
        onchange={(e) => number("pasteLines", e.currentTarget.value, 0, 10000)} /> <span class="hint">lines or more before typing them (0: never)</span></span>
    </label>
    <label>Log time stamps
      <span class="row"><input type="number" min="0" max="3600" step="10" value={prefs.logTimestamps}
        onchange={(e) => number("logTimestamps", e.currentTarget.value, 0, 3600)} /> <span class="hint">seconds apart, with output (0: none)</span></span>
    </label>
    <label>Keys
      <select value={prefs.keys} onchange={(e) => set("keys", e.currentTarget.value)}>
        <option value="profile">PuTTY's (Backspace, Home, End and F1-F12 as the profile sends them)</option>
        <option value="xterm">xterm's</option>
      </select>
    </label>
  </fieldset>
</div>

<style>
  .terminal {
    display: grid;
    gap: 12px;
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

  label {
    display: grid;
    grid-template-columns: 150px 1fr;
    align-items: center;
    gap: 8px;
  }

  .colours {
    display: grid;
    grid-template-columns: 150px 1fr;
    gap: 8px;
  }

  .colour-rows {
    display: grid;
    gap: 4px;
  }

  label.colour {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  label.colour input {
    width: 36px;
    height: 22px;
    padding: 1px 2px;
  }

  .sample {
    max-width: 420px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 4px;
    font-family: Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .block {
    display: inline-block;
    width: 0.6em;
    margin-left: 4px;
  }

  label.check {
    display: flex;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .row input {
    width: 90px;
  }

  .hint {
    color: var(--text-muted);
  }

  select,
  input {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 3px 6px;
    max-width: 420px;
  }
</style>
