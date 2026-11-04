/**
 * A serial session's line settings, kept as PuTTY keeps them: `-sercfg 9600,8,n,1,X` in the session's extra
 * arguments, which plink reads on Windows too. PuTTY's defaults are 9600 baud, 8 data bits, no parity, 1 stop bit
 * and XON/XOFF flow control.
 */

/** A serial line's settings, as PuTTY's -sercfg holds them */
export interface SerialLine {
  speed: number;
  dataBits: number;
  /** n none, o odd, e even, m mark, s space */
  parity: string;
  /** 1, 1.5 or 2 */
  stopBits: string;
  /** N none, X XON/XOFF, R RTS/CTS, D DSR/DTR */
  flow: string;
}

/** PuTTY's line when -sercfg doesn't say: 9600 8N1, XON/XOFF */
export const DEFAULT_LINE: SerialLine = { speed: 9600, dataBits: 8, parity: "n", stopBits: "1", flow: "X" };

/** The speeds offered in the list; any other can be typed */
export const SPEEDS = [300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];
/** The data bits offered */
export const DATA_BITS = [5, 6, 7, 8];
/** The stop bits offered */
export const STOP_BITS = ["1", "1.5", "2"];
/** The parities offered: -sercfg's letter, and its name */
export const PARITIES: [string, string][] = [["n", "None"], ["o", "Odd"], ["e", "Even"], ["m", "Mark"], ["s", "Space"]];
/** The flow controls offered: -sercfg's letter, and its name */
export const FLOWS: [string, string][] = [["N", "None"], ["X", "XON/XOFF"], ["R", "RTS/CTS"], ["D", "DSR/DTR"]];

/** Read a -sercfg value onto a line: settings in any order, as PuTTY reads them */
export function applySercfg(line: SerialLine, cfg: string): SerialLine {
  const out = { ...line };
  for (const part of cfg.split(",").map((p) => p.trim()).filter(Boolean)) {
    if (part === "1" || part === "1.5" || part === "2") out.stopBits = part;
    else if (/^[5-9]$/.test(part)) out.dataBits = Math.min(8, Number(part));
    else if (/^[noems]$/.test(part)) out.parity = part;
    else if (/^[NXRD]$/.test(part)) out.flow = part;
    else if (/^\d+$/.test(part)) out.speed = Number(part);
  }
  return out;
}

/** A line as -sercfg text: "9600,8,n,1,X" */
export function toSercfg(line: SerialLine): string {
  return `${line.speed},${line.dataBits},${line.parity},${line.stopBits},${line.flow}`;
}

/** The words of extra arguments, and where -sercfg's value is (-1 if there's none) */
function sercfgAt(extraArgs: string): { words: string[]; at: number } {
  const words = extraArgs.trim().split(/\s+/).filter(Boolean);
  const at = words.indexOf("-sercfg");
  return { words, at: at >= 0 && at + 1 < words.length ? at + 1 : -1 };
}

/** A session's line, from its extra arguments */
export function lineOf(extraArgs: string): SerialLine {
  const { words, at } = sercfgAt(extraArgs);
  return at < 0 ? { ...DEFAULT_LINE } : applySercfg(DEFAULT_LINE, words[at]);
}

/** Extra arguments with -sercfg set to the line, the other arguments as they were */
export function withLine(extraArgs: string, line: SerialLine): string {
  const { words, at } = sercfgAt(extraArgs);
  if (at >= 0) {
    words[at] = toSercfg(line);
    return words.join(" ");
  }
  const rest = words.filter((w) => w !== "-sercfg");
  return [...rest, "-sercfg", toSercfg(line)].join(" ");
}
