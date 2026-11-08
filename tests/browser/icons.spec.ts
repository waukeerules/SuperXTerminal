// The session icons' drawings (src/lib/icons/templates.ts) as the engines the app runs in fill them: each template's
// SVG put on a page of its own, and the engine asked which points of a shape are filled (SVGGeometryElement's
// isPointInFill), for shapes whose parts overlap.
import { expect, test } from "@playwright/test";
import { templates } from "../../src/lib/icons/templates";

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
