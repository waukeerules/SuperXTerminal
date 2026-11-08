// The app's menus (the tab strip's ▾ list, a tab's right-click menu, the Sessions tree's...) close on a click in a
// terminal. The terminal is its own page in an iframe, so the click never reaches the window; the window losing the
// focus to it is what's seen, stood in for here by a blur event on the window, as the iframe taking focus gives it.
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
      const items = Array.from({ length: 30 }, (_, i) => ({ label: `Media Server ${i + 1}`, checked: i === 25 }));
      const { unmount } = render(Menu, { items, x: 10, y: 10, onClose: () => {} });
      expect(screen.getByRole("menu").classList.contains("scrolls")).toBe(true);
      await expect.poll(() => shown.map((s) => s.replace(/\s+/g, " "))).toContain("✓ Media Server 26");
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
});
