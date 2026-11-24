// File Transfer's Compress checkbox (FileTransfer.svelte): it starts as the session's profile has compression, and
// changing it reconnects with ssh's compression on or off, in the same folders. The Rust side is stood in for by
// mockIPC: sftp_open notes what it was asked and answers with a connection of its own, and the listings are fixed.
// The window's drag and drop from Finder is stood in for by a listener that never fires.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";

vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) }));

import FileTransfer from "./FileTransfer.svelte";

// the panes measure their lists (bind:clientWidth), which wants a ResizeObserver; jsdom has none
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;

/** Each sftp_open's compress, as the tab asked it (null: as the profile has it) */
let opens: (boolean | null)[];
/** The connections closed */
let closed: number[];
/** The remote folders listed, in order */
let listed: string[];
/** Whether the session's profile has Compression on */
let profileCompresses: boolean;

beforeEach(() => {
  opens = [];
  closed = [];
  listed = [];
  profileCompresses = false;
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    if (cmd === "sftp_open") {
      const compress = payload.compress as boolean | null;
      opens.push(compress);
      return { id: opens.length, home: "/root", remote: "/root", local: "/Users/me", address: "root@nas", scp: false, compressed: compress ?? profileCompresses };
    }
    if (cmd === "sftp_close") return void closed.push(Number(payload.id));
    if (cmd === "sftp_list") {
      listed.push(String(payload.path));
      return { path: payload.path, entries: [] };
    }
    if (cmd === "local_list") return { path: payload.path, entries: [] };
    return null;
  });
});

/** The Compress checkbox */
const compress = () => screen.getByRole("checkbox", { name: /Compress/ }) as HTMLInputElement;

describe("Compress", () => {
  it("starts as the session's profile has compression", async () => {
    profileCompresses = true;
    render(FileTransfer, { store: "saved", id: "Lab/nas" });
    await expect.poll(() => compress().checked).toBe(true);
    expect(opens).toEqual([null]);
  });

  it("reconnects with compression on, in the folder that was open, and the old connection closed", async () => {
    render(FileTransfer, { store: "saved", id: "Lab/nas" });
    await expect.poll(() => compress().checked).toBe(false);
    // into a folder first, which it stays in
    await userEvent.clear(screen.getByLabelText("Remote (root@nas) folder"));
    await userEvent.type(screen.getByLabelText("Remote (root@nas) folder"), "/srv/media{Enter}");
    await expect.poll(() => listed.at(-1)).toBe("/srv/media");
    await userEvent.click(compress());
    await expect.poll(() => opens).toEqual([null, true]);
    expect(closed).toEqual([1]);
    await expect.poll(() => listed.at(-1)).toBe("/srv/media");
    await expect.poll(() => compress().checked).toBe(true);
    // and off again
    await userEvent.click(compress());
    await expect.poll(() => opens).toEqual([null, true, false]);
  });
});
