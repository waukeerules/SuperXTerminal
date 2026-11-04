<!--
  Edit Session, as SuperXPuTTY's: connection details, notes and file transfer paths, saved back to the session's
  own file (Sessions.xml for Saved sessions, PrivateSessions.xml for Private ones). Also used for a new session.
  "PuTTY session profile" becomes Profile: the name is looked up in the user's own profiles (Settings > Profiles).
  Its tabs: General, Connection, then SSH (its own algorithm lists) for an SSH session or Remote Desktop (its own
  RDP options) for an RDP one, Terminal and File transfer. Local overrides… opens the user's own values.
  Password is the session's own, kept in the Keychain (Credential Manager) on this computer only, never in its file;
  Move -pw here moves a -pw from the extra arguments there (session_passwords.rs).
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
  import AlgorithmsDialog from "./AlgorithmsDialog.svelte";
  import Combobox from "../ui/Combobox.svelte";
  import RdpOptionsForm from "./RdpOptionsForm.svelte";
  import { formatRdpOptions, parseRdpOptions, userDefaults, type RdpOptions } from "./rdpOptions";
  import { settings } from "../settings/store.svelte";
  import { isMac } from "../shortcuts";
  import { SECRET_STORE, THIS_COMPUTER } from "../platform";
  import { lineOf, withLine } from "./serial";
  import { PROTOCOLS, saveSession, unavailableReason, type FolderSetting, type SessionFields, type Store } from "./api";

  interface Props {
    /** Which sessions it's in: Saved, Private or Database */
    store: Store;
    /** The session's id; null for a new session */
    id: string | null;
    /** Its fields as stored, which the window starts from */
    fields: SessionFields;
    /** Its folder's settings, nearest first, for showing what it inherits */
    folders: FolderSetting[];
    /** The size of the icon shown, as the Sessions tree's */
    iconSize: number;
    /** The user's custom icons, for the icon and Choose Icon */
    customs: ReadonlyMap<string, CustomIcon>;
    /** Saved: the session's id, which a rename changes */
    onSaved: (id: string) => void;
    /** Cancel, or Escape: nothing changes */
    onCancel: () => void;
    /** Open Settings > Profiles, on the named profile (the session's) when there is one */
    onOpenProfiles: (name?: string) => void;
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
  /** The tooltip on a field locked in the database */
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
  /** The Remote Desktop tab's options, as its form edits them (draft.rdpOptions is them as text) */
  let rdpOptions = $state<RdpOptions>(untrack(() => parseRdpOptions(fields.rdpOptions ?? "")));
  /** What Default stands for on the Remote Desktop tab: Settings > Remote Desktop's */
  const rdpDefaults = $derived(userDefaults(settings.rdpDefaults));
  /** Why the SSH tab's lists can't be saved (a kind with its own list and none ticked), or "" */
  let algorithmsProblem = $state("");
  /** Counted up by Revert, so the SSH and Remote Desktop tabs start again from the session as opened */
  let revision = $state(0);
  /** Whether the session has a password of its own in the Keychain (none for a new session) */
  let hasOwnPassword = $state(false);
  /** A new password typed in; "" forgets the stored one, undefined keeps it */
  let password = $state<string | undefined>(undefined);
  /** The serial devices plugged in (/dev/cu.*), for the Serial line list */
  let serialDevices = $state<string[]>([]);

  /** A field as it counts for "changed": the port as a number, the rest as text */
  const meaning = (f: SessionFields, field: keyof SessionFields) => (field === "port" ? Number(f[field]) || 0 : String(f[field] ?? ""));
  /** Whether a field differs from the session as it was opened (or as a new one started) */
  const changed = (field: keyof SessionFields) => meaning(draft, field) !== meaning(fields, field);
  /** The fields that count as changes, for the count by Save */
  const FIELDS: (keyof SessionFields)[] = [
    "name", "color", "icon", "protocol", "profile", "host", "port", "username", "highlightRules", "extraArgs", "rdpOptions",
    "sshAlgorithms", "jumpHost", "script", "note", "localPath", "remotePath",
  ];
  /** How many fields have changed; a stored session saves only with changes, a new one (or a copy) any time */
  const changes = $derived(FIELDS.filter(changed).length + (password !== undefined ? 1 : 0));
  /** The window's tabs, and the fields on each: a tab with one changed shows a dot */
  const ALL_TABS: { name: string; fields: (keyof SessionFields)[]; for?: (protocol: string) => boolean }[] = [
    { name: "General", fields: ["name", "color", "icon", "protocol", "profile", "host", "port", "username", "note"] },
    { name: "Connection", fields: ["extraArgs", "jumpHost"] },
    { name: "SSH", fields: ["sshAlgorithms"], for: (p) => p === "SSH" || p === "" },
    { name: "Remote Desktop", fields: ["rdpOptions"], for: (p) => p === "RDP" },
    { name: "Terminal", fields: ["highlightRules", "script"] },
    { name: "File transfer", fields: ["localPath", "remotePath"] },
  ];
  /** The tabs for the connection type: SSH's own algorithms, or RDP's own options, only for those */
  const TABS = $derived(ALL_TABS.filter((t) => !t.for || t.for(draft.protocol.toUpperCase())));
  /** The tab shown */
  let tab = $state("General");
  /** Whether a tab has a field changed since the window opened (General's also when a password was typed or forgotten) */
  const tabChanged = (t: (typeof ALL_TABS)[number]) => t.fields.some(changed) || (t.name === "General" && password !== undefined);
  /** A -pw password stored in the extra arguments (shown masked), which Move -pw here can take into the Keychain */
  const storedPw = $derived(id !== null && /(^|\s)-pw\s+(••••••|"••••••")(\s|$)/.test(fields.extraArgs) && /(^|\s)-pw\s/.test(draft.extraArgs));

  /** Whether it's a serial session, which shows the line's settings in place of host and port */
  const serial = $derived(draft.protocol.toUpperCase() === "SERIAL");
  /** The serial line's settings, as the extra arguments' -sercfg has them */
  const line = $derived(lineOf(draft.extraArgs));

  /** Where the session is kept, in the title */
  const where = $derived(
    store === "saved" ? "Saved sessions (Sessions.xml)" : store === "private" ? "Private sessions (PrivateSessions.xml)" : "Database sessions",
  );
  /** A Database session with a -pw password typed in, which everyone reading the table can see */
  const sharedPassword = $derived(store === "database" && /(^|\s)-pw\s/.test(draft.extraArgs) && !/-pw\s+\{\w+\}/.test(draft.extraArgs));
  /** The window's title: New session or Edit session, and where it's kept */
  const title = $derived(id === null ? `New session — ${where}` : `Edit session: ${fields.name} — ${where}`);
  /** Why this computer can't open the protocol (e.g. no RDP client), or "" */
  const reason = $derived(unavailableReason(draft.protocol));
  /** A profile named that the user doesn't have, which opens with Default Settings */
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
    if (id !== null) hasOwnPassword = await invoke<boolean>("session_password_has", { id }).catch(() => false);
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

  /** Fill the serial port list with the ports this computer has now */
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
    if (algorithmsProblem && TABS.some((t) => t.name === "SSH")) {
      error = algorithmsProblem;
      tab = "SSH";
      return;
    }
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
      // the session's own password goes in the Keychain under its id, which a rename has already taken along
      if (password !== undefined) await invoke("session_password_set", { id: saved, password });
      onSaved(saved);
    } catch (e) {
      error = String(e);
    }
  }

  /**
   * Move -pw here: the stored -pw becomes the session's own password in the Keychain, and leaves the extra arguments
   * when the session is saved. Asked first, as SuperXPuTTY and other computers reading the session then lose it.
   */
  async function movePassword(): Promise<void> {
    if (id === null) return;
    const sure = await ask(
      `Move the -pw password into ${SECRET_STORE}?\n\nIt's then kept on ${THIS_COMPUTER} only, and taken out of the session's extra arguments when you save: SuperXPuTTY and other computers reading this session won't have it.`,
      { title: "Move -pw here", kind: "warning", okLabel: "Move it", cancelLabel: "Cancel" },
    );
    if (!sure) return;
    try {
      await invoke("session_password_from_pw", { store, id });
      hasOwnPassword = true;
      password = undefined;
      if (!locked("extra_args")) draft.extraArgs = withoutPw(draft.extraArgs);
    } catch (e) {
      error = String(e);
    }
  }

  /** Extra arguments without their -pw and its value, the rest as it was */
  function withoutPw(extra: string): string {
    return extra.replace(/(^|\s)-pw\s+("[^"]*"|\S+)/, "$1").replace(/\s{2,}/g, " ").trim();
  }

  /**
   * Revert: every field back as the window opened with it, a typed or forgotten password dropped (as Settings >
   * Profiles' Revert). A -pw already moved into the Keychain stays there, and comes back in the extra arguments too.
   */
  function revert(): void {
    draft = { ...fields };
    password = undefined;
    error = "";
    rdpOptions = parseRdpOptions(fields.rdpOptions ?? "");
    algorithmsProblem = "";
    revision++;
  }

  /**
   * The Remote Desktop tab's form changed: its options as RdpOptions text, or the text the session came with when
   * they're what it had (written another way, they'd count as a change)
   */
  function rdpChanged(): void {
    const now = formatRdpOptions(rdpOptions);
    draft.rdpOptions = now === formatRdpOptions(parseRdpOptions(fields.rdpOptions ?? "")) ? fields.rdpOptions : now;
  }

  // a connection type without the tab shown (SSH to Telnet, say) goes back to General
  $effect(() => {
    if (!TABS.some((t) => t.name === tab)) tab = "General";
  });

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

<svelte:window onkeydown={(e) => e.key === "Escape" && !choosingIcon && !overriding && !settingsShown() && onCancel()} />

<div class="backdrop" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-label={title}>
    <header>{title}</header>

    <div class="tabs" role="tablist">
      {#each TABS as t (t.name)}
        <button role="tab" class:active={tab === t.name} aria-selected={tab === t.name} onclick={() => (tab = t.name)}>
          {t.name}{@render dot(tabChanged(t))}</button>
      {/each}
    </div>

    <!-- every tab's fields are there, the others hidden, so a tab keeps what's typed in it; the window keeps one size -->
    <div class="body">
    <div class="panel" role="tabpanel" aria-label="General" hidden={tab !== "General"}>
      <div class="row">
        <label class="grow">{@render caption("Session name:", "name")}<input bind:value={draft.name} disabled={locked("name")} title={locked("name") ? lockTip : ""} /></label>
        <span class="colour">{@render caption("Colour:", "color")}
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
        <span>{@render caption("Icon:", "icon")}
          <button class="icon" title={locked("icon") ? lockTip : "Choose Icon"} aria-label="Choose Icon" disabled={locked("icon")}
            onclick={() => (choosingIcon = true)}>
            <Icon key={draft.icon} size={iconSize} {customs} />
          </button>
        </span>
      </div>

      <div class="row">
        <label>{@render caption("Connection type:", "protocol")}
          <select bind:value={draft.protocol} disabled={locked("protocol")} title={locked("protocol") ? lockTip : ""}
            onchange={() => protocolChanged()}>
            {#each PROTOCOLS as p}<option value={p}>{p}</option>{/each}
          </select>
        </label>
        <label class="grow">{@render caption("Profile:", "profile")}
          <Combobox bind:value={draft.profile} options={profileNames} placeholder={inherited("puttyProfile") || DEFAULT_PROFILE}
            disabled={locked("putty_profile")} title={locked("putty_profile") ? lockTip : ""} />
        </label>
        <button onclick={() => onOpenProfiles(draft.profile.trim() || undefined)} title="Settings > Profiles, on this session's profile">Profiles…</button>
      </div>
      {#if reason}<p class="note">{reason}</p>{/if}
      {#if profileMissing}<p class="note">Profile "{draft.profile.trim()}" isn't set up on {isMac ? "this Mac" : "this PC"}, so Default Settings is used. Add it in Profiles….</p>{/if}

      {#if serial}
        <div class="row">
          <label class="grow">{@render caption("Serial line:", "host")}
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
          <label class="grow">{@render caption("Host address:", "host")}<input bind:value={draft.host} disabled={locked("host")} title={locked("host") ? lockTip : ""}
            placeholder={draft.protocol.toUpperCase() === "SSH" ? "none: the profile's Host" : ""} /></label>
          <label>{@render caption("TCP port:", "port")}<input class="port" type="number" min="0" max="65535" bind:value={draft.port}
            placeholder={inherited("port")} disabled={locked("port")} title={locked("port") ? lockTip : ""} /></label>
        </div>
      {/if}

      <div class="row">
        <label class="grow">{@render caption("Login username:", "username")}<input bind:value={draft.username} placeholder={inherited("username") || "e.g. admin, or {user}"}
          disabled={locked("username")} title={locked("username") ? lockTip : ""} /></label>
      </div>
      <div class="row">
        <label class="grow">{@render ownCaption()}<input type="password" value={password ?? ""} oninput={(e) => (password = e.currentTarget.value)}
          placeholder={hasOwnPassword && password !== "" ? `Stored in ${SECRET_STORE}` : /(^|\s)-pw\s/.test(draft.extraArgs) ? "none: the -pw on the Connection tab" : "none: the profile's"}
          title={`Kept in ${SECRET_STORE} on ${THIS_COMPUTER} only, never in the session's file; used before its -pw and its profile's`} /></label>
        {#if hasOwnPassword && password !== ""}<button onclick={() => (password = "")} title="Forget the stored password">Forget</button>{/if}
        {#if storedPw}<button onclick={movePassword} title={`Keep the -pw password in ${SECRET_STORE} instead, on ${THIS_COMPUTER} only`}>Move -pw here</button>{/if}
      </div>

      <label class="notes">{@render caption("Notes:", "note")}
        <textarea bind:value={draft.note} rows="4" disabled={locked("notes")} title={locked("notes") ? lockTip : ""}></textarea>
      </label>
    </div>

    <div class="panel" role="tabpanel" aria-label="Connection" hidden={tab !== "Connection"}>
      <div class="row">
        <label class="grow">{@render caption("Extra arguments:", "extraArgs")}<input bind:value={draft.extraArgs} placeholder="PuTTY-style, e.g. -pw secret -L 8080:localhost:80"
          disabled={locked("extra_args")} title={locked("extra_args") ? lockTip : ""} /></label>
      </div>
      {#if sharedPassword}<p class="note warn">The table is shared: don't put a password here. Use a {"{key}"} placeholder, e.g. -pw {"{admin_password}"}, which each user sets in Tools &gt; Environment Variables.</p>{/if}
      <div class="row">
        <label class="grow">{@render caption("Jump host:", "jumpHost")}<input bind:value={draft.jumpHost} disabled={locked("jump_host")} title={locked("jump_host") ? lockTip : ""}
          placeholder={inherited("jumpHost") || "none; or a session, e.g. Bastions/bastion1, or admin@10.0.0.1:22"} /></label>
      </div>

    </div>

    {#if TABS.some((t) => t.name === "SSH")}
      <div class="panel" role="tabpanel" aria-label="SSH" hidden={tab !== "SSH"}>
        <!-- disabled as a whole when the Database table locks the column -->
        <fieldset class="plain" disabled={locked("ssh_algorithms")} title={locked("ssh_algorithms") ? lockTip : ""}>
          {#key revision}
            <AlgorithmsDialog inline title="" text={draft.sshAlgorithms ?? ""} otherwise={`its profile's list (${draft.profile || DEFAULT_PROFILE})`}
              onChange={(text, problem) => { draft.sshAlgorithms = text; algorithmsProblem = problem; }} />
          {/key}
        </fieldset>
      </div>
    {/if}

    {#if TABS.some((t) => t.name === "Remote Desktop")}
      <div class="panel" role="tabpanel" aria-label="Remote Desktop" hidden={tab !== "Remote Desktop"}>
        <p class="note">This session's own options. Left at Default, an option follows Settings &gt; Remote Desktop.</p>
        <fieldset class="plain" disabled={locked("rdp_options")} title={locked("rdp_options") ? lockTip : ""}>
          {#key revision}
            <RdpOptionsForm bind:value={rdpOptions} defaults={rdpDefaults} onChange={rdpChanged} />
          {/key}
        </fieldset>
        <div class="row">
          <button disabled={locked("rdp_options")} onclick={() => { rdpOptions = {}; rdpChanged(); revision++; }}
            title="Every option back to Default">Use Defaults</button>
        </div>
      </div>
    {/if}

    <div class="panel" role="tabpanel" aria-label="Terminal" hidden={tab !== "Terminal"}>
      <div class="row">
        <label>{@render caption("Highlight:", "highlightRules")}
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
        <label class="grow">{@render caption("SPSL script:", "script")}<input bind:value={draft.script} disabled={locked("spsl_script")} title={locked("spsl_script") ? lockTip : ""} /></label>
        <button onclick={() => (draft.script = "")}>Clear</button>
        <button onclick={browseScript}>Browse…</button>
      </div>

    </div>

    <div class="panel" role="tabpanel" aria-label="File transfer" hidden={tab !== "File transfer"}>
      <div class="row">
        <label class="grow">{@render caption("Local path:", "localPath")}<input bind:value={draft.localPath} /></label>
        <button onclick={browseLocal}>Browse…</button>
      </div>
      <div class="row">
        <label class="grow">{@render caption("Remote path:", "remotePath")}<input bind:value={draft.remotePath} /></label>
      </div>
    </div>
    </div>

    <footer>
      <button disabled={id === null} title={id === null ? "Save the session first" : "Your own values for this session, used when it opens; only you get them"}
        onclick={() => (overriding = true)}>Local overrides…</button>
      <span class="error" class:pending={!error && changes > 0}>{error || (changes > 0 ? `Unsaved changes (${changes})` : "")}</span>
      <button onclick={revert} disabled={changes === 0} title={changes === 0 ? "Nothing has changed" : "Put every field back as it was when the window opened"}>Revert</button>
      <button class="primary" onclick={save} disabled={id !== null && changes === 0}
        title={id !== null && changes === 0 ? "Nothing has changed" : ""}>Save</button>
      <button onclick={onCancel}>Cancel</button>
    </footer>
  </div>
</div>

<!-- the dot of a field changed since the window opened: always there, so captions keep their width, and seen only
     once it's changed -->
{#snippet dot(on: boolean)}
  <span class="dot" class:off={!on} title={on ? "Changed: not saved yet" : undefined} aria-label={on ? "changed" : undefined} aria-hidden={!on}></span>
{/snippet}

<!-- a field's caption, with a dot while it differs from the session as opened -->
{#snippet ownCaption()}
  <span class="caption">Password:{@render dot(password !== undefined)}</span>
{/snippet}

{#snippet caption(text: string, field: keyof SessionFields)}
  <span class="caption">{text}{@render dot(changed(field))}</span>
{/snippet}


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
    overflow: hidden;
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

  .body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* as the other dialogs' tabs (SSH algorithms…): the shown one joins the panel below */
  .tabs {
    display: flex;
    gap: 4px;
    border-bottom: 1px solid var(--border);
  }

  .tabs button {
    background: none;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    padding: 4px 12px;
    margin-bottom: -1px;
  }

  .tabs button.active {
    background: var(--bg-window);
    border-color: var(--border);
  }

  /* every tab the same height, so switching doesn't move Save; the fields scroll only on a small screen */
  .body {
    min-height: 340px;
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .panel[hidden] {
    display: none;
  }

  /* only to disable a tab's fields together (a column locked in the Database table): no box of its own */
  fieldset.plain {
    border: 0;
    margin: 0;
    padding: 0;
    min-width: 0;
  }

  .caption {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }

  /* a field changed since the window opened */
  .dot {
    display: inline-block;
    width: 7px;
    height: 7px;
    margin-left: 5px;
    border-radius: 50%;
    background: var(--accent);
    vertical-align: middle;
  }

  .caption .dot {
    margin-left: 0;
  }

  .dot.off {
    visibility: hidden;
  }

  /* an invisible column: the first caption of every row as wide as the widest, so the fields start in one line */
  .panel > .row > :first-child > .caption {
    min-width: 9.5em;
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

  .error.pending {
    color: var(--accent);
  }

  .error {
    flex: 1;
    color: #e5534b;
  }

  button:disabled {
    opacity: 0.45;
  }
</style>
