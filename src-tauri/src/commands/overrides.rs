//! A session's local overrides (the values this computer uses in place of the stored session's, and whether
//! they're on), their named presets, and which of this computer's serial devices a session's COM name is. They
//! are kept in the user's folder by `crate::overrides`, never in the shared session files.

use std::collections::BTreeMap;

use tauri::State;

use crate::{overrides, streams, AppState};

/// The user's local overrides, by session id
#[tauri::command]
pub(crate) fn overrides_get(state: State<'_, AppState>) -> BTreeMap<String, overrides::Override> {
    overrides::load(&state.user_folder)
}

/// Set (or, when empty, remove) sessions' local overrides; returns them all
#[tauri::command]
pub(crate) fn overrides_set(state: State<'_, AppState>, changes: Vec<(String, overrides::Override)>) -> Result<BTreeMap<String, overrides::Override>, String> {
    overrides::set(&state.user_folder, changes)
}

/// The user's override presets, by name
#[tauri::command]
pub(crate) fn override_presets_get(state: State<'_, AppState>) -> BTreeMap<String, overrides::Override> {
    overrides::presets(&state.user_folder)
}

/// Save an override preset under a name, or remove it (no value); returns them all
#[tauri::command]
pub(crate) fn override_presets_set(state: State<'_, AppState>, name: String, value: Option<overrides::Override>) -> Result<BTreeMap<String, overrides::Override>, String> {
    overrides::set_preset(&state.user_folder, &name, value)
}

/// The serial devices on this Mac, for choosing which one a COM name is
#[tauri::command]
pub(crate) fn serial_devices_list() -> Vec<String> {
    streams::serial_devices()
}

/// Remember which device a serial session's COM name is
#[tauri::command]
pub(crate) fn serial_device_set(state: State<'_, AppState>, name: String, device: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.serial_devices.insert(name, device);
    settings.save(&state.user_folder)
}
