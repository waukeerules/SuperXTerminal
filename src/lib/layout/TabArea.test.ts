// The tab area (dockview) with two session tabs: switching tabs puts the keyboard in the chosen tab's terminal. The
// terminal pages are stood in for by their iframes' postMessage (jsdom loads no page), which records the "focus" the
// tab area sends the page; the Rust side by mockIPC, answering every command with nothing.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { mockIPC } from "@tauri-apps/api/mocks";
import { TabArea } from "./TabArea";

/** The tab area under test, inside an element in the document */
let tabs: TabArea;
let element: HTMLElement;
/** The tab each "focus" message went to, in order */
let focused: string[];

beforeEach(() => {
  // dockview watches its element's size; jsdom has no ResizeObserver, and these tests need no sizes
  globalThis.ResizeObserver ??= class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
  mockIPC(() => null);
  focused = [];
  element = document.createElement("div");
  document.body.append(element);
  tabs = new TabArea(element, () => {});
});

afterEach(() => {
  tabs.api.dispose();
  element.remove();
});

/** Open a session's tab, and record the "focus" its page is sent */
function open(title: string): string {
  const id = tabs.openSession("saved", `Lab/${title}`, title, false, "terminal", null);
  const frame = tabs.api.getPanel(id)!.view.content.element.querySelector("iframe")!;
  frame.contentWindow!.postMessage = ((envelope: { tab: string; message: { type: string } }) => {
    if (envelope.message.type === "focus") focused.push(envelope.tab);
  }) as typeof window.postMessage;
  return id;
}

/** A tab's label in the tab strip, as the user clicks it */
function label(title: string): HTMLElement {
  const found = [...element.querySelectorAll<HTMLElement>(".session-tab")].find((e) => e.textContent?.includes(title));
  if (!found) throw new Error(`no tab ${title}`);
  return found;
}

describe("switching tabs", () => {
  it("puts the keyboard in the terminal of the tab clicked", async () => {
    const sw1 = open("sw1");
    open("sw2");
    focused = [];
    // dockview switches on the press; the click comes as the button is let go
    label("sw1").dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0 }));
    label("sw1").click();
    await expect.poll(() => focused).toEqual([sw1]);
    expect(tabs.api.activePanel?.id).toBe(sw1);
  });

  it("puts the keyboard in the terminal of the tab chosen from the open tabs' list or Next Tab", async () => {
    const sw1 = open("sw1");
    const sw2 = open("sw2");
    focused = [];
    tabs.activate(sw1);
    await expect.poll(() => focused).toEqual([sw1]);
    tabs.cycle(1);
    await expect.poll(() => focused).toEqual([sw1, sw2]);
  });
});
