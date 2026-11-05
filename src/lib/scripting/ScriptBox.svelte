<!--
  The box a script or the text to send is written in, for Edit Button and the Script Editor: Monaco, with SPSL's
  colours for a script (public/web/script, in an iframe as Highlighted Output is), and a Wrap lines switch kept in the
  preferences (scriptWrap). The box fills the space its dialog gives it and has no size handle of its own, so it can't
  be dragged past the dialog; Pop out grows it over the whole window to see more of a long script, and Pop in (or
  Escape) puts it back. It grows in place rather than moving, as moving an iframe would load its page again and lose
  the editor. Until the page says it's ready (and in tests, where no page loads), a plain text box stands in, so the
  text can always be read and written.
-->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { isDark } from "../settings/store.svelte";
  import { pref, setPrefs } from "../terminal/prefs";

  interface Props {
    /** The script or text, kept in step with what's typed */
    value: string;
    /** "spsl" for a script (its colours), "plain" for lines to send */
    language: "spsl" | "plain";
    /** The box's name, for screen readers and tests */
    label: string;
    /** Shown in the plain box while it's empty */
    placeholder?: string;
  }

  let { value = $bindable(), language, label, placeholder = "" }: Props = $props();

  /** This box's page, by the id in its URL, which every message carries */
  const id = `script-${Math.random().toString(36).slice(2)}`;
  let frame: HTMLIFrameElement | undefined = $state();
  let plainBox: HTMLTextAreaElement | undefined = $state();
  /** Whether the page has Monaco up; until then the plain box stands in */
  let ready = $state(false);
  /** The text as the page last had it, so its own changes aren't sent back to it */
  let shown = "";
  /** Whether the box is popped out over the window */
  let popped = $state(false);
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
      post({ type: "set", text: value, language, wrap, dark: isDark() });
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
  $effect(() => {
    if (ready) post({ type: "dark", dark: isDark() });
  });

  // while popped out, Escape puts it back rather than closing the dialog (caught before the dialog's own listener)
  $effect(() => {
    if (!popped) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      popped = false;
    };
    window.addEventListener("keydown", escape, true);
    return () => window.removeEventListener("keydown", escape, true);
  });

  /** Pop the box out over the window, or back in; the keyboard stays in the script */
  function togglePopped(): void {
    popped = !popped;
    queueMicrotask(focus);
  }

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

<div class="script-box" class:popped role="group" aria-label={popped ? `${label} box, popped out` : `${label} box`}>
  <div class="area">
    <!-- every key is Monaco's (its own ⌘F find among them), none the app's: data-own-keys (shortcuts.ts) -->
    <iframe bind:this={frame} src="/web/script/index.html?tab={id}" title={label} class:hidden={!ready} data-own-keys="*"></iframe>
    {#if !ready}
      <textarea bind:this={plainBox} bind:value aria-label={label} {placeholder} spellcheck="false" wrap={wrap ? "soft" : "off"}></textarea>
    {/if}
  </div>
  <div class="bar">
    <label class="wrap" title="Wrap long lines at the box's edge, or scroll sideways to see them">
      <input type="checkbox" checked={wrap} onchange={(e) => void setPrefs({ scriptWrap: e.currentTarget.checked ? "true" : "false" })} />
      Wrap lines
    </label>
    <span class="grow"></span>
    <button class="pop" onclick={togglePopped} title={popped ? "Put the script back in the dialog (Esc)" : "See more of the script: the box over the whole window"}>
      {popped ? "⤡ Pop in" : "⤢ Pop out"}
    </button>
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

  /* over the whole window, above the dialog it's in (which has no transform, so fixed is the window's) */
  .script-box.popped {
    position: fixed;
    inset: 24px;
    z-index: 200;
    height: auto;
    padding: 10px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.45);
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .grow {
    flex: 1;
  }

  .wrap {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .pop {
    font-size: 12px;
    padding: 1px 8px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
</style>
