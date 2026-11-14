//! Layouts (Settings > Layouts, View > Layouts): the saved arrangements of tabs and splits, listed, read, saved,
//! deleted and renamed. They're files in the shared folder, with this computer's last layout in the user's
//! folder; `crate::layouts` reads and writes them.

use tauri::State;

use crate::{layouts, shared_folder, AppState};

/// The layouts' names, "<Auto Restore>" first
#[tauri::command]
pub(crate) fn layouts_list(state: State<'_, AppState>) -> Vec<String> {
    layouts::list(&shared_folder(&state))
}

/// A layout's text; None when it hasn't been saved
#[tauri::command]
pub(crate) fn layout_read(state: State<'_, AppState>, name: String) -> Option<String> {
    layouts::read(&shared_folder(&state), &state.user_folder, &name)
}

/// Save a layout
#[tauri::command]
pub(crate) fn layout_save(state: State<'_, AppState>, name: String, text: String) -> Result<(), String> {
    layouts::save(&shared_folder(&state), &state.user_folder, &name, &text)
}

/// Delete a saved layout
#[tauri::command]
pub(crate) fn layout_delete(state: State<'_, AppState>, name: String) -> Result<(), String> {
    layouts::delete(&shared_folder(&state), &name)
}

/// Rename a saved layout
#[tauri::command]
pub(crate) fn layout_rename(state: State<'_, AppState>, from: String, to: String) -> Result<(), String> {
    layouts::rename(&shared_folder(&state), &from, &to)
}
