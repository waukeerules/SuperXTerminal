//! The app's menu bar: File, Edit, View, Tools and Help as SuperXPuTTY has them, with the keys from Settings >
//! Shortcuts and the View menu's ticks for what's shown. Built when the app starts and again when a setting that
//! shows in it changes (`commands/settings.rs`); choosing an item is handled in `run`'s menu event. Also opens
//! Help > Third-Party Notices.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Runtime};

use crate::settings::AppSettings;
use crate::{applog, keys, opening};

/// SuperXTerminal > Third-Party Notices: the components it ships and their licences (scripts/notices.mjs), in the
/// system's text editor ([`Opening::AsText`]). The app has the file among its resources; a development build reads
/// the project's.
pub(crate) fn open_notices<R: Runtime>(app: &AppHandle<R>) {
    let bundled = app.path().resource_dir().map(|d| d.join("THIRD-PARTY-NOTICES.txt")).ok().filter(|p| p.exists());
    let path = bundled.unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../THIRD-PARTY-NOTICES.txt"));
    // on Windows, the app for .txt files
    #[cfg(windows)]
    let opened = opening::open_in_app(&path).map_err(std::io::Error::other);
    #[cfg(not(windows))]
    let opened = opening::open_command(&path, opening::Opening::AsText).status().map(|_| ());
    if let Err(e) = opened {
        applog::warn(format!("Couldn't open {}: {e}", path.display()));
    }
}

/// The menu bar: the app menu (with Settings..., Cmd+,), File (New Local Shell, Cmd+T), Edit (which WebKit needs for
/// Cmd+C/V), Tools (Environment Variables...) and Window.
/// SuperXPuTTY's File, View and Tools menus join it as their features arrive.
/// Its keys are the settings' (Settings > Shortcuts). Tools > Use Local Overrides is ticked while they're used, and
/// View's panes and bars while they're shown ([`view_checks`]): the menu is made again whenever one of those changes
/// ([`window_changes`]), so the ticks follow however it changed: the item itself, the status bar's "Local overrides
/// off", or a pane's own ×.
pub(crate) fn menu<R: Runtime>(app: &AppHandle<R>, settings: &AppSettings) -> tauri::Result<Menu<R>> {
    let shortcuts = &settings.shortcuts;
    let overrides_on = settings.use_local_overrides;
    // an action's item, with its key now (keys.rs: the user's, else the Mac default)
    let action = |id: &str| MenuItem::with_id(app, id, keys::label(id), true, keys::key(shortcuts, id).as_deref());
    let plain = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>);
    // a View item that shows or hides something: ticked while it's shown, with its key as an action's
    let views = view_checks(settings);
    let shown = |id: &str| {
        let on = views.iter().any(|(view, on)| *view == id && *on);
        CheckMenuItem::with_id(app, id, keys::label(id), true, on, keys::key(shortcuts, id).as_deref())
    };
    let app_menu = Submenu::with_items(
        app,
        "SuperXTerminal",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &action("checkUpdates")?,
            &plain("thirdPartyNotices", "Third-Party Notices")?,
            &PredefinedMenuItem::separator(app)?,
            &action("settings")?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &action("newWindow")?,
            &action("newLocalShell")?,
            // SuperXPuTTY's File > Open session… (its Quick Selector)
            &action("openSession")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXPuTTY's File > Import sessions, Export sessions and Export sessions to CSV, in its order
            &Submenu::with_items(
                app,
                "Import Sessions",
                true,
                &[
                    &plain("import:superputty", "From SuperPuTTY XML File…")?,
                    &plain("import:putty", if cfg!(windows) { "From PuTTY/KiTTY (Registry)…" } else { "From PuTTY (~/.putty)…" })?,
                    &plain("import:puttyFolder", "From PuTTY Portable Settings…")?,
                    &plain("import:puttycm", "From PuTTY CM Export…")?,
                    // Remote Desktop Connection's recent computers are Windows' only: shown, greyed, on the Mac
                    &MenuItem::with_id(app, "import:rdpRegistry", "From RDP (Registry)…", cfg!(windows), None::<&str>)?,
                    &plain("import:rdp", "From Remote Desktop (.rdp) Files…")?,
                    &plain("import:mobaxterm", "From MobaXterm File…")?,
                    // SuperXTerminal's own: SecureCRT's Sessions folder
                    &plain("import:securecrt", "From SecureCRT…")?,
                    &plain("import:csv", "From CSV File…")?,
                    &plain("import:ssh", "From ssh config…")?,
                ],
            )?,
            &plain("exportXml", "Export Sessions…")?,
            &plain("exportCsv", "Export Sessions to CSV…")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXTerminal's own: every session in one table, its columns filtered, to change many at once
            &plain("editSessionsTable", "Edit Sessions as Table…")?,
            // SuperXPuTTY's File > Edit sessions externally and Reload sessions file
            &plain("editSessionsExternally", "Edit Sessions Externally…")?,
            &plain("reloadSessions", "Reload Sessions File")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXPuTTY's File > Save layout and Save layout as...
            &action("saveLayout")?,
            &action("saveLayoutAs")?,
            &PredefinedMenuItem::separator(app)?,
            // Cmd+W closes the tab, as in Terminal and iTerm2; the window closes with its red button
            &action("closeTab")?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            // the terminal's own Find (its Ctrl+Shift+F), Clear Scrollback, Reset and Copy All, with the Mac's keys
            &action("find")?,
            &action("copyAll")?,
            &action("clearScrollback")?,
            &action("resetTerminal")?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &action("nextTab")?,
            &action("prevTab")?,
            // SuperXPuTTY's Switch session…: the Quick Selector over the open tabs
            &action("switchSession")?,
            &PredefinedMenuItem::separator(app)?,
            &action("duplicateSession")?,
            &action("renameTab")?,
            &action("restartSession")?,
            &PredefinedMenuItem::separator(app)?,
            &action("splitRight")?,
            &action("splitDown")?,
            &action("joinAll")?,
            &PredefinedMenuItem::separator(app)?,
            &action("gotoCommandBar")?,
            &action("gotoConnectionBar")?,
            &action("focusActiveSession")?,
        ],
    )?;
    let tools = Submenu::with_items(
        app,
        "Tools",
        true,
        &[
            &action("scriptEditor")?,
            &PredefinedMenuItem::separator(app)?,
            &plain("databaseSessions", "Database Sessions…")?,
            &plain("environmentVariables", "Environment Variables…")?,
            &plain("environmentColours", "Environment Colours…")?,
            &PredefinedMenuItem::separator(app)?,
            // Settings' tabs in its order, as the app menu's Settings… opens them: "settings:" and the tab's name (Profiles
            // Settings… opens on Default Settings)
            &plain("settings:General", "General Settings…")?,
            &plain("profiles", "Profiles Settings…")?,
            &plain("settings:Terminal", "Terminal Settings…")?,
            &plain("settings:Remote Desktop", "Remote Desktop Settings…")?,
            &plain("settings:Shortcuts", "Shortcut Settings…")?,
            &plain("settings:Layouts", "Layout Settings…")?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(app, "toggleLocalOverrides", "Use Local Overrides", true, overrides_on, None::<&str>)?,
        ],
    )?;
    // shown or hidden, as SuperXPuTTY's View > Send Commands and View > Button Bar
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &shown("toggleSessionsPane")?,
            &shown("toggleSendCommands")?,
            &shown("toggleButtonBar")?,
            &shown("toggleStatusBar")?,
            &PredefinedMenuItem::separator(app)?,
            &shown("toggleSessionsAutoHide")?,
            // the saved layouts, in Settings > Layouts
            &action("toggleLayouts")?,
            &shown("toggleSessionDetail")?,
            &shown("toggleLogViewer")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXTerminal's own: the tabs tiled, to watch many sessions at once
            &shown("toggleGridView")?,
            &shown("toggleAlwaysOnTop")?,
            &action("fullScreen")?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &tools, &window])
}

