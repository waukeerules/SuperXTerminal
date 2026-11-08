//! The app's own settings (per user), kept as JSON in the user's folder (`sessions::user_folder`).
//!
//! The settings folder (where `Sessions.xml`, `Folders.xml` and the team's other shared files are, as SuperXPuTTY's
//! `SettingsFolder`), the icon size, and the environment colour name rules. More join them with the Settings window
//! later in M1.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The settings file's contents
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// The settings folder; none until the user chooses one
    pub settings_folder: Option<PathBuf>,
    /// Session icons' size in pixels, as SuperXPuTTY's Options > Icon size (16, 20, 24 or 32)
    pub icon_size: u32,
    /// Environment colour name rules, as SuperXPuTTY's EnvironmentColorRules: "Prod=#C62828;Staging=#EF6C00"
    pub environment_color_rules: String,
    /// The look: "system" (follow macOS), "light" or "dark"
    pub theme: String,
    /// The sessions tree's text size in pixels, as SuperXPuTTY's session tree font
    pub tree_font_size: u32,
    /// Database sessions' connection (the password is in the Keychain)
    pub database: crate::db::DbSettings,
    /// The button bar chosen by hand, as SuperXPuTTY's ActiveButtonBar; "" for the first
    pub active_button_bar: String,
    /// View > Button Bar and View > Send Commands, as ShowButtonBar and ShowToolBarCommands
    pub show_button_bar: bool,
    pub show_send_commands: bool,
    /// A button bar button asks before it runs (on by default): a click meant for something else, at the bottom of the
    /// window, mustn't type into a device
    pub confirm_buttons: bool,
    /// Send Commands goes to every visible tab rather than the ticked ones, as SendCommandsToVisibleOnly
    pub send_to_visible_only: bool,
    /// The Script Editor stays on top, as ScriptEditorTopMost
    pub script_editor_on_top: bool,
    /// A dropped session reconnects by itself, as SuperXPuTTY's AutoReconnect, AutoReconnectAttempts and
    /// AutoReconnectDelaySeconds (Options > Advanced)
    pub auto_reconnect: bool,
    pub auto_reconnect_attempts: u32,
    pub auto_reconnect_delay_seconds: u32,
    /// Tools > Use Local Overrides: off, sessions open as stored, whatever overrides the user has (kept for later)
    pub use_local_overrides: bool,
    /// SSH sessions run `ssh -v`, so ssh's own account of connecting and logging in shows in the tab (Settings >
    /// General > Diagnostics); off by default
    pub ssh_verbose: bool,
    /// The terminal's preferences, shared by every tab, as SuperXPuTTY's TerminalPreferences: names to string values,
    /// only those changed from their defaults (src/lib/terminal/prefs.ts has the defaults)
    pub terminal_prefs: std::collections::BTreeMap<String, String>,
    /// Keep a log of every session, as SuperXPuTTY's SessionLogEnabled; otherwise logs are temporary
    pub session_log_enabled: bool,
    /// Where kept logs go ("" for Documents/SuperXTerminal/logs), as SessionLogFolder
    pub session_log_folder: String,
    /// Which /dev/cu.* device each serial session's COM name is on this Mac, asked once (docs/PLAN.md)
    pub serial_devices: std::collections::BTreeMap<String, String>,
    /// Saved sessions (Sessions.xml and Folders.xml in the settings folder) are read and written, as SuperXPuTTY's
    /// UseSessionsXml; off, only Private and Database sessions are used and the files are left as they are
    pub use_sessions_xml: bool,
    /// Private sessions (PrivateSessions.xml in the user's own folder) are read and written; off, only Saved and
    /// Database sessions are used and the file is left as it is. One of the two stays on (Settings > General)
    pub use_private_sessions: bool,
    /// The Sessions pane's width in points, as dragged at its edge
    pub sessions_pane_width: u32,
    /// The Sessions pane is auto-hidden, as DockPanelSuite's unpinned pane in SuperXPuTTY: a tab at the window's left
    /// edge that slides it out
    pub sessions_auto_hide: bool,
    /// The layout opened at start ("" for none: a Local Shell; "<Auto Restore>" for the last one), as SuperXPuTTY's
    /// DefaultLayoutName
    pub default_layout_name: String,
    /// View > Layouts and View > Session Detail: the panes under the Sessions tree
    pub show_layouts: bool,
    pub show_session_detail: bool,
    /// Settings > Shortcuts: the actions' keys the user changed ("" for none); the others have keys.rs's defaults
    pub shortcuts: std::collections::BTreeMap<String, String>,
    /// The menu bar icon, as SuperXPuTTY's Minimize to tray: sessions from its menu, and SuperXTerminal kept running
    /// there when its last window closes
    pub show_in_menu_bar: bool,
    /// View > Log Viewer: the app's log under the tabs
    pub show_log_viewer: bool,
    /// Look for a newer release at start, as SuperXPuTTY's AutoUpdateCheck
    pub auto_update_check: bool,
    /// The GitHub repository ("owner/name") whose latest release the check compares with, as SuperXPuTTY's
    /// SuperPuTTY.UpdateRepository
    pub update_repository: String,
    /// SuperXPuTTY's View > Sessions, Status bar and Always on top
    pub show_sessions_pane: bool,
    pub show_status_bar: bool,
    pub always_on_top: bool,
    /// RDP sessions open in a tab with the built-in client (IronRDP), as SuperXPuTTY's General > Built-in client;
    /// off, in Windows App
    pub rdp_built_in: bool,
    /// Settings > Remote Desktop: the user's defaults for RDP sessions, in SuperXPuTTY's RdpOptions format ("" for
    /// SuperXPuTTY's own defaults)
    pub rdp_defaults: String,
    /// FileZilla (external) gets the session's password on its sftp:// address, as SuperXPuTTY's "Allow plain-text
    /// password arguments": off, since any process can read a program's arguments
    pub filezilla_passwords: bool,
    /// 1Password references (op://...) in passwords, Environment Variables and key files are read from 1Password
    /// (onepassword.rs), and Settings and Edit Session offer them. Off by default, as most people don't use 1Password:
    /// off, no reference is read and 1Password's CLI never runs
    pub one_password: bool,
    /// Where tabs' close button is: "tab" (on each tab) or "right" (one beside the tab list's ▾, closing the active tab)
    pub tab_close: String,
    /// The tab list's ▾ opens Switch Session (its tabs, with a filter to type into) rather than a plain list, for a
    /// window with many tabs open
    pub tab_list_switch_session: bool,
    /// The longest a tab's name is shown, in characters, before it's cut short with …; 0 shows the whole name
    pub tab_name_max: u32,
    /// The Sessions tree's dotted lines, as SuperXPuTTY's (its TreeView's ShowLines)
    pub tree_lines: bool,
    /// While searching, the Sessions pane widens (up to a limit) so the matches' names fit
    pub search_widens_pane: bool,
    /// The Sessions search's Aa and .* buttons, as SuperXPuTTY's search mode: match case, and a regular expression
    pub tree_search_case: bool,
    pub tree_search_regex: bool,
    /// Where the Sessions search looks (Search in…): "name", "host", "username", "folder", "note", "icon",
    /// "protocol", "profile"; empty for the first five
    pub tree_search_fields: Vec<String>,
    /// What a new script is, in the Script Editor's Run as and a new button's Action (Settings > General > Scripts):
    /// "rhai", "spsl" or "lines" (lines to send). Rhai by default, as the language with variables, loops and
    /// functions; a script opened from a file or a button already made keeps its own
    pub script_language: String,
    /// Settings > General > Command bar: how many days Send Commands' history is kept between runs, as SuperPuTTY's
    /// "Save history for N days" (`command_history.rs`); 0, the default, keeps it only until the app closes
    pub command_history_days: u32,
}

