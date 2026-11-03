// Settings > Profiles: which fields show as changed, what's greyed while there are changes, Save and Revert, and the
// list's Duplicate. The Rust side is stood in for by mockIPC: the profiles live in `stored`, and every save and delete
// is recorded in `calls`.
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { mockIPC } from "@tauri-apps/api/mocks";
import ProfilesTab from "./ProfilesTab.svelte";
import { DEFAULT_PROFILE, emptyProfile, type Profile } from "./profiles";

let stored: Profile[];
let calls: { cmd: string; args: Record<string, unknown> }[];

beforeEach(() => {
  stored = [
    { ...emptyProfile(DEFAULT_PROFILE), olderAlgorithms: true },
    { ...emptyProfile("root"), username: "root", port: 2222 },
  ];
  calls = [];
  mockIPC((cmd, args) => {
    const payload = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: payload });
    if (cmd === "profiles_list") return stored;
    if (cmd === "profiles_save") {
      const profile = payload.profile as Profile;
      const previous = String(payload.previousName ?? "");
      stored = [...stored.filter((p) => p.name !== (previous || profile.name)), { ...profile }];
      return stored;
    }
    if (cmd === "profiles_delete") {
      stored = stored.filter((p) => p.name !== payload.name);
      return stored;
    }
    // the dialog plugin's "Are you sure?": yes
    if (cmd === "plugin:dialog|ask") return true;
    return null;
  });
});

/** Render the tab and wait for the list to load */
async function open(initial: string | null = null) {
  render(ProfilesTab, { initial });
  await screen.findByRole("option", { name: "root" });
}

const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;
const dots = () => screen.queryAllByLabelText("changed");

describe("Settings > Profiles", () => {
  it("opens on the profile asked for, matched whatever its case", async () => {
    await open("ROOT");
    expect(screen.getByRole("option", { name: "root" }).getAttribute("aria-selected")).toBe("true");
    expect((screen.getByLabelText("Username") as HTMLInputElement).value).toBe("root");
  });

  it("has Save and Revert greyed until something changes, then marks the field", async () => {
    await open("root");
    expect(button("Save").disabled).toBe(true);
    expect(button("Revert").disabled).toBe(true);
    expect(dots()).toHaveLength(0);

    await userEvent.type(screen.getByLabelText("Username"), "x");
    expect(button("Save").disabled).toBe(false);
    expect(screen.getByText("Unsaved changes (1)")).toBeTruthy();
    expect(dots()).toHaveLength(1);
  });

  it("greys New, Remove and the other profiles while there are changes", async () => {
    await open("root");
    await userEvent.type(screen.getByLabelText("Username"), "x");
    expect(button("New").disabled).toBe(true);
    expect(button("Remove").disabled).toBe(true);
    expect(screen.getByRole("option", { name: DEFAULT_PROFILE }).getAttribute("aria-disabled")).toBe("true");
  });

  it("doesn't count retyping the saved value as a change", async () => {
    await open("root");
    // its label also holds the note "0: not set"
    const port = screen.getByLabelText(/^Port/) as HTMLInputElement;
    await userEvent.clear(port);
    await userEvent.type(port, "2222");
    expect(dots()).toHaveLength(0);
    expect(button("Save").disabled).toBe(true);
  });

  it("puts the fields back with Revert", async () => {
    await open("root");
    const username = screen.getByLabelText("Username") as HTMLInputElement;
    await userEvent.type(username, "x");
    await userEvent.click(button("Revert"));
    expect(username.value).toBe("root");
    expect(button("Save").disabled).toBe(true);
  });

  it("saves the changed profile under its old name", async () => {
    await open("root");
    await userEvent.type(screen.getByLabelText("Username"), "x");
    await userEvent.click(button("Save"));
    const save = calls.find((c) => c.cmd === "profiles_save");
    expect(save?.args.previousName).toBe("root");
    expect((save?.args.profile as Profile).username).toBe("rootx");
    expect(await screen.findByText("Saved.")).toBeTruthy();
    expect(dots()).toHaveLength(0);
  });

  it("duplicates from the list's menu under the next free name, without the password", async () => {
    await open("root");
    await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByRole("option", { name: "root" }) });
    await userEvent.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Duplicate" }));
    const save = calls.find((c) => c.cmd === "profiles_save");
    expect((save?.args.profile as Profile).name).toBe("root-1");
    expect(save?.args.previousName).toBe("");
    expect(save?.args.password).toBeNull();
    expect(await screen.findByRole("option", { name: "root-1" })).toBeTruthy();
  });

  it("can't delete Default Settings from the menu", async () => {
    await open();
    await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByRole("option", { name: DEFAULT_PROFILE }) });
    const remove = within(screen.getByRole("menu")).getByRole("menuitem", { name: "Delete…" });
    expect(remove.getAttribute("aria-disabled")).toBe("true");
  });
});
