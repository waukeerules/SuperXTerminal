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
