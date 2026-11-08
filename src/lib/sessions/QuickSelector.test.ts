// The Quick Selector (File > Open Session…, Window > Switch Session…): a click on a row takes it, as Enter takes the
// highlighted one, and the pointer moving over a row highlights it. Rendered on its own, with the open tabs given as
// its items and onChoose noting what's chosen; no Rust side is needed.
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import QuickSelector, { type QuickItem } from "./QuickSelector.svelte";

/** Two open tabs: a Local Shell and a switch, the one in front */
const TABS: QuickItem[] = [
  { key: "t1", name: "Local Shell", detail: "", icon: "terminal", environment: null },
  { key: "t2", name: "Switch", detail: "Private/Switch", icon: "switch", environment: null, current: true },
];

describe("Switch Session", () => {
  it("switches to the row clicked, with one click", async () => {
    const user = userEvent.setup();
    const chosen: string[] = [];
    render(QuickSelector, { title: "Switch Session", items: TABS, customs: new Map(), onChoose: (item) => chosen.push(item.name), onClose: () => {} });
    await user.click(screen.getByRole("option", { name: /Switch/ }));
    expect(chosen).toEqual(["Switch"]);
  });

  it("takes the highlighted row on Enter: the first that isn't the tab in front", async () => {
    const user = userEvent.setup();
    const chosen: string[] = [];
    render(QuickSelector, { title: "Switch Session", items: TABS, customs: new Map(), onChoose: (item) => chosen.push(item.name), onClose: () => {} });
    await user.keyboard("{Enter}");
    expect(chosen).toEqual(["Local Shell"]);
  });
});

describe("the Quick Selector over a dialog", () => {
  it("keeps its Escape from the window, so the dialog it was opened over (Edit Session) stays open", async () => {
    let windowEscapes = 0;
    const count = (e: KeyboardEvent) => e.key === "Escape" && windowEscapes++;
    window.addEventListener("keydown", count);
    let closed = 0;
    try {
      render(QuickSelector, { title: "Jump Host", items: [], customs: new Map(), onChoose: () => {}, onClose: () => closed++ });
      await userEvent.keyboard("{Escape}");
      expect(closed).toBe(1);
      expect(windowEscapes).toBe(0);
    } finally {
      window.removeEventListener("keydown", count);
    }
  });
});
