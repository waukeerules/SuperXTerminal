//! SuperXTerminal's Rust core: the commands the UI (`src/`) calls through Tauri's `invoke`.

mod buttonbars;
mod db;
mod editing;
mod folders;
mod highlighting;
mod launch;
mod logs;
mod profiles;
mod pty;
mod scripts;
mod secrets;
mod outbox;
mod sessions;
mod shortcuts;
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
    /// The Database sessions as last read from the table (or its cache), before queued changes are applied on top
    db_server: Mutex<Vec<sessions::StoredSession>>,
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
    /// The optional columns the table has (color, jump_host, highlight_rules), which writes include
    #[serde(skip)]
    optional_columns: Vec<String>,
    /// How many sessions the last read (or the cache) had
    count: usize,
    /// When the sessions shown were read, "HH:MM", for "offline, cached 09:41"
    synced_at: String,
    /// Changes waiting to be sent, and changes the table refused (Pending changes)
    pending: usize,
    failed: usize,
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
    let id = editing::save(&path, original_id.as_deref(), &fields, &other_ids(&state, store))?;
    if let Some(old) = original_id {
        follow_id(&state, &old, &id);
    }
    Ok(id)
}

/// Delete a session from its file
#[tauri::command]
fn session_delete(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<(), String> {
    let path = store_file(&state, store)?;
    editing::delete(&path, &id)?;
    shortcuts::forget(&state.user_folder, |s| s == id);
    Ok(())
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
    folders::move_folder(&path, &from, &to, &other_ids(&state, store))?;
    follow_folder(&state, &from, &to);
    Ok(())
}

/// Delete a folder and every session in it
#[tauri::command]
fn folder_delete(state: State<'_, AppState>, store: sessions::Store, path: String) -> Result<(), String> {
    let file = store_file(&state, store)?;
    folders::delete_folder(&file, &path)?;
    forget_folder(&state, &path);
    Ok(())
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
    let new = folders::move_session(&file, &id, &folder, &other_ids(&state, store))?;
    follow_id(&state, &id, &new);
    Ok(new)
}

/// Favorites and Recent follow a session renamed or moved
fn follow_id(state: &AppState, old: &str, new: &str) {
    if old != new {
        shortcuts::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
    }
}

/// Favorites and Recent follow the sessions below a renamed or moved folder ("Lab" or "@db/Lab")
fn follow_folder(state: &AppState, from: &str, to: &str) {
    let below = format!("{from}/");
    shortcuts::follow(&state.user_folder, |id| id.strip_prefix(&below).map(|rest| format!("{to}/{rest}")));
}

/// Favorites and Recent forget the sessions below a deleted folder
fn forget_folder(state: &AppState, path: &str) {
    let below = format!("{path}/");
    shortcuts::forget(&state.user_folder, |id| id.starts_with(&below));
}

/// The settings folder, or the user's folder when none is chosen, for shared files (ButtonBars.xml, scripts)
fn shared_folder(state: &AppState) -> PathBuf {
    state.settings.lock().unwrap().settings_folder.clone().unwrap_or_else(|| state.user_folder.clone())
}

/// The button bars, re-read from ButtonBars.xml each time (SuperXPuTTY reloads it whenever its menu opens)
#[tauri::command]
fn buttonbars_load(state: State<'_, AppState>) -> Vec<buttonbars::Bar> {
    let folder = state.settings.lock().unwrap().settings_folder.clone();
    buttonbars::load(&buttonbars::file(folder.as_deref(), &state.user_folder))
}

/// Save the button bars
#[tauri::command]
fn buttonbars_save(state: State<'_, AppState>, bars: Vec<buttonbars::Bar>) -> Result<(), String> {
    let folder = state.settings.lock().unwrap().settings_folder.clone();
    buttonbars::save(&buttonbars::file(folder.as_deref(), &state.user_folder), &bars)
}

/// Export one bar as SuperXPuTTY's JSON
#[tauri::command]
fn buttonbars_export(bar: buttonbars::Bar, path: PathBuf) -> Result<(), String> {
    std::fs::write(path, buttonbars::export(&bar)).map_err(|e| e.to_string())
}

/// Add the bars from an export file and save; returns the bars and the last one imported
#[tauri::command]
fn buttonbars_import(state: State<'_, AppState>, path: PathBuf) -> Result<(Vec<buttonbars::Bar>, String), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let folder = state.settings.lock().unwrap().settings_folder.clone();
    let file = buttonbars::file(folder.as_deref(), &state.user_folder);
    let mut bars = buttonbars::load(&file);
    let last = buttonbars::import(&mut bars, &json)?;
    buttonbars::save(&file, &bars)?;
    Ok((bars, last))
}

/// A session's own script (Edit Session's SPSL script), found as SuperXPuTTY finds it
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum SessionScript {
    /// The session has none
    None,
    /// An http(s) address, which the window fetches
    Url { url: String },
    Text { text: String },
    /// A file that isn't there, e.g. a Windows path (C:\...)
    Missing { path: String },
}

/// The script a session runs when it opens: a file (absolute, file://, or relative to the settings folder), or a URL
#[tauri::command]
fn session_script(state: State<'_, AppState>, store: sessions::Store, id: String) -> SessionScript {
    let spec = {
        let sessions = state.sessions.lock().unwrap();
        sessions.find(store, &id).map(|s| s.get("SPSLFileName").trim().to_string()).unwrap_or_default()
    };
    if spec.is_empty() {
        return SessionScript::None;
    }
    let lower = spec.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return SessionScript::Url { url: spec };
    }
    let path = PathBuf::from(spec.strip_prefix("file://").unwrap_or(&spec));
    let path = if path.is_absolute() { path } else { shared_folder(&state).join(path) };
    match std::fs::read_to_string(&path) {
        Ok(text) => SessionScript::Text { text },
        Err(_) => SessionScript::Missing { path: spec },
    }
}

