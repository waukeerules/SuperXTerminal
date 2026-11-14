//! The Settings window's own commands: reading and saving the app's settings, choosing the settings folder, the
//! keyboard shortcuts' table and the keys the system keeps, and what Settings asks of this computer (whether
//! 1Password's command line is there, which SSH algorithms ssh has). A save also redoes what shows the settings
//! outside the page: the menu bar, the windows' theme and the tray. The settings' file is `crate::settings`.

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};

use crate::menu::{menu, view_checks};
use crate::settings::AppSettings;
use crate::{algorithms, applog, command_history, keys, onepassword, tray, windows, AppState};

/// The app's settings, for the UI
#[tauri::command]
pub(crate) fn settings_get(state: State<'_, AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

/// Save the Settings window's General tab; the settings folder is changed with `settings_set_folder`
#[tauri::command]
pub(crate) fn settings_save(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    let mut current = state.settings.lock().unwrap();
    let folder = current.settings_folder.clone();
    let before = current.clone();
    *current = AppSettings { settings_folder: folder, ..settings };
    current.icon_size = match current.icon_size {
        16 | 20 | 24 | 32 => current.icon_size,
        _ => 24,
    };
    current.tree_font_size = current.tree_font_size.clamp(10, 24);
    current.shortcuts = keys::cleaned(&current.shortcuts);
    current.command_history_days = current.command_history_days.min(command_history::MOST_DAYS);
    current.save(&state.user_folder)?;
    onepassword::set_enabled(current.one_password);
    // Settings > General > Command bar's days shortened (or 0): what they no longer allow goes now, not at next start
    if current.command_history_days < before.command_history_days {
        let _ = command_history::prune(&state.user_folder, current.command_history_days, command_history::now());
    }
    let changes = window_changes(&before, &current);
    // the menu bar's keys follow Settings > Shortcuts
    if changes.menu && let Ok(bar) = menu(&app, &current) {
        let _ = app.set_menu(bar);
    }
    if changes.theme {
        windows::apply_window_theme(&app, &current.theme);
    }
    let saved = current.clone();
    drop(current);
    applog::info("Settings saved");
    // and the menu bar icon, Settings > General
    tray::refresh(&app);
    // every window reads them again, so another window's next save doesn't put back what it had
    let _ = app.emit(SETTINGS_CHANGED, ());
    Ok(saved)
}

/// What a settings save has to redo in the windows themselves (see [`window_changes`])
#[derive(Debug, PartialEq)]
pub(crate) struct WindowChanges {
    /// The menu bar again: its keys (Settings > Shortcuts), Local overrides, or what View ticks changed
    pub(crate) menu: bool,
    /// The menu bar, menus and title bars in the theme again: Settings > General > Theme changed
    pub(crate) theme: bool,
}

/// What a save from `before` to `after` changes in the windows. Each only when its own setting changed: on Windows,
/// setting the menu bar again lays the whole window out again, which showed as the window shifting at every save (a
/// checkbox in a terminal tab's menu saves too)
pub(crate) fn window_changes(before: &AppSettings, after: &AppSettings) -> WindowChanges {
    WindowChanges {
        menu: before.shortcuts != after.shortcuts
            || before.use_local_overrides != after.use_local_overrides
            || view_checks(before) != view_checks(after),
        theme: before.theme != after.theme,
    }
}

/// Use another settings folder (chosen in the UI), and remember it
#[tauri::command]
pub(crate) fn settings_set_folder(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.settings_folder = Some(PathBuf::from(path));
    settings.save(&state.user_folder)?;
    let _ = app.emit(SETTINGS_CHANGED, ());
    Ok(())
}

/// The event every window gets when the settings are saved
pub(crate) const SETTINGS_CHANGED: &str = "settings-changed";

/// Settings > General's Check 1Password: whether its CLI works here, through the 1Password app (onepassword.rs).
/// async: it waits for the user to approve it in 1Password
#[tauri::command(async)]
pub(crate) fn onepassword_check() -> Result<String, String> {
    onepassword::check()
}

/// What this computer's ssh has for each kind of algorithm, and its default order, for the algorithm lists in
/// Settings > Profiles and Edit Session (algorithms.rs)
#[tauri::command]
pub(crate) fn ssh_algorithms_available() -> Vec<algorithms::Available> {
    algorithms::available()
}

/// The Shortcuts editor's rows: every action, its default and its key now
#[tauri::command]
pub(crate) fn shortcuts_keys(state: State<'_, AppState>) -> Vec<keys::Row> {
    keys::rows(&state.settings.lock().unwrap().shortcuts)
}

/// The keys this system keeps (the Mac's Cmd+Q, Cmd+C...; Windows' Alt+F4 and the terminal's Ctrl+Shift+C/V), which
/// the Shortcuts editor won't give to an action
#[tauri::command]
pub(crate) fn shortcuts_reserved() -> Vec<&'static str> {
    keys::reserved().to_vec()
}

#[cfg(test)]
mod tests {
    use super::{window_changes, WindowChanges};
    use crate::settings::AppSettings;

    #[test]
    fn a_terminal_preference_saved_leaves_the_menu_bar_and_theme_alone() {
        // a checkbox in a terminal tab's menu saves the settings; setting the menu bar again shifted the whole window
        let before = AppSettings::default();
        let mut after = before.clone();
        after.terminal_prefs.insert("copyOnSelect".into(), "false".into());
        assert_eq!(window_changes(&before, &after), WindowChanges { menu: false, theme: false });
        let mut keys = before.clone();
        keys.shortcuts.insert("newTab".into(), "Ctrl+T".into());
        assert_eq!(window_changes(&before, &keys), WindowChanges { menu: true, theme: false });
        let mut overrides = before.clone();
        overrides.use_local_overrides = !before.use_local_overrides;
        assert_eq!(window_changes(&before, &overrides), WindowChanges { menu: true, theme: false });
        let mut theme = before.clone();
        theme.theme = "light".into();
        assert_eq!(window_changes(&before, &theme), WindowChanges { menu: false, theme: true });
    }
}
