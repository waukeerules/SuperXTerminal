// The server load graph (ServerLoadChart.svelte), opened from the status bar: its two charts' lines and their end
// labels (each line's latest value, so none is told by colour alone), Table as rows newest first, the arrow keys
// moving through the readings, the readout keeping its size (two rows) as they do, and Escape closing it. Readings are made up here, a minute of them 3 s apart; jsdom
// draws no layout, so the pointer isn't tested (the arrow keys move the same crosshair).
import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import ServerLoadChart from "./ServerLoadChart.svelte";
import type { Sample } from "./serverLoad.svelte";

/** 21 readings over the last minute: CPU climbing to 20%, memory 16%, disk 50%, traffic 2 KB/s in and 1 KB/s out */
function minute(): Sample[] {
  const now = Date.now();
  return Array.from({ length: 21 }, (_, i) => ({
    at: now - (20 - i) * 3000,
    cpu: i === 0 ? null : i,
    mem: 16,
    disk: 50,
    down: i === 0 ? null : 2048,
    up: i === 0 ? null : 1024,
  }));
}

/** Show the graph; how many times it asked to close */
function open(samples = minute()) {
  let closed = 0;
  render(ServerLoadChart, { samples, tab: "omv", onClose: () => closed++ });
  return () => closed;
}

describe("the server load graph", () => {
  it("draws CPU, memory and disk in one chart and traffic in another, each line labelled with its latest value", () => {
    open();
    expect(screen.getByRole("dialog", { name: "Server load of omv" })).toBeTruthy();
    const [use, network] = screen.getAllByRole("slider");
    expect(use.getAttribute("aria-label")).toMatch(/^Use/);
    expect(network.getAttribute("aria-label")).toMatch(/^Network/);
    expect([...use.querySelectorAll("path")].every((p) => p.getAttribute("d")?.startsWith("M"))).toBe(true);
    expect(use.querySelectorAll("path")).toHaveLength(3);
    expect(network.querySelectorAll("path")).toHaveLength(2);
    expect([...use.querySelectorAll("text.end")].map((t) => t.textContent)).toEqual(["CPU 20%", "Memory 16%", "Disk 50%"]);
    expect([...network.querySelectorAll("text.end")].map((t) => t.textContent)).toEqual(["In ↓ 2.0 KB/s", "Out ↑ 1.0 KB/s"]);
  });

  it("moves through the readings with the arrow keys, saying each one's figures", async () => {
    open();
    const use = screen.getAllByRole("slider")[0];
    expect(use.getAttribute("aria-valuetext")).toBe("now: CPU 20%, Memory 16%, Disk 50%");
    await fireEvent.keyDown(use, { key: "ArrowLeft" });
    await fireEvent.keyDown(use, { key: "ArrowLeft" });
    expect(use.getAttribute("aria-valuetext")).toBe("6 s ago: CPU 18%, Memory 16%, Disk 50%");
    expect(use.querySelector("line.crosshair")).not.toBeNull();
    expect(screen.getByText("6 s ago", { selector: ".when" })).toBeTruthy();
    // the first reading has no CPU: said as —
    await fireEvent.keyDown(use, { key: "Home" });
    expect(use.getAttribute("aria-valuetext")).toBe("1 min ago: CPU —, Memory 16%, Disk 50%");
  });

  it("keeps the readout two rows, hint or figures, so the panel doesn't jump as the pointer comes and goes", async () => {
    open();
    const rows = () => [...document.querySelectorAll(".readout > p")];
    expect(rows()).toHaveLength(2);
    expect(rows()[0].textContent).toMatch(/^Point at a chart/);
    const use = screen.getAllByRole("slider")[0];
    await fireEvent.keyDown(use, { key: "ArrowLeft" });
    await fireEvent.keyDown(use, { key: "ArrowLeft" });
    // Use's figures on the first row, Network's on the second (after the time's column), as the charts are
    const items = (row: Element) => [...row.children].map((c) => c.textContent?.replace(/\s+/g, " ").trim());
    expect(rows()).toHaveLength(2);
    expect(items(rows()[0])).toEqual(["6 s ago", "18% CPU", "16% Memory", "50% Disk"]);
    expect(items(rows()[1])).toEqual(["", "2.0 KB/s In ↓", "1.0 KB/s Out ↑"]);
  });

  it("shows the same readings as rows, newest first, under Table", async () => {
    open();
    await fireEvent.click(screen.getByRole("button", { name: "Table" }));
    const rows = within(screen.getByRole("table")).getAllByRole("row");
    expect(rows).toHaveLength(22);
    expect([...rows[1].querySelectorAll("td")].map((c) => c.textContent)).toEqual(["now", "20%", "16%", "50%", "2.0 KB/s", "1.0 KB/s"]);
    expect([...rows[21].querySelectorAll("td")].map((c) => c.textContent)).toEqual(["1 min ago", "—", "16%", "50%", "—", "—"]);
    expect(screen.getByRole("button", { name: "Graph" })).toBeTruthy();
  });

  it("closes on Escape and on ×, and says when there are no readings yet", async () => {
    const closed = open([]);
    expect(screen.getByText(/No readings yet/)).toBeTruthy();
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed()).toBe(1);
    await fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(closed()).toBe(2);
  });
});
