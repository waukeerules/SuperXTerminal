//! What's sent to tabs from the bottom of the window: the button bars (ButtonBars.xml in the shared folder, read
//! each time as SuperXPuTTY does, with export and import of one bar) and Send Commands' history. The bars' file is
//! `crate::buttonbars`, the history's `crate::command_history`.

use std::path::PathBuf;

use tauri::State;

use crate::{buttonbars, command_history, AppState};

/// The button bars, re-read from ButtonBars.xml each time (SuperXPuTTY reloads it whenever its menu opens)
#[tauri::command]
pub(crate) fn buttonbars_load(state: State<'_, AppState>) -> Vec<buttonbars::Bar> {
    let folder = state.settings.lock().unwrap().settings_folder.clone();
    buttonbars::load(&buttonbars::file(folder.as_deref(), &state.user_folder))
}

/// Save the button bars
#[tauri::command]
pub(crate) fn buttonbars_save(state: State<'_, AppState>, bars: Vec<buttonbars::Bar>) -> Result<(), String> {
    let folder = state.settings.lock().unwrap().settings_folder.clone();
    buttonbars::save(&buttonbars::file(folder.as_deref(), &state.user_folder), &bars)
}

/// Export one bar as SuperXPuTTY's JSON
#[tauri::command]
pub(crate) fn buttonbars_export(bar: buttonbars::Bar, path: PathBuf) -> Result<(), String> {
    std::fs::write(path, buttonbars::export(&bar)).map_err(|e| e.to_string())
}

/// Add the bars from an export file and save; returns the bars and the last one imported
#[tauri::command]
pub(crate) fn buttonbars_import(state: State<'_, AppState>, path: PathBuf) -> Result<(Vec<buttonbars::Bar>, String), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let folder = state.settings.lock().unwrap().settings_folder.clone();
    let file = buttonbars::file(folder.as_deref(), &state.user_folder);
    let mut bars = buttonbars::load(&file);
    let last = buttonbars::import(&mut bars, &json)?;
    buttonbars::save(&file, &bars)?;
    Ok((bars, last))
}

/// Send Commands' history saved from earlier runs, newest first, for the Command box as a window opens; none while
/// Settings > General > Command bar keeps it 0 days (until the app closes)
#[tauri::command]
pub(crate) fn command_history_load(state: State<'_, AppState>) -> Vec<String> {
    let days = state.settings.lock().unwrap().command_history_days;
    command_history::load(&state.user_folder, days, command_history::now())
}

/// Note a command sent from the Command box in the saved history (nothing is saved with 0 days)
#[tauri::command]
pub(crate) fn command_history_add(state: State<'_, AppState>, command: String) -> Result<(), String> {
    let days = state.settings.lock().unwrap().command_history_days;
    command_history::add(&state.user_folder, days, &command, command_history::now())
}
