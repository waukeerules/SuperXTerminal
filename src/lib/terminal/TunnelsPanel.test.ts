// A tab's Tunnels panel, rendered on its own: a row for each tunnel with its link and test, Copy, the restart it asks
// for when the profile's tunnels have changed, and Edit Tunnels…. What its buttons do in the window is stood in for
// by the callbacks it's given; the clipboard by a stand-in for navigator.clipboard.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import TunnelsPanel from "./TunnelsPanel.svelte";

/** What was copied */
let copied: string[];

beforeEach(() => {
  copied = [];
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async (text: string) => void copied.push(text) } });
});
afterEach(cleanup);

/** The panel for the OMV tab's tunnels, with its callbacks as mocks */
function open(changed = false) {
  const calls = { close: vi.fn(), link: vi.fn(), edit: vi.fn(), restart: vi.fn() };
  render(TunnelsPanel, {
    title: "OMV",
    tunnels: [{ kind: "local", listen: "8443", to: "localhost:443" }, { kind: "dynamic", listen: "1080", to: "" }],
    changed,
    profile: "OMV Tunnels",
    x: 10,
    y: 10,
    onClose: calls.close,
    onOpenLink: calls.link,
    onEdit: calls.edit,
    onRestart: calls.restart,
  });
  return calls;
}

describe("a tab's Tunnels panel", () => {
  it("lists each tunnel with what it does, a link for a web page, and a test to copy", async () => {
    const calls = open();
    const rows = within(screen.getByRole("dialog", { name: "Tunnels" })).getAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain("https://localhost:8443 on this PC opens localhost:443, as the server sees it.");
    await userEvent.click(within(rows[0]).getByRole("link", { name: "https://localhost:8443" }));
    expect(calls.link).toHaveBeenCalledWith("https://localhost:8443");
    await userEvent.click(within(rows[1]).getByRole("button", { name: "Copy" }));
    expect(copied).toEqual(["curl.exe --socks5-hostname localhost:1080 -I http://localhost"]);
    expect(within(rows[1]).getByRole("button", { name: "Copied" })).toBeTruthy();
  });

  it("asks for a restart when the profile's tunnels have changed, and restarts, or opens the profile", async () => {
    const calls = open(true);
    expect(screen.getByRole("note").textContent).toMatch(/The profile's tunnels have changed\. Restart the session to use them/);
    await userEvent.click(screen.getByRole("button", { name: "Restart Session" }));
    expect(calls.restart).toHaveBeenCalledOnce();
    expect(calls.close).toHaveBeenCalled();
    cleanup();
    const again = open();
    expect(screen.queryByRole("note")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Edit Tunnels…" }));
    expect(again.edit).toHaveBeenCalledOnce();
  });

  it("says a tunnel whose port another tab has isn't open here, and whose it is", () => {
    render(TunnelsPanel, {
      title: "OMV (2)", tunnels: [{ kind: "local", listen: "8443", to: "localhost:443" }], changed: false, taken: new Map([["8443", "OMV"]]),
      profile: "OMV Tunnels", x: 10, y: 10, onClose: () => {}, onOpenLink: () => {}, onEdit: () => {}, onRestart: () => {},
    });
    expect(screen.getByRole("note", { name: "Port in use" }).textContent).toBe(`Port 8443 on this PC is already the tunnel of the tab "OMV", so this tab's isn't open: use that tab's, or close it and restart this session.`);
  });

  it("closes on Escape", async () => {
    const calls = open();
    await userEvent.keyboard("{Escape}");
    expect(calls.close).toHaveBeenCalled();
  });
});
