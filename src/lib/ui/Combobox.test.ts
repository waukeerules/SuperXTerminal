// The Profile field's list: every option on opening, the current one ticked; narrowed as it's typed in; picked with
// a click or the keys; Escape closes only the list. A name not in the list can still be typed.
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import Combobox from "./Combobox.svelte";

const PROFILES = ["Default Settings", "root", "CyberArk", "netops"];

/** The field, opened on `value`, with what it was changed to kept in `changes` */
function field(value: string) {
  const changes: string[] = [];
  render(Combobox, { value, options: PROFILES, ariaLabel: "Profile", onChange: (v: string) => changes.push(v) });
  return { input: screen.getByRole("combobox", { name: "Profile" }) as HTMLInputElement, changes };
}

/** The options listed now */
const listed = () => screen.queryAllByRole("option").map((o) => o.textContent?.replace("✓", "").trim());

describe("Combobox", () => {
  it("shows every option on opening, the current one ticked, though a name is in the field", async () => {
    const user = userEvent.setup();
    const { input } = field("root");
    await user.click(input);
    expect(listed()).toEqual(PROFILES);
    const selected = screen.getAllByRole("option").filter((o) => o.getAttribute("aria-selected") === "true");
    expect(selected.map((o) => o.textContent)).toEqual(["✓root"]);
    expect(input.getAttribute("aria-expanded")).toBe("true");
  });

  it("narrows the list as it's typed in, ignoring case, and picks with the keys", async () => {
    const user = userEvent.setup();
    const { input, changes } = field("");
    await user.click(input);
    await user.type(input, "ET");
    expect(listed()).toEqual(["Default Settings", "netops"]);
    await user.keyboard("{ArrowDown}{Enter}");
    expect(input.value).toBe("netops");
    expect(changes.at(-1)).toBe("netops");
    expect(screen.queryByRole("listbox")).toBeNull();
  });

  it("picks with a click, and opens on the arrow button or the Down key", async () => {
    const user = userEvent.setup();
    const { input } = field("root");
    await user.click(screen.getByRole("button", { name: "Show the list" }));
    await user.click(screen.getByRole("option", { name: /CyberArk/ }));
    expect(input.value).toBe("CyberArk");
    await user.keyboard("{ArrowDown}");
    expect(listed()).toEqual(PROFILES);
  });

  it("closes the list on Escape without the Escape going further, and keeps a name not in it", async () => {
    const user = userEvent.setup();
    let escaped = false;
    window.addEventListener("keydown", (e) => e.key === "Escape" && (escaped = true), { once: true });
    const { input } = field("");
    await user.click(input);
    await user.type(input, "lab-only");
    expect(screen.getByText(/None match/)).toBeTruthy();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(escaped).toBe(false);
    expect(input.value).toBe("lab-only");
  });
});
