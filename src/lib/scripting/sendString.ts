/**
 * A button's "Send string", as SuperXPuTTY's SendStringParser: runs of text, Enter key presses (\r, or a line break
 * typed in the editor) each followed by 50 ms, and 1-second pauses (\p). Raw CRs are dropped; there's no Enter at the
 * end unless the text has one.
 */

/** The escape codes, for the Edit Button window */
export const SEND_STRING_HELP = "\\r Enter   \\n line feed   \\t Tab   \\b Backspace   \\e Escape   \\p 1-second pause   \\v paste clipboard   \\\\ backslash";

/** One thing to type, then how long to wait */
export interface SendStep {
  text: string;
  delayMs: number;
}

const ENTER_DELAY_MS = 50;
const PAUSE_MS = 1000;

/** The steps for a send string; `clipboard` is read only if \v is used */
export async function parseSendString(text: string, clipboard: () => Promise<string>): Promise<SendStep[]> {
  const steps: SendStep[] = [];
  let pending = "";
  const enter = () => {
    // the built-in terminal types a line feed in text as Enter too (TerminalControl.LineFeedsAsEnter)
    steps.push({ text: pending.replace(/\n/g, "\r") + "\r", delayMs: ENTER_DELAY_MS });
    pending = "";
  };
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "\r") continue;
    if (c === "\n") {
      enter();
      continue;
    }
    if (c !== "\\" || i === text.length - 1) {
      pending += c;
      continue;
    }
    const code = text[++i];
    switch (code) {
      case "r": enter(); break;
      case "p":
        if (pending) steps.push({ text: pending.replace(/\n/g, "\r"), delayMs: 0 });
        pending = "";
        steps.push({ text: "", delayMs: PAUSE_MS });
        break;
      case "v": pending += await clipboard(); break;
      case "n": pending += "\n"; break;
      case "t": pending += "\t"; break;
      case "b": pending += "\b"; break;
      case "e": pending += "\x1b"; break;
      case "\\": pending += "\\"; break;
      default: pending += "\\" + code;
    }
  }
  if (pending) steps.push({ text: pending.replace(/\n/g, "\r"), delayMs: 0 });
  return steps;
}
