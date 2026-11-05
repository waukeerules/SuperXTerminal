<!--
  Settings > Profiles: the user's own profiles, matched by name to the PuTTY profile a session names ("root",
  "Default Settings"), as their PuTTY profiles are on Windows. The shared session says what to connect to; the
  profile says how this user logs in. Empty fields fall back to Default Settings.
  A profile's right-click menu exports it to a .json file (no password) and imports profiles from one, as a
  colleague's or another computer's.
-->
<script lang="ts">
  import { SECRET_STORE, SSH_KEYS } from "../platform";
  import { onMount } from "svelte";
  import { ask, open, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { isMac } from "../shortcuts";
  import { context } from "../commands/sending.svelte";
  import { keyFetchExamples } from "./keyFetchExamples";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { coloursChoice, DEFAULT_PROFILE, deleteProfile, emptyProfile, listProfiles, ownColoursChoice, saveProfile, type Profile } from "./profiles";
  import AlgorithmsDialog from "../sessions/AlgorithmsDialog.svelte";
  import { drawnTheme, pref } from "../terminal/prefs";
  import { highlighting } from "../terminal/highlighting";
  import { loadBars } from "../commands/bars";
  import { settings } from "./store.svelte";

  interface Props {
    /** The profile to show first (Quick connect's Edit…, Edit Session's Profiles…); Default Settings when it's none */
    initial?: string | null;
    /** Close Settings, for what goes on in a tab (Log in & cache key) */
    onLeave?: () => void;
  }

  let { initial = null, onLeave = () => {} }: Props = $props();

  let profiles = $state<Profile[]>([]);
  let selected = $state(DEFAULT_PROFILE);
  /** The profile being edited, a copy until Save */
  let draft = $state<Profile>(emptyProfile(DEFAULT_PROFILE));
  /** The draft's name when it was loaded; "" for a new profile */
  let originalName = $state(DEFAULT_PROFILE);
  /** The profile as last loaded or saved (or as a new one starts), to tell which fields have changed */
  let saved = $state<Profile>(emptyProfile(DEFAULT_PROFILE));
  /** A new password typed in; undefined keeps the stored one */
  let password = $state<string | undefined>(undefined);
  let message = $state("");
  /** SSH algorithms… open over the tab */
  let algorithming = $state(false);
  /** The list's right-click menu, where it was opened */
  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

  /** Whether the profile shown is Default Settings, which can't be renamed or deleted */
  const isDefault = $derived(originalName.toLowerCase() === DEFAULT_PROFILE.toLowerCase());
  /** Default Settings, whose values a profile's empty fields show */
  const defaultProfile = $derived(profiles.find((p) => p.name.toLowerCase() === DEFAULT_PROFILE.toLowerCase()));

  /** The Colours box's choices (Settings > Terminal's theme is Follow the theme) */
  const CHOICES: Record<string, string> = { theme: "Follow the theme (Settings > Terminal)", dark: "Dark", light: "Light", custom: "Custom" };
  /** The same, short, for what Default Settings has */
  const SHORT: Record<string, string> = { theme: "the theme", dark: "Dark", light: "Light", custom: "Custom" };

  /** The terminal colours a profile can have, over Default Settings', Settings > Terminal's and the theme's */
  const COLOURS: { field: "background" | "foreground" | "selection" | "cursorColor"; theme: string; label: string; pref: string; putty: string }[] = [
    { field: "background", theme: "background", label: "Background", pref: "colorBackground", putty: "#000000" },
    { field: "foreground", theme: "foreground", label: "Text", pref: "colorForeground", putty: "#bbbbbb" },
    { field: "selection", theme: "selection", label: "Selection", pref: "colorSelection", putty: "#264f78" },
    // a session's or folder's environment colour still wins for its cursor
    { field: "cursorColor", theme: "cursor", label: "Cursor colour", pref: "colorCursor", putty: "#00ff00" },
  ];

  /** Where the Tab colour picker starts when the profile has none: a mid grey, which reads on a light or dark tab bar */
  const NO_TAB_COLOUR = "#808080";

  /** The Colours box's value: the profile's own choice, else "default" (Default Settings', which itself has "theme") */
  function shownChoice(profile: Profile): string {
    return ownColoursChoice(profile) || (isDefault ? "theme" : "default");
  }

  /** The choice the profile's tabs go by, the inherited one included: the colours can be set only when it's Custom */
  const choice = $derived(coloursChoice(draft, isDefault ? undefined : defaultProfile));

  /** Each theme's colours, by name, for what a tab without its own colours draws with */
  let themeColours = $state<Record<string, Record<string, string>>>({});
  onMount(() => {
    void highlighting().then((files) => {
      themeColours = Object.fromEntries(files.themes.map((t) => {
        const theme = t as { name?: string; colors?: Record<string, string> };
        return [String(theme.name ?? ""), theme.colors ?? {}];
      }));
    }).catch(() => {});
  });

  /**
   * The colour a tab draws with when the profile has none of its own, to start the picker on: Default Settings',
   * else Settings > Terminal's, else the theme's (the one chosen there, or Light or Dark as the app is)
   */
  function fallbackColour(field: (typeof COLOURS)[number]["field"]): string {
    void settings.terminalPrefs;
    void settings.theme;
    const c = COLOURS.find((c) => c.field === field)!;
    const theme = themeColours[choice === "dark" ? "Dark" : choice === "light" ? "Light" : drawnTheme()];
    if (choice !== "custom") return theme?.[c.theme] || (field === "cursorColor" ? theme?.foreground : "") || c.putty;
    return (!isDefault && defaultProfile?.[field]) || pref(c.pref) || theme?.[c.theme]
      || (field === "cursorColor" ? (!isDefault && defaultProfile?.foreground) || pref("colorForeground") || theme?.foreground : "") || c.putty;
  }

  /** A field that can be changed: a profile's own, or its password (kept in the Keychain) */
  type Field = keyof Profile | "password";

  /**
   * A field's value as it counts: numbers as numbers, and on Default Settings a blank three-way choice as what it
   * means there (keys offered, username not asked), so choosing that explicitly isn't a change
   */
  function meaning(profile: Profile, field: keyof Profile): unknown {
    const value = profile[field];
    if (field === "port" || field === "keepalive" || field === "keyLifetime" || field === "antiIdleSeconds") return Number(value) || 0;
    if (isDefault && field === "addressFamily") return value || "auto";
    if (isDefault && field === "offerKeys") return value ?? true;
    if (isDefault && field === "askUsername") return value ?? false;
    // Default Settings' blank is the built-in look, the same as choosing it
    if (isDefault && field === "cursorBlink") return value ?? true;
    if (isDefault && field === "cursor") return value || "block";
    if (isDefault && field === "bell") return value || "visual";
    if (field === "colours") return shownChoice(profile);
    return value ?? null;
  }

  /** Whether a field differs from the saved profile; the password when one was typed in or forgotten */
  function changed(field: Field): boolean {
    if (field === "password") return password !== undefined;
    return meaning(draft, field) !== meaning(saved, field);
  }

  /** The fields the editor shows, for counting the changes */
  const FIELDS: Field[] = [
    "name", "username", "password", "keyFile", "keyFetch", "keyLifetime", "offerKeys", "askUsername", "port", "jumpHost", "host", "keepalive",
    "addressFamily", "antiIdle", "antiIdleSeconds",
    "olderAlgorithms", "algorithms", "sshConfigHost", "sshOptions", "cursor", "cursorBlink", "bell",
    "colours", "background", "foreground", "selection", "cursorColor", "tabColor", "buttonBar",
  ];

  /** The button bars' names (ButtonBars.xml in the settings folder), for the Button bar box */
  let barNames = $state<string[]>([]);
  onMount(() => {
    void loadBars().then((bars) => (barNames = bars.map((b) => b.name))).catch(() => {});
  });
  /** The profile's bar when it isn't among the bars (renamed or deleted, or another computer's), so it still shows */
  const missingBar = $derived(draft.buttonBar && !barNames.some((n) => n.toLowerCase() === draft.buttonBar.toLowerCase()) ? draft.buttonBar : "");
  /** How many fields have changed since the profile was loaded or saved */
  const changes = $derived(FIELDS.filter(changed).length);
  onMount(async () => {
    profiles = await listProfiles();
    // a session's profile is matched by name whatever its case, as the launch matches it
    const wanted = initial ? profiles.find((p) => p.name.toLowerCase() === initial.trim().toLowerCase()) : undefined;
    select(wanted?.name ?? DEFAULT_PROFILE);
  });

  /** Edit a profile from the list */
  function select(name: string): void {
    const profile = profiles.find((p) => p.name === name);
    if (!profile) return;
    selected = name;
    draft = { ...profile };
    saved = { ...profile };
    originalName = profile.name;
    password = undefined;
    message = "";
  }

  /** Start a new profile: it asks for the password rather than offering SSH keys, as most are for network devices */
  function add(): void {
    selected = "";
    draft = { ...emptyProfile(""), offerKeys: false };
    saved = { ...draft };
    originalName = "";
    password = undefined;
    message = "Name it after the PuTTY profile your sessions use, e.g. root.";
  }

  /** Save the draft */
  async function save(): Promise<void> {
    try {
      profiles = await saveProfile(
        { ...draft, port: Number(draft.port) || 0, keepalive: Number(draft.keepalive) || 0, antiIdleSeconds: Math.max(0, Number(draft.antiIdleSeconds) || 0), keyLifetime: Math.max(0, Number(draft.keyLifetime) || 0) },
        originalName, password,
      );
      select(draft.name.trim());
      message = "Saved.";
    } catch (e) {
      message = String(e);
    }
  }

  /** Throw the changes away: the profile as it was loaded or saved (a new one, blank again) */
  function revert(): void {
    draft = { ...saved };
    password = undefined;
    message = "";
  }

  /** Remove a profile (the selected one by default), after asking */
  async function remove(name = originalName): Promise<void> {
    if (!(await ask(`Delete profile "${name}"? Sessions that name it use Default Settings until it's set up again.`,
      { title: "Delete profile", kind: "warning", okLabel: "Delete" }))) return;
    try {
      profiles = await deleteProfile(name);
      select(DEFAULT_PROFILE);
      message = `Removed "${name}".`;
    } catch (e) {
      message = String(e);
    }
  }

  /** The first free copy name: "root-1", "root-2"…; a copy of "root-1" is "root-2", not "root-1-1" */
  function copyName(name: string): string {
    const base = name.replace(/-\d+$/, "");
    const taken = new Set(profiles.map((p) => p.name.toLowerCase()));
    let n = 1;
    while (taken.has(`${base}-${n}`.toLowerCase())) n++;
    return `${base}-${n}`;
  }

  /** Save a copy of a profile under the next free name and open it; its stored password stays with the original */
  async function duplicate(name: string): Promise<void> {
    const original = profiles.find((p) => p.name === name);
    if (!original) return;
    const copy = copyName(name);
    try {
      profiles = await saveProfile({ ...original, name: copy, hasPassword: false }, "", undefined);
      select(copy);
      message = original.hasPassword ? `Duplicated as "${copy}". Its password isn't copied: type it again.` : `Duplicated as "${copy}".`;
    } catch (e) {
      message = String(e);
    }
  }

  /** Export a profile to a .json file, for a colleague or another computer; its password stays here */
  async function exportProfile(name: string): Promise<void> {
    const path = await saveDialog({ title: `Export profile "${name}"`, defaultPath: `${name} profile.json`, filters: [{ name: "Profile", extensions: ["json"] }] });
    if (!path) return;
    try {
      await invoke("profiles_export", { name, path });
      const profile = profiles.find((p) => p.name === name);
      message = profile?.hasPassword ? `Exported "${name}". Its password isn't in the file.` : `Exported "${name}".`;
    } catch (e) {
      message = String(e);
    }
  }

  /**
   * Add the profiles in a .json file (an exported profile, or a whole Profiles.json); one named as a profile already
   * here comes in under the next free name, as Duplicate's, so nothing here is replaced. The last one opens.
   */
  async function importProfiles(): Promise<void> {
    const path = await open({ title: "Import profiles", filters: [{ name: "Profile", extensions: ["json"] }] });
    if (typeof path !== "string") return;
    try {
      const found = await invoke<Profile[]>("profiles_read_file", { path });
      const added: string[] = [];
      const renamed: string[] = [];
      for (const profile of found) {
        const taken = profiles.some((p) => p.name.toLowerCase() === profile.name.toLowerCase());
        const name = taken ? copyName(profile.name) : profile.name;
        if (taken) renamed.push(`"${profile.name}" as "${name}"`);
        profiles = await saveProfile({ ...profile, name, hasPassword: false }, "", undefined);
        added.push(name);
      }
      select(added[added.length - 1]);
      message = [
        added.length === 1 ? `Imported "${added[0]}".` : `Imported ${added.length} profiles.`,
        renamed.length ? `Already here, so imported ${renamed.join(", ")}.` : "",
        "Passwords aren't in profile files: type them in.",
      ].filter(Boolean).join(" ");
    } catch (e) {
      message = String(e);
    }
  }

  /** The list's right-click menu: Duplicate, Export and Import, and Delete, greyed while there are unsaved changes */
  function showMenu(event: MouseEvent, name: string): void {
    event.preventDefault();
    const busy = changes > 0 ? "Save or revert the changes first" : false;
    const isDefaultProfile = name.toLowerCase() === DEFAULT_PROFILE.toLowerCase();
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: "Duplicate", disabled: busy, action: () => void duplicate(name) },
        { separator: true },
        { label: "Export…", action: () => void exportProfile(name) },
        { label: "Import…", disabled: busy, action: () => void importProfiles() },
        { separator: true },
        { label: "Delete…", disabled: busy || (isDefaultProfile ? "Default Settings can't be deleted" : false), action: () => void remove(name) },
      ],
    };
  }

  /** Choose a private key file */
  async function chooseKey(): Promise<void> {
    const path = await open({ title: "Choose a private key (OpenSSH format)", defaultPath: "~/.ssh" });
    if (typeof path === "string") draft.keyFile = path;
  }

  /** Examples for the Fetch command, for this system; the one last picked, for its note */
  const examples = keyFetchExamples(isMac);
  let example = $state<(typeof examples)[number] | null>(null);

  /** Fill the fetch from an example: its command and lifetime, and a key file when there's none */
  function useExample(name: string): void {
    const picked = examples.find((e) => e.name === name);
    if (!picked) return;
    draft.keyFetch = picked.command;
    draft.keyLifetime = picked.lifetime;
    if (!draft.keyFile.trim()) draft.keyFile = picked.keyFile;
    example = picked;
  }

  /** Why Log in & cache key can't run now, or "" when it can */
  const loginBlocked = $derived(
    !draft.keyFetch.trim() ? "Set a Fetch command first (Examples has some)"
      : !draft.keyFile.trim() ? "Set the Key file the fetch writes to"
      : changes > 0 || !originalName ? "Save the profile first: the fetch runs its saved command"
      : "",
  );

  /** The key file's age, for the key fetch's note: seconds, or null when there's none (or no key file) */
  let keyAge = $state<number | null>(null);
  $effect(() => {
    const file = draft.keyFile.trim();
    keyAge = null;
    if (file) void invoke<number | null>("profile_key_age", { keyFile: file }).then((age) => (keyAge = age ?? null), () => {});
  });

  /** "37 min" or "4 h 12 min" */
  const duration = (minutes: number) => {
    const h = Math.floor(minutes / 60), m = Math.round(minutes % 60);
    return h === 0 ? `${m} min` : m === 0 ? `${h} h` : `${h} h ${m} min`;
  };

  /** The key's state, as the next session will find it */
  const keyState = $derived.by(() => {
    if (!draft.keyFile.trim()) return "Set the Key file it writes to.";
    if (keyAge === null) return "No key yet: the next session fetches one.";
    const age = Math.floor(keyAge / 60), life = Number(draft.keyLifetime) || 0;
    if (life > 0 && age > life) return `The key is ${duration(age)} old, past its ${duration(life)}: the next session fetches a new one.`;
    return life > 0 ? `Key: ${duration(age)} old, good for ${duration(life)}.` : `Key: ${duration(age)} old.`;
  });

  /** A three-way choice (default / on / off) as a select's value, and back */
  const triToValue = (v: boolean | null) => (v === null ? "" : v ? "on" : "off");
  /** The select's value back as a three-way choice */
  const valueToTri = (v: string) => (v === "" ? null : v === "on");
