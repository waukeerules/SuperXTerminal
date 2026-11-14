// Puts the app's main window (src/App.svelte) on a test page, for tests/browser/window.spec.ts: the page imports
// this through the browser tests' Vite server, which compiles the window as the app's does, with its styles, so a
// test sees what's shown and hidden as the app would. The Rust side is stood in for with Tauri's mockIPC: the
// settings given here, one saved session, and nothing for everything else, so no tab's session ever starts.
import { mount } from "svelte";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import "virtual:dockview.css";
import "../../../src/app.css";
import App from "../../../src/App.svelte";

/** Every command the window sent, in order, for a test to wait on */
const sent: string[] = [];

/**
 * Show the main window with these settings over the app's own (`sessionsAutoHide` folds the Sessions pane to its
 * edge tab).
 * @returns `sent`: the commands the window has sent the Rust side
 */
export function showWindow(settings: Record<string, unknown>) {
  mockWindows("main");
  let kept: Record<string, unknown> = { settingsFolder: "/team", iconSize: 20, sessionsPaneWidth: 300, ...settings };
  mockIPC((cmd, args) => {
    sent.push(cmd);
    if (cmd === "settings_get") return kept;
    if (cmd === "settings_save") return (kept = (args as { settings: Record<string, unknown> }).settings);
    if (cmd === "sessions_load" || cmd === "sessions_shown") {
      return { sessions: [], folders: [], problems: [], savedFile: "/team/Sessions.xml", privateFile: "/me/PrivateSessions.xml", savedOff: false, privateOff: true };
    }
    if (cmd === "shortcuts_get" || cmd === "shortcuts_opened") return { Favorites: [], Recent: [], FavoritesExpanded: false, RecentExpanded: false };
    if (cmd === "profiles_list") return [{ name: "Default Settings" }];
    if (cmd === "overrides_get") return {};
    // lists the window reads as it opens: one button bar, as there always is, and none of the rest
    if (cmd === "buttonbars_load") return [{ name: "Default", buttons: [], tabIcons: [] }];
    if (["ui_themes_load", "requests_take", "icons_custom", "variables_list", "variables_built_in"].includes(cmd)) return [];
    if (cmd === "highlighting_load") return { ruleSets: [], themes: [], problems: [] };
    if (cmd === "sessions_files_changed") return false;
    if (cmd === "plugin:app|version") return "0.0.0";
    return null;
  }, { shouldMockEvents: true });
  mount(App, { target: document.getElementById("app")! });
  return { sent };
}
