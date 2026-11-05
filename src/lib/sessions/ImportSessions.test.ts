// File > Import Sessions' drop zone: the dialog opens with it (no file picker thrown up at once), files dragged over
// the window light it up, a dropped file of the right kind is saved by the core (the browser gives its contents, not
// its path) and planned, one of another kind, several, or a folder is turned down saying why, and clicking the zone
// opens the picker. The drags are the browser's drag events, as the window gets them (Tauri's own drop handling is
// off), dispatched on the window with a stand-in dataTransfer (jsdom has no DataTransfer); the Rust side is stood in
// for by mockIPC: the import's source and plan, saving a dropped file, and the dialog plugin's file picker.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import ImportSessions from "./ImportSessions.svelte";

/** The paths each plan was asked for, the files the core saved, and how many times the file picker opened */
let planned: string[][];
let saved: { name: string; text: string }[];
let picked: number;

beforeEach(() => {
  planned = [];
  saved = [];
  picked = 0;
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as { paths?: string[]; name?: string; data?: string };
    if (cmd === "sessions_import_source") return null;
    if (cmd === "sessions_import_plan") {
      planned.push(payload.paths ?? []);
      return { adds: 3, updates: 0, skips: 0, examples: ["Lab/sw1", "Lab/sw2", "Lab/sw3"], skipped: [], ignored: [], error: null };
    }
    if (cmd === "dropped_file_save") {
      saved.push({ name: payload.name!, text: atob(payload.data!) });
      return `/tmp/SuperXTerminal-dropped/1/${payload.name}`;
    }
    if (cmd === "plugin:dialog|open") {
      picked++;
      return "/Users/me/Downloads/picked.xml";
    }
    return null;
  });
});

/** A drag event of files from the system, as the window gets it: their names and contents, folders said to be so */
function drag(type: string, files: { name: string; text?: string; folder?: boolean }[] = []): DragEvent {
  const event = new Event(type, { bubbles: true, cancelable: true }) as DragEvent;
  Object.defineProperty(event, "dataTransfer", {
    value: {
      types: ["Files"],
      files: files.map((f) => new File([f.text ?? ""], f.name)),
      items: files.map((f) => ({ webkitGetAsEntry: () => ({ isDirectory: !!f.folder }) })),
      dropEffect: "none",
    },
  });
  window.dispatchEvent(event);
  return event;
}

/** The dialog for a SuperPuTTY XML file, and its drop zone */
function open(): HTMLElement {
  render(ImportSessions, { kind: "superputty", savedAvailable: true, onDone: () => {}, onCancel: () => {} });
  return screen.getByRole("button", { name: /Drop a SuperPuTTY \.xml file here/ });
}

describe("Import Sessions' drop zone", () => {
  it("opens without a file picker, lights up while files are dragged over, and plans the file dropped", async () => {
    const zone = open();
    expect(picked).toBe(0);
    expect(zone.textContent).toContain("Drop a SuperPuTTY .xml file here");
    drag("dragenter", [{ name: "Sessions.xml" }]);
    // the browser is told a copy is wanted, so it doesn't open the file itself
    expect(drag("dragover", [{ name: "Sessions.xml" }]).defaultPrevented).toBe(true);
    await expect.poll(() => zone.classList.contains("dragging")).toBe(true);
    const drop = drag("drop", [{ name: "Sessions.xml", text: "<ArrayOfSessionData/>" }]);
    expect(drop.defaultPrevented).toBe(true);
    await expect.poll(() => planned).toEqual([["/tmp/SuperXTerminal-dropped/1/Sessions.xml"]]);
    expect(saved).toEqual([{ name: "Sessions.xml", text: "<ArrayOfSessionData/>" }]);
    expect(zone.classList.contains("dragging")).toBe(false);
    expect(zone.textContent).toContain("Sessions.xml");
    await expect.poll(() => (screen.getByRole("button", { name: "Import" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("turns down a file of another kind, several, or a folder, saying why, and plans nothing", async () => {
    open();
    drag("drop", [{ name: "notes.txt" }]);
    expect((await screen.findByRole("alert")).textContent).toBe("notes.txt isn't a SuperPuTTY .xml file: it takes .xml.");
    drag("drop", [{ name: "one.xml" }, { name: "two.xml" }]);
    await expect.poll(() => screen.getByRole("alert").textContent).toBe("Drop one file: 2 were dropped.");
    drag("drop", [{ name: "Sessions", folder: true }]);
    await expect.poll(() => screen.getByRole("alert").textContent).toBe("Drop a file, not a folder: this takes a SuperPuTTY .xml file.");
    expect(saved).toEqual([]);
    expect(planned).toEqual([]);
  });

  it("opens the file picker when clicked, and plans the file chosen", async () => {
    const user = userEvent.setup();
    await user.click(open());
    await expect.poll(() => planned).toEqual([["/Users/me/Downloads/picked.xml"]]);
    expect(picked).toBe(1);
  });
});
