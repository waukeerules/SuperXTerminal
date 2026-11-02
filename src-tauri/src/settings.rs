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
    /// Send Commands goes to every visible tab rather than the ticked ones, as SendCommandsToVisibleOnly
    pub send_to_visible_only: bool,
    /// The Script Editor stays on top, as ScriptEditorTopMost
    pub script_editor_on_top: bool,
    /// A dropped session reconnects by itself, as SuperXPuTTY's AutoReconnect, AutoReconnectAttempts and
    /// AutoReconnectDelaySeconds (Options > Advanced)
    pub auto_reconnect: bool,
    pub auto_reconnect_attempts: u32,
    pub auto_reconnect_delay_seconds: u32,
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
    /// The Sessions pane's width in points, as dragged at its edge
    pub sessions_pane_width: u32,
    /// The Sessions pane is auto-hidden, as DockPanelSuite's unpinned pane in SuperXPuTTY: a tab at the window's left
    /// edge that slides it out
    pub sessions_auto_hide: bool,
}

impl Default for AppSettings {
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
            send_to_visible_only: false,
            script_editor_on_top: false,
            auto_reconnect: true,
            auto_reconnect_attempts: 6,
            auto_reconnect_delay_seconds: 10,
            terminal_prefs: std::collections::BTreeMap::new(),
            session_log_enabled: false,
            session_log_folder: String::new(),
            serial_devices: std::collections::BTreeMap::new(),
            use_sessions_xml: true,
            sessions_pane_width: 280,
            sessions_auto_hide: false,
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
