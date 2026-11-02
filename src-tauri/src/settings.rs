//! The app's own settings (per user), kept as JSON in the user's folder (`sessions::user_folder`).
//!
//! For now only the settings folder: where `Sessions.xml`, `Folders.xml` and the team's other shared files are, as
//! SuperXPuTTY's `SettingsFolder`. More settings join it with the Settings window later in M1.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The settings file's contents
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// The settings folder; none until the user chooses one
    pub settings_folder: Option<PathBuf>,
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