impl Default for AppSettings {
    /// The settings before the user has changed any, as SuperXPuTTY's defaults
    fn default() -> AppSettings {
        AppSettings {
            settings_folder: None,
            icon_size: 24,
            environment_color_rules: String::new(),
            theme: "system".into(),
            tree_font_size: 14,
            database: crate::db::DbSettings::default(),
            active_button_bar: String::new(),
            show_button_bar: true,
            show_send_commands: true,
            confirm_buttons: true,
            send_to_visible_only: false,
            script_editor_on_top: false,
            auto_reconnect: true,
            auto_reconnect_attempts: 6,
            auto_reconnect_delay_seconds: 10,
            ssh_verbose: false,
            use_local_overrides: true,
            terminal_prefs: std::collections::BTreeMap::new(),
            session_log_enabled: false,
            session_log_folder: String::new(),
            serial_devices: std::collections::BTreeMap::new(),
            use_sessions_xml: true,
            use_private_sessions: true,
            sessions_pane_width: 280,
            sessions_auto_hide: false,
            default_layout_name: String::new(),
            show_layouts: false,
            show_session_detail: false,
            shortcuts: std::collections::BTreeMap::new(),
            show_in_menu_bar: false,
            show_log_viewer: false,
            auto_update_check: true,
            update_repository: "waukeerules/SuperXTerminal".into(),
            show_sessions_pane: true,
            show_status_bar: true,
            always_on_top: false,
            rdp_built_in: true,
            rdp_defaults: String::new(),
            filezilla_passwords: false,
            one_password: false,
            tab_close: "tab".into(),
            tab_list_switch_session: false,
            tab_name_max: 30,
            tree_lines: true,
            search_widens_pane: true,
            tree_search_case: false,
            tree_search_regex: false,
            tree_search_fields: Vec::new(),
            script_language: "rhai".into(),
            command_history_days: 0,
        }
    }
}

impl AppSettings {
    /// Read the settings from the user's folder; missing or unreadable settings are the defaults
    pub fn load(user_folder: &Path) -> AppSettings {
        std::fs::read_to_string(user_folder.join("settings.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Write the settings to the user's folder, creating it when needed
    pub fn save(&self, user_folder: &Path) -> Result<(), String> {
        std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(user_folder.join("settings.json"), text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file from before Settings > General > Scripts reads with Rhai as the new scripts' language, and a
    /// choice made since is kept
    #[test]
    fn new_scripts_are_rhai_until_another_language_is_chosen() {
        let folder = std::env::temp_dir().join(format!("sxt-settings-language-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("settings.json"), r#"{"iconSize":20}"#).unwrap();
        assert_eq!(AppSettings::load(&folder).script_language, "rhai");

        let mut chosen = AppSettings::load(&folder);
        chosen.script_language = "spsl".into();
        chosen.save(&folder).unwrap();
        assert_eq!(AppSettings::load(&folder).script_language, "spsl");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
