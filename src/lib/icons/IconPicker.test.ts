// Choose Icon: the colour picked is the one every icon in the list is drawn in, as in the preview, so each shows how
// it would look; Default draws each in its own again. Each choice's drawing is compared with its template's (or
// logo's) drawn in the colour expected.
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import IconPicker from "./IconPicker.svelte";
import { templates } from "./templates";
import { logos } from "./brands";

/** A drawing as the page holds it, with each icon's own SVG id prefix (i0, i1, ...) the same */
function normal(svg: string): string {
  const holder = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  holder.innerHTML = svg;
  return holder.innerHTML.replace(/\bi\d+/g, "uid");
}

/** Each icon in the list: its key (from its title) and its drawing */
function choices(): { key: string; svg: SVGElement }[] {
  const byTitle = new Map([
    ...Object.entries(templates).map(([key, t]) => [t.title, key] as const),
    ...Object.entries(logos).map(([key, l]) => [l.title, key] as const),
  ]);
  return [...document.querySelectorAll<HTMLButtonElement>("button.choice")].map((button) => ({
    key: byTitle.get(button.title) ?? button.title,
    svg: button.querySelector("svg")!,
  }));
}

/** Whether a choice is drawn in `color`: its template's drawing in it, or a logo filled with it */
function drawnIn(key: string, svg: SVGElement, color: string): boolean {
  if (templates[key]) return normal(svg.innerHTML) === normal(templates[key].draw(color, "i0"));
  return svg.querySelector("path")?.getAttribute("fill")?.toUpperCase() === color.toUpperCase();
}

describe("Choose Icon", () => {
  it("draws every icon in the colour picked, and in its own again on Default", async () => {
    const user = userEvent.setup();
    render(IconPicker, { value: "switch", customs: new Map(), onChoose: () => {}, onCancel: () => {} });
    const all = choices();
    expect(all.length).toBeGreaterThan(10);
    expect(all.some((c) => logos[c.key])).toBe(true);

    await user.click(screen.getByRole("button", { name: "Purple" }));
    const purple = choices().filter((c) => !drawnIn(c.key, c.svg, "#6A3D9A")).map((c) => c.key);
    expect(purple, "icons not drawn in the colour picked").toEqual([]);

    await user.click(screen.getByRole("button", { name: "Default" }));
    const own = choices().filter((c) => !drawnIn(c.key, c.svg, templates[c.key]?.color ?? logos[c.key].color)).map((c) => c.key);
    expect(own, "icons not drawn in their own colour").toEqual([]);
  });
});
