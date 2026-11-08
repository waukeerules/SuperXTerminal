<!--
  A tab's tunnels, from its tunnel icon or Tunnels… in its menu: what each one the session's ssh opened makes
  reachable (tunnels.ts), a link to open for a web page, and a command to check it works, with Copy; or, for one
  whose port another tab already holds, that it isn't open here and whose it is. Edit Tunnels…
  opens the tab's profile in Settings > Profiles; ssh can't add or drop a tunnel on a connection it has made, so a
  change takes effect when the session restarts, which Restart Session does here. Placed at the pointer, kept inside
  the window; closes on Escape or a click outside.
-->
<script lang="ts">
  import type { Forward } from "../settings/profiles";
  import { describeTunnel, takenText } from "./tunnels";

  interface Props {
    /** The tab's title, for the heading */
    title: string;
    /** The tunnels the session's ssh opened */
    tunnels: readonly Forward[];
    /** The profile's tunnels changed since the session started */
    changed: boolean;
    /** Those whose port another tab held already, so they aren't open here: their listen, and that tab's title */
    taken?: ReadonlyMap<string, string>;
    /** The profile the session uses, for Edit Tunnels…; "" for none */
    profile: string;
    /** Where it opens: the pointer, in the window's pixels */
    x: number;
    y: number;
    /** Escape, a click outside, or one of its buttons done */
    onClose: () => void;
    /** Open a link in the browser */
    onOpenLink: (url: string) => void;
    /** Open the profile in Settings > Profiles, scrolled to its Tunnels */
    onEdit: () => void;
    /** Restart the session, so its tunnels are the profile's now */
    onRestart: () => void;
  }

  let { title, tunnels, changed, taken = new Map(), profile, x, y, onClose, onOpenLink, onEdit, onRestart }: Props = $props();

  const rows = $derived(tunnels.map((t) => ({ tunnel: t, info: describeTunnel(t), holder: taken.get(t.listen) })));
  /** The test command last copied, so its button says so for a moment */
  let copied = $state<string | null>(null);

  let element: HTMLDivElement;
  let left = $state(0);
  let top = $state(0);
  // keep it inside the window
  $effect(() => {
    const rect = element.getBoundingClientRect();
    left = Math.max(4, Math.min(x, window.innerWidth - rect.width - 4));
    top = Math.max(4, Math.min(y + 6, window.innerHeight - rect.height - 4));
  });

  /** Copy a test command */
  function copy(command: string): void {
    void navigator.clipboard.writeText(command).then(() => {
      copied = command;
      setTimeout(() => copied === command && (copied = null), 1500);
    }, () => {});
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onClose()} />

<!-- a click outside closes it -->
<div class="backdrop" role="presentation" onpointerdown={onClose}></div>
<div class="panel" role="dialog" aria-label="Tunnels" bind:this={element} style:left="{left}px" style:top="{top}px">
  <h3>Tunnels: {title}</h3>
  {#if changed}
    <p class="changed" role="note">The profile's tunnels have changed. Restart the session to use them: ssh can't change the
      tunnels of a connection it has made.</p>
  {/if}
  <ul>
    {#each rows as { tunnel, info, holder } (`${tunnel.kind} ${tunnel.listen} ${tunnel.to}`)}
      <li>
        {#if holder !== undefined}<p class="changed" role="note" aria-label="Port in use">{takenText(tunnel, holder)}</p>{/if}
        <div class="what"><span class="kind">{info.kind}</span>
          {#if info.link}<a href={info.link} onclick={(e) => { e.preventDefault(); onOpenLink(info.link!); }}>{info.link}</a>{/if}
        </div>
        <p>{info.text}</p>
        <div class="test">
          <span class="where">{info.test.where === "server" ? "Test, typed in the tab:" : "Test, typed on this computer:"}</span>
          <code>{info.test.command}</code>
          <button onclick={() => copy(info.test.command)}>{copied === info.test.command ? "Copied" : "Copy"}</button>
        </div>
      </li>
    {/each}
  </ul>
  <p class="note">A tunnel needs the server to allow TCP forwarding (AllowTcpForwarding in its sshd_config, or its SSH
    settings); a refused one shows in the tab as "administratively prohibited".</p>
  <div class="buttons">
    <button onclick={() => { onEdit(); onClose(); }} disabled={!profile} title={profile ? `Settings > Profiles > ${profile}` : "The session has no profile"}>Edit Tunnels…</button>
    <button class:primary={changed} onclick={() => { onRestart(); onClose(); }}>Restart Session</button>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 199;
  }

  .panel {
    position: fixed;
    z-index: 200;
    width: min(520px, calc(100vw - 8px));
    max-height: calc(100vh - 8px);
    overflow: auto;
    box-sizing: border-box;
    padding: 10px 14px 12px;
    background: var(--bg-pane);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.35);
    font-size: 13px;
  }

  h3 {
    margin: 0 0 6px;
    font-size: 13px;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    padding: 6px 0;
    border-top: 1px solid var(--border);
  }

  .what {
    display: flex;
    gap: 8px;
    align-items: baseline;
  }

  .kind {
    font-weight: 600;
  }

  a {
    color: var(--accent);
  }

  p {
    margin: 2px 0;
  }

  .note,
  .where,
  .changed {
    color: var(--text-muted);
  }

  .changed {
    color: #f0883e;
  }

  .test {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    margin-top: 2px;
  }

  code {
    padding: 1px 4px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 3px;
    user-select: text;
    word-break: break-all;
  }

  .note {
    margin-top: 6px;
    font-size: 12px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }

  button {
    color: var(--text);
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 8px;
  }

  button.primary {
    background: var(--accent);
    color: #fff;
  }

  button:disabled {
    opacity: 0.45;
  }
</style>
