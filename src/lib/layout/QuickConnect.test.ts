// The Quick connect toolbar with several hosts in its Host field: a tab for each, with the same Login, Password and
// Profile; a column of hosts pasted in; the question asked before a great many are opened; and the list of hosts
// its button opens under the Host box, one host on each line, which the Host box follows. The toolbar is
// rendered on its own, with onConnect noting what it would open. The Rust side is stood in for by mockIPC: the
// profiles, and the answer to the question (`answer`).
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import QuickConnect from "./QuickConnect.svelte";
import type { QuickConnect as QuickFields } from "../terminal/PageHost";

/** What onConnect was given, a call for each tab */
let opened: { quick: QuickFields; title: string }[];
/** What the status bar was told */
let problems: string[];
/** What was asked before opening, and the button it's answered with */
let asked: string[];
let answer: "Yes" | "No";

beforeEach(() => {
  opened = [];
  problems = [];
  asked = [];
  answer = "Yes";
  mockIPC((cmd, args) => {
    if (cmd === "profiles_list") return [{ name: "Default Settings" }, { name: "Lab" }];
    if (cmd === "plugin:dialog|message") {
      asked.push(String((args as Record<string, unknown>).message));
      return answer;
    }
    return null;
  });
});

/** The toolbar, with its Host box */
function bar() {
  render(QuickConnect, {
    onConnect: (quick: QuickFields, title: string) => opened.push({ quick, title }),
    onProblem: (text: string) => problems.push(text),
    onEditProfile: () => {},
  });
  return screen.getByLabelText(/^Host/) as HTMLInputElement;
}

describe("Quick connect with several hosts", () => {
  it("opens a tab for each, named for its host, with the Login and Password typed", async () => {
    const user = userEvent.setup();
    const host = bar();
    await user.type(host, "10.0.0.1, admin@sw2:2222 telnet://10.0.0.3");
    await user.type(screen.getByLabelText(/^Login/), "netops");
    await user.type(screen.getByLabelText(/^Password/), "secret{Enter}");
    await Promise.resolve();
    expect(opened.map((o) => o.title)).toEqual(["10.0.0.1", "admin@sw2:2222", "10.0.0.3"]);
    expect(opened.map((o) => o.quick.host)).toEqual(["10.0.0.1", "admin@sw2:2222", "telnet://10.0.0.3"]);
    expect(opened.every((o) => o.quick.username === "netops" && o.quick.password === "secret" && o.quick.protocol === "SSH")).toBe(true);
    expect(asked).toEqual([]);
  });

  it("still opens the one host typed, as before", async () => {
    const user = userEvent.setup();
    await user.type(bar(), "192.168.0.5{Enter}");
    await Promise.resolve();
    expect(opened.map((o) => o.quick.host)).toEqual(["192.168.0.5"]);
  });

  it("takes a column of hosts pasted in, with commas between, after one already typed", async () => {
    const user = userEvent.setup();
    const host = bar();
    await user.type(host, "sw1");
    await user.paste("10.0.0.1\n10.0.0.2\n");
    expect(host.value).toBe("sw1, 10.0.0.1, 10.0.0.2");
    await user.keyboard("{Enter}");
    await Promise.resolve();
    expect(opened.map((o) => o.quick.host)).toEqual(["sw1", "10.0.0.1", "10.0.0.2"]);
  });

  it("asks before opening more than ten, and opens none on No", async () => {
    const user = userEvent.setup();
    const eleven = Array.from({ length: 11 }, (_, i) => `10.0.0.${i + 1}`).join(",");
    answer = "No";
    const host = bar();
    await user.type(host, `${eleven}{Enter}`);
    await new Promise((done) => setTimeout(done));
    expect(asked).toEqual(["Open 11 sessions, one for each host typed?"]);
    expect(opened).toEqual([]);
    answer = "Yes";
    await user.type(host, "{Enter}");
    await new Promise((done) => setTimeout(done));
    expect(opened).toHaveLength(11);
  });

  it("opens none of them when one can't be opened, and says why", async () => {
    const user = userEvent.setup();
    await user.type(bar(), "10.0.0.1, mintty://10.0.0.2{Enter}");
    await Promise.resolve();
    expect(opened).toEqual([]);
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatch(/^MINTTY: /);
  });
});

describe("Quick connect's list of hosts", () => {
  /** The list's box of lines, once its button has opened it */
  const lines = () => screen.getByLabelText("One host on each line") as HTMLTextAreaElement;

  it("opens with the hosts already in the Host box, one on each line", async () => {
    const user = userEvent.setup();
    await user.type(bar(), "sw1, sw2;sw3");
    await user.click(screen.getByRole("button", { name: "List of hosts" }));
    expect(lines().value).toBe("sw1\nsw2\nsw3");
    expect(document.activeElement).toBe(lines());
  });

  it("takes rows pasted from a spreadsheet, and the Host box holds them with commas between", async () => {
    const user = userEvent.setup();
    const host = bar();
    await user.click(screen.getByRole("button", { name: "List of hosts" }));
    await user.paste("10.0.0.1\r\n10.0.0.2\r\nsw3.example.com\r\n");
    expect(host.value).toBe("10.0.0.1, 10.0.0.2, sw3.example.com");
    expect(screen.getByText(/^3 hosts/)).toBeTruthy();
  });

  it("goes on Escape, leaving the hosts in the Host box, where Enter connects to each", async () => {
    const user = userEvent.setup();
    const host = bar();
    await user.click(screen.getByRole("button", { name: "List of hosts" }));
    await user.type(lines(), "sw1{Enter}sw2{Enter}");
    await user.keyboard("{Escape}");
    expect(screen.queryByLabelText("One host on each line")).toBeNull();
    expect(host.value).toBe("sw1, sw2");
    expect(document.activeElement).toBe(host);
    await user.keyboard("{Enter}");
    await Promise.resolve();
    expect(opened.map((o) => o.quick.host)).toEqual(["sw1", "sw2"]);
  });

  it("goes when another box is clicked, the hosts kept", async () => {
    const user = userEvent.setup();
    const host = bar();
    await user.click(screen.getByRole("button", { name: "List of hosts" }));
    await user.type(lines(), "sw1{Enter}sw2");
    await user.click(screen.getByLabelText(/^Login/));
    await new Promise((done) => setTimeout(done));
    expect(screen.queryByLabelText("One host on each line")).toBeNull();
    expect(host.value).toBe("sw1, sw2");
  });

  it("closes on its button again", async () => {
    const user = userEvent.setup();
    bar();
    await user.click(screen.getByRole("button", { name: "List of hosts" }));
    await user.click(screen.getByRole("button", { name: "List of hosts" }));
    await new Promise((done) => setTimeout(done));
    expect(screen.queryByLabelText("One host on each line")).toBeNull();
  });

  it("connects to each host on Ctrl+Enter, where Enter alone starts a new line", async () => {
    const user = userEvent.setup();
    bar();
    await user.click(screen.getByRole("button", { name: "List of hosts" }));
    await user.type(lines(), "sw1{Enter}sw2{Control>}{Enter}{/Control}");
    await Promise.resolve();
    expect(opened.map((o) => o.title)).toEqual(["sw1", "sw2"]);
    expect(screen.queryByLabelText("One host on each line")).toBeNull();
  });
});
