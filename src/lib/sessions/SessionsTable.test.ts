// File > Edit Sessions as Table…: every session in one table, filtered column by column, its cells edited in place
// (text, Store, Folder, Icon, Colour), new sessions added as rows, columns resized and their widths kept, rows exported
// to CSV unsaved and a CSV brought in as edits, changed cells
// marked, and Save writing a file's rows in one go, Database's one by one, and moving a row whose Store changed;
// Database rows the user can't change, or locked, left alone. The Rust
// side is stood in for by mockIPC, which records what each save was asked and answers as the core would.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import SessionsTable from "./SessionsTable.svelte";
import { EnvironmentColors } from "../icons/environment";
import type { CsvSessions, DbState, SessionFields, SessionsLoaded, Store, TableRow } from "./api";

/** A session as the table reads it */
function row(store: Store, id: string, more: Partial<SessionFields> = {}, lock: Partial<TableRow> = {}): TableRow {
  const plain = id.replace(/^@db\//, "");
  const slash = plain.lastIndexOf("/");
  return {
    store, id, locked: false, lockedFields: "", hasPassword: false, ...lock,
    fields: {
      name: plain.slice(slash + 1), folder: slash < 0 ? "" : plain.slice(0, slash), protocol: "SSH", host: "", port: 22,
      username: "", profile: "Default Settings", color: "", icon: "switch", highlightRules: "", extraArgs: "", jumpHost: "",
      script: "", note: "", localPath: "", remotePath: "", rdpOptions: "", sshAlgorithms: "", buttonBar: "", ...more,
    },
  };
}

let rows: TableRow[];
/** Every call the table made to the core, in order */
let calls: { cmd: string; args: Record<string, unknown> }[];
/** What the window said it saved, once it closed */
let savedReport: string | null;
let closed: boolean;

const loaded: SessionsLoaded = { sessions: [], folders: [], problems: [], savedFile: "/x/Sessions.xml", privateFile: "/y/PrivateSessions.xml", savedOff: false };
const dbOn = { enabled: true, writable: true } as DbState;

beforeEach(() => {
  rows = [
    row("saved", "Lab/core1", { host: "10.0.0.1" }),
    row("saved", "Lab/sw1", { host: "10.0.0.2", color: "#2E7D32" }),
    row("private", "Home/nas", { host: "192.168.0.5", protocol: "SSH" }),
    row("database", "@db/Branch/rtr1", { host: "172.16.0.1" }),
  ];
  calls = [];
  // the window's storage, for the column widths: Node's own localStorage, which has no setItem without a file to
  // keep it in, comes before jsdom's, so a Map stands in, empty for each test
  const kept = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => kept.get(k) ?? null,
    setItem: (k: string, v: string) => void kept.set(k, v),
    removeItem: (k: string) => void kept.delete(k),
  });
  savedReport = null;
  closed = false;
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: a });
    if (cmd === "sessions_table_load") return rows;
    if (cmd === "sessions_table_save") {
      return (a.edits as [string | null, SessionFields][]).map(([, f]) => (f.folder ? `${f.folder}/${f.name}` : f.name));
    }
    if (cmd === "db_session_save") {
      const f = a.fields as SessionFields;
      return `@db/${f.folder}/${f.name}`;
    }
    if (cmd === "sessions_transfer") return { done: (a.ids as string[]).length, problems: [] };
    if (cmd === "session_passwords_set") return (a.ids as string[]).length;
    if (cmd === "sessions_load") return loaded;
    if (cmd === "profiles_list") return [{ name: "Default Settings", basedOn: "" }, { name: "Cisco", basedOn: "" }];
    if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
    // ask() is a message whose answer is the button pressed: here Drop Changes
    if (cmd === "plugin:dialog|message") return "Drop Changes";
    return null;
  });
});

/** Open the table, and wait for its rows */
async function open(db: DbState | null = dbOn) {
  render(SessionsTable, {
    loaded, db, colors: new EnvironmentColors([], []), customs: new Map(),
    onSaved: (report: string) => (savedReport = report), onClose: () => (closed = true),
  });
  await screen.findByRole("row", { name: "Lab/core1" });
}

