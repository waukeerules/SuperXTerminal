<!--
  An open serial tab's line settings, as PuTTY's Change Settings: the speed, data bits, stop bits, parity and flow
  control change at once, without reconnecting (e.g. ROMMON at 9600, then the switch's own 115200). They can also be
  saved to the tab's session, as -sercfg in its extra arguments.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import SerialLineFields from "../sessions/SerialLineFields.svelte";
  import { applySercfg, DEFAULT_LINE, toSercfg, withLine, type SerialLine } from "../sessions/serial";
  import { getSession, saveSession, type Store } from "../sessions/api";
  import type { PageHost } from "./PageHost";

  interface Props {
    /** The tab's title, for the window's title */
    title: string;
    /** The tab's host, which changes the line */
    host: PageHost;
    /** The tab's session, when it was opened from one */
    session: { store: Store; id: string } | null;
    /** Cancel, Escape, or after Apply */
    onClose: () => void;
  }

  let { title, host, session, onClose }: Props = $props();

  let line = $state<SerialLine>({ ...DEFAULT_LINE });
  let alsoSave = $state(false);
  let error = $state("");
  let ready = $state(false);

  onMount(async () => {
    try {
      const now = await host.serialLine();
      if (now === null) error = "The session isn't connected";
      else line = applySercfg(DEFAULT_LINE, now);
      ready = now !== null;
    } catch (e) {
      error = String(e);
    }
  });

  /** Change the open line's settings, and save them to the session too when asked */
  async function apply(): Promise<void> {
    error = "";
    try {
      await host.setSerialLine(toSercfg(line));
      if (alsoSave && session) {
        const fields = await getSession(session.store, session.id);
        await saveSession(session.store, session.id, { ...fields, extraArgs: withLine(fields.extraArgs, line) });
      }
      onClose();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Serial Settings">
    <header>Serial Settings — {title}</header>
    <SerialLineFields {line} disabled={!ready} onChange={(l) => (line = l)} />
    <label class="save">
      <input type="checkbox" bind:checked={alsoSave} disabled={!session} />
      Also save to the session{session ? "" : " (this tab wasn't opened from one)"}
    </label>
    <footer>
      <span class="error">{error}</span>
      <button class="primary" disabled={!ready} onclick={apply}>Apply</button>
      <button onclick={onClose}>Cancel</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 160;
  }
  .dialog {
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: calc(100vw - 32px);
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
  }
  button:disabled {
    opacity: 0.45;
  }
  header {
    font-weight: 600;
  }
  .save {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  footer {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: flex-end;
  }
  .error {
    flex: 1;
    color: #e5534b;
  }
</style>
