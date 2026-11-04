/**
 * The anti-idle string, as KiTTY's: typed into a session after a quiet spell, so a device's or shell's own idle
 * timeout (Cisco's exec-timeout, a shell's TMOUT), which ssh's keep-alives don't reach, doesn't log it out. The
 * profile keeps it as KiTTY writes it (Settings > Profiles, Anti-idle string); `antiIdleKeys` turns its escapes into
 * the keys typed, and PageHost sends them once nothing has been typed for the profile's seconds.
 */

/**
 * The keys an anti-idle string types, after KiTTY's SendKeyboardPlus: `\xHH` the character HH (hex), `\kHH` the key
 * HH (a Windows virtual-key code; the ones that are characters, as \k08 backspace or \k1B Escape), `\r`, `\n`, `\t`,
 * `\h` (backspace) and `\\`. A `\` at the end only ends the string (KiTTY's own examples end so, as `\x05\`), and any
 * other is typed as it is. KiTTY's pauses (\p, \sNN) and Ctrl+Break (\c) have no place in a string sent while idle,
 * so they're left out.
 * @param text The string as the profile keeps it
 * @returns What to type; "" for nothing
 */
export function antiIdleKeys(text: string): string {
  let out = "";
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c !== "\\") {
      out += c;
      continue;
    }
    const next = text[i + 1];
    if (next === undefined) break;
    const hex = /^[0-9a-f]{2}$/i.test(text.slice(i + 2, i + 4)) ? parseInt(text.slice(i + 2, i + 4), 16) : null;
    if ((next === "x" || next === "k") && hex !== null) {
      out += String.fromCharCode(hex);
      i += 3;
    } else if (next === "r" || next === "n" || next === "t" || next === "h" || next === "\\") {
      out += { r: "\r", n: "\n", t: "\t", h: "\b", "\\": "\\" }[next];
      i += 1;
    } else if (next === "p" || next === "c") {
      i += 1;
    } else if (next === "s") {
      i += /^\d{2}$/.test(text.slice(i + 2, i + 4)) ? 3 : 1;
    } else {
      out += c;
    }
  }
  return out;
}
