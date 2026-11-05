/**
 * SPSL's SENDKEY and what a terminal sends for a key, as SuperXPuTTY's SendKeyCommand and TerminalKeys: Enter is CR,
 * arrows are ESC [ A..D, Ctrl+letter and Ctrl+[ \ ] @ ^ _ are their control characters (Ctrl+] is 0x1D, Telnet's
 * escape), a character is itself, Alt puts ESC in front, and Shift, Alt or Ctrl with an arrow, Home, End or a function
 * key is xterm's modified sequence (Ctrl+Up is ESC [ 1 ; 5 A). Windows looks characters up on the keyboard layout;
 * here the US layout stands in, which gives the same answers for the keys scripts use.
 */

/** A key press: a named key ("ENTER") or a character ("c"), with modifiers */
export interface KeyPress {
  name: string | null;
  char: string | null;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
}

/** The key names SENDKEY understands, with what each is (aliases share one), as SendKeyCommand's table */
const NAMES: Record<string, string> = {
  ENTER: "ENTER", TAB: "TAB", ESC: "ESC", ESCAPE: "ESC", HOME: "HOME", END: "END", LEFT: "LEFT", RIGHT: "RIGHT",
  UP: "UP", DOWN: "DOWN", PGUP: "PGUP", PGDN: "PGDN", NUMLOCK: "NUMLOCK", SCROLLLOCK: "SCROLLLOCK", PRTSC: "PRTSC",
  BREAK: "BREAK", BACKSPACE: "BACKSPACE", BKSP: "BACKSPACE", BS: "BACKSPACE", CLEAR: "CLEAR", CAPSLOCK: "CAPSLOCK",
  INS: "INSERT", INSERT: "INSERT", DEL: "DELETE", DELETE: "DELETE", HELP: "HELP",
  F1: "F1", F2: "F2", F3: "F3", F4: "F4", F5: "F5", F6: "F6", F7: "F7", F8: "F8", F9: "F9", F10: "F10", F11: "F11",
  F12: "F12", F13: "F13", F14: "F14", F15: "F15", F16: "F16",
  MULTIPLY: "*", ADD: "+", SUBTRACT: "-", DIVIDE: "/", "+": "+", "%": "%", "^": "^",
  // SuperXTerminal's own: a lone space can't be written after SENDCHAR (the line's end is trimmed), and --More--
  // pages on with Space
  SPACE: " ",
};

/** Every SENDKEY key name, for the reference and the checker */
export const SEND_KEY_NAMES = Object.keys(NAMES);

/** Whether SENDKEY understands a key name */
export function isKeyName(name: string): boolean {
  return name.toUpperCase() in NAMES;
}

/** The US layout's shifted characters and the key under each */
const SHIFTED: Record<string, string> = {
  "!": "1", "@": "2", "#": "3", $: "4", "%": "5", "^": "6", "&": "7", "*": "8", "(": "9", ")": "0", _: "-", "+": "=",
  "{": "[", "}": "]", "|": "\\", ":": ";", '"': "'", "<": ",", ">": ".", "?": "/", "~": "`",
};

/** Whether a key on the keyboard types a character (SENDKEY of a single character) */
export function canTypeKey(ch: string): boolean {
  return /^[ -~]$/.test(ch);
}

/** The key for a single character in SENDKEY: itself, with the Shift it needs */
function characterKey(ch: string, modifiers: Omit<KeyPress, "name" | "char">): KeyPress {
  if (!canTypeKey(ch)) throw new Error(`SENDKEY: no key on this keyboard types "${ch}"; use SENDCHAR to type it`);
  // a letter is its key, so it types lower case unless + adds Shift, as on Windows
  if (/^[a-zA-Z]$/.test(ch)) return { name: null, char: modifiers.shift ? ch.toUpperCase() : ch.toLowerCase(), ...modifiers };
  return { name: null, char: ch, ...modifiers, shift: modifiers.shift || ch in SHIFTED };
}

/** A key name's press: names whose key is a character ("+", "MULTIPLY") are that character */
function namedKey(name: string, modifiers: Omit<KeyPress, "name" | "char">): KeyPress {
  const key = NAMES[name.toUpperCase()];
  return key.length === 1 ? characterKey(key, modifiers) : { name: key, char: null, ...modifiers };
}

/**
 * Parse a SENDKEY argument: a key name ("ENTER", "^HOME"), a character ("c", "^]"), or a key in braces with a repeat
 * count ("{TAB 3}"), each after optional ^ (Ctrl), % (Alt) and + (Shift). Null if there's no key.
 */
