// Settings > General's Restore from Backup…, beside Use Sessions.xml and Use PrivateSessions.xml: it lists that file's
// backups, newest first, and puts the one chosen back once the user says so; one that can't be read can't be chosen.
// The Rust side is stood in for by mockIPC: sessions_backups lists two backups of each file, sessions_restore notes
// what it was asked to restore, and the dialog plugin's ask() is answered with `reply`.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import Settings from "./Settings.svelte";
import { emptyProfile, DEFAULT_PROFILE } from "./profiles";
import { settings } from "./store.svelte";

/** What sessions_restore was asked: "store:name" */
let restored: string[];
/** The questions asked */
let asked: string[];
/** The answer to the question: its button's label */
let reply: string;

beforeEach(() => {
  Element.prototype.scrollIntoView = () => {};
  restored = [];
  asked = [];
  reply = "Restore";
  Object.assign(settings, { settingsFolder: "/team", useSessionsXml: true, usePrivateSessions: true });
  mockIPC((cmd, args) => {
    const a = args as { store: string; name: string; message: string };
    if (cmd === "profiles_list") return [emptyProfile(DEFAULT_PROFILE)];
    if (cmd === "shortcuts_keys" || cmd === "shortcuts_reserved") return [];
    if (cmd === "highlighting_load") return { themes: [], ruleSets: [], problems: [] };
    if (cmd === "sessions_backups") {
      const base = a.store === "saved" ? "Sessions" : "PrivateSessions";
      return [
        { name: `${base}.20261107_020000.xml`, saved: "2026-11-07 14:00:00", sessions: 12, problem: null },
        { name: `${base}.20261106_090000.xml`, saved: "2026-11-06 09:00:00", sessions: 0, problem: "not a sessions file" },
      ];
    }
    if (cmd === "sessions_restore") return void restored.push(`${a.store}:${a.name}`);
    if (cmd === "plugin:dialog|message") {
      asked.push(a.message);
      return reply;
    }
    return null;
  });
});

/** Open Restore from Backup… beside one of the files: 0 for Sessions.xml, 1 for PrivateSessions.xml */
async function open(user: ReturnType<typeof userEvent.setup>, which: 0 | 1): Promise<HTMLElement> {
  render(Settings, { onClose: () => {} });
  await user.click(screen.getAllByRole("button", { name: "Restore from Backup…" })[which]);
  return screen.findByRole("dialog", { name: which === 0 ? "Restore Sessions.xml from Backup" : "Restore PrivateSessions.xml from Backup" });
}

describe("Restore from Backup", () => {
  it("puts the PrivateSessions.xml backup chosen back, after asking", async () => {
    const user = userEvent.setup();
    const dialog = await open(user, 1);
    await user.click(await within(dialog).findByRole("option", { name: /2026-11-07 14:00:00\s*12 sessions/ }));
    await user.click(within(dialog).getByRole("button", { name: "Restore…" }));
    await expect.poll(() => restored).toEqual(["private:PrivateSessions.20261107_020000.xml"]);
    expect(asked[0]).toMatch(/Replace PrivateSessions\.xml with the version saved 2026-11-07 14:00:00 \(12 sessions\)/);
    // done, it closes
    await expect.poll(() => screen.queryByRole("dialog", { name: /Restore/ })).toBeNull();
  });

  it("restores Sessions.xml's own backups, and nothing when the question is cancelled", async () => {
    const user = userEvent.setup();
    reply = "Cancel";
    const dialog = await open(user, 0);
    await user.click(await within(dialog).findByRole("option", { name: /2026-11-07 14:00:00/ }));
    await user.click(within(dialog).getByRole("button", { name: "Restore…" }));
    await expect.poll(() => asked.length).toBe(1);
    expect(asked[0]).toMatch(/Replace Sessions\.xml/);
    expect(restored).toEqual([]);
  });

  it("can't restore a backup that can't be read", async () => {
    const user = userEvent.setup();
    const dialog = await open(user, 1);
    await user.click(await within(dialog).findByRole("option", { name: /2026-11-06 09:00:00\s*Can't be read/ }));
    expect((within(dialog).getByRole("button", { name: "Restore…" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("isn't offered for Sessions.xml before a settings folder is chosen", () => {
    Object.assign(settings, { settingsFolder: null });
    render(Settings, { onClose: () => {} });
    const [saved, priv] = screen.getAllByRole("button", { name: "Restore from Backup…" }) as HTMLButtonElement[];
    expect(saved.disabled).toBe(true);
    expect(priv.disabled).toBe(false);
  });
});
