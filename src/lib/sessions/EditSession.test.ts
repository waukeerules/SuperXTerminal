// Edit Session's tabs, which mark the one with a change unsaved; and its Password: the session's own, kept in the Keychain under its id, and Move -pw here, which takes the
// stored -pw there and saves the session without it. The Rust side is stood in for by mockIPC: what's saved, and
// what's stored as the session's password, are recorded.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import EditSession from "./EditSession.svelte";
import type { SessionFields } from "./api";

const SESSION: SessionFields = {
  name: "sw1", folder: "Lab", protocol: "SSH", host: "192.168.0.50", port: 22, username: "admin", profile: "", color: "",
  icon: "switch", highlightRules: "", extraArgs: "-pw •••••• -P 2222", jumpHost: "", script: "", note: "", localPath: "",
  remotePath: "", rdpOptions: "", sshAlgorithms: "",
};

let saved: SessionFields | null;
let stored: { id: string; password: string }[];
let moved: string[];

beforeEach(() => {
  saved = null;
  stored = [];
  moved = [];
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "session_password_has") return false;
    if (cmd === "session_password_from_pw") {
      moved.push(String(payload.id));
      return null;
    }
    if (cmd === "session_password_set") {
      stored.push({ id: String(payload.id), password: String(payload.password) });
      return null;
    }
    if (cmd === "session_save") {
      saved = payload.fields as SessionFields;
      return "Lab/sw1";
    }
    if (cmd === "profiles_list") return [{ name: "Default Settings" }];
    if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
    // Move -pw here asks first, with its own button labels: answered with its OK button
    if (cmd === "plugin:dialog|message") return "Move it";
    return null;
  });
});

/** Edit Session open on the session, with what it was saved as given to `onSaved` */
function open(fields: SessionFields = SESSION) {
  render(EditSession, {
    store: "saved", id: "Lab/sw1", fields, folders: [], iconSize: 24, customs: new Map(),
    onSaved: () => {}, onCancel: () => {}, onOpenProfiles: () => {},
  });
}

describe("Edit Session's password", () => {
  it("moves a stored -pw into the Keychain and saves the session without it", async () => {
    const user = userEvent.setup();
    open();
    await user.click(await screen.findByRole("button", { name: "Move -pw here" }));
    await expect.poll(() => moved).toEqual(["Lab/sw1"]);
    expect((screen.getByLabelText(/Extra arguments/) as HTMLInputElement).value).toBe("-P 2222");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => saved?.extraArgs).toBe("-P 2222");
  });

  it("keeps a typed password in the Keychain under the session's id, not in its file", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "-P 2222" });
    expect(screen.queryByRole("button", { name: "Move -pw here" })).toBeNull();
    await user.type(screen.getByLabelText(/^Password/), "s3cret");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await expect.poll(() => stored).toEqual([{ id: "Lab/sw1", password: "s3cret" }]);
    expect(JSON.stringify(saved)).not.toContain("s3cret");
  });
});

describe("Edit Session's tabs", () => {
  it("show one tab's fields at a time, and mark a tab with a change unsaved", async () => {
    const user = userEvent.setup();
    open();
    const general = screen.getByRole("tab", { name: "General" });
    const connection = screen.getByRole("tab", { name: "Connection" });
    expect(general.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel", { name: "General" })).toBeTruthy();
    // the notes are on General
    expect(screen.getByLabelText(/Notes/).closest("[role=tabpanel]")?.getAttribute("aria-label")).toBe("General");
    await user.click(connection);
    expect(connection.getAttribute("aria-selected")).toBe("true");
    expect(connection.querySelector(".dot:not(.off)")).toBeNull();
    // its dot is there all along, so the tab doesn't widen as it shows
    expect(connection.querySelector(".dot.off")).not.toBeNull();
    await user.type(screen.getByLabelText(/Jump host/), "bastion");
    expect(connection.querySelector(".dot:not(.off)")).not.toBeNull();
    expect(general.querySelector(".dot:not(.off)")).toBeNull();
  });
});

describe("Edit Session's Revert", () => {
  it("puts every field back as the window opened with it, and drops a typed password", async () => {
    const user = userEvent.setup();
    open({ ...SESSION, extraArgs: "-P 2222" });
    const revert = screen.getByRole("button", { name: "Revert" }) as HTMLButtonElement;
    expect(revert.disabled).toBe(true);
    await user.type(screen.getByLabelText(/Session name/), "-new");
    await user.type(screen.getByLabelText(/^Password/), "s3cret");
    await user.click(screen.getByRole("tab", { name: "Connection" }));
    await user.type(screen.getByLabelText(/Jump host/), "bastion");
    expect(revert.disabled).toBe(false);
    await user.click(revert);
    expect((screen.getByLabelText(/Session name/) as HTMLInputElement).value).toBe("sw1");
    expect((screen.getByLabelText(/Jump host/) as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText(/^Password/) as HTMLInputElement).value).toBe("");
    expect(revert.disabled).toBe(true);
    expect(screen.queryByText(/Unsaved changes/)).toBeNull();
  });
});
