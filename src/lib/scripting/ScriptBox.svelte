<!--
  The box a script or the text to send is written in, for Edit Button and the Script Editor: Monaco, with SPSL's
  or Rhai's colours for a script (public/web/script, in an iframe as Highlighted Output is), and a Wrap lines switch
  above it at the right (WrapLines.svelte, kept in the preferences as scriptWrap), or in the dialog's own row
  instead (Edit Button's Icon row: `ownWrap` off). The box fills the space its dialog gives it and has no size handle of its own, so it can't
  be dragged past the dialog: for more of a long script, the dialog itself is big (Edit Button takes most of the
  window, and the Script Editor can be resized from its corner). Its colours are the app's, or those it's given
  (`look`: Edit Themes…'s preview, of a theme not yet in use). Until the page says it's ready (and in tests, where no page loads), a plain text box stands in, so the
  text can always be read and written.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { isDark, ownThemeColours } from "../settings/store.svelte";
  import { pref } from "../terminal/prefs";
  import WrapLines from "./WrapLines.svelte";

  interface Props {
    /** The script or text, kept in step with what's typed */
    value: string;
    /** "spsl" or "rhai" for a script (its colours), "plain" for lines to send */
    language: "spsl" | "rhai" | "plain";
    /** The box's name, for screen readers and tests */
    label: string;
    /** Shown in the plain box while it's empty */
    placeholder?: string;
    /** Whether the box shows its own Wrap lines above it; off where the dialog shows WrapLines elsewhere */
    ownWrap?: boolean;
    /**
     * Colours to draw it in instead of the app's: light or dark, and a theme of the user's own's colours (null for
     * Monaco's own). Edit Themes… previews the theme being edited with it, as the app's theme isn't that one yet
     */
    look?: { dark: boolean; colors: Record<string, string> | null };
  }

  let { value = $bindable(), language, label, placeholder = "", ownWrap = true, look }: Props = $props();

  /** Light or dark, and a theme's colours: what it's given, else the app's */
  const colours = $derived(look ?? { dark: isDark(), colors: ownThemeColours() });

  /** This box's page, by the id in its URL, which every message carries */
  const id = `script-${Math.random().toString(36).slice(2)}`;
  let frame: HTMLIFrameElement | undefined = $state();
  let plainBox: HTMLTextAreaElement | undefined = $state();
  /** Whether the page has Monaco up; until then the plain box stands in */
  let ready = $state(false);
  /** The text as the page last had it, so its own changes aren't sent back to it */
  let shown = "";
  /** Whether long lines wrap (Settings' preference, shared by every script box) */
  const wrap = $derived(pref("scriptWrap") !== "false");

  /** Send a message to the page */
  function post(message: Record<string, unknown>): void {
    frame?.contentWindow?.postMessage({ channel: "sxt-host", tab: id, message }, "/");
  }

  /** The page talking: ready (send it everything), or the text changed */
  function onMessage(event: MessageEvent): void {
    if (!frame || event.source !== frame.contentWindow) return;
    const envelope = event.data;
    if (!envelope || envelope.channel !== "sxt-page" || envelope.tab !== id) return;
    const message = envelope.message as { type: string; text?: string };
    if (message.type === "ready") {
      ready = true;
      shown = value;
      post({ type: "set", text: value, language, wrap, ...colours });
    } else if (message.type === "change") {
      shown = message.text ?? "";
      value = shown;
    }
  }
  window.addEventListener("message", onMessage);
  onDestroy(() => window.removeEventListener("message", onMessage));

  // text from outside (Revert, a saved script opened): to the page, unless it's the page's own
  $effect(() => {
    const text = value;
    if (ready && text !== shown) {
      shown = text;
      post({ type: "text", text });
    }
  });
  $effect(() => {
    if (ready) post({ type: "language", language });
  });
  $effect(() => {
    if (ready) post({ type: "wrap", wrap });
  });
  // light or dark, and a theme of the user's own's colours (null for none), as the app is drawn or as given
  $effect(() => {
    const { dark, colors } = colours;
    if (ready) post({ type: "dark", dark, colors });
  });

  /** Put text at the cursor, in place of what's selected (the SPSL panel's double-click) */
  export function insert(text: string): void {
    if (ready) {
      post({ type: "insert", text });
      frame?.focus();
      return;
    }
    const box = plainBox;
    const at = box?.selectionStart ?? value.length;
    value = value.slice(0, at) + text + value.slice(box?.selectionEnd ?? at);
    queueMicrotask(() => box?.focus());
  }

  /** Put the cursor at the start of a line (1-based) and show it: a problem clicked under the box */
  export function goToLine(line: number): void {
    if (ready) {
      frame?.focus();
      post({ type: "reveal", line });
      return;
    }
    const box = plainBox;
    if (!box) return;
    const lines = value.split("\n");
    const at = lines.slice(0, Math.max(0, line - 1)).reduce((sum, l) => sum + l.length + 1, 0);
    box.focus();
    box.setSelectionRange(at, at);
  }

  /** Put the keyboard in the box */
  export function focus(): void {
    if (ready) {
      frame?.focus();
      post({ type: "focus" });
    } else {
      plainBox?.focus();
    }
  }
</script>

<div class="script-box" role="group" aria-label="{label} box">
  {#if ownWrap}
    <div class="bar"><WrapLines /></div>
  {/if}
  <div class="area">
    <!-- every key is Monaco's (its own ⌘F find among them), none the app's: data-own-keys (shortcuts.ts) -->
    <iframe bind:this={frame} src="/web/script/index.html?tab={id}" title={label} class:hidden={!ready} data-own-keys="*"></iframe>
    {#if !ready}
      <textarea bind:this={plainBox} bind:value aria-label={label} {placeholder} spellcheck="false" wrap={wrap ? "soft" : "off"}></textarea>
    {/if}
  </div>
</div>

<style>
  .script-box {
    display: flex;
    flex-direction: column;
    gap: 4px;
    height: 100%;
    min-height: 0;
  }

  .area {
    position: relative;
    flex: 1;
    min-height: 120px;
    border: 1px solid var(--border);
    border-radius: 4px;
    overflow: hidden;
  }

  iframe,
  textarea {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: 0;
    box-sizing: border-box;
  }

  iframe.hidden {
    visibility: hidden;
  }

  textarea {
    padding: 4px 6px;
    background: var(--bg-input);
    font-family: Menlo, "Lucida Console", monospace;
    font-size: 12px;
    resize: none;
  }

  /* above the box, at its right, out of the way of the script's first lines */
  .bar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
