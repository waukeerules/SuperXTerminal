// The script box (ScriptBox.svelte) as Edit Button and the Script Editor use it: a plain box until its page is
// ready, then the page (Monaco) given the text, language, Wrap and light or dark, its changes taken back, the Wrap
// lines switch kept in the preferences, and Pop out growing the box over the window (Escape puts it back). The page is stood in for by the iframe's postMessage, which records
// what the box sends it, and by messages "from" its window; the Rust side by mockIPC, which saves the settings.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import Harness from "./ScriptBoxHarness.svelte";
import { settings } from "../settings/store.svelte";

/** What the box sent its page, in order */
let sent: { type: string; [key: string]: unknown }[];
/** The settings saved, as settings_save was given them */
let saved: { terminalPrefs?: Record<string, string> }[];

beforeEach(() => {
  sent = [];
  saved = [];
  Object.assign(settings, { terminalPrefs: {} });
  mockIPC((cmd, args) => {
    if (cmd === "settings_save") {
      const next = (args as { settings: { terminalPrefs?: Record<string, string> } }).settings;
      saved.push(next);
      return next;
    }
    return null;
  });
});

/** Show the box, recording what it sends its page */
function open(start: string, language: "spsl" | "plain" = "spsl") {
  const view = render(Harness, { start, language });
  const frame = document.querySelector("iframe")!;
  frame.contentWindow!.postMessage = ((envelope: { message: { type: string } }) => sent.push(envelope.message)) as typeof window.postMessage;
  const tab = new URL(frame.src, location.href).searchParams.get("tab");
  /** The page saying something to the box */
  const page = (message: Record<string, unknown>) =>
    window.dispatchEvent(new MessageEvent("message", { data: { channel: "sxt-page", tab, message }, source: frame.contentWindow, origin: location.origin }));
  return { view, harness: view.component as unknown as { current(): string; setValue(t: string): void; insert(t: string): void }, page };
}

describe("the script box", () => {
  it("shows the text in a plain box until its page is ready, and inserts at the cursor there", async () => {
    const { harness } = open("WAITFOR #\nSENDLINE show clock");
    const box = screen.getByLabelText("Script") as HTMLTextAreaElement;
    expect(box.value).toBe("WAITFOR #\nSENDLINE show clock");
    // no size handle: the box can't be dragged past its dialog
    expect(getComputedStyle(box).resize).toBe("none");
    // every key is Monaco's (⌘F its own find), none caught for the app's shortcuts
    expect(document.querySelector("iframe")!.dataset.ownKeys).toBe("*");
    box.setSelectionRange(10, 10);
    harness.insert("SLEEP 500\n");
    expect(harness.current()).toBe("WAITFOR #\nSLEEP 500\nSENDLINE show clock");
  });

  it("gives its page the text, language and Wrap once it's ready, and takes the page's changes", async () => {
    const { harness, page } = open("WAITFOR #");
    page({ type: "ready" });
    await expect.poll(() => sent[0]).toEqual({ type: "set", text: "WAITFOR #", language: "spsl", wrap: true, dark: expect.any(Boolean) });
    // the plain box goes once the page is up
    expect(screen.queryByRole("textbox", { name: "Script" })).toBeNull();
    page({ type: "change", text: "WAITFOR /i #" });
    expect(harness.current()).toBe("WAITFOR /i #");
    // the page's own change isn't sent back to it; one from outside (Revert) is
    harness.setValue("SENDLINE x");
    await expect.poll(() => sent.filter((m) => m.type === "text")).toEqual([{ type: "text", text: "SENDLINE x" }]);
    harness.insert("SLEEP 1\n");
    expect(sent.at(-1)).toEqual({ type: "insert", text: "SLEEP 1\n" });
  });

  it("keeps Wrap lines in the preferences, and tells the page", async () => {
    const user = userEvent.setup();
    const { page } = open("x");
    page({ type: "ready" });
    const wrap = screen.getByRole("checkbox", { name: "Wrap lines" }) as HTMLInputElement;
    expect(wrap.checked).toBe(true);
    await user.click(wrap);
    await expect.poll(() => saved.at(-1)?.terminalPrefs).toEqual({ scriptWrap: "false" });
    await expect.poll(() => sent.at(-1)).toEqual({ type: "wrap", wrap: false });
    expect(wrap.checked).toBe(false);
  });

  it("pops out over the window without loading its page again, and Escape pops it back in, not closing the dialog", async () => {
    const user = userEvent.setup();
    const { page } = open("WAITFOR #");
    page({ type: "ready" });
    const frame = document.querySelector("iframe");
    /** The dialog's own Escape (Edit Button's), which mustn't fire while the box is popped out */
    let dialogClosed = 0;
    const dialogEscape = (e: KeyboardEvent) => e.key === "Escape" && dialogClosed++;
    window.addEventListener("keydown", dialogEscape);
    await user.click(screen.getByRole("button", { name: /Pop out/ }));
    const box = screen.getByRole("group", { name: "Script box, popped out" });
    expect(box.classList.contains("popped")).toBe(true);
    // the same iframe, not moved: its editor (and undo) is kept
    expect(document.querySelector("iframe")).toBe(frame);
    await user.keyboard("{Escape}");
    expect(screen.getByRole("group", { name: "Script box" }).classList.contains("popped")).toBe(false);
    expect(dialogClosed).toBe(0);
    // popped in, Escape is the dialog's again
    await user.keyboard("{Escape}");
    expect(dialogClosed).toBe(1);
    window.removeEventListener("keydown", dialogEscape);
  });
});
