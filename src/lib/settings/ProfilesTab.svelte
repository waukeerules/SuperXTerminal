<!--
  Settings > Profiles: the user's own profiles, matched by name to the PuTTY profile a session names ("root",
  "Default Settings"), as their PuTTY profiles are on Windows. The shared session says what to connect to; the
  profile says how this user logs in. Empty fields fall back to Default Settings.
  A profile's right-click menu exports it to a .json file (no password) and imports profiles from one, as a
  colleague's or another computer's.
-->
<script lang="ts">
  import { MONOSPACE_FONTS, SECRET_STORE, SSH_CONFIG, SSH_KEYS, THIS_COMPUTER } from "../platform";
  import { onMount, tick } from "svelte";
  import { ask, open, save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import { isMac } from "../shortcuts";
  import { context, jumpHostOf } from "../commands/sending.svelte";
  import { keyFetchExamples } from "./keyFetchExamples";
  import { chosenScript, isFullPath } from "../scripting/scriptPath";
  import { JUMP_HOST_NEEDS } from "../sessions/api";
  import { sampleFor, TUNNEL_SAMPLES } from "../terminal/tunnels";
  import { OP_KEY_EXAMPLE, OP_KEY_HINT, OP_REFERENCE_EXAMPLE, OP_REFERENCE_HINT } from "./onepassword";
  import ScriptPathNote from "../scripting/ScriptPathNote.svelte";
  import Menu, { type MenuItem } from "../ui/Menu.svelte";
  import { baseChain, coloursChoice, DEFAULT_PROFILE, deleteProfile, emptyProfile, listProfiles, MAX_FONT_POINTS, mergedProfile, ownColoursChoice, resolvedProfile, saveProfile, type Forward, type Profile } from "./profiles";
  import AlgorithmsDialog from "../sessions/AlgorithmsDialog.svelte";
  import { drawnTheme, pref } from "../terminal/prefs";
  import { highlighting } from "../terminal/highlighting";
  import { loadBars } from "../commands/bars";
  import { settings } from "./store.svelte";

  interface Props {
    /** The profile to show first (Quick connect's Edit…, Edit Session's Profiles…); Default Settings when it's none */
    initial?: string | null;
    /**
     * A field of it to show (its name in Profile, e.g. "forwards"): scrolled to and lit up for a moment, for Edit
     * Session's links to what its profile sets
     */
    field?: string | null;
    /** Close Settings, for what goes on in a tab (Log in & cache key) */
    onLeave?: () => void;
  }

  let { initial = null, field = null, onLeave = () => {} }: Props = $props();

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
  /** The team's profile templates (ProfileTemplates.json in the settings folder) */
  let templates = $state<Profile[]>([]);
  /** The team template the new profile being edited came from, to say what's left to fill in; "" for none */
  let fromTemplate = $state("");
  /** The templates not set up here yet, by name: offered under the profiles */
  const newTemplates = $derived(templates.filter((t) => !profiles.some((p) => p.name.toLowerCase() === t.name.toLowerCase())));

  /** Whether the profile shown is Default Settings, which can't be renamed or deleted */
  const isDefault = $derived(originalName.toLowerCase() === DEFAULT_PROFILE.toLowerCase());
  /** Default Settings, whose values a profile's empty fields show */
  const defaultSettings = $derived(profiles.find((p) => p.name.toLowerCase() === DEFAULT_PROFILE.toLowerCase()));
  /** The profile chosen in Based on, if it's there (not Default Settings, itself, or one based on this one) */
  const base = $derived(!isDefault && draft.basedOn.trim() ? profiles.find((p) => p.name.toLowerCase() === draft.basedOn.trim().toLowerCase()) : undefined);
  /**
   * What a field this profile leaves empty comes from: the profile it's based on, filled along its own Based on and
   * then from Default Settings; else Default Settings. Its hints show these values
   */
  const defaultProfile = $derived(base ? mergedProfile(resolvedProfile(profiles, base.name)!, defaultSettings) : defaultSettings);
  /** Where an empty field comes from, as the editor says it: the base profile's name, else Default Settings */
  const FALLBACK = $derived(base?.name ?? DEFAULT_PROFILE);
  /** "From netops" or "From Default Settings", for the choices that leave a field to it */
  const FROM = $derived(`From ${FALLBACK}`);
  /**
   * The profiles this one can be based on: not Default Settings (every profile is, already), not itself, and not one
   * based on it along the way, which would go round in a loop
   */
  const bases = $derived(profiles.filter((p) => p.name !== DEFAULT_PROFILE && p.name.toLowerCase() !== originalName.toLowerCase()
    && !baseChain(profiles, p.name).some((n) => n.toLowerCase() === originalName.toLowerCase() && originalName !== "")));

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
    if (field === "port" || field === "keepalive" || field === "keyLifetime" || field === "antiIdleSeconds" || field === "fontSize" || field === "scrollback") return Number(value) || 0;
    if (isDefault && field === "addressFamily") return value || "auto";
    if (isDefault && field === "offerKeys") return value ?? true;
    if (isDefault && field === "askUsername") return value ?? false;
    if (isDefault && field === "serverLoad") return value ?? false;
    // Default Settings' blank is the built-in look, the same as choosing it
    if (isDefault && field === "cursorBlink") return value ?? true;
    if (isDefault && field === "cursor") return value || "block";
    if (isDefault && field === "bell") return value || "visual";
    if (field === "forwards") return JSON.stringify(value ?? []);
    if (isDefault && (field === "agentForwarding" || field === "x11Forwarding" || field === "compression")) return value ?? false;
    if (isDefault && field === "keyBackspace") return value || "del";
    if (isDefault && field === "keyHomeEnd") return value || "standard";
    if (isDefault && field === "keyFunction") return value || "tilde";
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
    "name", "basedOn", "username", "password", "keyFile", "keyFetch", "keyLifetime", "offerKeys", "askUsername", "port", "jumpHost", "host", "keepalive",
    "addressFamily", "antiIdle", "antiIdleSeconds",
    "olderAlgorithms", "serverLoad", "algorithms", "sshConfigHost", "sshOptions",
    "agentForwarding", "x11Forwarding", "compression", "forwards", "tunnelsOff", "cursor", "cursorBlink", "bell",
    "fontFamily", "fontSize", "keyBackspace", "keyHomeEnd", "keyFunction", "scrollback", "sessionLog", "charset", "colours", "background", "foreground", "selection", "cursorColor", "tabColor", "buttonBar", "script", "logInHere",
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
    if (field) void reveal(field);
    await loadTemplates();
  });

  /** The field lit up after Edit Session's link to it, until the light fades */
  let revealed = $state<string | null>(null);

  /**
   * Scroll to a field's row (its caption carries the field's name) and light it up for a moment, so it's clear which
   * setting the link was for
   * @param name The field's name in Profile
   */
  async function reveal(name: string): Promise<void> {
    await tick();
    const caption = document.querySelector(`.profiles [data-field="${name}"]`);
    const row = caption?.closest("label, .tunnels, .field") ?? caption;
    // (jsdom, in the tests, has no scrolling)
    row?.scrollIntoView?.({ block: "center" });
    revealed = name;
    setTimeout(() => revealed === name && (revealed = null), 2000);
  }

  /** Edit a profile from the list */
  function select(name: string): void {
    const profile = profiles.find((p) => p.name === name);
    if (!profile) return;
    selected = name;
    fromTemplate = "";
    draft = { ...profile };
    saved = { ...profile };
    originalName = profile.name;
    password = undefined;
    message = "";
  }

  /** Start a new profile: it asks for the password rather than offering SSH keys, as most are for network devices */
  function add(): void {
    selected = "";
    fromTemplate = "";
    draft = { ...emptyProfile(""), offerKeys: false };
    saved = { ...draft };
    originalName = "";
    password = undefined;
    message = "Name it after the PuTTY profile your sessions use, e.g. root.";
  }

  /** Read the team's templates again (after one is shared); a file that can't be read says so here */
  async function loadTemplates(): Promise<void> {
    try {
      templates = (await invoke<Profile[] | null>("profile_templates")) ?? [];
    } catch (e) {
      templates = [];
      message = String(e);
    }
  }

  /**
   * Set up a team template as a profile of the same name: its fields filled in, for this person to add their login
   * and Save. Nothing is saved until then, and Revert gives a blank new profile
   */
  function setUp(template: Profile): void {
    selected = "";
    saved = { ...emptyProfile(""), offerKeys: false };
    draft = { ...emptyProfile(""), ...template, username: "", keyFile: "", hasPassword: false };
    originalName = "";
    password = undefined;
    message = "";
    fromTemplate = template.name;
  }

  /** Share a profile with the team as a template in the settings folder, without its login, after asking */
  async function share(name: string): Promise<void> {
    const replacing = templates.some((t) => t.name.toLowerCase() === name.toLowerCase());
    const question = (replacing ? `Replace the team's template "${name}" with this profile?` : `Share profile "${name}" with the team as a template?`)
      + " Its username, key file and password stay here; everyone with the settings folder can set it up as their own.";
    if (!(await ask(question, { title: "Share as Team Template", okLabel: replacing ? "Replace" : "Share" }))) return;
    try {
      templates = (await invoke<Profile[] | null>("profile_template_share", { name })) ?? templates;
      message = `Shared "${name}" with the team.`;
    } catch (e) {
      message = String(e);
    }
  }

  /** Save the draft */
  async function save(): Promise<void> {
    try {
      profiles = await saveProfile(
        { ...draft, port: Number(draft.port) || 0, keepalive: Number(draft.keepalive) || 0, antiIdleSeconds: Math.max(0, Number(draft.antiIdleSeconds) || 0), keyLifetime: Math.max(0, Number(draft.keyLifetime) || 0), fontSize: Math.max(0, Number(draft.fontSize) || 0), scrollback: Math.max(0, Math.floor(Number(draft.scrollback) || 0)) },
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
   * Add the profiles in a .json file (an exported profile, or a whole Profiles.json), or PuTTY's saved sessions in a
   * .reg file (reg export HKCU\Software\SimonTatham\PuTTY\Sessions), each as a profile of its name, as SuperXPuTTY's
   * sessions name them. One named as a profile already here comes in under the next free name, as Duplicate's, so
   * nothing here is replaced. The last one opens.
   */
  async function importProfiles(): Promise<void> {
    const path = await open({ title: "Import profiles", filters: [{ name: "Profiles, or PuTTY's sessions (reg export)", extensions: ["json", "reg"] }] });
    if (typeof path !== "string") return;
    const fromPutty = path.toLowerCase().endsWith(".reg");
    try {
      const found = await invoke<Profile[]>(fromPutty ? "profiles_read_putty" : "profiles_read_file", { path });
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
        fromPutty ? "From PuTTY: its proxies and algorithm lists aren't profile settings, so check those." : "Passwords aren't in profile files: type them in.",
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
        { label: "Share as Team Template…", disabled: busy || (!settings.settingsFolder ? "Choose a settings folder first (Settings > General)" : false), action: () => void share(name) },
        { label: "Import…", disabled: busy, action: () => void importProfiles() },
        { separator: true },
        { label: "Delete…", disabled: busy || (isDefaultProfile ? "Default Settings can't be deleted" : false), action: () => void remove(name) },
      ],
    };
  }

  /**
   * The menu for the list's empty space, below the profiles: New and Import. Without it Import is only found by
   * right-clicking a profile, which nobody guesses when what they want is a profile they don't have yet. A right-click
   * on a profile has already shown that profile's menu (and prevented the default), so it's left alone
   */
  function showListMenu(event: MouseEvent): void {
    if (event.defaultPrevented) return;
    event.preventDefault();
    const busy = changes > 0 ? "Save or revert the changes first" : false;
    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        { label: "New", disabled: busy, action: add },
        { label: "Import…", disabled: busy, action: () => void importProfiles() },
      ],
    };
  }

  /** Choose a private key file */
  async function chooseKey(): Promise<void> {
    const path = await open({ title: "Choose a private key (OpenSSH format)", defaultPath: "~/.ssh" });
    if (typeof path === "string") draft.keyFile = path;
  }

  /** Choose the login script (SPSL or Rhai) the profile's sessions run as they open */
  async function chooseScript(): Promise<void> {
    const path = await open({ title: "Choose the profile's login script", filters: [{ name: "Scripts", extensions: ["spsl", "rhai", "txt"] }] });
    if (typeof path === "string") draft.script = await chosenScript(path);
  }

  /** Examples for the Fetch command, for this system; the one last picked, for its note */
  const examples = $derived(keyFetchExamples(isMac, settings.onePassword));
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

  /**
   * Tunnel Samples…: common uses to start from (tunnels.ts), each added as a tunnel to adjust, its port moved up when
   * the profile has one on it already. Shown under the button
   */
  function showSamples(event: MouseEvent): void {
    // the window's click closes the menu: not this one, which opens it
    event.stopPropagation();
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = {
      x: rect.left,
      y: rect.bottom + 2,
      items: TUNNEL_SAMPLES.map((sample) => ({
        label: sample.label,
        action: () => (draft.forwards = [...draft.forwards, sampleFor(sample, draft.forwards)]),
      })),
    };
  }

  /**
   * Change one tunnel. The list is replaced, not changed in place: the saved profile shares it, and Revert and the
   * changes count compare against that. A dynamic tunnel goes nowhere, so its destination is cleared
   */
  function setForward(index: number, change: Partial<Forward>): void {
    draft.forwards = draft.forwards.map((f, i) => {
      if (i !== index) return f;
      const next = { ...f, ...change };
      return next.kind === "dynamic" ? { ...next, to: "" } : next;
    });
  }
