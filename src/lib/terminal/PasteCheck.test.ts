// The paste check (Settings > Terminal > Paste check): a paste of several lines is shown in the Paste window first,
// and typed only on Paste, as it stands in the window's box; Cancel and Escape type nothing. It's what stands between
// a clipboard that turned out to hold a whole configuration and the switch in the tab. Tested as the tab uses it:
// `checkPaste` is what PageHost waits on before it lets the page paste, and the window is the real one, rendered and
// clicked. The terminal's preferences are set straight in the settings store, so nothing of the Rust side is needed.
import { afterEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import PasteCheck from "./PasteCheck.svelte";
import { checkPaste, pasteLineCount, pasting } from "./pasting.svelte";
import { settings } from "../settings/store.svelte";

afterEach(() => {
  pasting.current = null;
  settings.terminalPrefs = {};
});

/** What a paste came to: "waiting" until the window is answered, then the text typed, or null for nothing */
function outcome(paste: Promise<string | null>): () => string | null {
  let result: string | null = "waiting";
  void paste.then((text) => (result = text));
  return () => result;
}

/** The window's box, with the text to be typed */
const box = () => screen.getByRole("textbox") as HTMLTextAreaElement;

describe("A paste of several lines", () => {
  it("is shown first, saying how many lines and which tab, and typed on Paste", async () => {
    const user = userEvent.setup();
    render(PasteCheck);
    const typed = outcome(checkPaste("conf t\r\nint gi0/1\r\nshutdown", "core-sw1"));
    const dialog = await screen.findByRole("dialog", { name: "Paste" });
    expect(dialog.textContent).toContain("Paste 3 lines into core-sw1?");
    expect(box().value).toBe("conf t\nint gi0/1\nshutdown");
    expect(typed()).toBe("waiting");

    await user.click(screen.getByRole("button", { name: "Paste" }));
    await expect.poll(typed).toBe("conf t\r\nint gi0/1\r\nshutdown");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("types nothing on Cancel, nor on Escape", async () => {
    const user = userEvent.setup();
    render(PasteCheck);
    const cancelled = outcome(checkPaste("reload\ny", "core-sw1"));
    await screen.findByRole("dialog");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await expect.poll(cancelled).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();

    const escaped = outcome(checkPaste("reload\ny", "core-sw1"));
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await expect.poll(escaped).toBeNull();
  });

  it("types what's in the box once it's been changed there, and can't paste a box emptied", async () => {
    const user = userEvent.setup();
    render(PasteCheck);
    const typed = outcome(checkPaste("show run\nreload", "core-sw1"));
    await screen.findByRole("dialog");
    await user.clear(box());
    expect((screen.getByRole("button", { name: "Paste" }) as HTMLButtonElement).disabled).toBe(true);
    await user.type(box(), "show run{Enter}show clock");
    expect(screen.getByRole("dialog").textContent).toContain("Paste 2 lines");
    // Ctrl+Return pastes (Cmd+Return on the Mac); Return alone only adds a line
    await user.keyboard("{Control>}{Enter}{/Control}");
    await expect.poll(typed).toBe("show run\r\nshow clock");
  });

  it("waits its turn behind a paste already being checked", async () => {
    const user = userEvent.setup();
    render(PasteCheck);
    const first = outcome(checkPaste("a\nb", "sw1"));
    const second = outcome(checkPaste("c\nd", "sw2"));
    expect((await screen.findByRole("dialog")).textContent).toContain("into sw1?");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await expect.poll(first).toBeNull();
    expect(second()).toBe("waiting");
    await expect.poll(() => screen.queryByRole("dialog")?.textContent ?? "").toContain("into sw2?");
    expect(box().value).toBe("c\nd");
    await user.click(screen.getByRole("button", { name: "Paste" }));
    await expect.poll(second).toBe("c\r\nd");
  });
});

describe("A paste that isn't checked", () => {
  it("is one line, which is typed at once as it is", async () => {
    render(PasteCheck);
    // a line break at the end doesn't make it two
    for (const text of ["show version", "show version\n", "show version\r\n"]) {
      expect(await checkPaste(text, "core-sw1")).toBe(text);
    }
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("has fewer lines than Settings > Terminal asks for, or any number with 0 there", async () => {
    render(PasteCheck);
    settings.terminalPrefs = { pasteLines: "3" };
    expect(await checkPaste("a\nb", "core-sw1")).toBe("a\nb");
    const three = outcome(checkPaste("a\nb\nc", "core-sw1"));
    await screen.findByRole("dialog");
    expect(three()).toBe("waiting");
    pasting.current = null;

    settings.terminalPrefs = { pasteLines: "0" };
    expect(await checkPaste("a\nb\nc\nd", "core-sw1")).toBe("a\nb\nc\nd");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("A paste's lines", () => {
  it("are counted by Windows', the Mac's and old Mac line breaks alike, one at the end not counting", () => {
    expect(pasteLineCount("")).toBe(1);
    expect(pasteLineCount("one")).toBe(1);
    expect(pasteLineCount("one\n")).toBe(1);
    expect(pasteLineCount("one\ntwo")).toBe(2);
    expect(pasteLineCount("one\r\ntwo\r\n")).toBe(2);
    expect(pasteLineCount("one\rtwo\rthree")).toBe(3);
    // an empty line in the middle is a line: it's an Enter at the prompt
    expect(pasteLineCount("one\n\ntwo")).toBe(3);
  });
});
