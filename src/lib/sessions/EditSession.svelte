<!--
  Edit Session, as SuperXPuTTY's: connection details, notes and file transfer paths, saved back to the session's
  own file (Sessions.xml for Saved sessions, PrivateSessions.xml for Private ones). Also used for a new session.
  "PuTTY session profile" becomes Profile: the name is looked up in the user's own profiles (Settings > Profiles).
  RDP options… opens the session's own Remote Desktop options; Local overrides… opens the user's own values.
-->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { ask, open } from "@tauri-apps/plugin-dialog";
  import Icon from "../icons/Icon.svelte";
  import IconPicker from "../icons/IconPicker.svelte";
  import type { CustomIcon } from "../icons/custom";
  import { PALETTE } from "../icons/palette";
  import { highlighting } from "../terminal/highlighting";
  import { listProfiles, DEFAULT_PROFILE, PROFILES_CHANGED } from "../settings/profiles";
  import { settingsShown } from "../settings/Settings.svelte";
  import SerialLineFields from "./SerialLineFields.svelte";
  import Overrides from "./Overrides.svelte";
  import RdpSessionOptions from "./RdpSessionOptions.svelte";
  import AlgorithmsDialog from "./AlgorithmsDialog.svelte";
  import { isMac } from "../shortcuts";
  import { lineOf, withLine } from "./serial";
  import { PROTOCOLS, saveSession, unavailableReason, type FolderSetting, type SessionFields, type Store } from "./api";

  interface Props {
    store: Store;
    /** The session's id; null for a new session */
    id: string | null;
    fields: SessionFields;
    /** Its folder's settings, nearest first, for showing what it inherits */
    folders: FolderSetting[];
    iconSize: number;
    customs: ReadonlyMap<string, CustomIcon>;
    onSaved: (id: string) => void;
    onCancel: () => void;
    onOpenProfiles: () => void;
    /** Database sessions: the columns locked in the table ("host,port"), shown greyed */
    lockedFields?: string;
    /** A free name in a folder: the name, else with -1, -2… added (`except` the session itself), as SuperXPuTTY's */
    freeName?: (store: Store, folder: string, name: string, except: string) => string;
    /** The session a new one was filled in from (Duplicate as…), whose -pw password it keeps */
    copyOf?: [Store, string] | null;
  }

  let { store, id, fields, folders, iconSize, customs, onSaved, onCancel, onOpenProfiles, lockedFields = "", freeName, copyOf = null }: Props = $props();

  /** Whether a column is locked in the table (Database sessions only) */
  const locked = (column: string) => lockedFields.split(",").map((f) => f.trim()).includes(column);
  const lockTip = "Locked in the database";

  // the window edits a copy, saved only on Save
  let draft = $state<SessionFields>(untrack(() => ({ ...fields })));
  let profileNames = $state<string[]>([]);
  let ruleSets = $state<{ id: string; name: string }[]>([]);
  let choosingIcon = $state(false);
  let choosingColour = $state(false);
  let error = $state("");
  /** Local overrides… open over this window */
  let overriding = $state(false);
  /** RDP options… open over this window */
  let rdpOptioning = $state(false);
  /** SSH algorithms… open over this window */
  let algorithming = $state(false);
  /** The serial devices plugged in (/dev/cu.*), for the Serial line list */
  let serialDevices = $state<string[]>([]);

  const serial = $derived(draft.protocol.toUpperCase() === "SERIAL");
  const line = $derived(lineOf(draft.extraArgs));

  const where = $derived(
    store === "saved" ? "Saved sessions (Sessions.xml)" : store === "private" ? "Private sessions (PrivateSessions.xml)" : "Database sessions",
  );
  const sharedPassword = $derived(store === "database" && /(^|\s)-pw\s/.test(draft.extraArgs) && !/-pw\s+\{\w+\}/.test(draft.extraArgs));
  const title = $derived(id === null ? `New session — ${where}` : `Edit session: ${fields.name} — ${where}`);
  const reason = $derived(unavailableReason(draft.protocol));
  const profileMissing = $derived(
    !!draft.profile.trim() && draft.profile.trim().toLowerCase() !== DEFAULT_PROFILE.toLowerCase() &&
      !profileNames.some((n) => n.toLowerCase() === draft.profile.trim().toLowerCase()),
  );

  /** The profiles' names, again whenever Settings > Profiles changes them */
  async function loadProfileNames(): Promise<void> {
    profileNames = (await listProfiles()).map((p) => p.name);
  }

  onMount(() => {
    window.addEventListener(PROFILES_CHANGED, loadProfileNames);
    return () => window.removeEventListener(PROFILES_CHANGED, loadProfileNames);
  });

  onMount(async () => {
    await loadProfileNames();
    ruleSets = (await highlighting()).ruleSets.map((r) => ({ id: String(r.id ?? ""), name: String(r.name ?? r.id ?? "") }));
  });

  /** What a field inherits from the session's folders, as "value (from Folder)", for its placeholder */
  function inherited(field: "username" | "puttyProfile" | "port" | "jumpHost" | "highlightRules"): string {
    for (const folder of folders) {
      const value = folder[field];
      if (value) return `${value} (from ${folder.path})`;
    }
    return "";
  }

  async function listSerialDevices(): Promise<void> {
    serialDevices = await invoke<string[]>("serial_devices_list");
  }

  $effect(() => {
    if (serial) void untrack(listSerialDevices);
  });

  /** The usual port of a connection type, as SuperXPuTTY's GetDefaultPort; null leaves the port as it is (Raw) */
  function defaultPort(protocol: string): number | null {
    const ports: Record<string, number | null> = { SSH: 22, TELNET: 23, RLOGIN: 513, VNC: 5900, RDP: 3389, RAW: null };
    const p = protocol.toUpperCase();
    return p in ports ? ports[p] : 0;
  }

  /** The connection type was chosen: its usual port, as SuperXPuTTY's Edit Session does */
  function protocolChanged(): void {
    const port = defaultPort(draft.protocol);
    if (port !== null && !locked("port")) draft.port = port;
  }

  /** Save and close, or show why it can't be saved */
  async function save(): Promise<void> {
    // a name already taken there is offered with -1, -2… added, as SuperXPuTTY's Edit Session
    const name = draft.name.trim();
    const free = name && !name.includes("/") && freeName ? freeName(store, draft.folder, name, id ?? "") : name;
    if (free !== name) {
      const sure = await ask(`A session named "${name}" already exists here.\n\nSave this one as "${free}"?`,
        { title: "Session name", kind: "info", okLabel: "Save", cancelLabel: "Cancel" });
      if (!sure) return;
      draft.name = free;
    }
    try {
      const saved = await saveSession(store, id, { ...draft, port: Number(draft.port) || 0 }, id === null ? copyOf : null);
      onSaved(saved);
    } catch (e) {
      error = String(e);
    }
  }

  /** Choose an SPSL script file */
  async function browseScript(): Promise<void> {
    const path = await open({ title: "Choose the session's SPSL script", filters: [{ name: "Scripts", extensions: ["spsl", "txt"] }] });
    if (typeof path === "string") draft.script = path;
  }

  /** Choose the local folder for file transfers */
  async function browseLocal(): Promise<void> {
    const path = await open({ directory: true, title: "Choose the local folder for file transfers" });
    if (typeof path === "string") draft.localPath = path;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !choosingIcon && !overriding && !rdpOptioning && !settingsShown() && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
    <header>{title}</header>

    <fieldset>
      <legend>Connection details</legend>
      <div class="row">
        <label class="grow">Session name: <input bind:value={draft.name} disabled={locked("name")} title={locked("name") ? lockTip : ""} /></label>
        <span class="colour">Colour:
          <button class="swatch" style:background={draft.color || "transparent"} title={locked("color") ? lockTip : draft.color || "None"}
            aria-label="Colour" disabled={locked("color")} onclick={() => (choosingColour = !choosingColour)}></button>
          {#if choosingColour}
            <div class="popover">
              <button onclick={() => { draft.color = ""; choosingColour = false; }}>None</button>
              {#each PALETTE as p}
                <button class="swatch" style:background={p.color} title={p.name} aria-label={p.name}
                  onclick={() => { draft.color = p.color; choosingColour = false; }}></button>
              {/each}
              <label class="more">More…
                <input type="color" value={draft.color || "#C62828"} onchange={(e) => { draft.color = e.currentTarget.value.toUpperCase(); choosingColour = false; }} />
              </label>
            </div>
          {/if}
        </span>
        <span>Icon:
          <button class="icon" title={locked("icon") ? lockTip : "Choose Icon"} aria-label="Choose Icon" disabled={locked("icon")}
            onclick={() => (choosingIcon = true)}>
            <Icon key={draft.icon} size={iconSize} {customs} />
          </button>
        </span>
      </div>

      <div class="row">
        <label>Connection type:
          <select bind:value={draft.protocol} disabled={locked("protocol")} title={locked("protocol") ? lockTip : ""}
            onchange={() => protocolChanged()}>
            {#each PROTOCOLS as p}<option value={p}>{p}</option>{/each}
          </select>
        </label>
        <label class="grow">Profile:
          <input list="profile-names" bind:value={draft.profile} placeholder={inherited("puttyProfile") || DEFAULT_PROFILE}
            disabled={locked("putty_profile")} title={locked("putty_profile") ? lockTip : ""} />
          <datalist id="profile-names">{#each profileNames as n}<option value={n}></option>{/each}</datalist>
        </label>
        <button onclick={onOpenProfiles} title="Settings > Profiles">Profiles…</button>
      </div>
      {#if reason}<p class="note">{reason}</p>{/if}
      {#if sharedPassword}<p class="note warn">The table is shared: don't put a password here. Use a {"{key}"} placeholder, e.g. -pw {"{admin_password}"}, which each user sets in Tools &gt; Environment Variables.</p>{/if}
      {#if profileMissing}<p class="note">Profile "{draft.profile.trim()}" isn't set up on {isMac ? "this Mac" : "this PC"}, so Default Settings is used. Add it in Profiles….</p>{/if}

      {#if serial}
        <div class="row">
          <label class="grow">Serial line:
            <input list="serial-devices" bind:value={draft.host} disabled={locked("host")} title={locked("host") ? lockTip : ""}
              placeholder={serialDevices[0] ?? (isMac ? "e.g. /dev/cu.usbserial-1, or COM1" : "e.g. COM1")} />
            <datalist id="serial-devices">{#each serialDevices as d}<option value={d}></option>{/each}</datalist>
          </label>
          <button onclick={listSerialDevices} title="List the serial devices plugged in now">Refresh</button>
        </div>
        <p class="note">{#if isMac}Pick a device from the list, or type a COM name (e.g. COM1) to share the session with Windows: the Mac asks once which device it is.
          {:else}Pick a COM port from the list (the ports present now), or type one.{/if}
          {#if serialDevices.length === 0}No serial device is plugged in now.{/if}</p>
        <div class="row">
          <SerialLineFields {line} disabled={locked("extra_args")} onChange={(l) => (draft.extraArgs = withLine(draft.extraArgs, l))} />
        </div>
      {:else}
        <div class="row">
          <label class="grow">Host address: <input bind:value={draft.host} disabled={locked("host")} title={locked("host") ? lockTip : ""} /></label>
          <label>TCP port: <input class="port" type="number" min="0" max="65535" bind:value={draft.port}
            placeholder={inherited("port")} disabled={locked("port")} title={locked("port") ? lockTip : ""} /></label>
        </div>
      {/if}

      <div class="row">
        <label class="grow">Login username: <input bind:value={draft.username} placeholder={inherited("username") || "e.g. admin, or {user}"}
          disabled={locked("username")} title={locked("username") ? lockTip : ""} /></label>
        <label>Highlight:
          <select bind:value={draft.highlightRules} disabled={locked("highlight_rules")} title={locked("highlight_rules") ? lockTip : ""}>
            <option value="">{inherited("highlightRules") ? `From the folder (${inherited("highlightRules")})` : "Default (Settings)"}</option>
            <option value="auto">Auto (picked from the prompt)</option>
            {#each ruleSets as r}<option value={r.id}>{r.name}</option>{/each}
            <option value="none">None</option>
            {#if draft.highlightRules && !["auto", "none"].includes(draft.highlightRules) && !ruleSets.some((r) => r.id === draft.highlightRules)}
              <option value={draft.highlightRules}>{draft.highlightRules} (not found)</option>
            {/if}
          </select>
        </label>
      </div>

      <div class="row">
        <label class="grow">Extra arguments: <input bind:value={draft.extraArgs} placeholder="PuTTY-style, e.g. -pw secret -L 8080:localhost:80"
          disabled={locked("extra_args")} title={locked("extra_args") ? lockTip : ""} /></label>
        <button disabled={draft.protocol.toUpperCase() !== "RDP" || locked("rdp_options")}
          title={locked("rdp_options") ? lockTip : draft.protocol.toUpperCase() !== "RDP" ? "For RDP sessions" : draft.rdpOptions ? `Its own: ${draft.rdpOptions}` : "Using the defaults (Settings > Remote Desktop)"}
          onclick={() => (rdpOptioning = true)}>RDP options…{draft.rdpOptions ? " ●" : ""}</button>
        <button disabled={!["SSH", ""].includes(draft.protocol.toUpperCase()) || locked("ssh_algorithms")}
          title={locked("ssh_algorithms") ? lockTip : !["SSH", ""].includes(draft.protocol.toUpperCase()) ? "For SSH sessions" : draft.sshAlgorithms ? `Its own: ${draft.sshAlgorithms}` : "Its profile's algorithms"}
          onclick={() => (algorithming = true)}>SSH algorithms…{draft.sshAlgorithms ? " ●" : ""}</button>
      </div>

      <div class="row">
        <label class="grow">Jump host: <input bind:value={draft.jumpHost} disabled={locked("jump_host")} title={locked("jump_host") ? lockTip : ""}
          placeholder={inherited("jumpHost") || "none; or a session, e.g. Bastions/bastion1, or admin@10.0.0.1:22"} /></label>
      </div>

      <div class="row">
        <label class="grow">SPSL script: <input bind:value={draft.script} disabled={locked("spsl_script")} title={locked("spsl_script") ? lockTip : ""} /></label>
        <button onclick={() => (draft.script = "")}>Clear</button>
        <button onclick={browseScript}>Browse…</button>
      </div>

      <label class="notes">Notes:
        <textarea bind:value={draft.note} rows="5" disabled={locked("notes")} title={locked("notes") ? lockTip : ""}></textarea>
      </label>
    </fieldset>

    <fieldset>
      <legend>File transfer options</legend>
      <div class="row">
        <label class="grow">Local path: <input bind:value={draft.localPath} /></label>
        <button onclick={browseLocal}>Browse…</button>
      </div>
      <div class="row">
        <label class="grow">Remote path: <input bind:value={draft.remotePath} /></label>
      </div>
    </fieldset>

    <footer>
      <button disabled={id === null} title={id === null ? "Save the session first" : "Your own values for this session, used when it opens; only you get them"}
        onclick={() => (overriding = true)}>Local overrides…</button>
      <span class="error">{error}</span>
      <button class="primary" onclick={save}>Save</button>
      <button onclick={onCancel}>Cancel</button>
    </footer>
  </div>
</div>

{#if rdpOptioning}
  <RdpSessionOptions name={draft.name} text={draft.rdpOptions ?? ""} onCancel={() => (rdpOptioning = false)}
    onDone={(text) => { draft.rdpOptions = text; rdpOptioning = false; }} />
{/if}
{#if algorithming}
  <AlgorithmsDialog title={`SSH algorithms: ${draft.name || "new session"}`} text={draft.sshAlgorithms ?? ""}
    otherwise={`its profile's list (${draft.profile || "Default Settings"})`} onCancel={() => (algorithming = false)}
    onDone={(text) => { draft.sshAlgorithms = text; algorithming = false; }} />
{/if}

{#if overriding && id !== null}
  <Overrides sessions={[{ id, store, name: fields.name }]} onCancel={() => (overriding = false)} onSaved={() => (overriding = false)} />
{/if}

{#if choosingIcon}
  <IconPicker value={draft.icon} {customs} onCancel={() => (choosingIcon = false)}
    onChoose={(key) => { draft.icon = key; choosingIcon = false; }} />
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 120;
    background: rgb(0 0 0 / 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .dialog {
    width: min(820px, 94vw);
    max-height: 94vh;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px;
    background: var(--bg-window);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.4);
  }

  header {
    color: var(--text-muted);
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
    align-items: center;
    gap: 12px;
  }

  label {
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }

  .grow {
    flex: 1;
  }

  .grow input {
    flex: 1;
  }

  input,
  select,
  textarea,
  button {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 4px 6px;
    min-width: 0;
  }

  .port {
    width: 80px;
  }

  .notes {
    flex-direction: column;
    align-items: stretch;
  }

  textarea {
    font: inherit;
    resize: vertical;
  }

  .colour {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .swatch {
    width: 30px;
    height: 30px;
    padding: 0;
  }

  .icon {
    padding: 2px;
    line-height: 0;
  }

  .popover {
    position: absolute;
    top: 100%;
    right: 0;
    z-index: 5;
    display: flex;
    gap: 4px;
    padding: 6px;
    background: var(--bg-pane);
    border: 1px solid var(--border);
    border-radius: 6px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.35);
  }

  .popover .swatch {
    width: 24px;
    height: 24px;
  }

  .more {
    position: relative;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 6px;
    overflow: hidden;
  }

  .more input {
    position: absolute;
    inset: 0;
    opacity: 0;
  }

  .note {
    margin: 0;
    color: var(--text-muted);
  }

  .note.warn {
    color: #e5534b;
  }

  :disabled {
    opacity: 0.55;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .error {
    flex: 1;
    color: #e5534b;
  }

  button:disabled {
    opacity: 0.45;
  }
</style>
