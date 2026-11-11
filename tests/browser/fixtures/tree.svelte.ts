// Puts the Sessions tree (src/lib/sessions/SessionTree.svelte) on a test page by itself, for tests/browser/tree.spec.ts,
// with as many sessions as a test asks for: the page imports this through the browser tests' Vite server, which
// compiles the component as the app's does. The Rust side is stood in for with Tauri's mockIPC: `sessions_load` and
// `sessions_shown` give the sessions made here, everything else nothing. A `.svelte.ts` file, so the tree's
// `openSessions` can be changed after it's mounted (a tab connecting), as the app changes it.
import { mount } from "svelte";
import { mockIPC } from "@tauri-apps/api/mocks";
import "../../../src/app.css";
import SessionTree from "../../../src/lib/sessions/SessionTree.svelte";
import { settings } from "../../../src/lib/settings/store.svelte";
import type { SessionView } from "../../../src/lib/sessions/api";
import type { SessionState } from "../../../src/lib/layout/TabArea";

/** A Saved SSH session with this id, its name and folder taken from it */
function session(id: string, n: number): SessionView {
  const name = id.slice(id.lastIndexOf("/") + 1);
  const folder = id.includes("/") ? id.slice(0, id.lastIndexOf("/")) : "";
  return {
    id, store: "saved", name, folder, protocol: "SSH", host: `10.${(n >> 16) & 255}.${(n >> 8) & 255}.${n & 255}`, port: 22, username: "netops", profile: "",
    icon: "switch", color: "", note: "", extraArgs: "", script: "", buttonBar: "", updatedBy: "", locked: false, lockedFields: "", pending: false, pendingNote: "",
  };
}

/**
 * `count` sessions in folders of `perFolder`: "Site 001/sw-001-001"… With `perFolder` as large as `count` they're all
 * in one folder, the worst there is for the tree: every one a row when it's open
 */
function sessions(count: number, perFolder: number): SessionView[] {
  return Array.from({ length: count }, (_, n) => {
    const site = String(Math.floor(n / perFolder) + 1).padStart(3, "0");
    return session(`Site ${site}/sw-${site}-${String((n % perFolder) + 1).padStart(5, "0")}`, n);
  });
}

/** The tree's properties, changed by a test after it's mounted */
const props = $state({
  openSessions: new Map<string, SessionState>(),
  onOpen: () => {},
  onLook: () => {},
  onOpenProfiles: () => {},
});

/** How many times the tree has asked for the sessions, files read or not */
let asked = 0;

/**
 * Show the tree with `count` sessions, `perFolder` to a folder, and `recent` (session ids) in Recent, which is open
 * @returns What a test drives it with: `reload` reads the sessions again as File > Reload Sessions does, `setOpen`
 *   says which sessions have a tab and how it stands, `asked` how often the sessions have been asked for
 */
export function showTree(count: number, perFolder: number, recent: string[] = []) {
  const list = sessions(count, perFolder);
  mockIPC((cmd) => {
    if (cmd === "sessions_load" || cmd === "sessions_shown") {
      asked += 1;
      return { sessions: list, folders: [], problems: [], savedFile: "/team/Sessions.xml", privateFile: "/me/PrivateSessions.xml", savedOff: false, privateOff: true };
    }
    if (cmd === "shortcuts_get" || cmd === "shortcuts_opened") return { Favorites: [], Recent: recent, FavoritesExpanded: false, RecentExpanded: recent.length > 0 };
    if (cmd === "overrides_get") return {};
    if (cmd === "icons_custom") return [];
    if (cmd === "sessions_files_changed") return false;
    return null;
  });
  Object.assign(settings, { loaded: true, settingsFolder: "/team", treeSearchFields: [], treeSearchCase: false, treeSearchRegex: false });
  const tree = mount(SessionTree, { target: document.getElementById("pane")!, props });
  return {
    reload: () => tree.reloadSessions(),
    setOpen: (states: [string, SessionState][]) => {
      props.openSessions = new Map(states);
    },
    asked: () => asked,
  };
}