/** A session's row */
const rowOf = (id: string) => screen.getByRole("row", { name: id });
/** A cell's field in a session's row, by its column */
const cell = (id: string, label: string) => within(rowOf(id)).getByLabelText(label) as HTMLInputElement & HTMLSelectElement;
/** A session's rows shown now, by id */
const shownIds = () => screen.queryAllByRole("row").map((r) => r.getAttribute("aria-label")).filter(Boolean);

describe("Edit Sessions as Table", () => {
  it("filters each column on its own, ignoring case, and Store by store", async () => {
    await open();
    expect(shownIds()).toEqual(["Lab/core1", "Lab/sw1", "Home/nas", "@db/Branch/rtr1"]);

    await userEvent.type(screen.getByLabelText("Filter Host"), "10.0");
    expect(shownIds()).toEqual(["Lab/core1", "Lab/sw1"]);
    await userEvent.type(screen.getByLabelText("Filter Name"), "SW");
    expect(shownIds()).toEqual(["Lab/sw1"]);
    expect(screen.getByText("1 of 4 sessions")).toBeTruthy();

    await userEvent.click(screen.getByRole("button", { name: "Clear Filters" }));
    await userEvent.selectOptions(screen.getByLabelText("Filter Store"), "database");
    expect(shownIds()).toEqual(["@db/Branch/rtr1"]);
    await userEvent.type(screen.getByLabelText("Filter Folder"), "nothing like it");
    expect(shownIds()).toEqual([]);
    expect(screen.getByText("No session matches the filters.")).toBeTruthy();
  });

  it("saves the cells changed: a file's rows in one write, Database's one by one, and a row moved to another store", async () => {
    await open();
    // Saved: a new host, and a move to another folder (Folder shows its path until it's clicked)
    const host = cell("Lab/core1", "Host");
    await userEvent.clear(host);
    await userEvent.type(host, "10.9.9.1");
    expect(host.closest("td")!.classList.contains("changed")).toBe(true);
    await userEvent.click(cell("Lab/sw1", "Folder"));
    const folder = cell("Lab/sw1", "Folder");
    await userEvent.clear(folder);
    await userEvent.type(folder, "Core{Enter}");
    expect(within(rowOf("Lab/sw1")).getByRole("button", { name: "Folder" }).textContent).toContain("Core");
    // Private: moved to Saved, nothing else changed
    await userEvent.selectOptions(cell("Home/nas", "Store"), "saved");
    // Database: a new port
    const port = cell("@db/Branch/rtr1", "Port");
    await userEvent.clear(port);
    await userEvent.type(port, "2222");
    expect(screen.getByText(/4 sessions, 4 changed/)).toBeTruthy();

    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => savedReport).toBe("Saved 4 sessions");

    // one write for Saved's two, none for Private (its row only moves)
    const files = calls.filter((c) => c.cmd === "sessions_table_save");
    expect(files).toHaveLength(1);
    expect(files[0].args.store).toBe("saved");
    const edits = files[0].args.edits as [string, SessionFields][];
    expect(edits.map(([id, f]) => [id, f.folder, f.host])).toEqual([["Lab/core1", "Lab", "10.9.9.1"], ["Lab/sw1", "Core", "10.0.0.2"]]);
    // Database's on its own, the port a number again
    const db = calls.filter((c) => c.cmd === "db_session_save");
    expect(db.map((c) => [c.args.originalId, (c.args.fields as SessionFields).port])).toEqual([["@db/Branch/rtr1", 2222]]);
    // the move, after the sessions are read again so it finds them
    const move = calls.findIndex((c) => c.cmd === "sessions_transfer");
    expect(calls[move].args).toMatchObject({ from: "private", ids: ["Home/nas"], to: "saved", deleteSource: true });
    expect(calls.slice(0, move).some((c) => c.cmd === "sessions_load")).toBe(true);
  });

  it("changes a session's icon in Choose Icon, and its colour from the palette", async () => {
    await open();
    await userEvent.click(cell("Lab/core1", "Icon"));
    const picker = screen.getByRole("dialog", { name: "Choose Icon" });
    await userEvent.click(within(picker).getByTitle("Core switch"));
    await userEvent.click(within(picker).getByRole("button", { name: "OK" }));
    expect(screen.queryByRole("dialog", { name: "Choose Icon" })).toBeNull();

    await userEvent.click(cell("Lab/sw1", "Colour"));
    await userEvent.click(within(screen.getByRole("dialog", { name: "Colour" })).getByRole("button", { name: "Red" }));
    expect(cell("Lab/sw1", "Colour").closest("td")!.classList.contains("changed")).toBe(true);

    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => savedReport).toBe("Saved 2 sessions");
    const edits = calls.find((c) => c.cmd === "sessions_table_save")!.args.edits as [string, SessionFields][];
    expect(edits.map(([id, f]) => [id, f.icon, f.color])).toEqual([["Lab/core1", "coreswitch", ""], ["Lab/sw1", "switch", "#C62828"]]);
  });

  it("won't save a row with no name, a port that isn't one, or a name another session in its folder has", async () => {
    await open();
    const save = screen.getByRole("button", { name: "Save" });
    const name = cell("Lab/sw1", "Name");
    await userEvent.clear(name);
    await userEvent.type(name, "CORE1");
    expect(rowOf("Lab/sw1").classList.contains("wrong")).toBe(true);
    expect(screen.getByRole("status").textContent).toContain("Another session is called Lab/CORE1");
    expect((save as HTMLButtonElement).disabled).toBe(true);

    await userEvent.clear(name);
    expect(screen.getByRole("status").textContent).toContain("needs a name");
    await userEvent.type(name, "sw1");
    const port = cell("Lab/sw1", "Port");
    await userEvent.clear(port);
    await userEvent.type(port, "ssh");
    expect(screen.getByRole("status").textContent).toContain("port is a number");
    await userEvent.clear(port);
    await userEvent.type(port, "22");
    // back as it was: nothing changed, nothing to save, nothing wrong
    expect(rowOf("Lab/sw1").classList.contains("wrong")).toBe(false);
    expect((save as HTMLButtonElement).disabled).toBe(true);
    expect(calls.some((c) => c.cmd === "sessions_table_save")).toBe(false);
  });

  it("leaves Database rows as they are when they can't be changed: read-only, locked, or a column locked", async () => {
    rows = [
      row("database", "@db/Branch/rtr1", { host: "172.16.0.1" }, { lockedFields: "host,port" }),
      row("database", "@db/Branch/rtr2", {}, { locked: true }),
      row("saved", "Lab/core1"),
    ];
    await open();
    expect(cell("@db/Branch/rtr1", "Host").disabled).toBe(true);
    expect(cell("@db/Branch/rtr1", "Port").disabled).toBe(true);
    expect(cell("@db/Branch/rtr1", "Username").disabled).toBe(false);
    expect(cell("@db/Branch/rtr1", "Host").closest("td")!.title).toBe("Locked in the database");
    for (const label of ["Host", "Name", "Store", "Icon", "Colour"]) expect(cell("@db/Branch/rtr2", label).disabled).toBe(true);
    // a Saved session can still go to Database
    expect([...cell("Lab/core1", "Store").options].map((o) => o.value)).toContain("database");
  });

  it("offers no move to Database, and changes none of its rows, when it's read-only", async () => {
    await open({ enabled: true, writable: false } as DbState);
    expect(cell("@db/Branch/rtr1", "Host").disabled).toBe(true);
    expect([...cell("Lab/core1", "Store").options].map((o) => o.value)).toEqual(["saved", "private"]);
  });

  it("says why a file's rows couldn't be saved, saves none of them, moves none, and stays open", async () => {
    await open();
    await userEvent.type(cell("Lab/core1", "Note"), "rack 4");
    await userEvent.selectOptions(cell("Lab/sw1", "Store"), "private");
    await userEvent.type(cell("Lab/sw1", "Username"), "admin");
    mockIPC((cmd) => {
      calls.push({ cmd, args: {} });
      if (cmd === "sessions_table_save") throw "Lab/sw1: There's already a session called Lab/sw1.";
      if (cmd === "sessions_table_load") return rows;
      return null;
    });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => screen.getByRole("status").textContent).toContain("Saved: Lab/sw1: There's already a session called Lab/sw1.");
    expect(calls.some((c) => c.cmd === "sessions_transfer")).toBe(false);
    expect(savedReport).toBeNull();
    expect(closed).toBe(false);
  });

  it("adds a new session as a row at the top, in the store filtered on, shown whatever the filters, saved as new", async () => {
    await open();
    await userEvent.selectOptions(screen.getByLabelText("Filter Store"), "private");
    await userEvent.type(screen.getByLabelText("Filter Host"), "nothing like it");
    await userEvent.click(screen.getByRole("button", { name: "New Session" }));
    expect(shownIds()).toEqual(["New session"]);
    const added = () => screen.getByRole("row", { name: "New session" });
    expect((within(added()).getByLabelText("Store") as HTMLSelectElement).value).toBe("private");
    // the name to type first, and a row with none can't be saved
    expect(document.activeElement).toBe(within(added()).getByLabelText("Name"));
    expect(screen.getByRole("status").textContent).toContain("needs a name");
    await userEvent.type(within(added()).getByLabelText("Name"), "nas2");
    await userEvent.type(within(added()).getByLabelText("Host"), "192.168.0.6");
    await userEvent.click(within(added()).getByRole("button", { name: "Folder" }));
    await userEvent.type(within(added()).getByLabelText("Folder"), "Home{Enter}");

    // a second one, taken away again before it's saved
    await userEvent.click(screen.getByRole("button", { name: "New Session" }));
    expect(screen.getAllByRole("row", { name: "New session" })).toHaveLength(2);
    await userEvent.click(within(screen.getAllByRole("row", { name: "New session" })[0]).getByRole("button", { name: "Remove new session" }));
    expect(screen.getAllByRole("row", { name: "New session" })).toHaveLength(1);

    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => savedReport).toBe("Saved 1 session");
    const save = calls.find((c) => c.cmd === "sessions_table_save")!;
    expect(save.args.store).toBe("private");
    const [[id, fields]] = save.args.edits as [string | null, SessionFields][];
    expect(id).toBeNull();
    expect(fields).toMatchObject({ name: "nas2", folder: "Home", host: "192.168.0.6", protocol: "SSH", port: 22, icon: "computer" });
    // made where it was put, not moved there
    expect(calls.some((c) => c.cmd === "sessions_transfer")).toBe(false);
  });

  it("saves a new session put in Database as a new row there", async () => {
    await open();
    await userEvent.click(screen.getByRole("button", { name: "New Session" }));
    const added = screen.getByRole("row", { name: "New session" });
    expect((within(added).getByLabelText("Store") as HTMLSelectElement).value).toBe("saved");
    await userEvent.selectOptions(within(added).getByLabelText("Store"), "database");
    await userEvent.type(within(added).getByLabelText("Name"), "rtr9");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => savedReport).toBe("Saved 1 session");
    const db = calls.filter((c) => c.cmd === "db_session_save");
    expect(db.map((c) => [c.args.originalId, (c.args.fields as SessionFields).name])).toEqual([[null, "rtr9"]]);
    expect(calls.some((c) => c.cmd === "sessions_table_save" || c.cmd === "sessions_transfer")).toBe(false);
  });

  it("resizes a column by dragging its heading's edge, keeps the width for next time, and a double click puts it back", async () => {
    // jsdom has no pointer capture; the drag's moves come to the grip either way
    HTMLElement.prototype.setPointerCapture ??= () => {};
    const { unmount } = render(SessionsTable, {
      loaded, db: dbOn, colors: new EnvironmentColors([], []), customs: new Map(), onSaved: () => {}, onClose: () => {},
    });
    await screen.findByRole("row", { name: "Lab/core1" });
    const hostCol = () => document.querySelectorAll("col")[6] as HTMLElement;
    expect(hostCol().style.width).toBe("160px");
    const grip = screen.getByRole("separator", { name: "Resize Host" });
    const pointer = (type: string, clientX: number) => {
      const event = new MouseEvent(type, { clientX, bubbles: true, cancelable: true });
      Object.defineProperty(event, "pointerId", { value: 1 });
      grip.dispatchEvent(event);
    };
    pointer("pointerdown", 500);
    pointer("pointermove", 580);
    pointer("pointerup", 580);
    await expect.poll(() => hostCol().style.width).toBe("240px");

    // kept: the next time the window opens
    unmount();
    await open();
    expect(hostCol().style.width).toBe("240px");
    await fireEvent.dblClick(screen.getByRole("separator", { name: "Resize Host" }));
    await expect.poll(() => hostCol().style.width).toBe("160px");
  });

  it("exports the rows shown to CSV as they are in the table, unsaved changes and moves included, saving nothing", async () => {
    await open();
    const host = cell("Lab/core1", "Host");
    await userEvent.clear(host);
    await userEvent.type(host, "10.9.9.1");
    await userEvent.selectOptions(cell("Lab/sw1", "Store"), "private");
    await userEvent.type(screen.getByLabelText("Filter Folder"), "lab");
    mockIPC((cmd, args) => {
      calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      if (cmd === "plugin:dialog|save") return "/Users/me/Desktop/Lab.csv";
      if (cmd === "sessions_table_export_csv") return [2, 0];
      return null;
    });
    await userEvent.click(screen.getByRole("button", { name: "Export CSV…" }));
    await expect.poll(() => screen.queryByText(/Exported 2 sessions to Lab\.csv\. Nothing saved\./)).toBeTruthy();
    const rows = calls.find((c) => c.cmd === "sessions_table_export_csv")!.args.rows as [Store, [Store, string] | null, SessionFields][];
    // the two Lab rows the filter shows, as edited: core1's new host, sw1 written in Private but found where it was read
    expect(rows.map(([store, read, f]) => [store, read, f.host])).toEqual([
      ["saved", ["saved", "Lab/core1"], "10.9.9.1"],
      ["private", ["saved", "Lab/sw1"], "10.0.0.2"],
    ]);
    expect(calls.some((c) => ["sessions_table_save", "db_session_save", "sessions_transfer"].includes(c.cmd))).toBe(false);
    // still changed, still to save
    expect(screen.getByText(/2 changed/)).toBeTruthy();
  });

  it("brings a CSV in as edits: the sessions it names changed, the rest added, a locked column and a bad line said", async () => {
    rows = [
      row("saved", "Lab/core1", { host: "10.0.0.1", extraArgs: "-P 2222 -pw ••••••" }),
      row("saved", "Lab/sw1", { host: "10.0.0.2" }),
      row("database", "@db/Branch/rtr1", { host: "172.16.0.1" }, { lockedFields: "host" }),
    ];
    const csv: CsvSessions = {
      ignored: ["Shoe size"],
      error: null,
      sessions: [
        // its own, with the export's masked password: the session's is kept
        { line: 2, source: "Saved", folder: "Lab", name: "core1", problem: null, values: [["Host", "10.5.5.5"], ["ExtraArgs", "-P 2222 -pw ********"]] },
        // no Source, other capitals: still that session
        { line: 3, source: "", folder: "lab/", name: "SW1", problem: null, values: [["Color", "#C62828"], ["Port", "2022"]] },
        // a new one, its masked password dropped as it has none
        { line: 4, source: "Private", folder: "Home", name: "nas2", problem: null, values: [["Host", "192.168.0.6"], ["ExtraArgs", "-pw ******** -A"]] },
        // the host is locked in the database; the username isn't
        { line: 5, source: "Database", folder: "Branch", name: "rtr1", problem: null, values: [["Host", "1.1.1.1"], ["Username", "netops"]] },
        { line: 6, source: "Saved", folder: "Lab", name: "sw9", problem: "unknown protocol \"Gopher\"", values: [] },
      ],
    };
    await open();
    mockIPC((cmd, args) => {
      calls.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      if (cmd === "plugin:dialog|open") return "/Users/me/Desktop/Lab.csv";
      if (cmd === "sessions_table_read_csv") return csv;
      return null;
    });
    await userEvent.click(screen.getByRole("button", { name: "Import CSV…" }));
    await expect.poll(() => screen.queryByText(/From Lab\.csv: 3 changed, 1 new; columns not used: Shoe size\. Not saved yet\./)).toBeTruthy();

    expect(cell("Lab/core1", "Host").value).toBe("10.5.5.5");
    expect(cell("Lab/core1", "Extra args").value).toBe("-P 2222 -pw ••••••");
    expect(cell("Lab/sw1", "Port").value).toBe("2022");
    expect(cell("Lab/sw1", "Colour").closest("td")!.classList.contains("changed")).toBe(true);
    expect(cell("@db/Branch/rtr1", "Host").value).toBe("172.16.0.1");
    expect(cell("@db/Branch/rtr1", "Username").value).toBe("netops");
    const added = screen.getByRole("row", { name: "New session" });
    expect((within(added).getByLabelText("Store") as HTMLSelectElement).value).toBe("private");
    expect((within(added).getByLabelText("Name") as HTMLInputElement).value).toBe("nas2");
    expect((within(added).getByLabelText("Extra args") as HTMLInputElement).value).toBe("-A");
    const said = screen.getByRole("status").textContent!;
    expect(said).toContain("Line 5: Host locked in the database, left as it is");
    expect(said).toContain("Line 6: unknown protocol \"Gopher\"");
    // nothing saved by the import itself
    expect(calls.some((c) => ["sessions_table_save", "db_session_save", "sessions_transfer"].includes(c.cmd))).toBe(false);
  });

  it("sets a session's own password, forgets a stored one, and puts a moved session's under its new id", async () => {
    rows = [
      row("saved", "Lab/core1", {}, { hasPassword: true }),
      row("saved", "Lab/sw1"),
      row("saved", "Lab/sw2"),
      // locked in a read-only table, but the password is this computer's
      row("database", "@db/Branch/rtr1", {}, { locked: true }),
    ];
    await open({ enabled: true, writable: true } as DbState);
    // the profile's password recommended, for sessions that share a login
    expect(screen.getByRole("columnheader", { name: /Password/ }).title).toMatch(/profile's password is usually better/);
    // never shown, only whether there's one
    expect(cell("Lab/core1", "Password").value).toBe("");
    expect(cell("Lab/core1", "Password").placeholder).toBe("stored");
    expect(cell("Lab/sw1", "Password").placeholder).toBe("none");
    // forgotten with ×, which ↶ takes back, then forgotten again
    await userEvent.click(within(rowOf("Lab/core1")).getByRole("button", { name: "Forget password" }));
    expect(cell("Lab/core1", "Password").placeholder).toBe("forgotten on Save");
    await userEvent.click(within(rowOf("Lab/core1")).getByRole("button", { name: "Keep password" }));
    expect(cell("Lab/core1", "Password").closest("td")!.classList.contains("changed")).toBe(false);
    await userEvent.click(within(rowOf("Lab/core1")).getByRole("button", { name: "Forget password" }));
    expect(cell("Lab/core1", "Password").closest("td")!.classList.contains("changed")).toBe(true);
    // a password alone, a password and a move to Database, and one on a locked row
    await userEvent.type(cell("Lab/sw1", "Password"), "s3cret");
    await userEvent.type(cell("Lab/sw2", "Password"), "s3cret");
    await userEvent.selectOptions(cell("Lab/sw2", "Store"), "database");
    expect(cell("@db/Branch/rtr1", "Password").disabled).toBe(false);
    await userEvent.type(cell("@db/Branch/rtr1", "Password"), "other");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => savedReport).toBe("Saved 4 sessions");
    const sets = calls.filter((c) => c.cmd === "session_passwords_set").map((c) => [c.args.password, c.args.ids]);
    expect(sets).toEqual(expect.arrayContaining([["", ["Lab/core1"]], ["s3cret", ["Lab/sw1", "@db/Lab/sw2"]], ["other", ["@db/Branch/rtr1"]]]));
    expect(sets).toHaveLength(3);
    // passwords after the move, and the fields themselves untouched: nothing but the move and the passwords
    const order = calls.map((c) => c.cmd).filter((c) => ["sessions_transfer", "session_passwords_set"].includes(c));
    expect(order[0]).toBe("sessions_transfer");
    expect(calls.some((c) => c.cmd === "sessions_table_save" || c.cmd === "db_session_save")).toBe(false);
  });

  it("offers Auto and None for highlighting, as Edit sessions does", async () => {
    await open();
    const options = [...cell("Lab/core1", "Highlight rules").options].map((o) => o.value);
    expect(options).toEqual(["", "auto", "none"]);
  });

  it("asks before dropping changes on Cancel, and keeps them when told to", async () => {
    await open();
    await userEvent.type(cell("Lab/core1", "Note"), "rack 4");
    // Cancel pressed in the question
    mockIPC((cmd) => (cmd === "plugin:dialog|message" ? "Cancel" : null));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await new Promise((r) => setTimeout(r, 0));
    expect(closed).toBe(false);
    expect(cell("Lab/core1", "Note").value).toBe("rack 4");
    mockIPC((cmd) => (cmd === "plugin:dialog|message" ? "Drop Changes" : null));
    await fireEvent.keyDown(window, { key: "Escape" });
    await expect.poll(() => closed).toBe(true);
  });
});
