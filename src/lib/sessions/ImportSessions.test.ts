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
/** Where the CSV template was saved (sessions_csv_template's path) */
let templates: string[];

beforeEach(() => {
  planned = [];
  saved = [];
  picked = 0;
  templates = [];
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
    if (cmd === "plugin:dialog|save") return "/Users/me/Downloads/sessions-template.csv";
    if (cmd === "sessions_csv_template") {
      templates.push(String((args as { path: string }).path));
      return null;
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

describe("Import Sessions from SecureCRT", () => {
  /** What the file picker was asked for */
  let pickerAsked: { directory?: boolean; title?: string }[];

  /** The dialog, with the core saying SecureCRT's Sessions folder is `source` (null: there's none here) */
  function openSecureCrt(source: string | null): void {
    pickerAsked = [];
    mockIPC((cmd, args) => {
      if (cmd === "sessions_import_source") return source;
      if (cmd === "sessions_import_plan") {
        planned.push((args as { paths: string[] }).paths);
        return { adds: 2, updates: 0, skips: 0, examples: ["ImportedFromSecureCRT/openmediavault.localdomain", "ImportedFromSecureCRT/Serial-COM3"], skipped: [], ignored: [], error: null };
      }
      if (cmd === "plugin:dialog|open") {
        pickerAsked.push((args as { options: { directory?: boolean; title?: string } }).options);
        return "D:\\Backup\\VanDyke\\Config\\Sessions";
      }
      return null;
    });
    render(ImportSessions, { kind: "securecrt", savedAvailable: true, onDone: () => {}, onCancel: () => {} });
  }

  it("reads SecureCRT's own Sessions folder at once, into a folder of its own", async () => {
    openSecureCrt("C:\\Users\\me\\AppData\\Roaming\\VanDyke\\Config\\Sessions");
    await expect.poll(() => planned).toEqual([["C:\\Users\\me\\AppData\\Roaming\\VanDyke\\Config\\Sessions"]]);
    expect(screen.getByRole("dialog", { name: "Import sessions from SecureCRT" })).toBeTruthy();
    expect(screen.getByText(/in the folder ImportedFromSecureCRT/)).toBeTruthy();
    expect(screen.getByText(/Passwords aren't imported/)).toBeTruthy();
    expect(await screen.findByText("ImportedFromSecureCRT/Serial-COM3")).toBeTruthy();
  });

  it("asks for a folder, not a file, when SecureCRT's isn't here or another is wanted", async () => {
    openSecureCrt(null);
    expect(planned).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: /SecureCRT's Sessions folder/ }));
    await expect.poll(() => planned).toEqual([["D:\\Backup\\VanDyke\\Config\\Sessions"]]);
    expect(pickerAsked).toEqual([expect.objectContaining({ directory: true, title: "Choose SecureCRT's Sessions folder" })]);
  });
});

describe("Import Sessions from a CSV file", () => {
  it("saves a template to fill in, and says what to do with it", async () => {
    render(ImportSessions, { kind: "csv", savedAvailable: true, onDone: () => {}, onCancel: () => {} });
    expect(screen.getByText(/Every column, with an example SSH, serial and Remote Desktop session to copy/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Save a Template…" }));
    await expect.poll(() => templates).toEqual(["/Users/me/Downloads/sessions-template.csv"]);
    expect(await screen.findByText(/Saved as sessions-template\.csv: replace its example rows with your sessions, then drop it here\./)).toBeTruthy();
  });

  it("isn't offered for another app's sessions", () => {
    open();
    expect(screen.queryByRole("button", { name: "Save a Template…" })).toBeNull();
  });
});
