// The app's menus (the tab strip's ▾ list, a tab's right-click menu, the Sessions tree's...) close on a click in a
// terminal. The terminal is its own page in an iframe, so the click never reaches the window; the window losing the
// focus to it is what's seen, stood in for here by a blur event on the window, as the iframe taking focus gives it.
// A terminal's own menu is drawn here too, over the whole window: its keys, its focus, and its submenus beside their
// item (jsdom has no layout, so where things are is stood in for by getBoundingClientRect).
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import Menu from "./Menu.svelte";

describe("a menu", () => {
  it("closes on a click in a terminal, which takes the window's focus", () => {
    let closed = 0;
    render(Menu, { items: [{ label: "Local Shell" }, { label: "Switch", checked: true }], x: 10, y: 10, onClose: () => closed++ });
    expect(screen.getByRole("menu")).toBeTruthy();
    window.dispatchEvent(new FocusEvent("blur"));
    expect(closed).toBe(1);
  });

  it("scrolls when it's long, with the ticked item in view, but not one with submenus, which would be cut off", async () => {
    const shown: string[] = [];
    const scrollIntoView = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) { shown.push(this.textContent?.trim() ?? ""); };
    try {
      // thirty tabs, the one in front far down the list
      const items = Array.from({ length: 30 }, (_, i) => ({ label: `File Server ${i + 1}`, checked: i === 25 }));
      const { unmount } = render(Menu, { items, x: 10, y: 10, onClose: () => {} });
      expect(screen.getByRole("menu").classList.contains("scrolls")).toBe(true);
      await expect.poll(() => shown.map((s) => s.replace(/\s+/g, " "))).toContain("✓ File Server 26");
      unmount();
      render(Menu, { items: [{ label: "New session", items: [{ label: "sw1" }] }], x: 10, y: 10, onClose: () => {} });
      expect(screen.getByRole("menu").classList.contains("scrolls")).toBe(false);
    } finally {
      Element.prototype.scrollIntoView = scrollIntoView;
    }
  });

  it("closes once, not again for its open submenu", async () => {
    let closed = 0;
    render(Menu, { items: [{ label: "New session", items: [{ label: "sw1" }] }], x: 10, y: 10, onClose: () => closed++ });
    // the pointer on the item opens its submenu, a second Menu
    screen.getByRole("menuitem", { name: /New session/ }).dispatchEvent(new MouseEvent("mouseenter"));
    await expect.poll(() => screen.queryAllByRole("menu").length).toBe(2);
    window.dispatchEvent(new FocusEvent("blur"));
    expect(closed).toBe(1);
  });

  it("shows each item's key, as a terminal's own menu does", () => {
    render(Menu, { items: [{ label: "Copy", key: "⌘C" }, { label: "Select All" }], x: 10, y: 10, onClose: () => {} });
    const keys = screen.getAllByRole("menuitem").map((row) => row.querySelector(".key")?.textContent ?? "");
    expect(keys).toEqual(["⌘C", ""]);
  });

  it("takes the focus when opened from a terminal, so Escape and a click back in a terminal close it", async () => {
    let closed = 0;
    render(Menu, { items: [{ label: "Paste" }], x: 10, y: 10, onClose: () => closed++, takeFocus: true });
    await expect.poll(() => document.activeElement === screen.getByRole("menu")).toBe(true);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(closed).toBe(1);
  });

  it("opens a submenu beside its item, over the window, so a menu that scrolls doesn't cut it off", async () => {
    const rect = HTMLElement.prototype.getBoundingClientRect;
    // the Theme row 300 pixels down, its menu 200 wide from 100 across; the window is 1024 wide (jsdom's)
    HTMLElement.prototype.getBoundingClientRect = function (this: HTMLElement) {
      return this.textContent?.trim().startsWith("Theme") && this.getAttribute("role") === "menuitem" ? new DOMRect(100, 300, 200, 24) : new DOMRect(0, 0, 150, 60);
    };
    try {
      render(Menu, { items: [{ label: "Copy" }, { label: "Theme", items: [{ label: "PuTTY" }, { label: "Light" }] }], x: 100, y: 280, onClose: () => {} });
      screen.getByRole("menuitem", { name: /Theme/ }).dispatchEvent(new MouseEvent("mouseenter"));
      await expect.poll(() => screen.queryAllByRole("menu").length).toBe(2);
      const sub = screen.getAllByRole("menu")[1];
      // on the item's right, overlapping it a little, and level with it, placed in the window rather than in the menu
      expect([sub.style.left, sub.style.top]).toEqual(["296px", "295px"]);
    } finally {
      HTMLElement.prototype.getBoundingClientRect = rect;
    }
  });

  it("shows a heading that nothing happens on: a terminal's menu names its tab so", () => {
    let closed = 0;
    render(Menu, { items: [{ label: "core-sw-01", heading: true }, { separator: true }, { label: "Paste" }], x: 10, y: 10, onClose: () => closed++ });
    const heading = screen.getByText("core-sw-01");
    expect(heading.getAttribute("role")).not.toBe("menuitem");
    heading.click();
    expect(closed).toBe(0);
    expect(screen.getAllByRole("menuitem").map((row) => row.textContent?.trim())).toEqual(["Paste"]);
  });
});
