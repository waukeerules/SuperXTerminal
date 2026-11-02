/**
 * Paste check, as SuperXPuTTY's TerminalControl and dlgPasteCheck: a paste of "pasteLines" lines or more (2 by default;
 * 0 never) is shown first, in a window where it can be changed, and typed only on Paste.
 */
import { pref } from "./prefs";

/** The paste being checked, if any (PasteCheck.svelte shows it) */
export const pasting = $state<{ current: { text: string; session: string; resolve: (text: string | null) => void } | null }>({
  current: null,
});

/** How many lines a paste is, as PasteLineCount: CR LF and CR are line breaks, one at the end doesn't count */
export function pasteLineCount(text: string): number {
  const lines = text.replace(/\r\n?/g, "\n").replace(/\n$/, "");
  return Math.max(1, lines.split("\n").length);
}

/** Whether a paste is checked first (ShouldCheckPaste) */
export function shouldCheckPaste(text: string): boolean {
  const threshold = parseInt(pref("pasteLines"), 10) || 0;
  return threshold > 0 && pasteLineCount(text) >= threshold;
}

/** The text to paste into `session`: as it is, as changed in the window, or null if cancelled */
export function checkPaste(text: string, session: string): Promise<string | null> {
  if (!shouldCheckPaste(text)) return Promise.resolve(text);
  return new Promise((resolve) => {
    const show = () => {
      // one at a time, as the window is modal
      if (pasting.current) {
        setTimeout(show, 100);
        return;
      }
      pasting.current = { text, session, resolve };
    };
    show();
  });
}