/// The saved scripts
#[tauri::command]
fn scripts_list(state: State<'_, AppState>) -> Vec<scripts::Entry> {
    scripts::list(&shared_folder(&state))
}

/// A saved script's text
#[tauri::command]
fn script_load(state: State<'_, AppState>, name: String, kind: String) -> Result<String, String> {
    scripts::load(&shared_folder(&state), &name, &kind)
}

/// Save a script in the library
#[tauri::command]
fn script_save(state: State<'_, AppState>, name: String, kind: String, text: String) -> Result<Vec<scripts::Entry>, String> {
    scripts::save(&shared_folder(&state), &name, &kind, &text)?;
    Ok(scripts::list(&shared_folder(&state)))
}

/// Delete a saved script
#[tauri::command]
fn script_delete(state: State<'_, AppState>, name: String, kind: String) -> Result<Vec<scripts::Entry>, String> {
    scripts::delete(&shared_folder(&state), &name, &kind)?;
    Ok(scripts::list(&shared_folder(&state)))
}

/// A text file the user chose (Script Editor's Open file...), with LF line breaks
#[tauri::command]
fn text_file_read(path: PathBuf) -> Result<String, String> {
    std::fs::read_to_string(path).map(|t| t.replace("\r\n", "\n")).map_err(|e| e.to_string())
}

/// Write a text file the user chose (Script Editor's Save to file...), with CRLF as SuperXPuTTY does
#[tauri::command]
fn text_file_write(path: PathBuf, text: String) -> Result<(), String> {
    std::fs::write(path, text.replace("\r\n", "\n").replace('\n', "\r\n")).map_err(|e| e.to_string())
}

/// A log just started: its id for writing, its file, and whether it goes when the tab closes
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LogStarted {
    id: u32,
    path: PathBuf,
    temporary: bool,
}

/// Start a tab's session log: kept in the chosen folder when the settings say so, else temporary
#[tauri::command]
fn log_open(state: State<'_, AppState>, logs: State<'_, logs::Logs>, name: String) -> Result<LogStarted, String> {
    let (keep, folder) = {
        let settings = state.settings.lock().unwrap();
        (settings.session_log_enabled, settings.session_log_folder.clone())
    };
    let folder = if keep { logs::kept_folder(&folder) } else { logs::temp_folder() };
    let (id, path) = logs.open(&folder, &name)?;
    Ok(LogStarted { id, path, temporary: !keep })
}

/// Add a tab's finished lines to its log
#[tauri::command]
fn log_write(logs: State<'_, logs::Logs>, id: u32, text: String) -> Result<(), String> {
    logs.write(id, &text)
}

/// Close a tab's log (a temporary one is deleted shortly after)
#[tauri::command]
fn log_close(logs: State<'_, logs::Logs>, id: u32, temporary: bool) {
    logs.close(id, temporary);
}

/// A log's text from a byte offset, and the offset after it (Search Output, Highlighted Output)
#[tauri::command]
fn log_read(path: PathBuf, offset: u64) -> Result<(String, u64), String> {
    logs::read_from(&path, offset)
}

