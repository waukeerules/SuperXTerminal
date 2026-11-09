<!--
  The Session Detail pane (View > Session Detail), as SuperXPuTTY's SessionDetail: the session selected in the tree,
  field by field in its property grid's order and names, with a field's description below when it's clicked. A -pw
  password shows masked. Read here; a double-click opens Edit Session.
-->
<script lang="ts">
  import Icon from "../icons/Icon.svelte";
  import type { CustomIcon } from "../icons/custom";
  import { getSession, type SessionFields, type SessionView } from "./api";

  interface Props {
    /** The session shown; null when none is selected */
    session: SessionView | null;
    /** The user's custom icons, for the session's icon */
    customs: ReadonlyMap<string, CustomIcon>;
    /** Edit…: open the session in Edit Session */
    onEdit: (session: SessionView) => void;
    /** The pane's close button */
    onClose: () => void;
  }

  let { session, customs, onEdit, onClose }: Props = $props();

  /** The rows: SuperXPuTTY's (DisplayName, Description), then the fields the Mac adds */
  const ROWS: { label: string; text: string; value: (f: SessionFields, s: SessionView) => string }[] = [
    { label: "Session Name", text: "This is the name of the session.", value: (f) => f.name },
    { label: "Image", text: "This is the image associated to the session.", value: (f) => f.icon },
    { label: "Host Name/Ip", text: "This is the host name or the IP address of the destination.", value: (f) => f.host },
    { label: "Note", text: "This is a note.", value: (f) => f.note },
    { label: "Port", text: "This is the port that will be used to connect to the destination.", value: (f) => (f.port ? String(f.port) : "") },
    { label: "Connection Type", text: "This is the login protocol.", value: (f) => f.protocol || "SSH" },
    { label: "Putty Profile", text: "This is the PuTTY session profile associated to this session.", value: (f) => f.profile },
    { label: "Username", text: "This is the username that will be used to login.", value: (f) => f.username },
    { label: "Extra Arguments", text: "Extra PuTTY arguments. A -pw password shows masked.", value: (f) => f.extraArgs },
    { label: "SPSL File", text: "SPSL Script Filename", value: (f) => f.script },
    { label: "Remote Path", text: "Remote Path used in file transfer", value: (f) => f.remotePath },
    { label: "Local Path", text: "Local path used in file transfer", value: (f) => f.localPath },
    { label: "Folder", text: "The folder the session is in.", value: (f, s) => `${s.store === "saved" ? "Saved" : s.store === "private" ? "Private" : "Database"} sessions${f.folder ? ` / ${f.folder}` : ""}` },
    { label: "Jump Host", text: "The session or user@host:port the connection goes through.", value: (f) => f.jumpHost },
    { label: "Colour", text: "The environment colour, on the tab and the frame around the terminal.", value: (f) => f.color },
    { label: "Highlighting", text: "The output highlighting rules; empty takes the folder's or the settings'.", value: (f) => f.highlightRules },
  ];

  let fields = $state<SessionFields | null>(null);
  let picked = $state(0);

  // the selected session's fields, read again whenever another is selected. The last one's stay until they come:
  // emptying the pane meanwhile ("Reading…") changed its height for a moment, and the tree above it jumped at every
  // click. An answer for a session no longer selected is dropped (stale)
  $effect(() => {
    const s = session;
    if (!s) {
      fields = null;
      return;
    }
    let stale = false;
    getSession(s.store, s.id).then((f) => !stale && (fields = f), () => {});
    return () => (stale = true);
  });
</script>

<div class="pane">
  <div class="title">
    <span>Session Detail</span>
    <button class="close" title="Hide the Session Detail pane (View > Session Detail)" aria-label="Hide the Session Detail pane" onclick={onClose}>×</button>
  </div>
  {#if session && fields}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="grid" ondblclick={() => session && onEdit(session)} title="Double-click to edit the session">
      {#each ROWS as row, i}
        {@const value = row.value(fields, session)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div class="row" class:picked={picked === i} role="row" tabindex="-1" onclick={() => (picked = i)}>
          <span class="name">{row.label}</span>
          <span class="value" class:strong={!!value}>
            {#if row.label === "Image" && value}<Icon key={value} size={14} {customs} />{/if}
            {#if row.label === "Colour" && value}<i class="swatch" style:background={value}></i>{/if}
            {value}
          </span>
        </div>
      {/each}
    </div>
    <div class="help">
      <b>{ROWS[picked].label}</b>
      <span>{ROWS[picked].text}</span>
    </div>
  {:else}
    <p class="empty">{session ? "Reading…" : "Select a session in the tree."}</p>
  {/if}
</div>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    border-top: 1px solid var(--border);
    min-height: 0;
    font-size: 12px;
  }

  .title {
    display: flex;
    align-items: center;
    padding: 4px 8px;
    color: var(--text-muted);
    font-size: 13px;
  }

  .title span {
    flex: 1;
  }

  .close {
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font-size: 15px;
    line-height: 1;
    padding: 0 3px;
  }

  .close:hover {
    color: var(--text);
  }

  .grid {
    /* the tree keeps most of the height; the rows scroll */
    max-height: 26vh;
    overflow: auto;
    margin: 0 6px;
    border: 1px solid var(--border);
    background: var(--bg-input);
  }

  .row {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    cursor: default;
  }

  .row.picked .name {
    background: var(--accent);
    color: #fff;
  }

  .name {
    padding: 2px 6px;
    border-right: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .value {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .value.strong {
    font-weight: 600;
  }

  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    flex: none;
  }

  .help {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 4px 6px 6px;
    padding: 4px 6px;
    min-height: 34px;
    border: 1px solid var(--border);
    border-radius: 3px;
  }

  .empty {
    margin: 4px 10px 10px;
    color: var(--text-muted);
  }
</style>
