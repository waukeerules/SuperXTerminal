// Puts the app's Menu (src/lib/ui/Menu.svelte) on a test page by itself, for tests/browser/menu.spec.ts: the page
// imports this through the browser tests' Vite server, which compiles the component as the app's does.
import { mount } from "svelte";
import Menu, { type MenuItem } from "../../../src/lib/ui/Menu.svelte";

/**
 * Show a menu at (x, y) in the page
 * @param items Its rows
 * @param x Its left edge
 * @param y Its top edge
 */
export function showMenu(items: MenuItem[], x: number, y: number): void {
  mount(Menu, { target: document.body, props: { items, x, y, onClose: () => {} } });
}
