// A file dropped on the main window where nothing takes it doesn't open there (the browser's own answer, which
// replaced the whole window with the file); drags within the window (text, a tab) aren't touched. The drags are
// events dispatched on the window with a stand-in dataTransfer (jsdom has no DataTransfer).
import { describe, expect, it } from "vitest";
import { stopFileDropsOpening } from "./fileDrops";

/** A drag event carrying these kinds of data, dispatched on the window; whether the browser's own answer was stopped */
function drag(type: string, types: string[]): boolean {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "dataTransfer", { value: { types } });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

describe("a file dropped on the window", () => {
  it("never opens there, while a drag of text or a tab is left alone", () => {
    stopFileDropsOpening(window);
    expect(drag("dragover", ["Files"])).toBe(true);
    expect(drag("drop", ["Files"])).toBe(true);
    expect(drag("dragover", ["text/plain"])).toBe(false);
    expect(drag("drop", ["text/plain"])).toBe(false);
  });
});
