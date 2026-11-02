//! SuperXTerminal's Rust core: the commands the UI (`src/`) calls through Tauri's `invoke`.

mod launch;
mod pty;
mod sessions;
mod settings;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::State;

use sessions::{Sessions, SessionsLoaded};
use settings::AppSettings;

/// What every command shares: the user's folder, their settings and the sessions as last read
pub struct AppState {
    user_folder: PathBuf,
    settings: Mutex<AppSettings>,
    sessions: Mutex<Sessions>,
}

/// Read (or read again) the session and folder files, and give the UI what it shows
#[tauri::command]
fn sessions_load(state: State<'_, AppState>) -> SessionsLoaded {
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    let mut problems = Vec::new();
    let loaded = Sessions::load(settings_folder.as_deref(), &state.user_folder, &mut problems);
    let result = SessionsLoaded {
        sessions: loaded.views(),
        folders: loaded
            .folders
            .iter()
            .flat_map(|(store, settings)| settings.values().map(move |setting| (*store, setting.clone())))
            .collect(),
        problems,
        saved_file: settings_folder.map(|f| f.join("Sessions.xml").display().to_string()),
        private_file: state.user_folder.join("PrivateSessions.xml").display().to_string(),
    };
    *state.sessions.lock().unwrap() = loaded;
    result
}

/// The app's settings, for the UI
#[tauri::command]
fn settings_get(state: State<'_, AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

/// The custom icons in `<SettingsFolder>/icons`: (name, PNG as a data URL) for each `.png`, as SuperXPuTTY's CustomIcons
#[tauri::command]
fn icons_custom(state: State<'_, AppState>) -> Vec<(String, String)> {
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

/// Use another settings folder (chosen in the UI), and remember it
#[tauri::command]
fn settings_set_folder(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.settings_folder = Some(PathBuf::from(path));
    settings.save(&state.user_folder)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let user_folder = sessions::user_folder();
    let settings = AppSettings::load(&user_folder);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(pty::Ptys::default())
        .manage(AppState { user_folder, settings: Mutex::new(settings), sessions: Mutex::new(Sessions::default()) })
        .invoke_handler(tauri::generate_handler![
            sessions_load,
            settings_get,
            settings_set_folder,
            icons_custom,
            pty::pty_open,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
