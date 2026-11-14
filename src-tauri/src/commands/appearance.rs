//! What the app's look is read from: the custom icons in the settings folder, the highlighting rule sets and
//! themes in the team's and the user's `highlighting` folders, and the UI themes (Settings > UI Themes). The
//! files' formats are `crate::highlighting` and `crate::ui_themes`.

use std::path::PathBuf;

use tauri::State;

use crate::{highlighting, ui_themes, AppState};

/// The custom icons in `<SettingsFolder>/icons`: (name, PNG as a data URL) for each `.png`, as SuperXPuTTY's CustomIcons
#[tauri::command]
pub(crate) fn icons_custom(state: State<'_, AppState>) -> Vec<(String, String)> {
    use base64::Engine;
    let Some(folder) = state.settings.lock().unwrap().settings_folder.clone() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(folder.join("icons")) else { return Vec::new() };
    let mut icons = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("png")) != Some(true) {
            continue;
        }
        let (Some(name), Ok(bytes)) = (path.file_stem().and_then(|n| n.to_str()), std::fs::read(&path)) else { continue };
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        icons.push((name.to_string(), format!("data:image/png;base64,{data}")));
    }
    icons
}

/// The highlighting rule sets and themes in the team's and the user's `highlighting` folders (the UI adds the defaults)
#[tauri::command]
pub(crate) fn highlighting_load(state: State<'_, AppState>) -> highlighting::Highlighting {
    let shared = state.settings.lock().unwrap().settings_folder.clone().map(|f| f.join("highlighting"));
    let user = state.user_folder.join("highlighting");
    let mut folders: Vec<&std::path::Path> = Vec::new();
    if let Some(shared) = &shared {
        folders.push(shared);
    }
    folders.push(&user);
    highlighting::load(&folders)
}

/// The user's own highlighting folder (Highlighted Output's folder button), made if it isn't there
#[tauri::command]
pub(crate) fn highlighting_user_folder(state: State<'_, AppState>) -> Result<PathBuf, String> {
    let folder = state.user_folder.join("highlighting");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok(folder)
}

/// The window's own themes (Settings > General > Look > Theme), from UiThemes.json in the user's folder
#[tauri::command]
pub(crate) fn ui_themes_load(state: State<'_, AppState>) -> Vec<ui_themes::UiTheme> {
    ui_themes::load(&state.user_folder)
}

/// Save the window's own themes, whole; what's saved (names trimmed), or why they can't be
#[tauri::command]
pub(crate) fn ui_themes_save(state: State<'_, AppState>, themes: Vec<ui_themes::UiTheme>) -> Result<Vec<ui_themes::UiTheme>, String> {
    ui_themes::save(&state.user_folder, &themes)
}