/// Open a file in its own app (Search Output's Open log)
#[tauri::command]
fn open_path(path: PathBuf) -> Result<(), String> {
    std::process::Command::new("open").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// The Favorites and Recent lists
#[tauri::command]
fn shortcuts_get(state: State<'_, AppState>) -> shortcuts::Shortcuts {
    shortcuts::load(&state.user_folder)
}

/// Add a session to Favorites, or take it out
#[tauri::command]
fn shortcuts_favorite(state: State<'_, AppState>, id: String, on: bool) -> Result<shortcuts::Shortcuts, String> {
    shortcuts::edit(&state.user_folder, |s| {
        s.favorites.retain(|f| *f != id);
        if on {
            s.favorites.push(id);
        }
    })
}

/// Record a session opened from the tree, first in Recent
#[tauri::command]
fn shortcuts_opened(state: State<'_, AppState>, id: String) -> Result<shortcuts::Shortcuts, String> {
    shortcuts::edit(&state.user_folder, |s| shortcuts::push_recent(&mut s.recent, &id))
}

/// Take one session out of Recent, or clear it (`id` None)
#[tauri::command]
fn shortcuts_forget_recent(state: State<'_, AppState>, id: Option<String>) -> Result<shortcuts::Shortcuts, String> {
    shortcuts::edit(&state.user_folder, |s| match &id {
        Some(id) => s.recent.retain(|r| r != id),
        None => s.recent.clear(),
    })
}

/// Remember Favorites or Recent being opened or closed by the user
#[tauri::command]
fn shortcuts_expanded(state: State<'_, AppState>, group: String, open: bool) -> Result<shortcuts::Shortcuts, String> {
    shortcuts::edit(&state.user_folder, |s| {
        if group == "favorites" {
            s.favorites_expanded = open;
        } else {
            s.recent_expanded = open;
        }
    })
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

/// Text with its {key} placeholders filled from Environment Variables, and the keys that have no value (left as
/// written), for Send Commands, the button bar and scripts, as SuperXPuTTY's SessionVariables
#[derive(serde::Serialize)]
struct Filled {
    text: String,
    missing: Vec<String>,
}

#[tauri::command]
fn variables_fill(state: State<'_, AppState>, text: String) -> Filled {
    let values = variables::values(&state.user_folder);
    Filled { missing: variables::missing(&text, &values), text: variables::substitute(&text, &values) }
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
    refresh_database(&state).await
}

/// Read the table again, keeping the cache when it can't be reached; used by Refresh and after every change
async fn refresh_database(state: &AppState) -> Result<DbState, String> {
    let settings = state.settings.lock().unwrap().database.clone();
    let user_folder = state.user_folder.clone();
    if !settings.enabled {
        state.sessions.lock().unwrap().database.clear();
        state.db_server.lock().unwrap().clear();
        let mut db_state = state.database.lock().unwrap();
        *db_state = DbState::default();
        return Ok(db_state.clone());
    }
    // as SuperXPuTTY's BeginSync: send what's queued first, then read
    let result = match if settings.read_only { Ok(()) } else { flush(state, &settings).await } {
        Ok(()) => db::fetch_all(&settings, &user_folder).await,
        Err(unreachable) => Err(unreachable),
    };
    let mut db_state = state.database.lock().unwrap();
    db_state.enabled = true;
    db_state.source = format!("{}.{}", settings.schema, settings.table);
    match result {
        Ok(fetched) => {
            let _ = db::write_cache(&user_folder, &settings, &fetched);
            db_state.token = fetched.token.clone();
            db_state.writable = fetched.writable;
            db_state.optional_columns = fetched.optional_columns.clone();
            db_state.error.clear();
            db_state.count = fetched.sessions.len();
            db_state.synced_at = chrono::Local::now().format("%H:%M").to_string();
            *state.db_server.lock().unwrap() = fetched.sessions;
            let mut sessions = state.sessions.lock().unwrap();
            sessions.folders.insert(sessions::Store::Database, fetched.folders.into_iter().map(|f| (f.path.clone(), f)).collect());
        }
        Err(error) => {
            // keep showing what was last read, from the cache if nothing is loaded yet
            if db_state.token.is_empty() && db_state.synced_at.is_empty() {
                show_cache(state, &settings, &mut db_state);
            }
            db_state.token.clear();
            db_state.error = error;
        }
    }
    show_database(state, &settings, &mut db_state);
    Ok(db_state.clone())
}

/// Show the table's sessions with the queued changes on top, and count what's pending (SuperXPuTTY's ShowSessions)
fn show_database(state: &AppState, settings: &db::DbSettings, db_state: &mut DbState) {
    let changes = outbox::changes(&state.user_folder, &settings.source());
    (db_state.pending, db_state.failed) = outbox::count(&changes);
    let server = state.db_server.lock().unwrap();
    let shown = if changes.is_empty() { server.clone() } else { outbox::apply(&server, &changes) };
    state.sessions.lock().unwrap().database = shown;
}

/// Show this connection's cache, as SuperXPuTTY does at start before the table is read; nothing if it's another's
fn show_cache(state: &AppState, settings: &db::DbSettings, db_state: &mut DbState) {
    let cached = db::read_cache(&state.user_folder, settings);
    let mut server = state.db_server.lock().unwrap();
    let mut sessions = state.sessions.lock().unwrap();
    match cached {
        Some(cached) => {
            db_state.writable = cached.writable;
            db_state.optional_columns = cached.optional_columns;
            db_state.count = cached.sessions.len();
            db_state.synced_at = cached.synced_at.get(11..16).unwrap_or_default().to_string();
            *server = cached.sessions;
            sessions.folders.insert(sessions::Store::Database, cached.folders.into_iter().map(|f| (f.path.clone(), f)).collect());
        }
        None => {
            *db_state = DbState { enabled: true, source: db_state.source.clone(), ..DbState::default() };
            server.clear();
            sessions.folders.remove(&sessions::Store::Database);
        }
    }
}

/// Send one change to the table, as SuperXPuTTY's ApplyChange. Rows this sync already changed (e.g. moved by a
/// queued folder rename) are updated without the version check: the version they were read at is older than our own.
async fn apply_change(
    settings: &db::DbSettings,
    user_folder: &std::path::Path,
    optional: &[String],
    change: &outbox::PendingChange,
    touched: &mut std::collections::HashSet<String>,
) -> Result<(), db::WriteError> {
    use outbox::Kind;
    let row = || change.row.clone().ok_or_else(|| db::WriteError::refused("The change has no session values."));
    match change.kind {
        Kind::Insert => {
            touched.insert(db::insert(settings, user_folder, optional, &row()?).await?);
        }
        Kind::Update => {
            let force = change.force || touched.contains(&change.row_id);
            db::update(settings, user_folder, optional, &change.row_id, &change.version, &row()?, force).await?;
            touched.insert(change.row_id.clone());
        }
        Kind::Delete => db::delete(settings, user_folder, &change.row_id).await?,
        Kind::RenameFolder => touched.extend(db::rename_folder(settings, user_folder, &change.from, &change.to).await?),
        Kind::DeleteFolder => {
            db::delete_folder(settings, user_folder, &change.from).await?;
        }
    }
    Ok(())
}

/// Send the queued changes in order, as SuperXPuTTY's Flush: a refused one is marked with why and left for the user,
/// the rest still go; stops with the reason if the table can't be reached
async fn flush(state: &AppState, settings: &db::DbSettings) -> Result<(), String> {
    let (_, user_folder, optional) = db_write_context(state);
    let source = settings.source();
    let mut touched = std::collections::HashSet::new();
    for change in outbox::changes(&user_folder, &source).into_iter().filter(|c| c.error.is_none()) {
        match apply_change(settings, &user_folder, &optional, &change, &mut touched).await {
            Ok(()) => outbox::remove(&user_folder, &source, &change.id)?,
            Err(e) if e.unavailable => return Err(e.message),
            Err(e) => outbox::set_error(&user_folder, &source, &change.id, Some(e.message), false)?,
        }
    }
    Ok(())
}

/// What a batch of changes from the tree did
#[derive(Default)]
struct Submitted {
    /// Saved or queued
    done: usize,
    /// Why each refused one was refused, e.g. "core-sw1: Host is locked in the database."
    problems: Vec<String>,
    /// The positions of the refused ones, so a move removes only what arrived
    refused: std::collections::HashSet<usize>,
}

/// Make changes from the tree, as SuperXPuTTY's SubmitBatch: online with nothing queued, each is written at once;
/// offline, or behind queued changes (to keep their order), or once the table drops mid-way, they're queued and shown
/// at once. One refused change doesn't stop the others. Then the table is read again (or, offline, the queue shown).
async fn submit(state: &AppState, changes: Vec<outbox::PendingChange>) -> Result<Submitted, String> {
    let (settings, user_folder, optional) = db_write_context(state);
    let mut result = Submitted::default();
    if settings.read_only {
        result.problems.push("The database connection is read-only.".into());
        result.refused.extend(0..changes.len());
        return Ok(result);
    }
    let source = settings.source();
    let online = {
        let d = state.database.lock().unwrap();
        d.error.is_empty() && !d.token.is_empty()
    };
    let mut direct = online && outbox::changes(&user_folder, &source).is_empty();
    let mut touched = std::collections::HashSet::new();
    for (i, change) in changes.into_iter().enumerate() {
        // a session still waiting to be added can only be changed in the queue
        if direct && !change.row_id.starts_with("pending:") {
            match apply_change(&settings, &user_folder, &optional, &change, &mut touched).await {
                Ok(()) => {
                    result.done += 1;
                    continue;
                }
                // the rest are queued, in order, after this one
                Err(e) if e.unavailable => direct = false,
                Err(e) => {
                    let what = change.row.as_ref().map(|r| r.name.clone()).unwrap_or_else(|| change.from.clone());
                    result.problems.push(format!("{what}: {}", e.message));
                    result.refused.insert(i);
                    continue;
                }
            }
        }
        outbox::add(&user_folder, &source, change)?;
        result.done += 1;
    }
    if state.database.lock().unwrap().error.is_empty() {
        refresh_database(state).await?;
    } else {
        let mut db_state = state.database.lock().unwrap();
        show_database(state, &settings, &mut db_state);
    }
    Ok(result)
}

/// One change from the tree: Err with why when the table refused it
async fn submit_one(state: &AppState, change: outbox::PendingChange) -> Result<(), String> {
    let submitted = submit(state, vec![change]).await?;
    match submitted.problems.first() {
        // a single change's reason, without its name in front
        Some(problem) if !submitted.refused.is_empty() => Err(problem.split_once(": ").map(|(_, why)| why).unwrap_or(problem).to_string()),
        Some(problem) => Err(problem.clone()),
        None => Ok(()),
    }
}

/// Show the cached sessions at once, before the table is read (at start and after the settings change)
#[tauri::command]
fn db_cached(state: State<'_, AppState>) -> DbState {
    let settings = state.settings.lock().unwrap().database.clone();
    let mut db_state = state.database.lock().unwrap();
    if !settings.enabled {
        state.sessions.lock().unwrap().database.clear();
        *db_state = DbState::default();
        return db_state.clone();
    }
    *db_state = DbState { enabled: true, source: format!("{}.{}", settings.schema, settings.table), ..DbState::default() };
    show_cache(&state, &settings, &mut db_state);
    show_database(&state, &settings, &mut db_state);
    db_state.clone()
}

/// The Database connection, the user's folder and the table's optional columns, for a write
fn db_write_context(state: &AppState) -> (db::DbSettings, PathBuf, Vec<String>) {
    let settings = state.settings.lock().unwrap().database.clone();
    let optional = state.database.lock().unwrap().optional_columns.clone();
    (settings, state.user_folder.clone(), optional)
}

/// A Database session as last read, refusing one that's locked whole
fn db_session(state: &AppState, id: &str) -> Result<sessions::StoredSession, String> {
    let sessions = state.sessions.lock().unwrap();
    let session = sessions.find(sessions::Store::Database, id).ok_or("The session is no longer in the database view; Refresh.")?.clone();
    if session.get("DatabaseLocked") == "true" {
        return Err(format!("\"{}\" is locked in the database.", session.get("SessionName")));
    }
    Ok(session)
}

/// Save a Database session from Edit Session (`original_id` None for a new one), then read the table back
#[tauri::command]
async fn db_session_save(state: State<'_, AppState>, original_id: Option<String>, fields: editing::SessionFields) -> Result<String, String> {
    folders::check_name(&fields.name)?;
    match &original_id {
        Some(id) => {
            let stored = db_session(&state, id)?;
            // fields locked in the table can't change (the trigger refuses too; this says which)
            let row = editing::to_row(&fields, Some(&stored));
            let before = db::Row::of(&stored, stored.folder());
            for locked in stored.get("DatabaseLockedFields").split(',').map(str::trim).filter(|f| !f.is_empty()) {
                if db::column_of(&row, locked) != db::column_of(&before, locked) {
                    return Err(format!("{locked} of \"{}\" is locked in the database.", before.name));
                }
            }
            let mut change = outbox::PendingChange::new(outbox::Kind::Update);
            change.row_id = stored.get("DatabaseId").to_string();
            change.version = stored.get("DatabaseVersion").to_string();
            change.row = Some(row);
            submit_one(&state, change).await?;
            let new = editing::SessionFields { folder: fields.folder.clone(), ..fields.clone() }.id();
            follow_id(&state, id, &format!("@db/{new}"));
        }
        None => {
            let mut change = outbox::PendingChange::new(outbox::Kind::Insert);
            change.row = Some(editing::to_row(&fields, None));
            submit_one(&state, change).await?;
        }
    }
    let id = editing::SessionFields { folder: fields.folder.clone(), ..fields }.id();
    Ok(format!("@db/{id}"))
}

/// Delete a Database session, then read the table back
#[tauri::command]
async fn db_session_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let stored = db_session(&state, &id)?;
    let mut change = outbox::PendingChange::new(outbox::Kind::Delete);
    change.row_id = stored.get("DatabaseId").to_string();
    change.from = stored.id().trim_start_matches("@db/").to_string();
    submit_one(&state, change).await?;
    shortcuts::forget(&state.user_folder, |s| s == id);
    Ok(())
}

/// Move a Database session to another folder of the table, then read it back; returns its new id
#[tauri::command]
async fn db_session_move(state: State<'_, AppState>, id: String, folder: String) -> Result<String, String> {
    let stored = db_session(&state, &id)?;
    if stored.get("DatabaseLockedFields").split(',').any(|f| f.trim() == "folder") {
        return Err(format!("The folder of \"{}\" is locked in the database.", stored.get("SessionName")));
    }
    let row = db::Row::of(&stored, &folder);
    let mut change = outbox::PendingChange::new(outbox::Kind::Update);
    change.row_id = stored.get("DatabaseId").to_string();
    change.version = stored.get("DatabaseVersion").to_string();
    change.row = Some(row.clone());
    submit_one(&state, change).await?;
    let folder = folder.trim_matches('/');
    let new = if folder.is_empty() { format!("@db/{}", row.name) } else { format!("@db/{folder}/{}", row.name) };
    follow_id(&state, &id, &new);
    Ok(new)
}

/// Rename or move a Database folder (every row below it, and its settings), then read the table back
#[tauri::command]
async fn db_folder_move(state: State<'_, AppState>, from: String, to: String) -> Result<(), String> {
    folders::check_name(to.rsplit('/').next().unwrap_or(&to))?;
    let blocked = {
        let sessions = state.sessions.lock().unwrap();
        sessions.database.iter().find(|s| {
            let folder = s.folder();
            (folder == from || folder.starts_with(&format!("{from}/")))
                && (s.get("DatabaseLocked") == "true" || s.get("DatabaseLockedFields").split(',').any(|f| f.trim() == "folder"))
        }).map(|s| s.get("SessionName").to_string())
    };
    if let Some(name) = blocked {
        return Err(format!("\"{name}\" in this folder is locked in the database, so the folder can't be renamed here."));
    }
    let mut change = outbox::PendingChange::new(outbox::Kind::RenameFolder);
    change.from = from.trim_matches('/').to_string();
    change.to = to.trim_matches('/').to_string();
    let (old, new) = (format!("@db/{}", change.from), format!("@db/{}", change.to));
    submit_one(&state, change).await?;
    follow_folder(&state, &old, &new);
    Ok(())
}

/// Delete a Database folder and the sessions it shows, then read the table back
#[tauri::command]
async fn db_folder_delete(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let blocked = {
        let sessions = state.sessions.lock().unwrap();
        sessions.database.iter().find(|s| {
            let folder = s.folder();
            (folder == path || folder.starts_with(&format!("{path}/"))) && s.get("DatabaseLocked") == "true"
        }).map(|s| s.get("SessionName").to_string())
    };
    if let Some(name) = blocked {
        return Err(format!("\"{name}\" in this folder is locked in the database, so the folder can't be removed here."));
    }
    let mut change = outbox::PendingChange::new(outbox::Kind::DeleteFolder);
    change.from = path.trim_matches('/').to_string();
    let gone = format!("@db/{}", change.from);
    submit_one(&state, change).await?;
    forget_folder(&state, &gone);
    Ok(())
}

/// Save a Database folder's settings in the folders table, then read the table back
#[tauri::command]
async fn db_folder_settings_save(state: State<'_, AppState>, fields: folders::FolderFields) -> Result<(), String> {
    let (settings, user_folder, _) = db_write_context(&state);
    let setting = sessions::FolderSetting {
        path: fields.path,
        color: fields.color,
        username: fields.username,
        putty_profile: fields.putty_profile,
        port: fields.port,
        jump_host: fields.jump_host,
        highlight_rules: fields.highlight_rules,
    };
    db::set_folder(&settings, &user_folder, &setting).await?;
    refresh_database(&state).await?;
    Ok(())
}

/// What a Move to / Copy to did
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferReport {
    done: usize,
    problems: Vec<String>,
}

/// Copy (or move) sessions to another root, each to the same folder path there, as SuperXPuTTY's Move to / Copy to.
/// A copy out of the table leaves its database fields behind (SuperXPuTTY's PlainCopy); a move deletes only what
/// arrived, and never a locked row.
#[tauri::command]
async fn sessions_transfer(
    state: State<'_, AppState>,
    from: sessions::Store,
    ids: Vec<String>,
    to: sessions::Store,
    delete_source: bool,
) -> Result<TransferReport, String> {
    use sessions::Store;
    if from == to {
        return Err("Choose another place to copy to.".into());
    }
    let picked: Vec<sessions::StoredSession> = {
        let all = state.sessions.lock().unwrap();
        ids.iter().filter_map(|id| all.find(from, id).cloned()).collect()
    };
    let mut problems = Vec::new();
    
    let arrived: Vec<sessions::StoredSession> = if to == Store::Database {
        let changes = picked
            .iter()
            .map(|session| {
                let mut change = outbox::PendingChange::new(outbox::Kind::Insert);
                change.row = Some(db::Row::of(session, session.folder()));
                change
            })
            .collect();
        let submitted = submit(&state, changes).await?;
        problems.extend(submitted.problems);
        picked.iter().enumerate().filter(|(i, _)| !submitted.refused.contains(i)).map(|(_, s)| s.clone()).collect()
    } else {
        let path = store_file(&state, to)?;
        let others = other_ids(&state, to);
        let mut added = Vec::new();
        sessions::update_file(&path, |list| {
            for session in &picked {
                // as a file session: no database fields, and an id without the @db prefix
                let folder = session.folder();
                let name = match session.get("SessionName") {
                    "" => session.id().rsplit('/').next().unwrap_or("").to_string(),
                    n => n.to_string(),
                };
                let id = if folder.is_empty() { name.clone() } else { format!("{folder}/{name}") };
                if list.iter().any(|s| s.id().eq_ignore_ascii_case(&id)) || others.iter().any(|o| o.eq_ignore_ascii_case(&id) && !(from != Store::Database && o == session.id())) {
                    problems.push(format!("{id}: there's already a session with that name there."));
                    continue;
                }
                let mut copy = sessions::StoredSession {
                    attributes: session.attributes.iter().filter(|(n, _)| !n.starts_with("Database")).cloned().collect(),
                };
                copy.set("SessionId", &id);
                copy.set("SessionName", &name);
                sessions::insert_sorted(list, copy);
                added.push(session.clone());
            }
            Ok(())
        })?;
        added
    };
    if delete_source && from == Store::Database {
        let mut deletes = Vec::new();
        for session in &arrived {
            if session.get("DatabaseLocked") == "true" {
                problems.push(format!("{}: locked in the database, so it stays there.", session.id()));
                continue;
            }
            let mut change = outbox::PendingChange::new(outbox::Kind::Delete);
            change.row_id = session.get("DatabaseId").to_string();
            change.from = session.id().trim_start_matches("@db/").to_string();
            deletes.push(change);
        }
        problems.extend(submit(&state, deletes).await?.problems);
    } else if delete_source {
        for session in &arrived {
            if let Err(e) = store_file(&state, from).and_then(|path| editing::delete(&path, session.id())) {
                problems.push(e);
            }
        }
    }
    if delete_source {
        // Favorites and Recent follow sessions moved to the other root
        for session in arrived.iter().filter(|s| s.get("DatabaseLocked") != "true") {
            let path = session.id().trim_start_matches("@db/");
            let new = if to == Store::Database { format!("@db/{path}") } else { path.to_string() };
            follow_id(&state, session.id(), &new);
        }
    }
    Ok(TransferReport { done: arrived.len(), problems })
}

/// How Database sessions stand now, without reading anything (after a change from the tree)
#[tauri::command]
fn db_status(state: State<'_, AppState>) -> DbState {
    state.database.lock().unwrap().clone()
}

/// One queued change, for the Pending changes window
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PendingView {
    id: String,
    what: String,
    queued_at: String,
    error: Option<String>,
    force: bool,
    /// An edit refused by the table, which Overwrite theirs can save over someone else's change
    can_overwrite: bool,
}

/// The connection's queued and refused changes, oldest first
#[tauri::command]
fn db_pending_list(state: State<'_, AppState>) -> Vec<PendingView> {
    let settings = state.settings.lock().unwrap().database.clone();
    outbox::changes(&state.user_folder, &settings.source())
        .into_iter()
        .map(|c| PendingView {
            what: c.describe(),
            can_overwrite: c.kind == outbox::Kind::Update && c.error.is_some(),
            id: c.id,
            queued_at: c.queued_at,
            error: c.error,
            force: c.force,
        })
        .collect()
}

/// Send the chosen changes again (`overwrite`: edits over someone else's change); they go now if the table can be
/// reached
#[tauri::command]
async fn db_pending_retry(state: State<'_, AppState>, ids: Vec<String>, overwrite: bool) -> Result<DbState, String> {
    let source = state.settings.lock().unwrap().database.source();
    for id in &ids {
        outbox::set_error(&state.user_folder, &source, id, None, overwrite)?;
    }
    refresh_database(&state).await
}

/// Drop the chosen changes; the sessions go back to what the table has
#[tauri::command]
fn db_pending_discard(state: State<'_, AppState>, ids: Vec<String>) -> Result<DbState, String> {
    let settings = state.settings.lock().unwrap().database.clone();
    for id in &ids {
        outbox::remove(&state.user_folder, &settings.source(), id)?;
    }
    let mut db_state = state.database.lock().unwrap();
    show_database(&state, &settings, &mut db_state);
    Ok(db_state.clone())
}

/// Create / upgrade schema with the window's values (and a password typed there), as SuperXPuTTY's button
#[tauri::command]
async fn db_create_schema(state: State<'_, AppState>, database: db::DbSettings, password: Option<String>) -> Result<String, String> {
    if let Some(password) = password.filter(|p| !p.is_empty()) {
        db::set_password(&password)?;
    }
    let user_folder = state.user_folder.clone();
    db::create_or_upgrade_schema(&database, &user_folder).await
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
        &[
            &MenuItem::with_id(app, "newLocalShell", "New Local Shell", true, Some("CmdOrCtrl+T"))?,
            &PredefinedMenuItem::separator(app)?,
            // Cmd+W closes the tab, as in Terminal and iTerm2; the window closes with its red button
            &MenuItem::with_id(app, "closeTab", "Close Tab", true, Some("CmdOrCtrl+W"))?,
        ],
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
            &PredefinedMenuItem::separator(app)?,
            // the terminal's own Find (its Ctrl+Shift+F) and Clear Scrollback, with the Mac's keys
            &MenuItem::with_id(app, "find", "Find…", true, Some("CmdOrCtrl+F"))?,
            &MenuItem::with_id(app, "clearScrollback", "Clear Scrollback", true, Some("CmdOrCtrl+K"))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;
    let tools = Submenu::with_items(
        app,
        "Tools",
        true,
        &[
            &MenuItem::with_id(app, "scriptEditor", "Script Editor…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "databaseSessions", "Database Sessions…", true, None::<&str>)?,
            &MenuItem::with_id(app, "environmentVariables", "Environment Variables…", true, None::<&str>)?,
        ],
    )?;
    // shown or hidden, as SuperXPuTTY's View > Send Commands and View > Button Bar
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &MenuItem::with_id(app, "toggleSendCommands", "Send Commands", true, None::<&str>)?,
            &MenuItem::with_id(app, "toggleButtonBar", "Button Bar", true, None::<&str>)?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &tools, &window])
}