/// The View menu's items that are shown or hidden, each with whether it's shown now, for its tick (as a Mac app's
/// View menu, and SuperXPuTTY's, tick the panes and bars on screen). Each is a setting, saved when the item is chosen,
/// so the menu is made again then ([`window_changes`]) and the tick follows. Full Screen isn't among them: the Mac's
/// own View menu doesn't tick it either. Nor is Layouts…, which opens Settings > Layouts.
pub(crate) fn view_checks(settings: &AppSettings) -> [(&'static str, bool); 9] {
    [
        ("toggleSessionsPane", settings.show_sessions_pane),
        ("toggleSendCommands", settings.show_send_commands),
        ("toggleButtonBar", settings.show_button_bar),
        ("toggleStatusBar", settings.show_status_bar),
        ("toggleSessionsAutoHide", settings.sessions_auto_hide),
        ("toggleSessionDetail", settings.show_session_detail),
        ("toggleLogViewer", settings.show_log_viewer),
        ("toggleAlwaysOnTop", settings.always_on_top),
        ("toggleGridView", settings.grid_view),
    ]
}

#[cfg(test)]
mod tests {
    use super::view_checks;
    use crate::settings::AppSettings;
    use crate::{window_changes, WindowChanges};

    #[test]
    fn the_view_menu_ticks_what_is_shown_and_follows_each_change() {
        let mut settings = AppSettings::default();
        settings.show_sessions_pane = true;
        settings.show_send_commands = false;
        settings.show_button_bar = true;
        settings.show_status_bar = true;
        settings.sessions_auto_hide = false;
        settings.show_session_detail = true;
        settings.show_log_viewer = false;
        settings.always_on_top = true;
        assert_eq!(
            view_checks(&settings),
            [
                ("toggleSessionsPane", true),
                ("toggleSendCommands", false),
                ("toggleButtonBar", true),
                ("toggleStatusBar", true),
                ("toggleSessionsAutoHide", false),
                ("toggleSessionDetail", true),
                ("toggleLogViewer", false),
                ("toggleAlwaysOnTop", true),
                ("toggleGridView", false),
            ]
        );
        // each shown or hidden makes the menu again, so its tick follows
        let mut log = settings.clone();
        log.show_log_viewer = true;
        assert_eq!(window_changes(&settings, &log), WindowChanges { menu: true, theme: false });
        let mut top = settings.clone();
        top.always_on_top = false;
        assert_eq!(window_changes(&settings, &top), WindowChanges { menu: true, theme: false });
        let mut grid = settings.clone();
        grid.grid_view = true;
        assert_eq!(window_changes(&settings, &grid), WindowChanges { menu: true, theme: false });
    }
}
