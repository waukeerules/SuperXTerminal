/**
 * Remote Desktop options, as SuperXPuTTY's RdpOptions (rdp_options.rs on the Rust side): "Key=Value;Key=Value" with
 * only the options that are set. Settings > Remote Desktop holds the user's defaults; a session's RdpOptions attribute
 * (Edit Session > RDP options…) holds its own, laid over them. `Retina` is the Mac's own; SuperXPuTTY skips it.
 */

export interface RdpOptions {
  /** "FitTab" or "Fixed" */
  display?: "FitTab" | "Fixed";
  width?: number;
  height?: number;
  colorDepth?: number;
  smartSizing?: boolean;
  clipboard?: boolean;
  drives?: boolean;
  printers?: boolean;
  smartCards?: boolean;
  microphone?: boolean;
  /** 0 play on this computer, 1 on the remote computer, 2 don't play */
  audio?: number;
  /** Windows key combinations: 0 on this computer, 1 on the remote computer, 2 only full screen */
  keyboard?: number;
  /** Server authentication: 0 connect anyway, 1 don't connect, 2 warn */
  authentication?: number;
  gateway?: string;
  console?: boolean;
  /** The desktop at the screen's full Retina resolution */
  retina?: boolean;
}

export type FlagKey = "smartSizing" | "clipboard" | "drives" | "printers" | "smartCards" | "microphone" | "console" | "retina";
export type NumberKey = "colorDepth" | "audio" | "keyboard" | "authentication";

/** Each option's key in the text, in SuperXPuTTY's order */
const KEYS: [keyof RdpOptions, string, "text" | "number" | "flag"][] = [
  ["display", "Display", "text"],
  ["width", "Width", "number"],
  ["height", "Height", "number"],
  ["colorDepth", "ColorDepth", "number"],
  ["smartSizing", "SmartSizing", "flag"],
  ["clipboard", "Clipboard", "flag"],
  ["drives", "Drives", "flag"],
  ["printers", "Printers", "flag"],
  ["smartCards", "SmartCards", "flag"],
  ["microphone", "Microphone", "flag"],
  ["audio", "Audio", "number"],
  ["keyboard", "Keyboard", "number"],
  ["authentication", "Authentication", "number"],
  ["gateway", "Gateway", "text"],
  ["console", "Console", "flag"],
  ["retina", "Retina", "flag"],
];

/** SuperXPuTTY's defaults (its Settings.settings), and Retina off */
export const BUILT_IN_DEFAULTS: Required<RdpOptions> = {
  display: "FitTab", width: 1920, height: 1080, colorDepth: 32, smartSizing: true, clipboard: true, drives: false,
  printers: false, smartCards: false, microphone: false, audio: 0, keyboard: 2, authentication: 2, gateway: "",
  console: false, retina: false,
};

/** Read "Key=Value;Key=Value"; unknown keys and bad values are skipped */
export function parseRdpOptions(text: string): RdpOptions {
  const options: Record<string, unknown> = {};
  for (const pair of (text ?? "").split(";")) {
    const at = pair.indexOf("=");
    if (at <= 0) continue;
    const [key, value] = [pair.slice(0, at).trim(), pair.slice(at + 1).trim()];
    const known = KEYS.find(([, k]) => k === key);
    if (!known) continue;
    const [name, , kind] = known;
    if (name === "display") options.display = value === "Fixed" ? "Fixed" : "FitTab";
    else if (kind === "text") options[name] = value;
    else if (kind === "number") {
      if (/^\d+$/.test(value)) options[name] = Number(value);
    } else if (/^(1|true|yes)$/i.test(value)) options[name] = true;
    else if (/^(0|false|no)$/i.test(value)) options[name] = false;
  }
  return options as RdpOptions;
}

/** Write the options that are set ("" when none is) */
export function formatRdpOptions(options: RdpOptions): string {
  const pairs: string[] = [];
  for (const [name, key] of KEYS) {
    const value = options[name];
    if (value === undefined || value === null) continue;
    const text = typeof value === "boolean" ? (value ? "1" : "0") : String(value);
    // ; and = separate the pairs, so they can't be part of a value
    pairs.push(`${key}=${text.replace(/[;=]/g, "")}`);
  }
  return pairs.join(";");
}

/** `base` with `over`'s set options in their place */
export function overlay(base: RdpOptions, over: RdpOptions): RdpOptions {
  const result: Record<string, unknown> = { ...base };
  for (const [name] of KEYS) if (over[name] !== undefined) result[name] = over[name];
  return result as RdpOptions;
}

/** The user's defaults: SuperXPuTTY's with Settings > Remote Desktop's laid over them */
export function userDefaults(text: string): Required<RdpOptions> {
  return overlay(BUILT_IN_DEFAULTS, parseRdpOptions(text)) as Required<RdpOptions>;
}
