// One side of File Transfer (FilePane.svelte): its right-click menu closing on a click elsewhere, and its checkboxes
// choosing several files to copy at once. The pane is rendered alone with a listing of its own; what it asks its
// owner (copy, delete...) is caught by the props.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import FilePane from "./FilePane.svelte";
import type { Entry, Listing } from "./api";

// the list's width is measured (bind:clientWidth), which wants a ResizeObserver; jsdom has none
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;

/** An entry of the listing: a file, or a folder with `dir` */
function entry(name: string, dir = false): Entry {
  return { name, dir, link: false, size: dir ? 0 : 43, modified: 0, date: "", permissions: dir ? "drwxr-xr-x" : "-rw-------", owner: "root", group: "root" };
}

/** /root, as the screenshot's remote side: a folder and three files */
const LISTING: Listing = { path: "/root", entries: [entry("docker", true), entry(".lesshst"), entry(".profile"), entry("get-pip.py")] };

/** Render the remote side, with its copies caught */
function pane(onCopy = vi.fn()) {
  render(FilePane, {
    side: "remote", title: "Remote (root@nas)", home: "/root", listing: LISTING, loading: false, error: "",
    onNavigate: () => {}, onCopy, onDrop: () => {}, onDelete: () => {},
  });
  return onCopy;
}

/** A file's row in the list */
const row = (name: string) => screen.getByText(name).closest("tr") as HTMLTableRowElement;

describe("the right-click menu", () => {
  it("closes on a click elsewhere, as every other menu in the app does", async () => {
    pane();
    await userEvent.pointer({ keys: "[MouseRight]", target: row(".lesshst") });
    expect(screen.getByRole("menu")).toBeTruthy();
    await userEvent.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes when another right-click opens a menu elsewhere, so only one is open", async () => {
    pane();
    await userEvent.pointer({ keys: "[MouseRight]", target: row(".lesshst") });
    await userEvent.pointer({ keys: "[MouseRight]", target: document.body });
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("the checkboxes", () => {
  /** A file's checkbox */
  const box = (name: string) => screen.getByRole("checkbox", { name: `Choose ${name}` }) as HTMLInputElement;

  it("tick several files without holding a key, and Download copies them all", async () => {
    const onCopy = pane();
    await userEvent.click(box(".lesshst"));
    await userEvent.click(box("get-pip.py"));
    expect(box(".lesshst").checked).toBe(true);
    expect(box("get-pip.py").checked).toBe(true);
    expect(screen.getByText(/2 selected/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: /Download/ }));
    expect(onCopy).toHaveBeenCalledWith(["/root/.lesshst", "/root/get-pip.py"]);
  });

  it("untick one without losing the others", async () => {
    pane();
    await userEvent.click(box(".lesshst"));
    await userEvent.click(box(".profile"));
    await userEvent.click(box(".lesshst"));
    expect(box(".lesshst").checked).toBe(false);
    expect(box(".profile").checked).toBe(true);
  });

  it("show what a click on a row selected, which still selects that row alone", async () => {
    pane();
    await userEvent.click(box(".lesshst"));
    await userEvent.click(screen.getByText(".profile"));
    expect(box(".lesshst").checked).toBe(false);
    expect(box(".profile").checked).toBe(true);
  });

  it("all at once from the heading's, and none again", async () => {
    pane();
    const all = screen.getByRole("checkbox", { name: "Choose all" }) as HTMLInputElement;
    await userEvent.click(all);
    expect(["docker", ".lesshst", ".profile", "get-pip.py"].every((name) => box(name).checked)).toBe(true);
    expect(all.checked).toBe(true);
    await userEvent.click(all);
    expect(["docker", ".lesshst", ".profile", "get-pip.py"].some((name) => box(name).checked)).toBe(false);
  });

  it("don't open a folder when its box is double-clicked", async () => {
    const onNavigate = vi.fn();
    render(FilePane, {
      side: "remote", title: "Remote (root@nas)", home: "/root", listing: LISTING, loading: false, error: "",
      onNavigate, onCopy: () => {}, onDrop: () => {},
    });
    await userEvent.dblClick(box("docker"));
    expect(onNavigate).not.toHaveBeenCalled();
  });
});
