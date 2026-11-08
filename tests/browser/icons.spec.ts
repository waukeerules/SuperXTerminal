// The session icons' drawings (src/lib/icons/templates.ts) as the engines the app runs in fill them: each template's
// SVG put on a page of its own, and the engine asked which points of a shape are filled (SVGGeometryElement's
// isPointInFill), for shapes whose parts overlap; and where each is drawn, by the box the engine puts round it.
import { expect, test, type Page } from "@playwright/test";
import { templateOrder, templates } from "../../src/lib/icons/templates";

/** Put an icon on the page, 200 pixels square, in its own colour */
async function drawIcon(page: Page, key: string) {
  await page.setContent(`<body style="margin:0"><svg viewBox="0 0 1 1" width="200" height="200" style="display:block">${templates[key].draw(templates[key].color, "t")}</svg>`);
}

/**
 * Where an icon's marks are drawn, in its 0..1, strokes included: the box round each of its shapes (all of them, or
 * those stroked white, the glyph's), as the engine places it, transforms and all
 */
const drawnBox = (page: Page, whiteOnly = false) => page.evaluate((whiteOnly) => {
  const box = { left: 1, top: 1, right: 0, bottom: 0 };
  for (const shape of document.querySelectorAll<SVGGraphicsElement>("svg path, svg ellipse, svg circle, svg rect, svg line, svg polygon, svg polyline")) {
    if (whiteOnly && shape.getAttribute("stroke")?.toUpperCase() !== "#FFFFFF" && shape.getAttribute("fill")?.toUpperCase() !== "#FFFFFF") continue;
    // the box round its geometry, and half a stroke's width beyond it (the icons' transforms only turn, not scale)
    const at = shape.getBoundingClientRect();
    const stroke = shape.getAttribute("stroke");
    const half = stroke && stroke !== "none" ? Number(shape.getAttribute("stroke-width") ?? 1) / 2 : 0;
    box.left = Math.min(box.left, at.left / 200 - half);
    box.top = Math.min(box.top, at.top / 200 - half);
    box.right = Math.max(box.right, at.right / 200 + half);
    box.bottom = Math.max(box.bottom, at.bottom / 200 + half);
  }
  return box;
}, whiteOnly);

test("fills the Chat bubble where its tail joins it, with no hole the background shows through", async ({ page }) => {
  await page.setContent(`<svg viewBox="0 0 1 1" width="200" height="200">${templates.chat.draw("#2E7D32", "t")}</svg>`);
  /** Whether the bubble (the first shape drawn) fills a point, in the icon's 0..1 */
  const filled = (x: number, y: number) => page.evaluate(({ x, y }) => {
    const bubble = document.querySelector("path") as SVGGeometryElement;
    return bubble.isPointInFill(new DOMPoint(x, y));
  }, { x, y });
  // where the tail overlaps the bubble's bottom edge
  expect(await filled(0.3, 0.66)).toBe(true);
  expect(await filled(0.4, 0.665)).toBe(true);
  // the bubble, and the tail below it, as before
  expect(await filled(0.5, 0.4)).toBe(true);
  expect(await filled(0.24, 0.8)).toBe(true);
  // and outside both
  expect(await filled(0.8, 0.85)).toBe(false);
});

test("draws the Feed's dot and arcs clear of its square's edge", async ({ page }) => {
  await drawIcon(page, "rss");
  // the square's edge is at 0.06 and 0.94
  const marks = await drawnBox(page, true);
  expect(marks.right).toBeLessThan(0.8);
  expect(marks.top).toBeGreaterThan(0.2);
});

test("draws every icon inside its square", async ({ page }) => {
  const over: string[] = [];
  for (const key of templateOrder) {
    await drawIcon(page, key);
    const box = await drawnBox(page);
    if (box.left < -0.01 || box.top < -0.01 || box.right > 1.01 || box.bottom > 1.01) over.push(`${key} ${JSON.stringify(box)}`);
  }
  expect(over).toEqual([]);
});