/// The main window, made here rather than in tauri.conf.json so its web view can have Writing Tools off: macOS put
/// its "Write with Siri" badge by the terminal's cursor, and offered to rewrite what's typed into a session
fn main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    let builder = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
        .title("SuperXTerminal")
        .inner_size(1400.0, 900.0)
        .min_inner_size(800.0, 500.0)
        .disable_drag_drop_handler();
    #[cfg(target_os = "macos")]
    let builder = builder.with_webview_configuration(writing_tools_off());
    builder.build()?;
    Ok(())
}

/// A WebKit configuration with Writing Tools off (writingToolsBehavior = none, macOS 15 and later; older macOS has
/// none to turn off). Set by key, as objc2-web-kit has no binding for it yet.
#[cfg(target_os = "macos")]
fn writing_tools_off() -> objc2::rc::Retained<objc2_web_kit::WKWebViewConfiguration> {
    use objc2::{sel, MainThreadMarker};
    use objc2_foundation::{ns_string, NSNumber, NSObjectNSKeyValueCoding, NSObjectProtocol};
    let mtm = MainThreadMarker::new().expect("the window is made on the main thread");
    let config = unsafe { objc2_web_kit::WKWebViewConfiguration::new(mtm) };
    if config.respondsToSelector(sel!(setWritingToolsBehavior:)) {
        // NSWritingToolsBehaviorNone
        unsafe { config.setValue_forKey(Some(&NSNumber::new_isize(-1)), ns_string!("writingToolsBehavior")) };
    }
    config
}

