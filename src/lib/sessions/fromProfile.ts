/**
 * What a session's profile adds to it when it opens, for Edit Session's lists under its Connection, SSH and Terminal
 * tabs ("From profile "Web Tunnels", when the session opens"). None of it shows among the session's own fields, so a
 * session could look plain when its profile gives it tunnels, a font, a character set or a login script. Each row is
 * a setting the profile (or the profile it's based on) sets, else Default Settings', in words, with the field's name
 * in Profile, for the row's Edit… link to Settings > Profiles scrolled to it. Only what differs from the app's own
 * defaults is listed, so a profile that changes nothing lists nothing. Mirrors how a launch takes each one
 * (src-tauri/src/launch.rs).
 */
import { DEFAULT_PROFILE, resolvedProfile, type Profile } from "../settings/profiles";
import { completeTunnels, describeTunnel } from "../terminal/tunnels";
import { isReference, referencedItem } from "../settings/onepassword";

/** One setting the profile gives the session */
export interface ProfileRow {
  /** Its name, as Settings > Profiles labels it */
  label: string;
  /** Its value, in words */
  value: string;
  /** The profile it's set in: the session's, or Default Settings */
  whose: string;
  /** The field's name in Profile, for Settings > Profiles to scroll to */
  field: keyof Profile;
}

/** Which of Edit Session's tabs a list is for */
export type ProfileTab = "connection" | "ssh" | "terminal";

/** What the session sets itself, which wins over its profile's (so that row isn't listed) */
export interface SessionOwn {
  /** A jump host of its own, or its folders' */
  jumpHost: boolean;
  /** Its own SSH algorithm lists */
  algorithms: boolean;
  /** Its own button bar */
  buttonBar: boolean;
  /** Its own login script */
  script: boolean;
}

/** The character sets' names, as Settings > Profiles lists them */
const CHARSETS: Record<string, string> = { latin1: "ISO-8859-1 (Latin-1)", latin9: "ISO-8859-15 (Latin-9)", windows1252: "Windows-1252", cp437: "CP437 (DOS, line drawing)" };
/** The F1-F12 styles' names */
const FUNCTION_KEYS: Record<string, string> = { linux: "Linux", xtermr6: "Xterm R6", vt100: "VT100+", sco: "SCO" };
/** The Colours choices' names */
const COLOURS: Record<string, string> = { dark: "Dark", light: "Light", custom: "Custom" };

/**
 * The rows for one of Edit Session's tabs
 * @param tab The tab
 * @param profiles Every profile, to follow Based on and fall back to Default Settings
 * @param name The profile the session opens with
 * @param own What the session sets itself
 */
export function profileRows(tab: ProfileTab, profiles: Profile[], name: string, own: SessionOwn): ProfileRow[] {
  const profile = resolvedProfile(profiles, name) ?? profiles.find((p) => p.name === DEFAULT_PROFILE);
  const fallback = profiles.find((p) => p.name === DEFAULT_PROFILE);
  const whose = profile?.name ?? DEFAULT_PROFILE;
  const rows: ProfileRow[] = [];
  /**
   * A setting the profile sets, else Default Settings': `show` gives its words, "" for the app's own default
   * (or unset), which isn't listed
   */
  const add = <K extends keyof Profile>(label: string, field: K, show: (value: Profile[K]) => string) => {
    const mine = profile ? show(profile[field]) : "";
    // unset: null, "" or 0, or not there at all (a profile saved before the field was added)
    const value = profile?.[field];
    const set = !(value == null || value === "" || value === 0);
    if (mine) rows.push({ label, value: mine, whose, field });
    else if (!set && fallback && fallback !== profile) {
      const theirs = show(fallback[field]);
      if (theirs) rows.push({ label, value: theirs, whose: DEFAULT_PROFILE, field });
    }
  };
  const on = (v: unknown) => (v === true ? "on" : "");
  const text = (v: unknown) => (typeof v === "string" ? v.trim() : "");

  if (tab === "connection") {
    // a profile's own tunnels only (Default Settings' for its own sessions): every session would clash on them
    const tunnels = completeTunnels(profile?.forwards ?? []).map((t) => (t.kind === "dynamic" ? `SOCKS ${t.listen}` : `${describeTunnel(t).kind} ${t.listen} → ${t.to}`));
    // turned off, they're kept: said so, as a session's tunnels don't open
    const off = profile?.tunnelsOff === true;
    if (tunnels.length) rows.push({ label: "Tunnels", value: off ? `off (kept: ${tunnels.join("; ")})` : tunnels.join("; "), whose, field: "forwards" });
    add("Agent forwarding", "agentForwarding", on);
    add("X11 forwarding", "x11Forwarding", on);
    add("Compression", "compression", on);
    add("Keep-alive", "keepalive", (v) => (v ? `every ${v} s` : ""));
    add("Extra ssh arguments", "sshOptions", text);
    add("SSH config host", "sshConfigHost", text);
    if (!own.jumpHost) add("Jump host", "jumpHost", text);
  } else if (tab === "ssh") {
    add("Key file", "keyFile", (v) => (isReference(text(v)) ? `${referencedItem(text(v))} in 1Password, through its SSH agent` : text(v)));
    add("Offer SSH keys", "offerKeys", (v) => (v === false ? "off: only the key file and password" : ""));
    add("Ask for a username", "askUsername", (v) => (v === true ? 'on: "login as:" in the tab' : ""));
    add("Allow older algorithms", "olderAlgorithms", (v) => (v === true ? "on" : ""));
    if (!own.algorithms) add("SSH algorithms", "algorithms", (v) => (text(v) ? "lists of its own" : ""));
    add("Internet protocol", "addressFamily", (v) => (v === "ipv4" ? "IPv4" : v === "ipv6" ? "IPv6" : ""));
    add("Show server load", "serverLoad", on);
  } else {
    add("Font", "fontFamily", text);
    add("Font size", "fontSize", (v) => (v ? `${v} pt` : ""));
    add("Colours", "colours", (v) => COLOURS[text(v)] ?? "");
    add("Tab colour", "tabColor", text);
    add("Cursor", "cursor", (v) => (v === "underline" ? "Underline" : v === "bar" ? "Vertical line" : ""));
    add("Cursor blinks", "cursorBlink", (v) => (v === false ? "no" : ""));
    add("Bell", "bell", (v) => (v === "sound" ? "Sound" : v === "none" ? "None" : ""));
    add("Backspace sends", "keyBackspace", (v) => (v === "bs" ? "Control-H" : ""));
    add("Home and End", "keyHomeEnd", (v) => (v === "rxvt" ? "rxvt" : ""));
    add("Function keys", "keyFunction", (v) => FUNCTION_KEYS[text(v)] ?? "");
    add("Character set", "charset", (v) => CHARSETS[text(v)] ?? "");
    add("Scrollback", "scrollback", (v) => (v ? `${v} lines` : ""));
    add("Session log", "sessionLog", (v) => (v === true ? "kept" : v === false ? "deleted when the tab closes" : ""));
    add("Anti-idle string", "antiIdle", text);
    if (!own.buttonBar) add("Button bar", "buttonBar", text);
    if (!own.script) add("Login script", "script", text);
  }
  return rows;
}
