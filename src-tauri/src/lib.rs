//! SuperXTerminal's Rust core: the commands the UI (`src/`) calls through Tauri's `invoke`.

mod db;
mod editing;
mod folders;
mod highlighting;
mod launch;
mod profiles;
mod pty;
mod sessions;
mod variables;
mod settings;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Runtime, State};

use profiles::{Profile, Profiles};
use sessions::{Sessions, SessionsLoaded};
use settings::AppSettings;

/// What every command shares: the user's folder, their settings and the sessions as last read
pub struct AppState {
    user_folder: PathBuf,
    settings: Mutex<AppSettings>,
    sessions: Mutex<Sessions>,
    profiles: Mutex<Profiles>,
    /// Database sessions' state: the change token of the last read, and how the last read went
    database: Mutex<DbState>,
}

/// How Database sessions stand, for the tree's root
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DbState {
    enabled: bool,
    /// The last read's change token; "" when only the cache has been read
    #[serde(skip)]
    token: String,
    writable: bool,
    /// Why the last read failed, if it did (the cached sessions are shown meanwhile)
    error: String,
    /// The table's name, "schema.table", for the root's tooltip
    source: String,
}

/// The user's profiles, "Default Settings" first; passwords are only marked as stored, never sent
#[tauri::command]
fn profiles_list(state: State<'_, AppState>) -> Vec<Profile> {
    state.profiles.lock().unwrap().profiles.clone()
}

/// Add or change a profile (`previous_name` is its name before a rename; empty for a new one). `password`: None
/// keeps the stored one, "" forgets it, anything else replaces it.
#[tauri::command]
fn profiles_save(
    state: State<'_, AppState>,
    profile: Profile,
    previous_name: String,
    password: Option<String>,
) -> Result<Vec<Profile>, String> {
    let name = profile.name.trim().to_string();
    if name.is_empty() {
        return Err("A profile needs a name.".into());
    }
    let mut profiles = state.profiles.lock().unwrap();
    let renaming = !previous_name.is_empty() && !previous_name.eq_ignore_ascii_case(&name);
    if (previous_name.is_empty() || renaming) && profiles.get(&name).is_some() {
        return Err(format!("There's already a profile called {name}."));
    }
    if renaming && profiles::is_default(&previous_name) {
        return Err("Default Settings can't be renamed.".into());
    }
    // a renamed profile takes its stored password along
    if renaming && password.is_none() {
        if let Some(old) = profiles::password(&previous_name) {
            profiles::set_password(&name, &old)?;
        }
        profiles::set_password(&previous_name, "")?;
    }
    if let Some(password) = &password {
        profiles::set_password(&name, password)?;
    }
    let mut profile = Profile { name: name.clone(), ..profile };
    profile.has_password = profiles::password(&name).is_some();
    let key = if previous_name.is_empty() { name.clone() } else { previous_name };
    match profiles.profiles.iter().position(|p| p.name.eq_ignore_ascii_case(&key)) {
        Some(index) => profiles.profiles[index] = profile,
        None => profiles.profiles.push(profile),
    }
    profiles.save(&state.user_folder)?;
    Ok(profiles.profiles.clone())
}

/// Remove a profile and its stored password; "Default Settings" can't be removed
#[tauri::command]
fn profiles_delete(state: State<'_, AppState>, name: String) -> Result<Vec<Profile>, String> {
    if profiles::is_default(&name) {
        return Err("Default Settings can't be removed.".into());
    }
    let mut profiles = state.profiles.lock().unwrap();
    profiles.profiles.retain(|p| !p.name.eq_ignore_ascii_case(&name));
    profiles::set_password(&name, "")?;
    profiles.save(&state.user_folder)?;
    Ok(profiles.profiles.clone())
}