</script>

<div class="profiles">
  <div class="list">
    <ul role="listbox" aria-label="Profiles">
      {#each profiles as profile (profile.name)}
        <!-- another profile can't be opened over unsaved changes: Save or Revert first -->
        <li role="option" aria-selected={profile.name === selected} aria-disabled={changes > 0}
          class:selected={profile.name === selected} class:locked={changes > 0}
          title={changes > 0 && profile.name !== selected ? "Save or revert the changes first" : undefined}
          onclick={() => changes === 0 && select(profile.name)}
          oncontextmenu={(e) => showMenu(e, profile.name)}
          onkeydown={(e) => e.key === "Enter" && changes === 0 && select(profile.name)} tabindex="0">
          {profile.name}
        </li>
      {/each}
    </ul>
    <div class="list-buttons">
      <button onclick={add} disabled={changes > 0}>New</button>
      <button onclick={() => void remove()} disabled={changes > 0 || isDefault || originalName === ""}>Remove</button>
    </div>
  </div>

  <div class="form">
    <!-- stays put while the fields scroll -->
    <div class="head">
      <strong class="title">{originalName || draft.name.trim() || "New profile"}</strong>
      <span class="message" class:pending={changes > 0}>
        {changes > 0 ? `Unsaved changes (${changes})` : message}
      </span>
      <button onclick={revert} disabled={changes === 0} title="Put the fields back as they were">Revert</button>
      <button class="primary" onclick={save} disabled={changes === 0}>Save</button>
    </div>

    <div class="fields">
    <p class="hint">
      A session's PuTTY profile is looked up here by name. The session's own fields win, then its folder's
      defaults, then the profile, then Default Settings.
    </p>

    <fieldset>
      <legend>Profile</legend>
      <label>{@render caption("Name", "name")}<input bind:value={draft.name} disabled={isDefault} placeholder="e.g. root" /></label>
    </fieldset>

    <fieldset>
      <legend>Login</legend>
      <label>{@render caption("Username", "username")}<input bind:value={draft.username} placeholder="e.g. admin" /></label>
      <label>{@render caption("Password", "password")}
        <input type="password" value={password ?? ""} oninput={(e) => (password = e.currentTarget.value)}
          placeholder={draft.hasPassword ? `Stored in ${SECRET_STORE}` : "none"} />
        {#if draft.hasPassword}<button onclick={() => (password = "")} title="Forget the stored password">Forget</button>{/if}
      </label>
      <label>{@render caption("Key file", "keyFile")}<input bind:value={draft.keyFile} placeholder="ssh's own keys and the agent" />
        <button onclick={chooseKey}>Choose…</button></label>
      <label>{@render caption("Fetch command", "keyFetch")}<input bind:value={draft.keyFetch}
        placeholder="none; for keys issued for a short time (CyberArk, Teleport…): pick from Examples" />
        <select class="examples" aria-label="Fetch command examples" value=""
          onchange={(e) => { useExample(e.currentTarget.value); e.currentTarget.value = ""; }}>
          <option value="">Examples…</option>
          {#each examples as e (e.name)}<option value={e.name}>{e.name}</option>{/each}
        </select></label>
      {#if example}<p class="note under">{example.name}: {example.note}</p>{/if}
      <label>{@render caption("Lifetime", "keyLifetime")}<input class="short" type="number" min="0" bind:value={draft.keyLifetime} />
        <span class="note">minutes a fetched key is good for (0: fetched only when there's none)</span></label>
      {#if draft.keyFetch.trim()}
        <p class="note under">{keyState} The fetch runs in the session's tab, through {isMac ? "your shell (its ~/.zshrc read, so a function works)" : "PowerShell"},
          so its sign-in (a link, a PIN) happens there; {"{key_file}"} and your Environment Variables are filled in. See docs/CYBERARK.md.</p>
      {/if}
      <p class="under">
        <button disabled={!!loginBlocked} title={loginBlocked || "Run the fetch now in a tab of its own, so sessions log in with the key"}
          onclick={() => { context.tabs?.openKeyFetch(originalName); onLeave(); }}>Log in & cache key</button>
        {#if loginBlocked}<span class="note">{loginBlocked}</span>{/if}
      </p>
      <!-- Default Settings has no default of its own to point at: its blank is the built-in behaviour -->
      <label>{@render caption("SSH keys", "offerKeys")}
        <select value={triToValue(isDefault ? (draft.offerKeys ?? true) : draft.offerKeys)}
          onchange={(e) => (draft.offerKeys = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="on">Offer {SSH_KEYS} and the agent's</option>
          <option value="off">Don't offer them: ask for the password (switches, routers)</option>
        </select></label>
      <p class="note under">A Cisco closes the connection after 3 failed attempts, and every key offered is one, so with
        no stored password its password prompt never comes. A key file above is always used.</p>
      <label>{@render caption("No username", "askUsername")}
        <select value={triToValue(isDefault ? (draft.askUsername ?? false) : draft.askUsername)}
          onchange={(e) => (draft.askUsername = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="off">Use this computer's user name (ssh's own)</option>
          <option value="on">Ask in the tab ("login as:")</option>
        </select></label>
    </fieldset>

    <fieldset>
      <legend>Connection</legend>
      <label>{@render caption("Port", "port")}<input class="short" type="number" min="0" max="65535" bind:value={draft.port} /> <span class="note">0: not set</span></label>
      <label>{@render caption("Jump host", "jumpHost")}<input bind:value={draft.jumpHost} placeholder="a session, or admin@10.0.0.1:22" /></label>
      <label>{@render caption("Host", "host")}<input bind:value={draft.host} placeholder="none; e.g. example.cyberark.cloud" /></label>
      <p class="note under">Used, at the Port above, by sessions with no host of their own, as a PuTTY saved session's
        Host Name; and by sessions whose username names their host as {"{host_address}"} (a CyberArk SSH gateway, which
        takes the target in the username). Otherwise a session's own host wins. See docs/CYBERARK.md.</p>
      <label>{@render caption("Keep-alive", "keepalive")}<input class="short" type="number" min="0" bind:value={draft.keepalive} /> <span class="note">{isDefault ? "seconds (0: 15, so a lost connection ends the session in about 45 s)" : "seconds (0: Default Settings')"}</span></label>
      <label>{@render caption("Internet protocol", "addressFamily")}
        <select value={draft.addressFamily || (isDefault ? "auto" : "")} onchange={(e) => (draft.addressFamily = e.currentTarget.value)}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="auto">Auto: any address the name has</option>
          <option value="ipv4">IPv4 only</option>
          <option value="ipv6">IPv6 only</option>
        </select></label>
      <label>{@render caption("Anti-idle string", "antiIdle")}<input bind:value={draft.antiIdle} placeholder={isDefault ? "none; e.g. \\x05\\ (Ctrl+E)" : "from Default Settings"} /></label>
      <label>{@render caption("Anti-idle after", "antiIdleSeconds")}<input class="short" type="number" min="0" bind:value={draft.antiIdleSeconds} /> <span class="note">{isDefault ? "seconds without typing (0: 180)" : "seconds without typing (0: Default Settings', else 180)"}</span></label>
      <p class="note under">Typed into the session after that long with nothing typed, and again after each such spell, as
        KiTTY's: a device's own idle timeout (a Cisco's exec-timeout) doesn't see ssh's keep-alives and logs the session
        out. KiTTY's escapes: \xHH a character by its hex code (\x05 is Ctrl+E, the end of the line on a Cisco, which
        shows nothing), \kHH a key, \r, \n, \t, \h (backspace) and \\; a \ at the end only ends it. Not while a
        script runs in the tab.</p>
      <label>{@render caption("Older algorithms", "olderAlgorithms")}
        <select value={triToValue(draft.olderAlgorithms)} onchange={(e) => (draft.olderAlgorithms = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="on">Allow (older Cisco and similar devices)</option>
          <option value="off">Don't allow</option>
        </select></label>
      <label>{@render caption("SSH algorithms", "algorithms")}
        <button onclick={() => (algorithming = true)} title={draft.algorithms ? `Its own: ${draft.algorithms}` : isDefault ? "ssh's own lists" : "From Default Settings"}>
          Choose…</button>
        <span class="note">{draft.algorithms ? "its own lists" : isDefault ? "ssh's own lists" : "from Default Settings"}</span></label>
      <label>{@render caption("SSH config host", "sshConfigHost")}<input bind:value={draft.sshConfigHost} placeholder="a Host in ~/.ssh/config" /></label>
      <label>{@render caption("More ssh options", "sshOptions")}<input bind:value={draft.sshOptions} placeholder="e.g. -L 8080:localhost:80" /></label>
    </fieldset>

    <fieldset>
      <legend>Terminal</legend>
      <label>{@render caption("Cursor", "cursor")}
        <select value={isDefault ? draft.cursor || "block" : draft.cursor} onchange={(e) => (draft.cursor = e.currentTarget.value)}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="block">Block</option>
          <option value="underline">Underline</option>
          <option value="bar">Vertical line</option>
        </select></label>
      <label>{@render caption("Cursor blinks", "cursorBlink")}
        <select value={triToValue(isDefault ? (draft.cursorBlink ?? true) : draft.cursorBlink)}
          onchange={(e) => (draft.cursorBlink = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="on">Yes</option>
          <option value="off">No</option>
        </select></label>
      <label>{@render caption("Bell", "bell")}
        <select value={isDefault ? draft.bell || "visual" : draft.bell} onchange={(e) => (draft.bell = e.currentTarget.value)}>
          {#if !isDefault}<option value="">From Default Settings</option>{/if}
          <option value="visual">Flash</option>
          <option value="sound">Sound</option>
          <option value="none">None</option>
        </select></label>
      <label>{@render caption("Colours", "colours")}
        <select value={shownChoice(draft)} onchange={(e) => (draft.colours = e.currentTarget.value)}>
          {#if !isDefault}<option value="default">From Default Settings ({SHORT[coloursChoice(defaultProfile, undefined)]})</option>{/if}
          {#each Object.entries(CHOICES) as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select></label>
      {#each COLOURS as c (c.field)}
        {@const off = choice !== "custom"}
        <label class:off>{@render caption(c.label, c.field)}
          <span class="colour">
            <input type="color" value={off ? fallbackColour(c.field) : draft[c.field] || fallbackColour(c.field)} aria-label={c.label} disabled={off}
              oninput={(e) => (draft[c.field] = e.currentTarget.value)} />
            <span class="hint">{off ? (choice === "theme" ? "the theme's" : `the ${CHOICES[choice]} theme's`) : draft[c.field] || (isDefault ? "Settings > Terminal's" : defaultProfile?.[c.field] ? `From Default Settings (${defaultProfile[c.field]})` : "From Default Settings")}</span>
            <button disabled={off || !draft[c.field]} onclick={(e) => { e.preventDefault(); draft[c.field] = ""; }}
              title={isDefault ? "Use Settings > Terminal's colour" : "Use Default Settings' colour"}>{isDefault ? "Settings'" : "Default's"}</button>
          </span></label>
      {/each}
      <!-- not a terminal colour, so it can be set whatever the Colours choice; an environment colour wins over it -->
      <label>{@render caption("Tab colour", "tabColor")}
        <span class="colour">
          <input type="color" value={draft.tabColor || (!isDefault && defaultProfile?.tabColor) || NO_TAB_COLOUR} aria-label="Tab colour"
            oninput={(e) => (draft.tabColor = e.currentTarget.value)} />
          <span class="hint">{draft.tabColor || (!isDefault && defaultProfile?.tabColor ? `From Default Settings (${defaultProfile.tabColor})` : "None")}</span>
          <button disabled={!draft.tabColor} onclick={(e) => { e.preventDefault(); draft.tabColor = ""; }}
            title={isDefault ? "No tab colour" : "Use Default Settings' tab colour"}>{isDefault ? "None" : "Default's"}</button>
        </span></label>
      <!-- the bar shown while one of the profile's tabs is active; a bar picked by hand still wins until the tab changes -->
      <label>{@render caption("Button bar", "buttonBar")}
        <select value={draft.buttonBar} onchange={(e) => (draft.buttonBar = e.currentTarget.value)}>
          <option value="">{isDefault || !defaultProfile?.buttonBar ? "The one for the tab's icon" : `From Default Settings (${defaultProfile.buttonBar})`}</option>
          {#each barNames as name (name)}<option value={name}>{name}</option>{/each}
          {#if missingBar}<option value={missingBar}>{missingBar} (not in the button bars)</option>{/if}
        </select></label>
    </fieldset>

    </div>
  </div>
</div>

<!-- a field's caption, with a dot while it differs from the saved profile -->
{#snippet caption(text: string, field: Field)}
  <span class="caption">{text}{#if changed(field)}<span class="dot" title="Changed: not saved yet" aria-label="changed"></span>{/if}</span>
{/snippet}

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}
<svelte:window onclick={() => (menu = null)} />

{#if algorithming}
  <AlgorithmsDialog title={`SSH algorithms: ${draft.name || "new profile"}`} text={draft.algorithms ?? ""}
    otherwise={isDefault ? "ssh's own list (with the older algorithms, if allowed)" : "Default Settings' list"}
    onCancel={() => (algorithming = false)} onDone={(text) => { draft.algorithms = text; algorithming = false; }} />
{/if}

<style>
  .profiles {
    display: flex;
    gap: 16px;
    min-height: 0;
    height: 100%;
  }

  .list {
    width: 200px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  ul {
    flex: 1;
    margin: 0;
    padding: 4px;
    list-style: none;
    border: 1px solid var(--border);
    background: var(--bg-input);
    overflow: auto;
  }

  li {
    padding: 4px 8px;
    border-radius: 3px;
  }

  li.selected {
    background: var(--accent);
    color: #fff;
  }

  .list-buttons,
  .head {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  /* the editor: its header fixed, the fields scrolling under it */
  .form {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .head {
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 40%;
  }

  .fields {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-right: 4px;
  }

  li.locked:not(.selected) {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .caption {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }

  /* a field changed since it was loaded or saved */
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
    flex: none;
  }

  .hint,
  .note,
  .message {
    color: var(--text-muted);
    margin: 0;
  }

  /* a note for the row above, lined up with its control (label's first column and gap) */
  .note.under {
    margin-left: 138px;
    font-size: 12px;
  }

  p.under:not(.note) {
    margin: 0 0 0 138px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .examples {
    max-width: 150px;
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: 4px;
    display: grid;
    gap: 6px;
    padding: 8px 12px;
  }

  label {
    display: grid;
    grid-template-columns: 130px 1fr auto;
    align-items: center;
    gap: 8px;
  }

  label:has(.short) {
    grid-template-columns: 130px 90px 1fr;
  }

  input,
  select {
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: 3px;
    padding: 3px 6px;
    min-width: 0;
  }

  .message {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .message.pending {
    color: var(--accent);
  }
  label.off {
    opacity: 0.5;
  }

  .colour {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .colour input[type="color"] {
    width: 36px;
    height: 22px;
    padding: 1px 2px;
  }
</style>