export function parseSendKey(arg: string): { key: KeyPress; repeat: number } | null {
  const text = arg.trim();
  let i = 0;
  const mods = { ctrl: false, alt: false, shift: false };
  // a bare key name after modifiers: "ENTER", "^HOME"
  while (i < text.length && "+^%".includes(text[i])) {
    if (text[i] === "+") mods.shift = true;
    else if (text[i] === "^") mods.ctrl = true;
    else mods.alt = true;
    i++;
  }
  const rest = text.slice(i);
  if (rest.length >= 2 && !rest.startsWith("{") && isKeyName(rest)) return { key: namedKey(rest, mods), repeat: 1 };
  // modifiers alone press nothing; {+} or ADD presses the + key
  if (rest.length === 0) return null;
  if (rest[0] === "}") throw new Error("Detected '}' before any '{' was found.");
  if (rest[0] !== "{") return { key: characterKey(rest[0], mods), repeat: 1 };
  // {NAME} or {NAME count}; "{} n}" is the } key n times
  const braced = /^\{(\}|[^}\s]+)(?:\s+(\d+))?\}/.exec(rest);
  if (!braced) throw new Error(`SENDKEY ${arg}: a key in braces is written {NAME} or {NAME count}, e.g. {TAB 3}`);
  const name = braced[1];
  const repeat = Math.max(1, braced[2] ? parseInt(braced[2], 10) : 1);
  if (isKeyName(name)) return { key: namedKey(name, mods), repeat };
  if (name.length === 1) return { key: characterKey(name, mods), repeat };
  throw new Error(`SENDKEY presses one key; "${name}" isn't a key name`);
}

/** xterm's modifier number: 1 plus 1 for Shift, 2 for Alt, 4 for Ctrl */
function modifierNumber(key: KeyPress): number {
  return 1 + (key.shift ? 1 : 0) + (key.alt ? 2 : 0) + (key.ctrl ? 4 : 0);
}

/** An arrow, Home or End */
function cursor(letter: string, modifier: number): string {
  return modifier > 1 ? `\x1b[1;${modifier}${letter}` : `\x1b[${letter}`;
}

/** An editing or function key: ESC [ number ~ */
function tilde(n: number, modifier: number): string {
  return `\x1b[${n}${modifier > 1 ? `;${modifier}` : ""}~`;
}

/** The sequence for a named key, as TerminalKeys.NamedKey */
function namedSequence(name: string, shift: boolean, modifier: number): string {
  const f14 = ["P", "Q", "R", "S"];
  const fTilde: Record<string, number> = { F5: 15, F6: 17, F7: 18, F8: 19, F9: 20, F10: 21, F11: 23, F12: 24 };
  switch (name) {
    case "ENTER": return "\r";
    case "TAB": return shift ? "\x1b[Z" : "\t";
    case "ESC": return "\x1b";
    case "BACKSPACE": return "\x7f";
    case "UP": return cursor("A", modifier);
    case "DOWN": return cursor("B", modifier);
    case "RIGHT": return cursor("C", modifier);
    case "LEFT": return cursor("D", modifier);
    case "HOME": return cursor("H", modifier);
    case "END": return cursor("F", modifier);
    case "INSERT": return tilde(2, modifier);
    case "DELETE": return tilde(3, modifier);
    case "PGUP": return tilde(5, modifier);
    case "PGDN": return tilde(6, modifier);
    case "F1": case "F2": case "F3": case "F4": {
      const letter = f14[Number(name.slice(1)) - 1];
      return modifier > 1 ? `\x1b[1;${modifier}${letter}` : `\x1bO${letter}`;
    }
    case "F13": case "F14": case "F15": case "F16":
      // xterm sends F13-F16 as Shift+F1-F4
      return `\x1b[1;2${f14[Number(name.slice(1)) - 13]}`;
    default:
      if (name in fTilde) return tilde(fTilde[name], modifier);
      // Caps Lock, Print Screen, Break (a Telnet or serial break, which a character can't be)...: nothing
      return "";
  }
}

/** The control character Ctrl gives with a character, as in xterm: Ctrl+C is 0x03, Ctrl+] is 0x1D; null if none */
export function controlCharacter(ch: string): string | null {
  if (/^[a-zA-Z]$/.test(ch)) return String.fromCharCode(ch.toUpperCase().charCodeAt(0) - 64);
  switch (ch) {
    case "@": case "2": case " ": return "\0";
    case "[": case "3": return "\x1b";
    case "\\": case "4": return "\x1c";
    case "]": case "5": return "\x1d";
    case "^": case "6": return "\x1e";
    case "_": case "-": case "/": case "7": return "\x1f";
    case "?": case "8": return "\x7f";
    default: return null;
  }
}

/** What a terminal sends for a key press, as TerminalKeys.Sequence */
export function keySequence(key: KeyPress): string {
  const modifier = modifierNumber(key);
  if (key.name) {
    const named = namedSequence(key.name, key.shift, modifier);
    // keys whose sequence has no modifier form take Alt as an ESC in front
    return key.alt && named.length === 1 ? `\x1b${named}` : named;
  }
  let text = key.char ?? "";
  if (!text) return "";
  if (key.ctrl && text === " ") return (key.alt ? "\x1b" : "") + "\0";
  // the character the key types: a letter with Shift is upper case, otherwise as written
  if (key.ctrl) text = controlCharacter(text) ?? text;
  return key.alt ? `\x1b${text}` : text;
}
