<!--
  The Wrap lines switch for script boxes: whether long lines wrap at the box's edge, kept in the preferences
  (scriptWrap) and shared by every script box, which follows it (ScriptBox.svelte). The script box shows it above
  itself at the right; a dialog with a row to spare puts it there instead (Edit Button's Icon row) and tells the
  box not to show its own.
-->
<script lang="ts">
  import { pref, setPrefs } from "../terminal/prefs";

  /** Whether long lines wrap (on unless turned off) */
  const wrap = $derived(pref("scriptWrap") !== "false");
</script>

<label class="wrap" title="Wrap long lines at the box's edge, or scroll sideways to see them">
  <input type="checkbox" checked={wrap} onchange={(e) => void setPrefs({ scriptWrap: e.currentTarget.checked ? "true" : "false" })} />
  Wrap lines
</label>

<style>
  .wrap {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-muted);
    white-space: nowrap;
  }
</style>