</script>

<div class="profiles">
  <div class="list">
    <ul role="listbox" aria-label="Profiles" oncontextmenu={showListMenu}>
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
      {#if newTemplates.length}
        <li class="group" role="presentation">Team templates</li>
        {#each newTemplates as template (template.name)}
          <!-- not set up here yet: a click fills a new profile from it -->
          <li role="option" aria-selected="false" aria-disabled={changes > 0} class="template" class:locked={changes > 0}
            title={changes > 0 ? "Save or revert the changes first" : "Set it up as your own: add your login and Save"}
            onclick={() => changes === 0 && setUp(template)}
            onkeydown={(e) => e.key === "Enter" && changes === 0 && setUp(template)} tabindex="0">
            {template.name}
          </li>
        {/each}
      {/if}
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

    {#if fromTemplate}
      <p class="from-template">From the team's template "{fromTemplate}": add your username and password (or key file), then Save.</p>
    {/if}
    <fieldset>
      <legend>Profile</legend>
      <label>{@render caption("Name", "name")}<input bind:value={draft.name} disabled={isDefault} placeholder="e.g. root" /></label>
      {#if !isDefault}
        <label>{@render caption("Based on", "basedOn")}
          <select value={draft.basedOn} onchange={(e) => (draft.basedOn = e.currentTarget.value)}>
            <option value="">Default Settings only</option>
            {#each bases as p (p.name)}<option value={p.name}>{p.name}</option>{/each}
            {#if draft.basedOn && !base}<option value={draft.basedOn}>{draft.basedOn} (not among the profiles)</option>{/if}
          </select></label>
        <p class="note under">What this profile leaves empty comes from the one it's based on (and what that one leaves,
          from its own), then Default Settings: netops-old can be netops with older algorithms, and follow netops' other
          changes. Its password too, when it has none of its own.</p>
      {/if}
    </fieldset>

    <fieldset>
      <legend>Login</legend>
      <label>{@render caption("Username", "username")}<input bind:value={draft.username} placeholder="e.g. admin" /></label>
      <label>{@render caption("Password", "password")}
        <input type="password" value={password ?? ""} oninput={(e) => (password = e.currentTarget.value)}
          placeholder={draft.hasPassword ? `Stored in ${SECRET_STORE}` : settings.onePassword ? `none; or a 1Password reference, ${OP_REFERENCE_EXAMPLE}` : "none"}
          title={settings.onePassword ? OP_REFERENCE_HINT : ""} />
        {#if draft.hasPassword}<button onclick={() => (password = "")} title="Forget the stored password">Forget</button>{/if}
      </label>
      <label>{@render caption("Key file", "keyFile")}<input bind:value={draft.keyFile} placeholder={settings.onePassword ? `ssh's own keys and the agent; or a key in 1Password, ${OP_KEY_EXAMPLE}` : "ssh's own keys and the agent"} title={settings.onePassword ? OP_KEY_HINT : ""} />
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
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="on">Offer {SSH_KEYS} and the agent's</option>
          <option value="off">Don't offer them: ask for the password (switches, routers)</option>
        </select></label>
      <p class="note under">A Cisco closes the connection after 3 failed attempts, and every key offered is one, so with
        no stored password its password prompt never comes. A key file above is always used.</p>
      <label>{@render caption("No username", "askUsername")}
        <select value={triToValue(isDefault ? (draft.askUsername ?? false) : draft.askUsername)}
          onchange={(e) => (draft.askUsername = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="off">Use this computer's user name (ssh's own)</option>
          <option value="on">Ask in the tab ("login as:")</option>
        </select></label>
    </fieldset>

    <fieldset>
      <legend>Connection</legend>
      <label>{@render caption("Port", "port")}<input class="short" type="number" min="0" max="65535" bind:value={draft.port} /> <span class="note">0: not set</span></label>
      <label>{@render caption("Jump host", "jumpHost")}<input bind:value={draft.jumpHost} placeholder="a saved session (▾), or admin@10.0.0.1:22" />
        <!-- a saved session to go through, found by its name or folder -->
        <button aria-label="Choose a session as the jump host" title="Choose a saved session to go through"
          onclick={() => context.pickSession("Jump Host", (s) => (draft.jumpHost = jumpHostOf(s)), undefined, draft.jumpHost)}>▾</button></label>
      <p class="note under" role="note" aria-label="What a jump server needs">{JUMP_HOST_NEEDS}</p>
      <label>{@render caption("Host", "host")}<input bind:value={draft.host} placeholder="none; e.g. example.cyberark.cloud" /></label>
      <p class="note under">Used, at the Port above, by sessions with no host of their own, as a PuTTY saved session's
        Host Name; and by sessions whose username names their host as {"{host_address}"} (a CyberArk SSH gateway, which
        takes the target in the username). Otherwise a session's own host wins{#if !isDefault}, unless Log in here is
        ticked{/if}. See docs/CYBERARK.md.</p>
      <!-- not on Default Settings: every session would then log in to one host -->
      {#if !isDefault}
        <label>{@render caption("Log in here", "logInHere")}<span class="check"><input type="checkbox" bind:checked={draft.logInHere} />
          a jump server: its sessions log in to this Host, and the login script goes on to theirs</span></label>
        <p class="note under">Each tab logs in to this Host with this profile's username, password and key. The
          session's own host, username and port are left for the profile's SPSL script (Terminal, below), as
          {"{host_address}"}, {"{host_port}"} and {"{session_username}"}: for example
          SENDLINE ssh {"{session_username}@{host_address}"}. See docs/SPSL.md.</p>
      {/if}
      <label>{@render caption("Keep-alive", "keepalive")}<input class="short" type="number" min="0" bind:value={draft.keepalive} /> <span class="note">{isDefault ? "seconds (0: 15, so a lost connection ends the session in about 45 s)" : `seconds (0: ${FALLBACK}')`}</span></label>
      <label>{@render caption("Internet protocol", "addressFamily")}
        <select value={draft.addressFamily || (isDefault ? "auto" : "")} onchange={(e) => (draft.addressFamily = e.currentTarget.value)}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="auto">Auto: any address the name has</option>
          <option value="ipv4">IPv4 only</option>
          <option value="ipv6">IPv6 only</option>
        </select></label>
      <label>{@render caption("Anti-idle string", "antiIdle")}<input bind:value={draft.antiIdle} placeholder={isDefault ? "none; e.g. \\x05\\ (Ctrl+E)" : "from Default Settings"} /></label>
      <label>{@render caption("Anti-idle after", "antiIdleSeconds")}<input class="short" type="number" min="0" bind:value={draft.antiIdleSeconds} /> <span class="note">{isDefault ? "seconds without typing (0: 180)" : `seconds without typing (0: ${FALLBACK}', else 180)`}</span></label>
      <p class="note under">Typed into the session after that long with nothing typed, and again after each such spell, as
        KiTTY's: a device's own idle timeout (a Cisco's exec-timeout) doesn't see ssh's keep-alives and logs the session
        out. KiTTY's escapes: \xHH a character by its hex code (\x05 is Ctrl+E, the end of the line on a Cisco, which
        shows nothing), \kHH a key, \r, \n, \t, \h (backspace) and \\; a \ at the end only ends it. Not while a
        script runs in the tab, nor in a local shell, cmd or PowerShell tab: only sessions to another device.</p>
      <label>{@render caption("Older algorithms", "olderAlgorithms")}
        <select value={triToValue(draft.olderAlgorithms)} onchange={(e) => (draft.olderAlgorithms = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="on">Allow (older Cisco and similar devices)</option>
          <option value="off">Don't allow</option>
        </select></label>
      <label>{@render caption("Server load", "serverLoad")}
        <select value={triToValue(isDefault ? (draft.serverLoad ?? false) : draft.serverLoad)}
          onchange={(e) => (draft.serverLoad = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="on">Show in the status bar (Linux servers)</option>
          <option value="off">Don't show</option>
        </select></label>
      <p class="note under">The server's CPU, memory, root disk and network traffic, every 3 seconds, while one of this
        profile's tabs is in front, read with cat and df: nothing on the server changes. {#if isMac}Read over the tab's
        own connection, with no second login.{:else}Read over a second login of its own (Windows' OpenSSH can't share the
        tab's), which needs a key, a key file or ssh-agent: a server that asks for a password gives none.{/if} Linux
        only: leave it off for Cisco and other network devices, which would log the attempt as a command, and for
        sessions through CyberArk. Not for a Log in here profile.</p>
      <label>{@render caption("SSH algorithms", "algorithms")}
        <button onclick={() => (algorithming = true)} title={draft.algorithms ? `Its own: ${draft.algorithms}` : isDefault ? "ssh's own lists" : FROM}>
          Choose…</button>
        <span class="note">{draft.algorithms ? "its own lists" : isDefault ? "ssh's own lists" : "from Default Settings"}</span></label>
      <label>{@render caption("SSH config host", "sshConfigHost")}<input bind:value={draft.sshConfigHost} placeholder={`a Host in ${SSH_CONFIG}`} /></label>
      <label>{@render caption("Extra ssh arguments", "sshOptions")}<input bind:value={draft.sshOptions} placeholder="ssh's own, e.g. -o Compression=yes"
        title="Passed to ssh as written, before a session's own Extra arguments (which are PuTTY's, translated)" /></label>
      {#each [["agentForwarding", "Agent forwarding", "Forward ssh-agent (-A): a login from there onwards uses your keys"], ["x11Forwarding", "X11 forwarding", "Forward X11 (-X): needs an X server on " + THIS_COMPUTER], ["compression", "Compression", "Compress (-C): for a slow link"]] as [field, label, on] (field)}
        {@const key = field as "agentForwarding" | "x11Forwarding" | "compression"}
        <label>{@render caption(label, key)}
          <select value={triToValue(isDefault ? (draft[key] ?? false) : draft[key])} onchange={(e) => (draft[key] = valueToTri(e.currentTarget.value))}>
            {#if !isDefault}<option value="">{FROM}</option>{/if}
            <option value="on">{on}</option>
            <option value="off">Off</option>
          </select></label>
      {/each}
      <div class="tunnels">
        <span class="caption">{@render caption("Tunnels", "forwards")}</span>
        {#if draft.forwards.length}
          <!-- off keeps them, for later: the session opens none -->
          <label class="check tunnels-on"><input type="checkbox" checked={draft.tunnelsOff !== true}
              onchange={(e) => (draft.tunnelsOff = e.currentTarget.checked ? null : true)} />
            <span>Open these tunnels: {draft.tunnelsOff === true
              ? "off, kept here, but the profile's sessions open none until it's ticked again"
              : "each tab of the profile's sessions opens them"}</span></label>
          <!-- the columns' names: which side each port is on changes with the type, as the hints say -->
          <div class="tunnel head" aria-hidden="true"><span>Type</span><span>Listens on</span><span>Goes to</span></div>
        {/if}
        {#each draft.forwards as forward, i (i)}
          <div class="tunnel" class:off={draft.tunnelsOff === true}>
            <select aria-label="Tunnel {i + 1} kind" value={forward.kind} onchange={(e) => setForward(i, { kind: e.currentTarget.value as Forward["kind"] })}>
              <option value="local">Local (-L)</option>
              <option value="remote">Remote (-R)</option>
              <option value="dynamic">Dynamic, SOCKS (-D)</option>
            </select>
            <input aria-label="Tunnel {i + 1} port" value={forward.listen} placeholder={forward.kind === "remote" ? "a port on the server" : `a port on ${THIS_COMPUTER}`}
              title={forward.kind === "remote" ? "The port it listens on, on the server" : `The port it listens on, on ${THIS_COMPUTER} (address:port for another address)`}
              oninput={(e) => setForward(i, { listen: e.currentTarget.value })} />
            <input aria-label="Tunnel {i + 1} to" value={forward.to} disabled={forward.kind === "dynamic"}
              placeholder={forward.kind === "dynamic" ? "anywhere asked" : forward.kind === "remote" ? `host:port, as ${THIS_COMPUTER} sees it` : "host:port, as the server sees it"}
              oninput={(e) => setForward(i, { to: e.currentTarget.value })} />
            <button aria-label="Remove tunnel {i + 1}" onclick={() => (draft.forwards = draft.forwards.filter((_, j) => j !== i))}>✕</button>
          </div>
        {/each}
        <div class="tunnel-buttons">
          <button onclick={() => (draft.forwards = [...draft.forwards, { kind: "local", listen: "", to: "" }])}>Add tunnel</button>
          <button onclick={showSamples}>Tunnel Samples…</button>
        </div>
      </div>
      <div class="note under tunnels-help" role="note" aria-label="About tunnels">
        <p>A tunnel reaches something through the SSH connection, as if you were on the server: for what the server can
          reach and {THIS_COMPUTER} can't. Each type, in the columns above:</p>
        <!-- the same columns as the tunnels, each type with an example row and what it does -->
        <table class="tunnel-kinds">
          <thead><tr><th>Type</th><th>Listens on</th><th>Goes to</th><th>Then</th></tr></thead>
          <tbody>
            <tr><td>Local (-L)</td><td>a port on {THIS_COMPUTER}: 8443</td><td>host:port as the server sees it: 10.0.0.5:443</td>
              <td>https://localhost:8443 here opens the device's web page</td></tr>
            <tr><td>Remote (-R)</td><td>a port on the server: 9090</td><td>host:port as {THIS_COMPUTER} sees it: localhost:8000</td>
              <td>the server's localhost:9090 reaches a web server here</td></tr>
            <tr><td>Dynamic, SOCKS (-D)</td><td>a port on {THIS_COMPUTER}: 1080</td><td>nothing: anywhere it's asked</td>
              <td>a browser set to SOCKS proxy localhost:1080 browses from the server's side</td></tr>
          </tbody>
        </table>
        <p>Local is the usual one; Tunnel Samples… has more. Each tab of the profile's sessions opens them, lists them at
          its top with links, and its tunnel icon shows how to test each. Changes take effect when the session restarts.
          The server must allow TCP forwarding (AllowTcpForwarding in its sshd_config, or its SSH settings).</p>
        <p>This profile's own {isDefault ? "(for sessions with no profile of their own)" : "(or the profile it's based on's, never Default Settings')"}.
          A port on {THIS_COMPUTER} can be only one tab's: a second tab of the profile's sessions connects without the
          Local and SOCKS tunnels, and says which tab has them. File Transfer leaves them to the tab.</p>
      </div>
    </fieldset>

    <fieldset>
      <legend>Terminal</legend>
      <label>{@render caption("Cursor", "cursor")}
        <select value={isDefault ? draft.cursor || "block" : draft.cursor} onchange={(e) => (draft.cursor = e.currentTarget.value)}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="block">Block</option>
          <option value="underline">Underline</option>
          <option value="bar">Vertical line</option>
        </select></label>
      <label>{@render caption("Cursor blinks", "cursorBlink")}
        <select value={triToValue(isDefault ? (draft.cursorBlink ?? true) : draft.cursorBlink)}
          onchange={(e) => (draft.cursorBlink = valueToTri(e.currentTarget.value))}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="on">Yes</option>
          <option value="off">No</option>
        </select></label>
      <label>{@render caption("Bell", "bell")}
        <select value={isDefault ? draft.bell || "visual" : draft.bell} onchange={(e) => (draft.bell = e.currentTarget.value)}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="visual">Flash</option>
          <option value="sound">Sound</option>
          <option value="none">None</option>
        </select></label>
      <label>{@render caption("Font", "fontFamily")}<input bind:value={draft.fontFamily} list="profile-fonts"
        placeholder={isDefault || !defaultProfile?.fontFamily ? "the theme's; or an installed font" : `${FROM} (${defaultProfile.fontFamily})`} /></label>
      <datalist id="profile-fonts">{#each MONOSPACE_FONTS as font (font)}<option value={font}></option>{/each}</datalist>
      <label>{@render caption("Font size", "fontSize")}<input class="short" type="number" min="0" max={MAX_FONT_POINTS} bind:value={draft.fontSize} />
        <span class="note">{isDefault || !defaultProfile?.fontSize ? "points (0: the theme's)" : `points (0: ${FALLBACK}', ${defaultProfile.fontSize})`}</span></label>
      <p class="note under">As PuTTY's Window > Appearance > Font. Its tabs draw in it while Settings > Terminal > Font is
        The profile's (the default); a font or size chosen there, or a size from Ctrl+wheel, wins over it.</p>
      <label>{@render caption("Backspace sends", "keyBackspace")}
        <select value={isDefault ? draft.keyBackspace || "del" : draft.keyBackspace} onchange={(e) => (draft.keyBackspace = e.currentTarget.value)}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="del">Control-? (127, PuTTY's default)</option>
          <option value="bs">Control-H (8)</option>
        </select></label>
      <label>{@render caption("Home and End", "keyHomeEnd")}
        <select value={isDefault ? draft.keyHomeEnd || "standard" : draft.keyHomeEnd} onchange={(e) => (draft.keyHomeEnd = e.currentTarget.value)}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="standard">Standard (ESC[1~ and ESC[4~)</option>
          <option value="rxvt">rxvt</option>
        </select></label>
      <label>{@render caption("Function keys", "keyFunction")}
        <select value={isDefault ? draft.keyFunction || "tilde" : draft.keyFunction} onchange={(e) => (draft.keyFunction = e.currentTarget.value)}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="tilde">ESC[n~ (PuTTY's default)</option>
          <option value="linux">Linux console</option>
          <option value="xtermr6">Xterm R6</option>
          <option value="vt100">VT100+</option>
          <option value="sco">SCO</option>
        </select></label>
      <p class="note under">As PuTTY's Terminal > Keyboard, for a device that reads the keys another way: an older Unix
        or switch that deletes with Control-H shows ^? when Backspace is pressed. Settings > Terminal > Keys set to xterm's
        turns these off for every tab; a local shell always has xterm's.</p>
      <label>{@render caption("Scrollback", "scrollback")}<input class="short" type="number" min="0" step="1000" bind:value={draft.scrollback} />
        <span class="note">{isDefault || !defaultProfile?.scrollback ? "lines (0: Settings > Terminal's)" : `lines (0: ${FALLBACK}', ${defaultProfile.scrollback})`}</span></label>
      <label>{@render caption("Session log", "sessionLog")}
        <select value={triToValue(draft.sessionLog)} onchange={(e) => (draft.sessionLog = valueToTri(e.currentTarget.value))}>
          <option value="">{isDefault ? "As Settings > General > Session logs says" : FROM}</option>
          <option value="on">Keep a log of each session</option>
          <option value="off">Don't keep logs</option>
        </select></label>
      <p class="note under">More scrollback for a core router's long show commands than a lab box's; a log kept for every
        session to production devices. Kept logs go to Settings > General > Session logs' folder; otherwise a tab's log
        is deleted when it closes.</p>
      <label>{@render caption("Character set", "charset")}
        <select value={isDefault ? draft.charset || "utf8" : draft.charset} onchange={(e) => (draft.charset = e.currentTarget.value === "utf8" && isDefault ? "" : e.currentTarget.value)}>
          {#if !isDefault}<option value="">{FROM}</option>{/if}
          <option value="utf8">UTF-8</option>
          <option value="latin1">ISO-8859-1 (Latin-1)</option>
          <option value="latin9">ISO-8859-15 (Latin-9, with €)</option>
          <option value="windows1252">Windows-1252</option>
          <option value="cp437">CP437 (DOS, line drawing)</option>
        </select></label>
      <p class="note under">As PuTTY's Remote character set, for a device that doesn't send UTF-8: its accents, or its
        boxes and lines, show as � or stray letters until this matches it. What's typed is sent in it too.
        {#if !isMac}Telnet, Raw and Serial sessions only on Windows: for SSH, Windows' console turns the device's
        characters into UTF-8 before SuperXTerminal sees them.{/if}</p>
      <label>{@render caption("Colours", "colours")}
        <select value={shownChoice(draft)} onchange={(e) => (draft.colours = e.currentTarget.value)}>
          {#if !isDefault}<option value="default">{FROM} ({SHORT[coloursChoice(defaultProfile, undefined)]})</option>{/if}
          {#each Object.entries(CHOICES) as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select></label>
      {#each COLOURS as c (c.field)}
        {@const off = choice !== "custom"}
        <label class:off>{@render caption(c.label, c.field)}
          <span class="colour">
            <input type="color" value={off ? fallbackColour(c.field) : draft[c.field] || fallbackColour(c.field)} aria-label={c.label} disabled={off}
              oninput={(e) => (draft[c.field] = e.currentTarget.value)} />
            <span class="hint">{off ? (choice === "theme" ? "the theme's" : `the ${CHOICES[choice]} theme's`) : draft[c.field] || (isDefault ? "Settings > Terminal's" : defaultProfile?.[c.field] ? `${FROM} (${defaultProfile[c.field]})` : FROM)}</span>
            <button disabled={off || !draft[c.field]} onclick={(e) => { e.preventDefault(); draft[c.field] = ""; }}
              title={isDefault ? "Use Settings > Terminal's colour" : `Use ${FALLBACK}' colour`}>{isDefault ? "Settings'" : "Default's"}</button>
          </span></label>
      {/each}
      <!-- not a terminal colour, so it can be set whatever the Colours choice; an environment colour wins over it -->
      <label>{@render caption("Tab colour", "tabColor")}
        <span class="colour">
          <input type="color" value={draft.tabColor || (!isDefault && defaultProfile?.tabColor) || NO_TAB_COLOUR} aria-label="Tab colour"
            oninput={(e) => (draft.tabColor = e.currentTarget.value)} />
          <span class="hint">{draft.tabColor || (!isDefault && defaultProfile?.tabColor ? `${FROM} (${defaultProfile.tabColor})` : "None")}</span>
          <button disabled={!draft.tabColor} onclick={(e) => { e.preventDefault(); draft.tabColor = ""; }}
            title={isDefault ? "No tab colour" : `Use ${FALLBACK}' tab colour`}>{isDefault ? "None" : "Default's"}</button>
        </span></label>
      <!-- the bar shown while one of the profile's tabs is active; a bar picked by hand still wins until the tab changes -->
      <label>{@render caption("Button bar", "buttonBar")}
        <select value={draft.buttonBar} onchange={(e) => (draft.buttonBar = e.currentTarget.value)}>
          <option value="">{isDefault || !defaultProfile?.buttonBar ? "The one for the tab's icon" : `${FROM} (${defaultProfile.buttonBar})`}</option>
          {#each barNames as name (name)}<option value={name}>{name}</option>{/each}
          {#if missingBar}<option value={missingBar}>{missingBar} (not in the button bars)</option>{/if}
        </select></label>
      <label>{@render caption("Login script", "script")}<input bind:value={draft.script}
          placeholder={isDefault ? "none; a file, or an http(s) address" : defaultProfile?.script ? `${FROM} (${defaultProfile.script})` : "none; a file, or an http(s) address"} />
        <button onclick={chooseScript}>Choose…</button></label>
      {#if isFullPath(draft.script)}<div class="note under"><ScriptPathNote spec={draft.script} onCopied={(spec) => (draft.script = spec)} /></div>{/if}
      <p class="note under">Run in each of the profile's tabs as it opens, when the session has no login script of its
        own (Edit Session > Terminal), so a session can be just a name and a host. SPSL, starting with #!/bin/spsl; or
        Rhai, starting with #!/bin/rhai or named .rhai. It starts as the connection does: begin with WAITPROMPT (Rhai:
        wait_prompt()) to run once the login is done. A path that isn't
        absolute is in the settings folder.{#if isDefault} Default Settings' runs for every session without one.{/if}</p>
    </fieldset>

    </div>
  </div>
</div>

<!-- a field's caption, with a dot while it differs from the saved profile -->
{#snippet caption(text: string, field: Field)}
  <span class="caption" class:revealed={revealed === field} data-field={field}>{text}{#if changed(field)}<span class="dot" title="Changed: not saved yet" aria-label="changed"></span>{/if}</span>
{/snippet}

{#if menu}
  <Menu items={menu.items} x={menu.x} y={menu.y} onClose={() => (menu = null)} />
{/if}
<svelte:window onclick={() => (menu = null)} />

{#if algorithming}
  <AlgorithmsDialog title={`SSH algorithms: ${draft.name || "new profile"}`} text={draft.algorithms ?? ""}
    otherwise={isDefault ? "ssh's own list (with the older algorithms, if allowed)" : `${FALLBACK}' list`}
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

  /* the guidance for a profile being set up from a team template, over its fields */
  .from-template {
    margin: 0;
    padding: 6px 8px;
    border-radius: 4px;
    background: var(--bg-input);
    border-left: 3px solid var(--accent);
  }

  /* the team's templates not set up here: under a heading, in italics until set up */
  li.group {
    margin-top: 8px;
    color: var(--text-muted);
    font-size: 12px;
  }

  li.template {
    font-style: italic;
    color: var(--text-muted);
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

  /* the field Edit Session's link was for: lit up, then back */
  .caption.revealed {
    color: var(--accent);
    font-weight: 600;
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

  /* the tunnels' explanation: a paragraph for each kind */
  .tunnels-help p {
    margin: 0 0 4px;
  }

  /* each type of tunnel in the tunnels' own columns, with an example */
  .tunnel-kinds {
    border-collapse: collapse;
    margin: 2px 0 6px;
  }

  .tunnel-kinds th,
  .tunnel-kinds td {
    padding: 2px 8px 2px 0;
    text-align: left;
    vertical-align: top;
  }

  .tunnel-kinds th {
    font-weight: normal;
    border-bottom: 1px solid var(--border);
  }

  .tunnel-kinds td:first-child {
    white-space: nowrap;
    color: var(--text);
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

  /* Tunnels: its caption in the labels' column, the rows and Add tunnel in the field's */
  .tunnels {
    display: grid;
    grid-template-columns: 130px 1fr;
    gap: 6px 8px;
    align-items: center;
  }

  .tunnels > :not(.caption) {
    grid-column: 2;
  }


  .tunnel {
    display: grid;
    grid-template-columns: 150px 1fr 1fr auto;
    gap: 6px;
  }

  /* the columns' names, over the type, port and destination (the remove button's column left empty) */
  .tunnel.head {
    grid-template-columns: 150px 1fr 1fr 30px;
    font-size: 12px;
    color: var(--text-muted);
  }

  /* the tunnels turned off: still there to edit, shown as not in use */
  .tunnel.off {
    opacity: 0.55;
  }

  .tunnel-buttons {
    display: flex;
    gap: 8px;
    justify-self: start;
  }

  /* a checkbox with what it does beside it, in the field's column */
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    grid-column: 2 / -1;
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