/// Read (or read again) the session and folder files, and give the UI what it shows
#[tauri::command]
fn sessions_load(state: State<'_, AppState>) -> SessionsLoaded {
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    let mut problems = Vec::new();
    let mut loaded = Sessions::load(settings_folder.as_deref(), &state.user_folder, &mut problems);
    // Database sessions come from the table (or its cache), not the files: keep them
    {
        let mut current = state.sessions.lock().unwrap();
        loaded.database = std::mem::take(&mut current.database);
        if let Some(folders) = current.folders.remove(&sessions::Store::Database) {
            loaded.folders.insert(sessions::Store::Database, folders);
        }
    }
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

/// The highlighting rule sets and themes in the team's and the user's `highlighting` folders (the UI adds the defaults)
#[tauri::command]
fn highlighting_load(state: State<'_, AppState>) -> highlighting::Highlighting {
    let shared = state.settings.lock().unwrap().settings_folder.clone().map(|f| f.join("highlighting"));
    let user = state.user_folder.join("highlighting");
    let mut folders: Vec<&std::path::Path> = Vec::new();
    if let Some(shared) = &shared {
        folders.push(shared);
    }
    folders.push(&user);
    highlighting::load(&folders)
}

/// The file a store's sessions are in, or an error when there's no settings folder yet
fn store_file(state: &AppState, store: sessions::Store) -> Result<PathBuf, String> {
    if store == sessions::Store::Database {
        return Err("Changing Database sessions comes with the next part of M2.".into());
    }
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    sessions::file_of(store, settings_folder.as_deref(), &state.user_folder).ok_or_else(|| "Choose the settings folder first.".into())
}

/// One session's fields for Edit Session (a `-pw` password masked)
#[tauri::command]
fn session_get(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<editing::SessionFields, String> {
    let sessions = state.sessions.lock().unwrap();
    let session = sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?;
    Ok(editing::SessionFields::of(session))
}

/// Save a session from Edit Session (`original_id` None for a new one); returns its id. The caller reloads the tree.
#[tauri::command]
fn session_save(
    state: State<'_, AppState>,
    store: sessions::Store,
    original_id: Option<String>,
    fields: editing::SessionFields,
) -> Result<String, String> {
    let path = store_file(&state, store)?;
    // ids must be unique across Saved and Private sessions, as in SuperXPuTTY
    editing::save(&path, original_id.as_deref(), &fields, &other_ids(&state, store))
}

/// Delete a session from its file
#[tauri::command]
fn session_delete(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<(), String> {
    let path = store_file(&state, store)?;
    editing::delete(&path, &id)
}

/// The session ids in the store other than `store`, which a change in `store` must not clash with
fn other_ids(state: &AppState, store: sessions::Store) -> Vec<String> {
    let sessions = state.sessions.lock().unwrap();
    let other = if store == sessions::Store::Saved { &sessions.private } else { &sessions.saved };
    other.iter().map(|s| s.id().to_string()).collect()
}

/// Rename or move a folder (every session below goes with it, and its settings)
#[tauri::command]
fn folder_move(state: State<'_, AppState>, store: sessions::Store, from: String, to: String) -> Result<(), String> {
    folders::check_name(to.rsplit('/').next().unwrap_or(&to))?;
    let path = store_file(&state, store)?;
    folders::move_folder(&path, &from, &to, &other_ids(&state, store))
}

/// Delete a folder and every session in it
#[tauri::command]
fn folder_delete(state: State<'_, AppState>, store: sessions::Store, path: String) -> Result<(), String> {
    let file = store_file(&state, store)?;
    folders::delete_folder(&file, &path)
}

/// Save a folder's settings (colour and defaults)
#[tauri::command]
fn folder_settings_save(state: State<'_, AppState>, store: sessions::Store, fields: folders::FolderFields) -> Result<(), String> {
    let file = store_file(&state, store)?;
    folders::save_settings(&file, &fields)
}

/// Move a session into another folder of its store; returns its new id
#[tauri::command]
fn session_move(state: State<'_, AppState>, store: sessions::Store, id: String, folder: String) -> Result<String, String> {
    let file = store_file(&state, store)?;
    folders::move_session(&file, &id, &folder, &other_ids(&state, store))
}

/// The user's Environment Variables for the window; secrets' values stay in the Keychain
#[tauri::command]
fn variables_list(state: State<'_, AppState>) -> Vec<variables::Variable> {
    variables::list(&state.user_folder)
}

/// Save the Environment Variables window; a secret left empty keeps its stored value
#[tauri::command]
fn variables_save(state: State<'_, AppState>, variables: Vec<variables::Variable>) -> Result<Vec<variables::Variable>, String> {
    variables::save(&state.user_folder, &variables)?;
    Ok(variables::list(&state.user_folder))
}

/// Save the Settings window's General tab; the settings folder is changed with `settings_set_folder`
#[tauri::command]
fn settings_save(state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    let mut current = state.settings.lock().unwrap();
    let folder = current.settings_folder.clone();
    *current = AppSettings { settings_folder: folder, ..settings };
    current.icon_size = match current.icon_size {
        16 | 20 | 24 | 32 => current.icon_size,
        _ => 24,
    };
    current.tree_font_size = current.tree_font_size.clamp(10, 24);
    current.save(&state.user_folder)?;
    Ok(current.clone())
}

/// Database sessions' settings for the window, and whether a password is stored
#[tauri::command]
fn db_settings_get(state: State<'_, AppState>) -> (db::DbSettings, bool) {
    (state.settings.lock().unwrap().database.clone(), db::has_password())
}

/// Save Database sessions' settings; `password` None keeps the stored one, "" forgets it
#[tauri::command]
fn db_settings_save(state: State<'_, AppState>, database: db::DbSettings, password: Option<String>) -> Result<(), String> {
    if let Some(password) = password {
        db::set_password(&password)?;
    }
    let mut settings = state.settings.lock().unwrap();
    settings.database = database;
    settings.save(&state.user_folder)?;
    // a new connection: its first read sets a new token
    state.database.lock().unwrap().token.clear();
    Ok(())
}

/// Test connection with the window's values (and a password typed there, not saved yet)
#[tauri::command]
async fn db_test(state: State<'_, AppState>, database: db::DbSettings, password: Option<String>) -> Result<db::TestResult, String> {
    let user_folder = state.user_folder.clone();
    db::test(&database, &user_folder, password.as_deref()).await
}

/// Read the table again (or, when it can't be reached, keep the cache) and say how it went
#[tauri::command]
async fn db_refresh(state: State<'_, AppState>) -> Result<DbState, String> {
    let settings = state.settings.lock().unwrap().database.clone();
    let user_folder = state.user_folder.clone();
    if !settings.enabled {
        state.sessions.lock().unwrap().database.clear();
        let mut db_state = state.database.lock().unwrap();
        *db_state = DbState::default();
        return Ok(db_state.clone());
    }
    let result = db::fetch_all(&settings, &user_folder).await;
    let mut db_state = state.database.lock().unwrap();
    db_state.enabled = true;
    db_state.source = format!("{}.{}", settings.schema, settings.table);
    match result {
        Ok(fetched) => {
            let _ = db::write_cache(&user_folder, &settings, &fetched);
            db_state.token = fetched.token.clone();
            db_state.writable = fetched.writable;
            db_state.error.clear();
            let mut sessions = state.sessions.lock().unwrap();
            sessions.database = fetched.sessions;
            sessions.folders.insert(sessions::Store::Database, fetched.folders.into_iter().map(|f| (f.path.clone(), f)).collect());
        }
        Err(error) => {
            // keep showing what was last read, from the cache if nothing is loaded yet
            let mut sessions = state.sessions.lock().unwrap();
            if sessions.database.is_empty() {
                let (cached, folders, writable) = db::read_cache(&user_folder);
                sessions.database = cached;
                sessions.folders.insert(sessions::Store::Database, folders.into_iter().map(|f| (f.path.clone(), f)).collect());
                db_state.writable = writable;
            }
            db_state.error = error;
        }
    }
    Ok(db_state.clone())
}

/// Whether the table changed since the last read (one small query); the UI refreshes when it did
#[tauri::command]
async fn db_check(state: State<'_, AppState>) -> Result<bool, String> {
    let settings = state.settings.lock().unwrap().database.clone();
    let token = state.database.lock().unwrap().token.clone();
    if !settings.enabled || token.is_empty() {
        return Ok(token.is_empty() && settings.enabled);
    }
    let user_folder = state.user_folder.clone();
    let (changed, _) = db::check(&settings, &user_folder, &token).await?;
    Ok(changed)
}

/// Save the connection to a JSON file colleagues can import, as SuperXPuTTY's Export: no password unless it's a
/// {key} placeholder, and no nickname
#[tauri::command]
fn db_export(database: db::DbSettings, path: String) -> Result<(), String> {
    let file = db::ExportFile {
        format: "superxputty-database-settings".into(),
        version: 1,
        host: database.host,
        port: database.port,
        database: database.database,
        schema: database.schema,
        table: database.table,
        user: database.user,
        password: Some(database.password_placeholder).filter(|p| p.starts_with('{') && p.ends_with('}')),
        ssl_mode: database.ssl_mode,
        read_only: database.read_only,
        check_seconds: database.check_seconds,
    };
    let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Read an exported connection (SuperXPuTTY's or ours) into the window's values
#[tauri::command]
fn db_import(state: State<'_, AppState>, path: String) -> Result<db::DbSettings, String> {
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let file: db::ExportFile = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| format!("Not a Database sessions settings file: {e}"))?;
    if file.format != "superxputty-database-settings" {
        return Err("Not a Database sessions settings file.".into());
    }
    let current = state.settings.lock().unwrap().database.clone();
    Ok(db::DbSettings {
        enabled: true,
        host: file.host,
        port: file.port,
        database: file.database,
        schema: file.schema,
        table: file.table,
        user: file.user,
        ssl_mode: file.ssl_mode,
        read_only: file.read_only,
        check_seconds: file.check_seconds,
        password_placeholder: file.password.unwrap_or_default(),
        nickname: current.nickname,
    })
}

/// Use another settings folder (chosen in the UI), and remember it
#[tauri::command]
fn settings_set_folder(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.settings_folder = Some(PathBuf::from(path));
    settings.save(&state.user_folder)
}

/// The menu bar: the app menu (with Settings..., Cmd+,), File (New Local Shell, Cmd+T), Edit (which WebKit needs for
/// Cmd+C/V), Tools (Environment Variables...) and Window.
/// SuperXPuTTY's File, View and Tools menus join it as their features arrive.
fn menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let app_menu = Submenu::with_items(
        app,
        "SuperXTerminal",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[&MenuItem::with_id(app, "newLocalShell", "New Local Shell", true, Some("CmdOrCtrl+T"))?],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let tools = Submenu::with_items(
        app,
        "Tools",
        true,
        &[
            &MenuItem::with_id(app, "databaseSessions", "Database Sessions…", true, None::<&str>)?,
            &MenuItem::with_id(app, "environmentVariables", "Environment Variables…", true, None::<&str>)?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &tools, &window])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let user_folder = sessions::user_folder();
    let settings = AppSettings::load(&user_folder);
    let profiles = Profiles::load(&user_folder);
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .menu(menu)
        // menu items the UI handles arrive there as a "menu" event with the item's id
        .on_menu_event(|app, event| {
            let _ = app.emit("menu", event.id().0.as_str());
        })
        .manage(pty::Ptys::default())
        .manage(AppState {
            user_folder,
            settings: Mutex::new(settings),
            sessions: Mutex::new(Sessions::default()),
            profiles: Mutex::new(profiles),
            database: Mutex::new(DbState::default()),
        })
        .invoke_handler(tauri::generate_handler![
            sessions_load,
            settings_get,
            settings_set_folder,
            settings_save,
            icons_custom,
            highlighting_load,
            session_get,
            session_save,
            session_delete,
            session_move,
            folder_move,
            folder_delete,
            folder_settings_save,
            variables_list,
            db_settings_get,
            db_settings_save,
            db_test,
            db_refresh,
            db_check,
            db_export,
            db_import,
            variables_save,
            profiles_list,
            profiles_save,
            profiles_delete,
            pty::pty_open,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