/// Keep macOS 27's "Write with Siri" badge (AppKit's "Campo" lightweight UI) away from the terminal. AppKit shows it by
/// the cursor of any text field it finds eligible, and the terminal's hidden input (xterm.js types through it, under
/// its own drawn cursor) is one; what's typed there goes to a session, so it isn't writing to help with. There's no
/// public switch and WebKit passes no field setting for it, so in this app NSTextInputContext finds no field eligible
/// and shows no affordance (the eligibility check alone wasn't enough). Nothing changes where these private methods
/// don't exist (older macOS, or a later one that renames them).
#[cfg(target_os = "macos")]
fn no_siri_badge() {
    use objc2::ffi::{class_getInstanceMethod, method_setImplementation};
    use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
    use objc2::sel;
    extern "C-unwind" fn not_eligible(_this: *mut AnyObject, _cmd: Sel) -> Bool {
        Bool::NO
    }
    extern "C-unwind" fn show_nothing(_this: *mut AnyObject, _cmd: Sel, _mode: isize) {}
    let Some(class) = AnyClass::get(c"NSTextInputContext") else { return };
    unsafe {
        let eligible = class_getInstanceMethod(class, sel!(_isCampoLightweightUIEligible));
        if !eligible.is_null() {
            let imp: Imp = std::mem::transmute(not_eligible as extern "C-unwind" fn(*mut AnyObject, Sel) -> Bool);
            method_setImplementation(eligible as *mut _, imp);
        }
        let show = class_getInstanceMethod(class, sel!(_showLightweightAffordanceWithMode:));
        if !show.is_null() {
            let imp: Imp = std::mem::transmute(show_nothing as extern "C-unwind" fn(*mut AnyObject, Sel, isize));
            method_setImplementation(show as *mut _, imp);
        }
    }
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
        .setup(|app| {
            logs::sweep_temp();
            #[cfg(target_os = "macos")]
            no_siri_badge();
            main_window(app.handle())?;
            Ok(())
        })
        .manage(pty::Ptys::default())
        .manage(logs::Logs::default())
        .manage(AppState {
            user_folder,
            settings: Mutex::new(settings),
            sessions: Mutex::new(Sessions::default()),
            profiles: Mutex::new(profiles),
            database: Mutex::new(DbState::default()),
            db_server: Mutex::new(Vec::new()),
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
            db_cached,
            db_check,
            db_export,
            db_import,
            db_session_save,
            db_session_delete,
            db_session_move,
            db_folder_move,
            db_folder_delete,
            db_folder_settings_save,
            sessions_transfer,
            db_create_schema,
            db_pending_list,
            shortcuts_get,
            log_open,
            log_write,
            log_close,
            log_read,
            open_path,
            buttonbars_load,
            buttonbars_save,
            buttonbars_export,
            buttonbars_import,
            scripts_list,
            session_script,
            script_load,
            script_save,
            script_delete,
            text_file_read,
            text_file_write,
            shortcuts_favorite,
            shortcuts_opened,
            shortcuts_forget_recent,
            shortcuts_expanded,
            db_status,
            db_pending_retry,
            db_pending_discard,
            variables_save,
            variables_fill,
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
