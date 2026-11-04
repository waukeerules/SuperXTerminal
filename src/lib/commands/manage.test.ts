// The changes Manage Button Bars makes: moving and copying buttons between bars without doubling them, merging a bar
// imported as "Name (2)" into the one it copies, and spotting duplicate buttons and labels used for two commands.
// Plain functions on plain bars, so nothing is stood in for.
import { describe, expect, it } from "vitest";
import type { Bar, BarButton } from "./bars";
import { clashes, copies, duplicateBar, duplicates, merge, reorder, transfer } from "./manage";

/** A button sending `text`, labelled `label` */
const button = (label: string, text = `${label}\n`): BarButton => ({ label, tooltip: "", color: "", icon: "", action: "SendString", text });

/** A bar with these buttons */
const bar = (name: string, ...buttons: BarButton[]): Bar => ({ name, buttons, tabIcons: [] });

/** Each bar's button labels, by bar name */
const labels = (bars: Bar[]) => Object.fromEntries(bars.map((b) => [b.name, b.buttons.map((x) => x.label)]));

describe("moving and copying buttons", () => {
  const bars = [bar("Cisco", button("show run"), button("show ver"), button("wr")), bar("Linux", button("ls"))];

  it("moves the buttons chosen to the end of the other bar", () => {
    const { bars: next, skipped } = transfer(bars, "Cisco", [0, 2], "Linux", false);
    expect(labels(next)).toEqual({ Cisco: ["show ver"], Linux: ["ls", "show run", "wr"] });
    expect(skipped).toBe(0);
  });

  it("copies them, leaving the first bar as it was", () => {
    const { bars: next } = transfer(bars, "Cisco", [1], "Linux", true);
    expect(labels(next)).toEqual({ Cisco: ["show run", "show ver", "wr"], Linux: ["ls", "show ver"] });
  });

  it("doesn't add a button the other bar has the same of, and a move still takes it off", () => {
    const withCopy = [bars[0], bar("Linux", button("ls"), button("wr"))];
    const copied = transfer(withCopy, "Cisco", [2], "Linux", true);
    expect(labels(copied.bars).Linux).toEqual(["ls", "wr"]);
    expect(copied.skipped).toBe(1);
    const moved = transfer(withCopy, "Cisco", [2], "Linux", false);
    expect(labels(moved.bars)).toEqual({ Cisco: ["show run", "show ver"], Linux: ["ls", "wr"] });
  });

  it("leaves the bars given alone", () => {
    transfer(bars, "Cisco", [0], "Linux", false);
    expect(labels(bars)).toEqual({ Cisco: ["show run", "show ver", "wr"], Linux: ["ls"] });
  });
});

describe("merging a bar into another", () => {
  it("adds the buttons it hasn't got, joins the tab icons and takes the bar away", () => {
    const original = { ...bar("Cisco", button("show run"), button("wr")), tabIcons: ["switch"] };
    const imported = { ...bar("Cisco (2)", button("show run"), button("show ip int br"), button("wr", "write memory\n")), tabIcons: ["router", "Switch"] };
    const result = merge([original, imported], "Cisco (2)", "Cisco");
    expect(labels(result.bars)).toEqual({ Cisco: ["show run", "wr", "show ip int br", "wr"] });
    expect(result.bars[0].tabIcons).toEqual(["switch", "router"]);
    expect(result).toMatchObject({ added: 2, skipped: 1, clashing: 1 });
  });
});

describe("what needs tidying", () => {
  it("finds a button the same as one before it, but not one with another command", () => {
    const b = bar("Cisco", button("wr"), button("show run"), button("WR", "wr\n"), button("wr", "write memory\n"));
    expect([...duplicates(b)]).toEqual([2]);
    expect([...clashes(b)].sort()).toEqual([0, 2, 3]);
  });

  it("takes a bar named as an import names a clash for a copy of the other", () => {
    const bars = [bar("Cisco"), bar("Cisco (2)"), bar("Linux (3)"), bar("Juniper")];
    expect(copies(bars)).toEqual([{ copy: "Cisco (2)", of: "Cisco" }]);
  });
});

describe("bars", () => {
  it("copies a bar after itself under a name not taken", () => {
    const bars = [bar("Cisco", button("wr")), bar("Cisco copy"), bar("Linux")];
    const next = duplicateBar(bars, "Cisco");
    expect(next.map((b) => b.name)).toEqual(["Cisco", "Cisco copy (2)", "Cisco copy", "Linux"]);
    expect(labels(next)["Cisco copy (2)"]).toEqual(["wr"]);
  });

  it("puts an item somewhere else in its list", () => {
    expect(reorder(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
    expect(reorder(["a", "b", "c"], 2, 5)).toEqual(["a", "b", "c"]);
  });
});
