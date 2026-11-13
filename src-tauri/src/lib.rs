//! SuperXTerminal's Rust core: the commands the UI (`src/`) calls through Tauri's `invoke`.

mod algorithms;
mod applog;
mod appimage;
mod askpass;
mod buttonbars;
mod cli;
mod charset;
mod onepassword;
mod own_process;
mod ui_themes;
mod clipboard;
mod clipboard_contents;
mod clipboard_files;
#[cfg(any(target_os = "macos", test))]
mod clipboard_forms;
mod command_history;
mod command_tap;
mod console;
mod db;
mod demo;
mod demo_device;
mod editing;
mod external;
mod file_watch;
mod folders;
mod highlighting;
mod importing;
mod keys;
mod launch;
mod layouts;
mod logs;
mod ppk;
mod profiles;
mod pty;
mod putty_reg;
mod rdp;
mod rdp_clipboard;
mod rdp_drives;
mod rdp_options;
mod rdp_sound;
mod rdp_win_keys;
mod remount;
mod rhai_scripts;
mod scripts;
mod secrets;
mod server_load;
mod outbox;
mod overrides;
// taskbar pins exist on Windows alone
#[cfg(windows)]
mod pins;
mod saved_cache;
mod sessions;
mod session_passwords;
mod shortcuts;
mod streams;
mod tray;
mod variables;
mod settings;
mod saving;
mod sftp;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

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
    /// The session files' contents when last read, so a change made elsewhere can be noticed
    files_seen: Mutex<file_watch::Seen>,
    /// What the last read of the session files said; None until they've been read ([`sessions_shown`])
    files_read: Mutex<Option<FilesRead>>,
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
    // the profiles based on a renamed one follow it, rather than falling back to Default Settings
    if renaming {
        for other in profiles.profiles.iter_mut().filter(|p| p.based_on.trim().eq_ignore_ascii_case(&key)) {
            other.based_on = name.clone();
        }
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

/// Export a profile to a file, without its password (Settings > Profiles, Export…)
#[tauri::command]
fn profiles_export(state: State<'_, AppState>, name: String, path: String) -> Result<(), String> {
    let profiles = state.profiles.lock().unwrap();
    let profile = profiles.get(&name).ok_or_else(|| format!("There's no profile called {name}."))?;
    std::fs::write(&path, profiles::export_text(profile)?).map_err(|e| e.to_string())
}

/// PuTTY's saved sessions in a `reg export` file, as profiles for Settings > Profiles' Import… to add (putty_reg.rs)
#[tauri::command]
fn profiles_read_putty(path: String) -> Result<Vec<Profile>, String> {
    putty_reg::read(&std::fs::read(&path).map_err(|e| e.to_string())?)
}

/// The team's profile templates (ProfileTemplates.json in the settings folder), for Settings > Profiles to offer the
/// ones not set up here; none without a settings folder
#[tauri::command]
fn profile_templates(state: State<'_, AppState>) -> Result<Vec<Profile>, String> {
    let folder = state.settings.lock().unwrap().settings_folder.clone();
    folder.map_or(Ok(Vec::new()), |f| profiles::templates(&f))
}

/// Share a profile with the team as a template in the settings folder, without its login (Settings > Profiles'
/// Share as Team Template)
#[tauri::command]
fn profile_template_share(state: State<'_, AppState>, name: String) -> Result<Vec<Profile>, String> {
    let folder = state.settings.lock().unwrap().settings_folder.clone().ok_or("Choose a settings folder first (Settings > General): the templates are kept there for the team.")?;
    let profile = state.profiles.lock().unwrap().get(&name).cloned().ok_or_else(|| format!("There's no profile called {name}."))?;
    let shared = profiles::share_template(&folder, &profile)?;
    applog::info(format!("Shared profile {name} as a team template"));
    Ok(shared)
}

/// The profiles in a file, for Settings > Profiles' Import… to add (it names them and saves each)
#[tauri::command]
fn profiles_read_file(path: String) -> Result<Vec<Profile>, String> {
    profiles::import_text(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
}

/// What the last read of the session files said besides the sessions themselves, kept so they can be shown again
/// without reading the files ([`sessions_shown`])
#[derive(Clone)]
struct FilesRead {
    /// The files that couldn't be read, and why
    problems: Vec<String>,
    /// The settings folder couldn't be reached: Saved sessions are the copy kept on this computer ([`saved_cache`])
    saved_offline: bool,
    /// When that copy was read; "" when not offline, or there's none
    saved_cached_at: String,
}

/// Read (or read again) the session and folder files, and give the UI what it shows
#[tauri::command]
fn sessions_load(app: AppHandle, state: State<'_, AppState>) -> SessionsLoaded {
    let private_off = private_off(&state);
    // what the files are known by from now on: their sizes and times as they are before they're read (a change made
    // meanwhile then shows at the next check), and the hash of the very bytes read here, not of a second reading
    let files = session_files(&state);
    let stamps = file_watch::stamps(&files);
    let reading = file_watch::Reading::default();
    let mut problems = Vec::new();
    // Saved sessions are the copy kept on this computer while the settings folder can't be reached (a network drive
    // that's away), rather than none
    let saved_cache::Loaded { sessions: mut loaded, saved_offline, saved_cached_at } =
        saved_cache::load_with(saved_folder(&state).as_deref(), &state.user_folder, &mut problems, &|path| reading.text(path));
    *state.files_seen.lock().unwrap() = file_watch::Seen::read(reading.fingerprint(&files), stamps, std::time::Instant::now());
    if saved_offline {
        applog::warn(format!("The settings folder can't be reached: Saved sessions are {}", if saved_cached_at.is_empty() { "not shown".into() } else { format!("shown as read {saved_cached_at}") }));
    }
    // Private sessions turned off: not shown or used (PrivateSessions.xml is left as it is)
    if private_off {
        loaded.private.clear();
        loaded.folders.remove(&sessions::Store::Private);
    }
    // Database sessions come from the table (or its cache), not the files: keep them
    {
        let mut current = state.sessions.lock().unwrap();
        loaded.database = std::mem::take(&mut current.database);
        if let Some(folders) = current.folders.remove(&sessions::Store::Database) {
            loaded.folders.insert(sessions::Store::Database, folders);
        }
    }
    let counts = (loaded.saved.len(), loaded.private.len(), loaded.database.len());
    *state.sessions.lock().unwrap() = loaded;
    applog::info(format!("Read the sessions: {} saved, {} private, {} from the database", counts.0, counts.1, counts.2));
    for problem in &problems {
        applog::warn(format!("Sessions: {problem}"));
    }
    // the Mac: the network share the settings folder is on is remembered while it's there, to be connected again
    // when it has been away (remount.rs); Windows reaches a share by its address, so needs none of it
    if cfg!(target_os = "macos") && !saved_offline {
        if let Some(folder) = saved_folder(&state) {
            remount::remember(&state.user_folder, &folder);
        }
    }
    let read = FilesRead { problems, saved_offline, saved_cached_at };
    *state.files_read.lock().unwrap() = Some(read.clone());
    tray::refresh(&app);
    shown(&state, read)
}

/// The sessions as they stand, for the UI to show again, without reading the files: after the Database sessions
/// changed (their cache shown at start, the table read, a pending change sent or dropped), which the files have no
/// part in. Reading them for that parsed every Saved and Private session again, three times as the app started, and
/// fetched the files again each time when the settings folder is a network drive. Before the files have been read
/// at all, it reads them.
#[tauri::command]
fn sessions_shown(app: AppHandle, state: State<'_, AppState>) -> SessionsLoaded {
    let read = state.files_read.lock().unwrap().clone();
    match read {
        Some(read) => {
            tray::refresh(&app);
            shown(&state, read)
        }
        None => sessions_load(app, state),
    }
}

/// What the UI shows of the sessions as they stand, with what the last read of the files said
fn shown(state: &AppState, read: FilesRead) -> SessionsLoaded {
    // the settings read, and their lock let go, before the sessions' is taken (see private_off)
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    let saved_off = !state.settings.lock().unwrap().use_sessions_xml;
    let private_off = private_off(state);
    let sessions = state.sessions.lock().unwrap();
    SessionsLoaded {
        sessions: sessions.views(),
        folders: sessions
            .folders
            .iter()
            .flat_map(|(store, settings)| settings.values().map(move |setting| (*store, setting.clone())))
            .collect(),
        problems: read.problems,
        saved_file: settings_folder.map(|f| f.join("Sessions.xml").display().to_string()),
        private_file: state.user_folder.join("PrivateSessions.xml").display().to_string(),
        saved_off,
        private_off,
        saved_offline: read.saved_offline,
        saved_cached_at: read.saved_cached_at,
    }
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
    if store == sessions::Store::Saved && !state.settings.lock().unwrap().use_sessions_xml {
        return Err("Saved sessions are turned off (Settings > General > Use Sessions.xml).".into());
    }
    if store == sessions::Store::Private && private_off(state) {
        return Err("Private sessions are turned off (Settings > General > Use PrivateSessions.xml).".into());
    }
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    // The settings folder away (a network drive that dropped): Saved sessions are shown from the copy on this computer
    // and can't be changed. A save reads the file first and takes one it can't find for a team with no sessions yet,
    // so a drive that came back between the read and the write would have its sessions replaced by the one saved
    if store == sessions::Store::Saved && settings_folder.as_deref().is_some_and(|folder| !saved_cache::reachable(folder)) {
        return Err(SAVED_OFFLINE.into());
    }
    sessions::file_of(store, settings_folder.as_deref(), &state.user_folder).ok_or_else(|| "Choose the settings folder first.".into())
}

/// Why Saved sessions can't be changed while the settings folder can't be reached
const SAVED_OFFLINE: &str = "The settings folder can't be reached, so Saved sessions can't be changed now. They're shown as last read, and can be changed once it's back.";

/// Whether Private sessions are turned off: Use PrivateSessions.xml unticked, and Saved sessions in use (without them,
/// Private sessions stay on whatever the setting says, so sessions always have somewhere to be, as when the settings
/// folder has gone since it was turned off)
fn private_off(state: &AppState) -> bool {
    // read, and the lock let go, before saved_folder takes it: the settings' Mutex can't be taken twice, and taking it
    // in one statement with saved_folder hung the app as it first read the sessions
    let use_private = state.settings.lock().unwrap().use_private_sessions;
    private_off_when(use_private, saved_folder(state).is_some())
}

/// `private_off`'s rule: off only when it's turned off and Saved sessions are in use
fn private_off_when(use_private: bool, saved_in_use: bool) -> bool {
    !use_private && saved_in_use
}

/// The settings folder when Saved sessions are in use (Sessions.xml and Folders.xml are read from it); None when
/// there's none yet or they're turned off, as SuperXPuTTY's UseSessionsXml
fn saved_folder(state: &AppState) -> Option<PathBuf> {
    let settings = state.settings.lock().unwrap();
    settings.settings_folder.clone().filter(|_| settings.use_sessions_xml)
}

/// One session's fields for Edit Session (a `-pw` password masked)
#[tauri::command]
fn session_get(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<editing::SessionFields, String> {
    let sessions = state.sessions.lock().unwrap();
    let session = sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?;
    Ok(editing::SessionFields::of(session))
}

/// Save a session from Edit Session (`original_id` None for a new one); returns its id. The caller reloads the tree.
/// `copy_of` is the session a new one was filled in from (Duplicate as…, Save session as…), whose -pw it keeps.
#[tauri::command]
fn session_save(
    state: State<'_, AppState>,
    store: sessions::Store,
    original_id: Option<String>,
    fields: editing::SessionFields,
    copy_of: Option<(sessions::Store, String)>,
) -> Result<String, String> {
    let path = store_file(&state, store)?;
    let fields = with_copied_password(&state, fields, copy_of.as_ref());
    // ids must be unique across Saved and Private sessions, as in SuperXPuTTY
    let id = editing::save(&path, original_id.as_deref(), &fields, &other_ids(&state, store))?;
    if let Some(old) = original_id {
        follow_id(&state, &old, &id);
    }
    Ok(id)
}

/// One session for File > Edit Sessions as Table…: where it is, every field Edit Session has (a `-pw` masked), and,
/// for a Database session, what the table locks against changes
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TableRow {
    store: sessions::Store,
    id: String,
    fields: editing::SessionFields,
    /// Database sessions: the whole row locked in the table
    locked: bool,
    /// Database sessions: the columns locked in the table, comma-separated ("host,port")
    locked_fields: String,
    /// It has a password of its own in the Keychain (Edit Session's Password), from the list, the Keychain not read
    has_password: bool,
}

/// Every session, Saved, Private and Database, with all its fields, for File > Edit Sessions as Table…. The tree's
/// view (sessions_load) has only the fields it shows, and asking for each session's fields one by one would be one
/// call per session
#[tauri::command]
fn sessions_table_load(state: State<'_, AppState>) -> Vec<TableRow> {
    use sessions::Store;
    let with_password = session_passwords::listed(&state.user_folder);
    let sessions = state.sessions.lock().unwrap();
    let rows = |store: Store, list: &[sessions::StoredSession]| -> Vec<TableRow> {
        list.iter()
            .map(|s| TableRow {
                store,
                id: s.id().to_string(),
                fields: editing::SessionFields::of(s),
                locked: s.get("DatabaseLocked") == "true",
                locked_fields: s.get("DatabaseLockedFields").to_string(),
                has_password: with_password.contains(s.id()),
            })
            .collect()
    };
    let mut all = rows(Store::Saved, &sessions.saved);
    all.extend(rows(Store::Private, &sessions.private));
    all.extend(rows(Store::Database, &sessions.database));
    all
}

/// Save the rows changed in File > Edit Sessions as Table… that are in one file (Saved or Private): each
/// `(original id, fields)`, the id None for a new session, in one write and one backup ([`editing::save_many`]), all or
/// none. Favorites, Recent,
/// overrides and the Keychain password follow each renamed or moved session. Returns each session's id now. Database
/// rows go one by one through db_session_save, which checks each against the table's version and locks.
#[tauri::command]
fn sessions_table_save(state: State<'_, AppState>, store: sessions::Store, edits: Vec<(Option<String>, editing::SessionFields)>) -> Result<Vec<String>, String> {
    let path = store_file(&state, store)?;
    let ids = editing::save_many(&path, &edits, &other_ids(&state, store))?;
    for ((old, _), new) in edits.iter().zip(&ids) {
        if let Some(old) = old {
            follow_id(&state, old, new);
        }
    }
    Ok(ids)
}

/// A new session's fields with the masked -pw of the session it was copied from put back, as SuperXPuTTY's
/// Duplicate copies the session whole; the window only ever shows the mask
fn with_copied_password(state: &AppState, mut fields: editing::SessionFields, copy_of: Option<&(sessions::Store, String)>) -> editing::SessionFields {
    if let Some((store, id)) = copy_of {
        if let Some(source) = state.sessions.lock().unwrap().find(*store, id) {
            fields.extra_args = editing::unmask_password(&fields.extra_args, source.get("ExtraArgs"));
        }
    }
    fields
}

/// Whether a session has a password of its own (Edit Session's Password), without reading the Keychain
#[tauri::command]
fn session_password_has(state: State<'_, AppState>, id: String) -> bool {
    session_passwords::has(&state.user_folder, &id)
}

/// Store a session's own password in the Keychain, or forget it when `password` is empty
#[tauri::command]
fn session_password_set(state: State<'_, AppState>, id: String, password: String) -> Result<(), String> {
    session_passwords::set(&state.user_folder, &id, &password)
}

/// Edit sessions… and Edit Sessions as Table…: one password stored as each session's own, in the Keychain on this
/// computer only, or forgotten from each when `password` is empty. Returns how many
#[tauri::command]
fn session_passwords_set(state: State<'_, AppState>, ids: Vec<String>, password: String) -> Result<usize, String> {
    session_passwords::set_many(&state.user_folder, &ids, &password)
}

/// Edit Session's Move to Password: the session's stored -pw becomes its own password in the Keychain. The -pw stays
/// in its extra arguments until the window saves them without it.
#[tauri::command]
fn session_password_from_pw(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<(), String> {
    let extra = {
        let sessions = state.sessions.lock().unwrap();
        let session = sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?;
        session.get("ExtraArgs").to_string()
    };
    let password = editing::stored_password(&extra).ok_or("The session has no -pw password.")?;
    session_passwords::set(&state.user_folder, &id, &password)
}

/// Delete a session from its file
#[tauri::command]
fn session_delete(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<(), String> {
    let path = store_file(&state, store)?;
    editing::delete(&path, &id)?;
    forget_id(&state, &id);
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
fn session_move(state: State<'_, AppState>, store: sessions::Store, id: String, folder: String, name: Option<String>) -> Result<String, String> {
    let file = store_file(&state, store)?;
    let new = folders::move_session(&file, &id, &folder, &other_ids(&state, store), name.as_deref())?;
    follow_id(&state, &id, &new);
    Ok(new)
}

/// Move a selection of folders and sessions (ids) into the folder `to` of their store, "" for the top
/// ([`folders::move_many`]): one read, one backup and one write of the sessions file however many are moved, where a
/// folder_move or session_move for each wrote it over each time. Favorites, Recent, the local overrides and the
/// sessions' own passwords follow in one pass too. Returns what moved and what couldn't; the caller reloads the tree.
#[tauri::command]
fn items_move(state: State<'_, AppState>, store: sessions::Store, folders: Vec<String>, sessions: Vec<String>, to: String) -> Result<folders::Moved, String> {
    let file = store_file(&state, store)?;
    let moved = folders::move_many(&file, &folders, &sessions, &to, &other_ids(&state, store))?;
    if !moved.renamed.is_empty() {
        let new_ids: std::collections::HashMap<&str, &str> = moved.renamed.iter().map(|(old, new)| (old.as_str(), new.as_str())).collect();
        let rename = |id: &str| new_ids.get(id).map(|new| new.to_string());
        shortcuts::follow(&state.user_folder, rename);
        overrides::follow(&state.user_folder, rename);
        session_passwords::follow(&state.user_folder, rename);
    }
    Ok(moved)
}

/// Delete a selection of folders (with every session in them) and sessions (ids) from their store
/// ([`folders::delete_many`]): one read, one backup and one write of the sessions file however many there are, where
/// a folder_delete or session_delete for each wrote it over each time. Favorites, Recent, the local overrides and the
/// sessions' own passwords forget them in one pass too. Returns what couldn't be deleted and why; the caller reloads.
#[tauri::command]
fn items_delete(state: State<'_, AppState>, store: sessions::Store, folders: Vec<String>, sessions: Vec<String>) -> Result<Vec<String>, String> {
    let file = store_file(&state, store)?;
    let deleted = folders::delete_many(&file, &folders, &sessions)?;
    let ids: std::collections::HashSet<&str> = deleted.gone.iter().map(String::as_str).collect();
    // the sessions deleted, and, as forget_folder, any id below a deleted folder, in any spelling of it (a Favorite
    // of a session that had already gone from the file)
    let gone = |id: &str| ids.contains(id) || folders.iter().any(|f| folders::under(id, f.trim_matches('/')).is_some_and(|rest| !rest.is_empty()));
    shortcuts::forget(&state.user_folder, gone);
    overrides::forget(&state.user_folder, gone);
    session_passwords::forget(&state.user_folder, gone);
    Ok(deleted.problems)
}

/// Favorites, Recent, local overrides and the session's own password follow a session renamed or moved
fn follow_id(state: &AppState, old: &str, new: &str) {
    if old != new {
        shortcuts::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
        overrides::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
        session_passwords::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
    }
}

/// Favorites, Recent, local overrides and the session's own password forget a deleted session
fn forget_id(state: &AppState, gone: &str) {
    shortcuts::forget(&state.user_folder, |s| s == gone);
    overrides::forget(&state.user_folder, |s| s == gone);
    session_passwords::forget(&state.user_folder, |s| s == gone);
}

/// Favorites and Recent follow the sessions below a renamed or moved folder ("Lab" or "@db/Lab"), any spelling of it
fn follow_folder(state: &AppState, from: &str, to: &str) {
    let rename = |id: &str| folders::under(id, from).filter(|rest| !rest.is_empty()).map(|rest| format!("{to}{rest}"));
    shortcuts::follow(&state.user_folder, rename);
    overrides::follow(&state.user_folder, rename);
    session_passwords::follow(&state.user_folder, rename);
}

/// Favorites and Recent forget the sessions below a deleted folder
fn forget_folder(state: &AppState, path: &str) {
    let gone = |id: &str| folders::under(id, path).is_some_and(|rest| !rest.is_empty());
    shortcuts::forget(&state.user_folder, gone);
    overrides::forget(&state.user_folder, gone);
    session_passwords::forget(&state.user_folder, gone);
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

/// The script a session runs as it opens (Edit Session's login script, else its profile's), found as SuperXPuTTY finds
/// it: SPSL (#!/bin/spsl) or Rhai (#!/bin/rhai, or a .rhai file), which the window tells apart
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum SessionScript {
    /// The session has none, nor has its profile
    None,
    /// An http(s) address, which the window fetches
    Url { url: String, profile: Option<String> },
    /// The script's text, and where it's from as written (`source`: a path), whose .rhai says it's a Rhai script
    Text { text: String, profile: Option<String>, source: String },
    /// A file that isn't there, e.g. a Windows path (C:\...) on the Mac. `looked` is where a relative one was looked
    /// for, in this computer's settings folder, so the tab can say where to save a copy of its own
    Missing { path: String, looked: Option<String>, profile: Option<String> },
}

/// Which script a session runs as it opens, and the profile it comes from (None for the session's own): the session's
/// SPSL script, else its profile's (the session's, else its folders', as a launch picks it; local overrides
/// included), else Default Settings'
fn script_spec(sessions: &sessions::Sessions, profiles: &Profiles, store: sessions::Store, session: &sessions::StoredSession) -> Option<(String, Option<String>)> {
    let own = session.get("SPSLFileName").trim();
    if !own.is_empty() {
        return Some((own.to_string(), None));
    }
    let mut profile_name = session.get("PuttySession").to_string();
    for folder in sessions.along(store, session.folder()) {
        if profiles::is_default(&profile_name) && !folder.putty_profile.is_empty() {
            profile_name = folder.putty_profile.clone();
        }
    }
    profiles::script_of(profiles, &profile_name).map(|(spec, name)| (spec.to_string(), Some(name.to_string())))
}

/// The script a session runs when it opens: a file (absolute, file://, or relative to the settings folder), or a URL
#[tauri::command]
fn session_script(state: State<'_, AppState>, store: sessions::Store, id: String) -> SessionScript {
    // each lock taken and let go on its own: std's Mutex isn't reentrant
    let overrides_on = state.settings.lock().unwrap().use_local_overrides;
    let folder = shared_folder(&state);
    let profiles = state.profiles.lock().unwrap().clone();
    let found = {
        let sessions = state.sessions.lock().unwrap();
        sessions
            .find(store, &id)
            .map(|s| overrides::applied(&state.user_folder, s, overrides_on))
            .and_then(|s| script_spec(&sessions, &profiles, store, &s))
    };
    let Some((spec, profile)) = found else { return SessionScript::None };
    read_script(&spec, profile, &folder)
}

/// What a Quick connect tab's script needs: the script its profile runs, and the values the typed fields give it in
/// place of a saved session's ({host_address}, {session_username}...), so a jump server profile's script (Log in here)
/// goes on to the host typed
#[derive(serde::Serialize)]
struct QuickScript {
    script: SessionScript,
    values: std::collections::BTreeMap<String, String>,
}

/// The script a Quick connect tab runs as it opens (its profile's, else Default Settings'), and the typed fields as
/// the session's values, parsed as the launch parses them ("user@host:port")
#[tauri::command]
fn quick_connect_script(state: State<'_, AppState>, quick: launch::QuickConnect) -> Result<QuickScript, String> {
    let session = quick.session()?;
    let folder = shared_folder(&state);
    let profiles = state.profiles.lock().unwrap().clone();
    // a one-off session is in no folder, so no folder's profile applies
    let script = match script_spec(&sessions::Sessions::default(), &profiles, sessions::Store::Saved, &session) {
        Some((spec, profile)) => read_script(&spec, profile, &folder),
        None => SessionScript::None,
    };
    Ok(QuickScript { script, values: quick_values(&session) })
}

/// A Quick connect session's values for its scripts, as a saved session's are (sending.svelte.ts's sessionValues):
/// its host, port (the protocol's usual one when none is typed), username, profile and protocol, named by its host
fn quick_values(session: &sessions::StoredSession) -> std::collections::BTreeMap<String, String> {
    let protocol = session.get("Proto").to_string();
    let port = match session.port() {
        0 => match protocol.to_ascii_uppercase().as_str() {
            "" | "SSH" => "22".to_string(),
            "TELNET" => "23".to_string(),
            _ => String::new(),
        },
        port => port.to_string(),
    };
    [
        ("host_address", session.get("Host").to_string()),
        ("host_port", port),
        ("session_name", session.get("Host").to_string()),
        ("session_username", session.get("Username").to_string()),
        ("session_profile", session.get("PuttySession").to_string()),
        ("session_protocol", protocol),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// A script's text from where it's named: an http(s) address is left for the window to fetch; a file is absolute,
/// file://, or relative to the settings folder
fn read_script(spec: &str, profile: Option<String>, folder: &std::path::Path) -> SessionScript {
    let lower = spec.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return SessionScript::Url { url: spec.to_string(), profile };
    }
    let path = PathBuf::from(spec.strip_prefix("file://").unwrap_or(spec));
    // a relative one typed on Windows ("scripts\login.spsl") is found on the Mac too, where \ is a letter in a name
    let relative = !path.is_absolute();
    let path = if relative { folder.join(spec.replace('\\', "/")) } else { path };
    match std::fs::read_to_string(&path) {
        Ok(text) => SessionScript::Text { text, profile, source: spec.to_string() },
        Err(_) => SessionScript::Missing { path: spec.to_string(), looked: relative.then(|| path.display().to_string()), profile },
    }
}

/// A login script file picked with Choose… (Edit Session, Settings > Profiles) as the session or profile keeps it:
/// relative to the settings folder when it's in there, so the Mac and Windows both find it (scripts::as_spec)
#[tauri::command]
fn script_chosen(state: State<'_, AppState>, path: String) -> String {
    scripts::as_spec(&shared_folder(&state), std::path::Path::new(&path))
}

/// Copy a login script that's a full path into the settings folder's scripts folder, for the name a shared session
/// or profile can keep (scripts::copy_in); a different file of that name there is replaced only with `replace`
#[tauri::command]
fn script_copy_in(state: State<'_, AppState>, path: String, replace: bool) -> Result<scripts::CopiedIn, String> {
    let path = path.trim();
    scripts::copy_in(&shared_folder(&state), std::path::Path::new(path.strip_prefix("file://").unwrap_or(path)), replace)
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

/// Where a script's SAVE writes: a full path as it is, one in the home folder (`~/…`, or on Windows `~\…` and
/// `%USERPROFILE%\…`, as launch.rs's expand_home reads them) there, anything else in `output`
/// (Documents/SuperXTerminal/output, beside the session logs' own folder)
fn script_output_path(path: &str, home: &std::path::Path, output: &std::path::Path) -> PathBuf {
    let path = path.trim();
    let expanded = launch::expand_home_in(path, Some(home));
    if expanded != path {
        return PathBuf::from(expanded);
    }
    let given = PathBuf::from(path);
    if given.is_absolute() { given } else { output.join(given) }
}

/// A script's SAVE: write (or with `append`, add to) a text file, making its folder; its full path, for messages.
/// Lines end as this system's do (CRLF on Windows), as the output came with LF
#[tauri::command]
fn script_output_save(path: String, text: String, append: bool) -> Result<PathBuf, String> {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    let output = dirs::document_dir().unwrap_or_else(|| home.clone()).join("SuperXTerminal").join("output");
    let file = script_output_path(&path, &home, &output);
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    let text = if cfg!(windows) { text.replace("\r\n", "\n").replace('\n', "\r\n") } else { text };
    use std::io::Write;
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&file)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    handle.write_all(text.as_bytes()).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

/// Where a dropped file is saved: a folder of its own under `base` (so two files of one name don't meet), with the
/// file's own name only (a name with folders in it, or .., can't reach outside it)
fn dropped_file_path(base: &std::path::Path, name: &str, unique: &str) -> PathBuf {
    let own = std::path::Path::new(name).file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty() && *n != "..").unwrap_or("dropped");
    base.join(unique).join(own)
}

/// A file dropped on a dialog's drop zone (Import Sessions'): the browser gives its contents but not its path, so it's
/// saved in the temporary folder for the dialog to read like a chosen file. Its path, under its own name
#[tauri::command]
fn dropped_file_save(name: String, data: String) -> Result<PathBuf, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let unique = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let path = dropped_file_path(&std::env::temp_dir().join("SuperXTerminal-dropped"), &name, &unique);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// A log just started: its id for writing, its file, and whether it goes when the tab closes
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LogStarted {
    id: u32,
    path: PathBuf,
    temporary: bool,
}

/// Start a tab's session log: kept in the chosen folder when the tab's profile says so (`keep`, its Session log),
/// else when Settings > General says so (`keep` None); otherwise temporary
#[tauri::command]
fn log_open(state: State<'_, AppState>, logs: State<'_, logs::Logs>, name: String, keep: Option<bool>) -> Result<LogStarted, String> {
    let (keep, folder) = {
        let settings = state.settings.lock().unwrap();
        (keep.unwrap_or(settings.session_log_enabled), settings.session_log_folder.clone())
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

/// Highlighted Output's next chunk of a file: base64 bytes (512 KB at most), the offset after them, and whether the
/// file started again
#[tauri::command]
fn log_chunk(path: PathBuf, offset: u64) -> Result<(String, u64, bool), String> {
    logs::read_chunk(&path, offset, 512 * 1024)
}

/// The user's own highlighting folder (Highlighted Output's folder button), made if it isn't there
#[tauri::command]
fn highlighting_user_folder(state: State<'_, AppState>) -> Result<PathBuf, String> {
    let folder = state.user_folder.join("highlighting");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok(folder)
}

/// Open a file in its own app (Search Output's Open log)
#[tauri::command]
fn open_path(path: PathBuf) -> Result<(), String> {
    open_in_app(&path)
}

/// How [`open_command`] is to open a file
#[cfg_attr(windows, allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum Opening {
    /// In the app the system has for it
    InItsApp,
    /// In the default text editor, whatever app its type opens in (the sessions file, the notices)
    AsText,
    /// Its folder shown, with it selected where the system can
    InItsFolder,
}

/// The command that opens `path` as `how` asks, on the Mac and Linux (Windows has its own, in [`open_in_app`] and
/// [`reveal`]). The Mac's `open`, with `-t` for the default text editor and `-R` for Finder with the file selected.
/// Linux's `xdg-open`, as the Linux build had the Mac's `open` and nothing opened. It has no text-editor or select
/// option, so a text file opens in the app for its type (a text editor, for .txt and .xml), and for InItsFolder the
/// folder it's in is opened.
#[cfg(not(windows))]
fn open_command(path: &std::path::Path, how: Opening) -> std::process::Command {
    if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        match how {
            Opening::InItsApp => {}
            Opening::AsText => {
                command.arg("-t");
            }
            Opening::InItsFolder => {
                command.arg("-R");
            }
        }
        command.arg(path);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        // the system's environment, not an AppImage's, or xdg-open and the app it opens fail on its libraries
        crate::appimage::tidy(&mut command);
        match (how, path.parent()) {
            (Opening::InItsFolder, Some(folder)) if !folder.as_os_str().is_empty() => command.arg(folder),
            _ => command.arg(path),
        };
        command
    }
}

/// Open a file or folder in the app the system uses for it: Explorer's "start" on Windows, else [`open_command`]
fn open_in_app(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// Start cmd without a console window flashing up
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // start's first quoted argument is a window title, hence the empty one
        std::process::Command::new("cmd").args(["/c", "start", ""]).arg(path).creation_flags(CREATE_NO_WINDOW).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        open_command(path, Opening::InItsApp).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Open a web link (http or https) in the default browser: a link Ctrl+clicked in a terminal, Settings' links, a new
/// version's page. Through the opener plugin, except in an AppImage on Linux, where the plugin's xdg-open would
/// inherit the AppImage's libraries and fail (appimage.rs); there it's the system's xdg-open, given the system's
/// environment. Anything but http and https is refused, as TerminalControl's Ctrl+click is.
#[tauri::command]
fn open_link(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("Not a web link: {url}"));
    }
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_some() {
        let mut command = std::process::Command::new("xdg-open");
        crate::appimage::tidy(&mut command);
        return command.arg(&url).spawn().map(|_| ()).map_err(|e| format!("Couldn't open {url}: {e}"));
    }
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| format!("Couldn't open {url}: {e}"))
}

/// Notepad++, where its installer puts it, for editing the sessions file on Windows
#[cfg(windows)]
fn notepad_plus_plus() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .iter()
        .filter_map(|var| std::env::var_os(var))
        .map(|root| PathBuf::from(root).join("Notepad++").join("notepad++.exe"))
        .chain(std::env::var_os("LOCALAPPDATA").map(|root| PathBuf::from(root).join("Programs").join("Notepad++").join("notepad++.exe")))
        .find(|p| p.exists())
}

/// File > Edit Sessions Externally, as SuperXPuTTY's: the sessions file (Sessions.xml, or PrivateSessions.xml when
/// Saved sessions are turned off) in Notepad++ on Windows when it's installed, else the system's app for it; the
/// Mac's default text editor there. The file is made first if there isn't one yet. The tree reads it again when it's
/// saved (`sessions_files_changed`). On Linux, the app .xml files open in. Returns what to say in the status bar.
#[tauri::command]
fn sessions_edit_externally(state: State<'_, AppState>) -> Result<String, String> {
    let saved_on = state.settings.lock().unwrap().use_sessions_xml;
    let store = if saved_on { sessions::Store::Saved } else { sessions::Store::Private };
    let path = store_file(&state, store)?;
    if !path.exists() {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder).map_err(|e| format!("Couldn't make {}: {e}", folder.display()))?;
        }
        let list = {
            let sessions = state.sessions.lock().unwrap();
            if saved_on { sessions.saved.clone() } else { sessions.private.clone() }
        };
        std::fs::write(&path, sessions::write_sessions(&list)).map_err(|e| format!("Couldn't make {}: {e}", path.display()))?;
    }
    #[cfg(windows)]
    {
        if let Some(editor) = notepad_plus_plus() {
            std::process::Command::new(editor).arg(&path).spawn().map_err(|e| format!("Couldn't start Notepad++: {e}"))?;
            return Ok(format!("Editing {} in Notepad++; the sessions are read again when it's saved", path.display()));
        }
        open_in_app(&path)?;
    }
    #[cfg(not(windows))]
    open_command(&path, Opening::AsText).spawn().map_err(|e| e.to_string())?;
    Ok(format!("Editing {}; the sessions are read again when it's saved", path.display()))
}

/// File > Export Sessions…, as SuperXPuTTY's: every session (Saved, Private and Database) in one Sessions.xml, which
/// SuperXPuTTY and File > Import Sessions > From SuperPuTTY XML File read. Database sessions go as plain sessions, in
/// their folders, without the table's id, version or locks. Returns how many were written.
#[tauri::command]
fn sessions_export_xml(state: State<'_, AppState>, path: PathBuf) -> Result<usize, String> {
    let list: Vec<sessions::StoredSession> = {
        let sessions = state.sessions.lock().unwrap();
        let database = sessions.database.iter().map(|s| {
            let mut plain = sessions::StoredSession {
                attributes: s.attributes.iter().filter(|(name, _)| !name.starts_with("Database")).cloned().collect(),
            };
            let id = s.id().strip_prefix("@db/").unwrap_or(s.id()).to_string();
            plain.set("SessionId", &id);
            plain
        });
        let mut all: Vec<sessions::StoredSession> = sessions.saved.iter().chain(sessions.private.iter()).cloned().chain(database).collect();
        all.sort_by_key(|s| s.id().to_lowercase());
        all
    };
    std::fs::write(&path, sessions::write_sessions(&list)).map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;
    Ok(list.len())
}

/// What this computer's ssh has for each kind of algorithm, and its default order, for the algorithm lists in
/// Settings > Profiles and Edit Session (algorithms.rs)
#[tauri::command]
fn ssh_algorithms_available() -> Vec<algorithms::Available> {
    algorithms::available()
}

/// Where an import that reads no chosen file reads (ssh config, PuTTY's and KiTTY's saved sessions, Remote Desktop's
/// recent computers), for the import window; None for imports that ask for a file
#[tauri::command]
fn sessions_import_source(kind: String) -> Option<String> {
    importing::source_of_import(&kind)
}

/// Open a session in another app: "open" for RDP (Windows App) and VNC (Screen Sharing), "terminal" for Connect
/// external (its ssh command in a Terminal window), "filezilla" for FileZilla; returns what to say in the status bar
// async: a 1Password reference in its values waits for the user's approval, which mustn't hold up the window
#[tauri::command(async)]
fn session_external(state: State<'_, AppState>, store: sessions::Store, id: String, how: String) -> Result<String, String> {
    let profiles = state.profiles.lock().unwrap().clone();
    let values = variables::values(&state.user_folder);
    let (serial_devices, overrides_on) = {
        let settings = state.settings.lock().unwrap();
        (settings.serial_devices.clone(), settings.use_local_overrides)
    };
    let sessions = state.sessions.lock().unwrap();
    let session = &overrides::applied(&state.user_folder, sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?, overrides_on);
    // the folders' username and port, as for a tab
    let mut username = session.get("Username").to_string();
    let mut port = session.port();
    for folder in sessions.along(store, session.folder()) {
        if username.is_empty() {
            username = folder.username.clone();
        }
        if port == 0 {
            port = folder.port;
        }
    }
    let host = variables::substitute(session.get("Host").trim(), &values);
    let username = variables::substitute(&username, &values);
    match (how.as_str(), session.get("Proto").to_ascii_uppercase().as_str()) {
        ("open", "RDP") => external::open_rdp(session, &host, &username, &state.settings.lock().unwrap().rdp_defaults.clone()),
        ("open", "VNC") => external::open_vnc(&host, port, &username),
        ("filezilla", _) => {
            // an SSH session's login as it connects in a tab (session, folders, profile); others as stored
            let launch = launch::launch(&sessions, &profiles, &values, &serial_devices, store, session).ok();
            let login = launch.as_ref().and_then(|l| l.login.clone());
            let (host, port, username, key_file) = match &login {
                Some(l) => (l.host.clone(), l.port, l.username.clone(), l.key_file.clone()),
                None => (host, port, username, String::new()),
            };
            // no username anywhere: the one ssh would use, as a tab connects with it
            let username = if username.is_empty() { external::ssh_username(&host, port).unwrap_or_default() } else { username };
            let password = if state.settings.lock().unwrap().filezilla_passwords { launch.and_then(|l| l.password) } else { None };
            let local = variables::substitute(session.get("LocalPath"), &values);
            let remote = variables::substitute(session.get("RemotePath"), &values);
            let opened = external::open_filezilla(&host, port, &username, password.as_deref(), &local, &remote)?;
            // FileZilla takes no key on its command line: it uses ssh-agent and its own list
            Ok(if key_file.is_empty() {
                opened
            } else {
                format!("{opened}. Its key {key_file} isn't passed to FileZilla: add it in FileZilla's Settings > SFTP, or run ssh-add {key_file}")
            })
        }
        ("terminal", _) => {
            let launch = launch::launch(&sessions, &profiles, &values, &serial_devices, store, session)?;
            if launch.stream.is_some() || launch.local {
                return Err("Only SSH sessions open in Terminal.".into());
            }
            let words: Vec<String> = launch.command.get_argv().iter().map(|w| w.to_string_lossy().to_string()).collect();
            let mut line = external::shell_line(&words);
            // the password, if any, goes to ssh through this app (askpass.rs), never on the command line
            if let Some(password) = launch.password.as_deref() {
                let program = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().to_string();
                let token = askpass::register(password)?;
                line = format!(
                    "SSH_ASKPASS={} SSH_ASKPASS_REQUIRE=force {}={token} {line}",
                    external::shell_line(&[program]),
                    askpass::TOKEN_VAR
                );
            }
            external::open_in_terminal(&line)?;
            Ok(format!("Opened {} in Terminal", session.get("SessionName")))
        }
        (_, proto) => Err(format!("{proto} sessions don't open in another app.")),
    }
}

/// The session files, in the order they're known by ([`file_watch`]): the private two in the user's folder, then
/// Sessions.xml and Folders.xml in the settings folder when Saved sessions are in use
fn session_files(state: &AppState) -> Vec<PathBuf> {
    let mut files = vec![state.user_folder.join("PrivateSessions.xml"), state.user_folder.join("PrivateFolders.xml")];
    if let Some(folder) = saved_folder(state) {
        files.push(folder.join("Sessions.xml"));
        files.push(folder.join("Folders.xml"));
    }
    files
}

/// Whether a session file changed since it was last read (a teammate saving to the shared folder, SuperXPuTTY, an
/// editor); the tree then reads them again. A settings folder going away or coming back counts, so Saved sessions
/// change to the copy kept on this computer and back ([`saved_cache`]). The files are only read when their sizes or
/// times differ, or once a minute ([`file_watch::Seen::changed`]): this is asked every few seconds, and with the
/// settings folder on a network drive it used to fetch every file whole each time.
// async: a network drive that's away can take many seconds to say so, which mustn't hold up the window each time
#[tauri::command(async)]
fn sessions_files_changed(state: State<'_, AppState>) -> bool {
    // the Mac: a settings folder on a network share that went with the network isn't put back by macOS when the
    // network returns. If its server answers again, the share is connected again here (once each time it comes
    // back), and the check below then finds the folder there. It may wait on macOS asking for the share's password,
    // which is one more reason this isn't on the window's thread
    if cfg!(target_os = "macos") {
        if let Some(folder) = saved_folder(&state).filter(|folder| !saved_cache::reachable(folder)) {
            remount::reconnect(&state.user_folder, &folder);
        }
    }
    let files = session_files(&state);
    state.files_seen.lock().unwrap().changed(&files, std::time::Instant::now(), file_watch::read_or_nothing)
}

/// The serial devices on this Mac, for choosing which one a COM name is
#[tauri::command]
fn serial_devices_list() -> Vec<String> {
    streams::serial_devices()
}

/// Remember which device a serial session's COM name is
#[tauri::command]
fn serial_device_set(state: State<'_, AppState>, name: String, device: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.serial_devices.insert(name, device);
    settings.save(&state.user_folder)
}

/// The user's local overrides, by session id
#[tauri::command]
fn overrides_get(state: State<'_, AppState>) -> BTreeMap<String, overrides::Override> {
    overrides::load(&state.user_folder)
}

/// Set (or, when empty, remove) sessions' local overrides; returns them all
#[tauri::command]
fn overrides_set(state: State<'_, AppState>, changes: Vec<(String, overrides::Override)>) -> Result<BTreeMap<String, overrides::Override>, String> {
    overrides::set(&state.user_folder, changes)
}

/// How old a profile's key file is, in seconds; None when there's none (the profile editor's key fetch)
#[tauri::command]
fn profile_key_age(key_file: String) -> Option<u64> {
    launch::key_age(&key_file)
}

/// The user's override presets, by name
#[tauri::command]
fn override_presets_get(state: State<'_, AppState>) -> BTreeMap<String, overrides::Override> {
    overrides::presets(&state.user_folder)
}

/// Save an override preset under a name, or remove it (no value); returns them all
#[tauri::command]
fn override_presets_set(state: State<'_, AppState>, name: String, value: Option<overrides::Override>) -> Result<BTreeMap<String, overrides::Override>, String> {
    overrides::set_preset(&state.user_folder, &name, value)
}

/// The Favorites and Recent lists
#[tauri::command]
fn shortcuts_get(state: State<'_, AppState>) -> shortcuts::Shortcuts {
    shortcuts::load(&state.user_folder)
}

/// Add a session to Favorites, or take it out
#[tauri::command]
fn shortcuts_favorite(app: AppHandle, state: State<'_, AppState>, id: String, on: bool) -> Result<shortcuts::Shortcuts, String> {
    let changed = shortcuts::edit(&state.user_folder, |s| {
        s.favorites.retain(|f| *f != id);
        if on {
            s.favorites.push(id);
        }
    });
    tray::refresh(&app);
    changed
}

/// Record a session opened from the tree, first in Recent
#[tauri::command]
fn shortcuts_opened(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<shortcuts::Shortcuts, String> {
    let changed = shortcuts::edit(&state.user_folder, |s| shortcuts::push_recent(&mut s.recent, &id));
    tray::refresh(&app);
    changed
}

/// Take one session out of Recent, or clear it (`id` None)
#[tauri::command]
fn shortcuts_forget_recent(app: AppHandle, state: State<'_, AppState>, id: Option<String>) -> Result<shortcuts::Shortcuts, String> {
    let changed = shortcuts::edit(&state.user_folder, |s| match &id {
        Some(id) => s.recent.retain(|r| r != id),
        None => s.recent.clear(),
    });
    tray::refresh(&app);
    changed
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

/// Where each {placeholder} is used, by name in lower case: sessions (host, username, extra arguments, jump host),
/// folders, profiles and the user's own local overrides and presets, for Environment Variables to say what's used and
/// what has no value yet
#[tauri::command]
fn variables_usage(state: State<'_, AppState>) -> BTreeMap<String, Vec<String>> {
    let mut usage: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut note = |texts: &[&str], place: String| {
        for text in texts {
            for name in variables::names_in(text) {
                let places = usage.entry(name).or_default();
                if !places.contains(&place) {
                    places.push(place.clone());
                }
            }
        }
    };
    {
        let sessions = state.sessions.lock().unwrap();
        for session in sessions.saved.iter().chain(&sessions.private).chain(&sessions.database) {
            let name = session.id().trim_start_matches("@db/").to_string();
            note(&[session.get("Host"), session.get("Username"), session.get("ExtraArgs"), session.get("JumpHost")], format!("session {name}"));
        }
        for folders in sessions.folders.values() {
            for folder in folders.values() {
                note(&[&folder.username, &folder.jump_host], format!("folder {}", folder.path));
            }
        }
    }
    for profile in &state.profiles.lock().unwrap().profiles {
        note(
            &[&profile.username, &profile.host, &profile.jump_host, &profile.key_fetch, &profile.ssh_options],
            format!("profile {}", profile.name),
        );
    }
    for (id, o) in overrides::load(&state.user_folder) {
        let texts = [o.host.as_deref(), o.username.as_deref(), o.extra_args.as_deref(), o.jump_host.as_deref()].map(|t| t.unwrap_or(""));
        note(&texts, format!("your override for {}", id.trim_start_matches("@db/")));
    }
    for (name, o) in overrides::presets(&state.user_folder) {
        let texts = [o.host.as_deref(), o.username.as_deref(), o.extra_args.as_deref(), o.jump_host.as_deref()].map(|t| t.unwrap_or(""));
        note(&texts, format!("preset {name}"));
    }
    usage
}

/// The profiles that sessions, folders, local overrides and presets name but that aren't set up on this computer,
/// each with the places naming it ([`profiles::missing`]), for Settings > Profiles to list under the user's own. Those
/// sessions open with Default Settings until it's set up.
#[tauri::command]
fn profiles_missing(state: State<'_, AppState>) -> Vec<(String, Vec<String>)> {
    let mut named: Vec<(String, String)> = Vec::new();
    {
        let sessions = state.sessions.lock().unwrap();
        for session in sessions.saved.iter().chain(&sessions.private).chain(&sessions.database) {
            let name = session.id().trim_start_matches("@db/").to_string();
            named.push((session.get("PuttySession").to_string(), format!("session {name}")));
        }
        for folders in sessions.folders.values() {
            for folder in folders.values() {
                named.push((folder.putty_profile.clone(), format!("folder {}", folder.path)));
            }
        }
    }
    for (id, o) in overrides::load(&state.user_folder) {
        named.push((o.putty_profile.unwrap_or_default(), format!("your override for {}", id.trim_start_matches("@db/"))));
    }
    for (name, o) in overrides::presets(&state.user_folder) {
        named.push((o.putty_profile.unwrap_or_default(), format!("preset {name}")));
    }
    let profiles = state.profiles.lock().unwrap();
    profiles::missing(&profiles, named.iter().map(|(n, p)| (n.as_str(), p.clone())))
}

/// The placeholders SuperXTerminal fills itself, with what each is
#[tauri::command]
fn variables_built_in() -> Vec<(String, String)> {
    variables::BUILT_IN.iter().map(|(n, d)| (n.to_string(), d.to_string())).collect()
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

/// Put the user's environment variables into `text`, and name any it uses that aren't set
#[tauri::command]
fn variables_fill(state: State<'_, AppState>, text: String) -> Filled {
    let values = variables::values(&state.user_folder);
    Filled { missing: variables::missing(&text, &values), text: variables::substitute(&text, &values) }
}

/// Save the Settings window's General tab; the settings folder is changed with `settings_set_folder`
#[tauri::command]
fn settings_save(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    let mut current = state.settings.lock().unwrap();
    let folder = current.settings_folder.clone();
    let before = current.clone();
    *current = AppSettings { settings_folder: folder, ..settings };
    current.icon_size = match current.icon_size {
        16 | 20 | 24 | 32 => current.icon_size,
        _ => 24,
    };
    current.tree_font_size = current.tree_font_size.clamp(10, 24);
    current.shortcuts = keys::cleaned(&current.shortcuts);
    current.command_history_days = current.command_history_days.min(command_history::MOST_DAYS);
    current.save(&state.user_folder)?;
    onepassword::set_enabled(current.one_password);
    // Settings > General > Command bar's days shortened (or 0): what they no longer allow goes now, not at next start
    if current.command_history_days < before.command_history_days {
        let _ = command_history::prune(&state.user_folder, current.command_history_days, command_history::now());
    }
    let changes = window_changes(&before, &current);
    // the menu bar's keys follow Settings > Shortcuts
    if changes.menu && let Ok(bar) = menu(&app, &current) {
        let _ = app.set_menu(bar);
    }
    if changes.theme {
        apply_window_theme(&app, &current.theme);
    }
    let saved = current.clone();
    drop(current);
    applog::info("Settings saved");
    // and the menu bar icon, Settings > General
    tray::refresh(&app);
    // every window reads them again, so another window's next save doesn't put back what it had
    let _ = app.emit(SETTINGS_CHANGED, ());
    Ok(saved)
}

/// What a settings save has to redo in the windows themselves (see [`window_changes`])
#[derive(Debug, PartialEq)]
struct WindowChanges {
    /// The menu bar again: its keys (Settings > Shortcuts), Local overrides, or what View ticks changed
    menu: bool,
    /// The menu bar, menus and title bars in the theme again: Settings > General > Theme changed
    theme: bool,
}

/// What a save from `before` to `after` changes in the windows. Each only when its own setting changed: on Windows,
/// setting the menu bar again lays the whole window out again, which showed as the window shifting at every save (a
/// checkbox in a terminal tab's menu saves too)
fn window_changes(before: &AppSettings, after: &AppSettings) -> WindowChanges {
    WindowChanges {
        menu: before.shortcuts != after.shortcuts
            || before.use_local_overrides != after.use_local_overrides
            || view_checks(before) != view_checks(after),
        theme: before.theme != after.theme,
    }
}

/// The View menu's items that are shown or hidden, each with whether it's shown now, for its tick (as a Mac app's
/// View menu, and SuperXPuTTY's, tick the panes and bars on screen). Each is a setting, saved when the item is chosen,
/// so the menu is made again then ([`window_changes`]) and the tick follows. Full Screen isn't among them: the Mac's
/// own View menu doesn't tick it either. Nor is Layouts…, which opens Settings > Layouts.
fn view_checks(settings: &AppSettings) -> [(&'static str, bool); 9] {
    [
        ("toggleSessionsPane", settings.show_sessions_pane),
        ("toggleSendCommands", settings.show_send_commands),
        ("toggleButtonBar", settings.show_button_bar),
        ("toggleStatusBar", settings.show_status_bar),
        ("toggleSessionsAutoHide", settings.sessions_auto_hide),
        ("toggleSessionDetail", settings.show_session_detail),
        ("toggleLogViewer", settings.show_log_viewer),
        ("toggleAlwaysOnTop", settings.always_on_top),
        ("toggleGridView", settings.grid_view),
    ]
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
    // a demo has no database: its sessions are the cache's, shown as if just read
    if demo::on() {
        let mut db_state = state.database.lock().unwrap();
        db_state.source = format!("{}.{}", settings.schema, settings.table);
        show_cache(state, &settings, &mut db_state);
        db_state.enabled = true;
        db_state.error.clear();
        show_database(state, &settings, &mut db_state);
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
            sessions.folders.insert(sessions::Store::Database, fetched.folders.into_iter().map(|f| (f.path.to_lowercase(), f)).collect());
        }
        Err(error) => {
            applog::warn(format!("Database sessions couldn't be read: {error}"));
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
            sessions.folders.insert(sessions::Store::Database, cached.folders.into_iter().map(|f| (f.path.to_lowercase(), f)).collect());
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
async fn db_session_save(
    state: State<'_, AppState>,
    original_id: Option<String>,
    fields: editing::SessionFields,
    copy_of: Option<(sessions::Store, String)>,
) -> Result<String, String> {
    folders::check_name(&fields.name)?;
    let fields = with_copied_password(&state, fields, copy_of.as_ref());
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
    forget_id(&state, &id);
    Ok(())
}

/// Move a Database session to another folder of the table, then read it back; returns its new id
#[tauri::command]
async fn db_session_move(state: State<'_, AppState>, id: String, folder: String, name: Option<String>) -> Result<String, String> {
    let stored = db_session(&state, &id)?;
    if stored.get("DatabaseLockedFields").split(',').any(|f| f.trim() == "folder") {
        return Err(format!("The folder of \"{}\" is locked in the database.", stored.get("SessionName")));
    }
    let mut row = db::Row::of(&stored, &folder);
    if let Some(name) = name {
        folders::check_name(&name)?;
        row.name = name;
    }
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

/// Copy (or move) sessions to another root, each to the same folder path there or to the folder given in `folders`, as
/// SuperXPuTTY's Move to / Copy to and drags; copies within one root too when `folders` are given (the tree's Paste).
/// A copy out of the table leaves its database fields behind (SuperXPuTTY's PlainCopy); a move deletes only what
/// arrived, and never a locked row.
#[tauri::command]
async fn sessions_transfer(
    state: State<'_, AppState>,
    from: sessions::Store,
    ids: Vec<String>,
    to: sessions::Store,
    delete_source: bool,
    folders: Option<Vec<String>>,
    rename_taken: bool,
) -> Result<TransferReport, String> {
    transfer(&state, from, ids, to, delete_source, folders, rename_taken).await
}

/// [`sessions_transfer`] itself, on the app's state rather than the command's handle to it, so the tests can run it
async fn transfer(
    state: &AppState,
    from: sessions::Store,
    ids: Vec<String>,
    to: sessions::Store,
    delete_source: bool,
    folders: Option<Vec<String>>,
    rename_taken: bool,
) -> Result<TransferReport, String> {
    use sessions::Store;
    // within one root only as copies to folders given (the tree's Paste)
    if from == to && (delete_source || folders.is_none()) {
        return Err("Choose another place to copy to.".into());
    }
    // each session with the folder it goes to: the one given (a drop on a folder), else the same path there
    let picked: Vec<(sessions::StoredSession, String)> = {
        let all = state.sessions.lock().unwrap();
        ids.iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let session = all.find(from, id)?.clone();
                let folder = folders.as_ref().and_then(|f| f.get(i)).map(|f| f.trim_matches('/').to_string()).unwrap_or_else(|| session.folder().to_string());
                Some((session, folder))
            })
            .collect()
    };
    let name_of = |session: &sessions::StoredSession| match session.get("SessionName") {
        "" => session.id().rsplit('/').next().unwrap_or("").to_string(),
        n => n.to_string(),
    };
    let join = |folder: &str, name: &str| if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") };
    // a name taken there (Saved and Private sessions share ids) goes with -1, -2... added when the user agreed, as
    // SuperXPuTTY's MakeUniqueSessionId; otherwise it's left out
    let free = |taken: &dyn Fn(&str) -> bool, folder: &str, name: &str| -> Option<String> {
        if !taken(&join(folder, name)) {
            return Some(name.to_string());
        }
        if !rename_taken {
            return None;
        }
        (1..1000).map(|i| format!("{name}-{i}")).find(|n| !taken(&join(folder, n)))
    };
    let mut problems = Vec::new();

    // what arrived: each session and its path there (folder/name)
    let arrived: Vec<(sessions::StoredSession, String)> = if to == Store::Database {
        let mut ids_there: Vec<String> = state.sessions.lock().unwrap().database.iter().map(|s| s.id().trim_start_matches("@db/").to_lowercase()).collect();
        let mut sent = Vec::new();
        let mut changes = Vec::new();
        for (session, folder) in &picked {
            let taken = |id: &str| ids_there.contains(&id.to_lowercase());
            let Some(name) = free(&taken, folder, &name_of(session)) else {
                problems.push(format!("{}: there's already a session with that name there.", join(folder, &name_of(session))));
                continue;
            };
            ids_there.push(join(folder, &name).to_lowercase());
            let mut row = db::Row::of(session, folder);
            row.name = name.clone();
            let mut change = outbox::PendingChange::new(outbox::Kind::Insert);
            change.row = Some(row);
            changes.push(change);
            sent.push((session.clone(), join(folder, &name)));
        }
        let submitted = submit(state, changes).await?;
        problems.extend(submitted.problems);
        sent.into_iter().enumerate().filter(|(i, _)| !submitted.refused.contains(i)).map(|(_, s)| s).collect()
    } else {
        let path = store_file(state, to)?;
        let others = other_ids(state, to);
        let mut added = Vec::new();
        sessions::update_file(&path, |list| {
            for (session, folder) in &picked {
                let mine: Vec<String> = list.iter().map(|s| s.id().to_string()).collect();
                let taken = |id: &str| mine.iter().chain(&others).any(|o| o.eq_ignore_ascii_case(id));
                let Some(name) = free(&taken, folder, &name_of(session)) else {
                    problems.push(format!("{}: there's already a session with that name there.", join(folder, &name_of(session))));
                    continue;
                };
                // as a file session: no database fields, and an id without the @db prefix
                let mut copy = sessions::StoredSession {
                    attributes: session.attributes.iter().filter(|(n, _)| !n.starts_with("Database")).cloned().collect(),
                };
                copy.set("SessionId", &join(folder, &name));
                copy.set("SessionName", &name);
                sessions::insert_sorted(list, copy);
                added.push((session.clone(), join(folder, &name)));
            }
            Ok(())
        })?;
        added
    };
    if delete_source && from == Store::Database {
        let mut deletes = Vec::new();
        for (session, _) in &arrived {
            if session.get("DatabaseLocked") == "true" {
                problems.push(format!("{}: locked in the database, so it stays there.", session.id()));
                continue;
            }
            let mut change = outbox::PendingChange::new(outbox::Kind::Delete);
            change.row_id = session.get("DatabaseId").to_string();
            change.from = session.id().trim_start_matches("@db/").to_string();
            deletes.push(change);
        }
        problems.extend(submit(state, deletes).await?.problems);
    } else if delete_source {
        for (session, _) in &arrived {
            if let Err(e) = store_file(state, from).and_then(|path| editing::delete(&path, session.id())) {
                problems.push(e);
            }
        }
    }
    if delete_source {
        // Favorites and Recent follow sessions moved to the other root
        for (session, path) in arrived.iter().filter(|(s, _)| s.get("DatabaseLocked") != "true") {
            let new = if to == Store::Database { format!("@db/{path}") } else { path.clone() };
            follow_id(state, session.id(), &new);
        }
    }
    Ok(TransferReport { done: arrived.len(), problems })
}

/// The new values for Edit sessions…; None leaves a field as each session has it (SuperXPuTTY's SessionBulkEdit.Change)
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct BulkChange {
    username: Option<String>,
    profile: Option<String>,
    port: Option<u16>,
    protocol: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    extra_args: Option<String>,
    jump_host: Option<String>,
    highlight_rules: Option<String>,
}

impl BulkChange {
    /// The session attributes changed and their new values
    fn attributes(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        let mut put = |name: &'static str, value: &Option<String>| {
            if let Some(v) = value {
                out.push((name, v.trim().to_string()));
            }
        };
        put("Username", &self.username);
        put("PuttySession", &self.profile.as_ref().map(|p| if p.trim().is_empty() { "Default Settings".into() } else { p.clone() }));
        put("Port", &self.port.map(|p| p.to_string()));
        put("Proto", &self.protocol);
        put("ImageKey", &self.icon);
        put("Color", &self.color);
        put("ExtraArgs", &self.extra_args);
        put("JumpHost", &self.jump_host);
        put("HighlightRules", &self.highlight_rules);
        out
    }

    /// The table's columns of the fields changed, to check against locked_fields
    fn columns(&self) -> Vec<&'static str> {
        self.attributes()
            .iter()
            .map(|(name, _)| match *name {
                "Username" => "username",
                "PuttySession" => "putty_profile",
                "Port" => "port",
                "Proto" => "protocol",
                "ImageKey" => "icon",
                "Color" => "color",
                "ExtraArgs" => "extra_args",
                "JumpHost" => "jump_host",
                _ => "highlight_rules",
            })
            .collect()
    }
}

/// Change the same fields on many sessions at once, as SuperXPuTTY's Edit sessions…: Saved and Private ones in their
/// files, written once each; Database ones to the table in one batch, leaving those with a changed field locked.
#[tauri::command]
async fn sessions_bulk_edit(state: State<'_, AppState>, items: Vec<(sessions::Store, String)>, change: BulkChange) -> Result<TransferReport, String> {
    use sessions::Store;
    let attributes = change.attributes();
    if attributes.is_empty() {
        return Err("Tick the fields to change.".into());
    }
    let mut done = 0;
    let mut problems = Vec::new();
    for store in [Store::Saved, Store::Private] {
        let ids: Vec<&String> = items.iter().filter(|(s, _)| *s == store).map(|(_, id)| id).collect();
        if ids.is_empty() {
            continue;
        }
        let path = store_file(&state, store)?;
        sessions::update_file(&path, |list| {
            for id in &ids {
                match list.iter_mut().find(|s| s.id() == id.as_str()) {
                    Some(session) => {
                        for (name, value) in &attributes {
                            session.set(name, value);
                        }
                        done += 1;
                    }
                    None => problems.push(format!("{id}: no longer in its file.")),
                }
            }
            Ok(())
        })?;
    }
    let columns = change.columns();
    let mut changes = Vec::new();
    for (_, id) in items.iter().filter(|(s, _)| *s == Store::Database) {
        let stored = match db_session(&state, id) {
            Ok(s) => s,
            Err(e) => {
                problems.push(e);
                continue;
            }
        };
        let name = stored.get("SessionName").to_string();
        if stored.get("DatabaseLocked") == "true" {
            problems.push(format!("{name}: locked in the database."));
            continue;
        }
        let locked: Vec<&str> = stored.get("DatabaseLockedFields").split(',').map(str::trim).collect();
        if let Some(column) = columns.iter().find(|c| locked.contains(c)) {
            problems.push(format!("{name}: {column} is locked in the database."));
            continue;
        }
        let mut copy = stored.clone();
        for (attribute, value) in &attributes {
            copy.set(attribute, value);
        }
        let mut pending = outbox::PendingChange::new(outbox::Kind::Update);
        pending.row_id = stored.get("DatabaseId").to_string();
        pending.version = stored.get("DatabaseVersion").to_string();
        pending.row = Some(db::Row::of(&copy, stored.folder()));
        changes.push(pending);
    }
    if !changes.is_empty() {
        let submitted = submit(&state, changes).await?;
        done += submitted.done;
        problems.extend(submitted.problems);
    }
    Ok(TransferReport { done, problems })
}

/// Export sessions to a CSV file, as SuperXPuTTY's Export sessions to CSV: the ones given (a folder's, the
/// selection's), or every session. Returns how many were written and how many had a -pw password masked.
#[tauri::command]
fn sessions_export_csv(state: State<'_, AppState>, items: Option<Vec<(sessions::Store, String)>>, path: PathBuf) -> Result<(usize, usize), String> {
    use sessions::Store;
    let all = state.sessions.lock().unwrap();
    let every = [(Store::Saved, &all.saved), (Store::Private, &all.private), (Store::Database, &all.database)];
    let list: Vec<(Store, &sessions::StoredSession)> = match &items {
        Some(items) => items.iter().filter_map(|(store, id)| all.find(*store, id).map(|s| (*store, s))).collect(),
        None => every.iter().flat_map(|(store, list)| list.iter().map(move |s| (*store, s))).collect(),
    };
    importing::export(&list, &path)?;
    let masked = list.iter().filter(|(_, s)| importing::has_password(s.get("ExtraArgs"))).count();
    Ok((list.len(), masked))
}

/// One row for [`sessions_table_export_csv`]: its store now, where it was read (its store and id; None for a new row),
/// and its fields now
type TableCsvRow = (sessions::Store, Option<(sessions::Store, String)>, editing::SessionFields);

/// Edit Sessions as Table…'s Export CSV…: the rows shown, as they are in the table, saved or not, in Export Sessions to
/// CSV's form (so Import Sessions > CSV reads it back). Each `(store now, where it was read: its store and id, fields
/// now)`, None for a new row; a session read keeps what the table doesn't show (its paths, RDP options...) and its
/// -pw, which the CSV writes as ********. A row moved to another store but not saved is written in its new one.
/// Nothing is saved. Returns how many were written and how many had a -pw masked.
#[tauri::command]
fn sessions_table_export_csv(state: State<'_, AppState>, rows: Vec<TableCsvRow>, path: PathBuf) -> Result<(usize, usize), String> {
    let edited: Vec<(sessions::Store, sessions::StoredSession)> = {
        let all = state.sessions.lock().unwrap();
        rows.iter()
            .map(|(store, read, fields)| {
                let stored = read.as_ref().and_then(|(from, id)| all.find(*from, id));
                let mut session = stored.cloned().unwrap_or_else(|| sessions::StoredSession::new(&fields.id()));
                let password = stored.map(|s| s.get("ExtraArgs").to_string()).unwrap_or_default();
                fields.apply(&mut session, &password);
                (*store, session)
            })
            .collect()
    };
    let list: Vec<(sessions::Store, &sessions::StoredSession)> = edited.iter().map(|(store, s)| (*store, s)).collect();
    importing::export(&list, &path)?;
    let masked = list.iter().filter(|(_, s)| importing::has_password(s.get("ExtraArgs"))).count();
    Ok((list.len(), masked))
}

/// Edit Sessions as Table…'s Import CSV…: a CSV file's sessions, read as Import Sessions > CSV reads them, for the
/// table to match to its rows; nothing is saved until the table's Save
#[tauri::command]
fn sessions_table_read_csv(path: PathBuf) -> Result<importing::CsvSessions, String> {
    importing::read_csv_file(&path)
}

/// The window's own themes (Settings > General > Look > Theme), from UiThemes.json in the user's folder
#[tauri::command]
fn ui_themes_load(state: State<'_, AppState>) -> Vec<ui_themes::UiTheme> {
    ui_themes::load(&state.user_folder)
}

/// Save the window's own themes, whole; what's saved (names trimmed), or why they can't be
#[tauri::command]
fn ui_themes_save(state: State<'_, AppState>, themes: Vec<ui_themes::UiTheme>) -> Result<Vec<ui_themes::UiTheme>, String> {
    ui_themes::save(&state.user_folder, &themes)
}

/// Settings > General's Check 1Password: whether its CLI works here, through the 1Password app (onepassword.rs).
/// async: it waits for the user to approve it in 1Password
#[tauri::command(async)]
fn onepassword_check() -> Result<String, String> {
    onepassword::check()
}

/// Save a CSV template for File > Import Sessions: the columns, with example rows to copy (importing::template)
#[tauri::command]
fn sessions_csv_template(path: PathBuf) -> Result<(), String> {
    importing::write_template(&path)
}

/// The plan for importing sessions (`kind` "csv", "mobaxterm", "rdp" or "ssh"), worked out without changing anything
fn import_plan(state: &AppState, kind: &str, paths: &[PathBuf], store: sessions::Store) -> Result<importing::Plan, String> {
    let all = state.sessions.lock().unwrap();
    let use_saved = saved_folder(state).is_some();
    let existing = importing::Existing { saved: &all.saved, private: &all.private, use_saved };
    let store = if use_saved { store } else { sessions::Store::Private };
    importing::plan(kind, paths, store, &existing)
}

/// What an import would do, for the window to show before anything changes
#[tauri::command]
fn sessions_import_plan(state: State<'_, AppState>, kind: String, paths: Vec<PathBuf>, store: sessions::Store) -> Result<importing::Summary, String> {
    Ok(import_plan(&state, &kind, &paths, store)?.summary())
}

/// Import sessions, as planned again from the same files; returns how many were added and updated. The caller reloads.
#[tauri::command]
fn sessions_import_apply(state: State<'_, AppState>, kind: String, paths: Vec<PathBuf>, store: sessions::Store) -> Result<(usize, usize), String> {
    let plan = import_plan(&state, &kind, &paths, store)?;
    if let Some(error) = plan.error {
        return Err(error);
    }
    importing::apply(&plan, |store| store_file(&state, store), |store| other_ids(&state, store))
}

/// ~/.ssh/config, for Import from ssh config
#[tauri::command]
fn ssh_config_path() -> String {
    importing::ssh_config_path().display().to_string()
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
    // a demo's Database sessions never change (demo.rs)
    if demo::on() {
        return Ok(false);
    }
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

/// Whether this account can make files in a folder chosen as the settings folder, so the UI can warn first (see
/// [`saving::writable`])
#[tauri::command]
fn folder_writable(path: String) -> bool {
    saving::writable(std::path::Path::new(&path))
}

/// Whether there's a folder at `path` that can be reached now: asked of a settings folder typed in Settings > General
/// before it's used, as one that isn't there (a slip of the keys, a drive that's away) would show no Saved sessions
#[tauri::command]
fn folder_exists(path: String) -> bool {
    saved_cache::reachable(std::path::Path::new(&path))
}

/// Use another settings folder (chosen in the UI), and remember it
#[tauri::command]
fn settings_set_folder(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.settings_folder = Some(PathBuf::from(path));
    settings.save(&state.user_folder)?;
    let _ = app.emit(SETTINGS_CHANGED, ());
    Ok(())
}

/// The event every window gets when the settings are saved
const SETTINGS_CHANGED: &str = "settings-changed";

/// SuperXTerminal > Third-Party Notices: the components it ships and their licences (scripts/notices.mjs), in the
/// system's text editor ([`Opening::AsText`]). The app has the file among its resources; a development build reads
/// the project's.
fn open_notices<R: Runtime>(app: &AppHandle<R>) {
    let bundled = app.path().resource_dir().map(|d| d.join("THIRD-PARTY-NOTICES.txt")).ok().filter(|p| p.exists());
    let path = bundled.unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../THIRD-PARTY-NOTICES.txt"));
    // on Windows, the app for .txt files
    #[cfg(windows)]
    let opened = open_in_app(&path).map_err(std::io::Error::other);
    #[cfg(not(windows))]
    let opened = open_command(&path, Opening::AsText).status().map(|_| ());
    if let Err(e) = opened {
        applog::warn(format!("Couldn't open {}: {e}", path.display()));
    }
}

/// The menu bar: the app menu (with Settings..., Cmd+,), File (New Local Shell, Cmd+T), Edit (which WebKit needs for
/// Cmd+C/V), Tools (Environment Variables...) and Window.
/// SuperXPuTTY's File, View and Tools menus join it as their features arrive.
/// Its keys are the settings' (Settings > Shortcuts). Tools > Use Local Overrides is ticked while they're used, and
/// View's panes and bars while they're shown ([`view_checks`]): the menu is made again whenever one of those changes
/// ([`window_changes`]), so the ticks follow however it changed: the item itself, the status bar's "Local overrides
/// off", or a pane's own ×.
fn menu<R: Runtime>(app: &AppHandle<R>, settings: &AppSettings) -> tauri::Result<Menu<R>> {
    let shortcuts = &settings.shortcuts;
    let overrides_on = settings.use_local_overrides;
    // an action's item, with its key now (keys.rs: the user's, else the Mac default)
    let action = |id: &str| MenuItem::with_id(app, id, keys::label(id), true, keys::key(shortcuts, id).as_deref());
    let plain = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>);
    // a View item that shows or hides something: ticked while it's shown, with its key as an action's
    let views = view_checks(settings);
    let shown = |id: &str| {
        let on = views.iter().any(|(view, on)| *view == id && *on);
        CheckMenuItem::with_id(app, id, keys::label(id), true, on, keys::key(shortcuts, id).as_deref())
    };
    let app_menu = Submenu::with_items(
        app,
        "SuperXTerminal",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &action("checkUpdates")?,
            &plain("thirdPartyNotices", "Third-Party Notices")?,
            &PredefinedMenuItem::separator(app)?,
            &action("settings")?,
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
            &action("newWindow")?,
            &action("newLocalShell")?,
            // SuperXPuTTY's File > Open session… (its Quick Selector)
            &action("openSession")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXPuTTY's File > Import sessions, Export sessions and Export sessions to CSV, in its order
            &Submenu::with_items(
                app,
                "Import Sessions",
                true,
                &[
                    &plain("import:superputty", "From SuperPuTTY XML File…")?,
                    &plain("import:putty", if cfg!(windows) { "From PuTTY/KiTTY (Registry)…" } else { "From PuTTY (~/.putty)…" })?,
                    &plain("import:puttyFolder", "From PuTTY Portable Settings…")?,
                    &plain("import:puttycm", "From PuTTY CM Export…")?,
                    // Remote Desktop Connection's recent computers are Windows' only: shown, greyed, on the Mac
                    &MenuItem::with_id(app, "import:rdpRegistry", "From RDP (Registry)…", cfg!(windows), None::<&str>)?,
                    &plain("import:rdp", "From Remote Desktop (.rdp) Files…")?,
                    &plain("import:mobaxterm", "From MobaXterm File…")?,
                    // SuperXTerminal's own: SecureCRT's Sessions folder
                    &plain("import:securecrt", "From SecureCRT…")?,
                    &plain("import:csv", "From CSV File…")?,
                    &plain("import:ssh", "From ssh config…")?,
                ],
            )?,
            &plain("exportXml", "Export Sessions…")?,
            &plain("exportCsv", "Export Sessions to CSV…")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXTerminal's own: every session in one table, its columns filtered, to change many at once
            &plain("editSessionsTable", "Edit Sessions as Table…")?,
            // SuperXPuTTY's File > Edit sessions externally and Reload sessions file
            &plain("editSessionsExternally", "Edit Sessions Externally…")?,
            &plain("reloadSessions", "Reload Sessions File")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXPuTTY's File > Save layout and Save layout as...
            &action("saveLayout")?,
            &action("saveLayoutAs")?,
            &PredefinedMenuItem::separator(app)?,
            // Cmd+W closes the tab, as in Terminal and iTerm2; the window closes with its red button
            &action("closeTab")?,
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
            // the terminal's own Find (its Ctrl+Shift+F), Clear Scrollback, Reset and Copy All, with the Mac's keys
            &action("find")?,
            &action("copyAll")?,
            &action("clearScrollback")?,
            &action("resetTerminal")?,
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
            &action("nextTab")?,
            &action("prevTab")?,
            // SuperXPuTTY's Switch session…: the Quick Selector over the open tabs
            &action("switchSession")?,
            &PredefinedMenuItem::separator(app)?,
            &action("duplicateSession")?,
            &action("renameTab")?,
            &action("restartSession")?,
            &PredefinedMenuItem::separator(app)?,
            &action("splitRight")?,
            &action("splitDown")?,
            &action("joinAll")?,
            &PredefinedMenuItem::separator(app)?,
            &action("gotoCommandBar")?,
            &action("gotoConnectionBar")?,
            &action("focusActiveSession")?,
        ],
    )?;
    let tools = Submenu::with_items(
        app,
        "Tools",
        true,
        &[
            &action("scriptEditor")?,
            &PredefinedMenuItem::separator(app)?,
            &plain("databaseSessions", "Database Sessions…")?,
            &plain("environmentVariables", "Environment Variables…")?,
            &plain("environmentColours", "Environment Colours…")?,
            &PredefinedMenuItem::separator(app)?,
            // Settings' tabs in its order, as the app menu's Settings… opens them: "settings:" and the tab's name (Profiles
            // Settings… opens on Default Settings)
            &plain("settings:General", "General Settings…")?,
            &plain("profiles", "Profiles Settings…")?,
            &plain("settings:Terminal", "Terminal Settings…")?,
            &plain("settings:Remote Desktop", "Remote Desktop Settings…")?,
            &plain("settings:Shortcuts", "Shortcut Settings…")?,
            &plain("settings:Layouts", "Layout Settings…")?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(app, "toggleLocalOverrides", "Use Local Overrides", true, overrides_on, None::<&str>)?,
        ],
    )?;
    // shown or hidden, as SuperXPuTTY's View > Send Commands and View > Button Bar
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &shown("toggleSessionsPane")?,
            &shown("toggleSendCommands")?,
            &shown("toggleButtonBar")?,
            &shown("toggleStatusBar")?,
            &PredefinedMenuItem::separator(app)?,
            &shown("toggleSessionsAutoHide")?,
            // the saved layouts, in Settings > Layouts
            &action("toggleLayouts")?,
            &shown("toggleSessionDetail")?,
            &shown("toggleLogViewer")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXTerminal's own: the tabs tiled, to watch many sessions at once
            &shown("toggleGridView")?,
            &shown("toggleAlwaysOnTop")?,
            &action("fullScreen")?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &tools, &window])
}

/// The Shortcuts editor's rows: every action, its default and its key now
#[tauri::command]
fn shortcuts_keys(state: State<'_, AppState>) -> Vec<keys::Row> {
    keys::rows(&state.settings.lock().unwrap().shortcuts)
}

/// The keys this system keeps (the Mac's Cmd+Q, Cmd+C...; Windows' Alt+F4 and the terminal's Ctrl+Shift+C/V), which
/// the Shortcuts editor won't give to an action
#[tauri::command]
fn shortcuts_reserved() -> Vec<&'static str> {
    keys::reserved().to_vec()
}

/// What a new window starts with: a session to open ("store:id", Connect in new window), or a running tab to take over
/// (Move to New Window: its session's pty id, title and how its tab looks, as the window that had it describes it)
#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct WindowStart {
    open: Option<String>,
    adopt: Option<serde_json::Value>,
    /// Load in New Window: a layout to open
    layout: Option<String>,
}

/// The layouts' names, "<Auto Restore>" first
#[tauri::command]
fn layouts_list(state: State<'_, AppState>) -> Vec<String> {
    layouts::list(&shared_folder(&state))
}

/// A layout's text; None when it hasn't been saved
#[tauri::command]
fn layout_read(state: State<'_, AppState>, name: String) -> Option<String> {
    layouts::read(&shared_folder(&state), &state.user_folder, &name)
}

/// Save a layout
#[tauri::command]
fn layout_save(state: State<'_, AppState>, name: String, text: String) -> Result<(), String> {
    layouts::save(&shared_folder(&state), &state.user_folder, &name, &text)
}

/// Delete a saved layout
#[tauri::command]
fn layout_delete(state: State<'_, AppState>, name: String) -> Result<(), String> {
    layouts::delete(&shared_folder(&state), &name)
}

/// Rename a saved layout
#[tauri::command]
fn layout_rename(state: State<'_, AppState>, from: String, to: String) -> Result<(), String> {
    layouts::rename(&shared_folder(&state), &from, &to)
}

/// The Log Viewer: entries after `after` (0 for all kept)
#[tauri::command]
fn applog_since(after: u64) -> Vec<applog::Entry> {
    applog::since(after)
}

/// A line from the UI (the status bar's messages, as SuperXPuTTY's ReportStatus logs them)
#[tauri::command]
fn applog_write(level: String, text: String) {
    match level.as_str() {
        "WARN" => applog::warn(text),
        "ERROR" => applog::error(text),
        _ => applog::info(text),
    }
}

/// The Log Viewer's Clear (the file keeps them)
#[tauri::command]
fn applog_clear() {
    applog::clear();
}

/// The Log Viewer's Show Log File: the file in Finder
#[tauri::command]
fn applog_reveal() -> Result<(), String> {
    reveal(&applog::file())
}

/// Settings > General's Show buttons beside Use Sessions.xml and Use PrivateSessions.xml: the file in Finder or
/// File Explorer, whether or not it's in use. Sessions.xml is in the settings folder, PrivateSessions.xml in this
/// user's own folder (on this computer); Database sessions have no file. Returns what to tell the user when the file
/// isn't there yet and its folder is shown instead, so an empty-looking folder isn't a puzzle
#[tauri::command]
fn sessions_reveal(state: State<'_, AppState>, store: sessions::Store) -> Result<Option<String>, String> {
    let (shown, note) = file_or_folder(&sessions_file_of(&state, store)?);
    reveal(&shown)?;
    Ok(note)
}

/// Sessions.xml's or PrivateSessions.xml's file, for Restore from Backup: Database sessions have none, and Sessions.xml
/// none until a settings folder is chosen
fn sessions_file_of(state: &AppState, store: sessions::Store) -> Result<PathBuf, String> {
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    sessions::file_of(store, settings_folder.as_deref(), &state.user_folder).ok_or_else(|| match store {
        sessions::Store::Database => "Database sessions aren't kept in a file".to_string(),
        _ => "No settings folder has been chosen yet".to_string(),
    })
}

/// Settings > General's Restore from Backup: the backups of Sessions.xml or PrivateSessions.xml, newest first
#[tauri::command]
fn sessions_backups(state: State<'_, AppState>, store: sessions::Store) -> Result<Vec<sessions::Backup>, String> {
    Ok(sessions::backups(&sessions_file_of(&state, store)?))
}

/// Put one of Sessions.xml's or PrivateSessions.xml's backups back (the file now is backed up first). The Sessions
/// tree reads it again, as for any change on disk (`sessions_files_changed`)
#[tauri::command]
fn sessions_restore(state: State<'_, AppState>, store: sessions::Store, name: String) -> Result<(), String> {
    let file = sessions_file_of(&state, store)?;
    sessions::restore(&file, &name)?;
    applog::info(format!("Restored {} from {name}", file.display()));
    Ok(())
}

/// What to show in Finder or File Explorer for a sessions file: the file, or its folder when it isn't there yet (no
/// session has been saved to it), since revealing a missing file opens Explorer on Documents instead and Finder on
/// nothing. The folder comes with a note saying the file will be made there
fn file_or_folder(file: &std::path::Path) -> (PathBuf, Option<String>) {
    if file.exists() {
        return (file.to_path_buf(), None);
    }
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let what = if name == "Sessions.xml" { "Saved" } else { "Private" };
    let note = format!("There's no {name} yet: it's made in this folder when the first {what} session is saved.");
    (file.parent().map(std::path::Path::to_path_buf).unwrap_or_default(), Some(note))
}

/// Send Commands' history saved from earlier runs, newest first, for the Command box as a window opens; none while
/// Settings > General > Command bar keeps it 0 days (until the app closes)
#[tauri::command]
fn command_history_load(state: State<'_, AppState>) -> Vec<String> {
    let days = state.settings.lock().unwrap().command_history_days;
    command_history::load(&state.user_folder, days, command_history::now())
}

/// Note a command sent from the Command box in the saved history (nothing is saved with 0 days)
#[tauri::command]
fn command_history_add(state: State<'_, AppState>, command: String) -> Result<(), String> {
    let days = state.settings.lock().unwrap().command_history_days;
    command_history::add(&state.user_folder, days, &command, command_history::now())
}

/// Show a file in its folder: selected in Finder on the Mac (`open -R`) and Explorer on Windows (`/select,`); on
/// Linux the folder alone, in the file manager ([`open_command`])
pub fn reveal(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut select = std::ffi::OsString::from("/select,");
        select.push(path.as_os_str());
        std::process::Command::new("explorer").arg(select).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        open_command(path, Opening::InItsFolder).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// What's been asked to open from outside (the command line, a second start, a link), waiting for the window in
/// front to take it (`requests_take`)
static PENDING: Mutex<Vec<cli::Request>> = Mutex::new(Vec::new());

/// The menu bar icon's items ("tray:..."): a session opens in the window in front, or in a new one when none is open
fn tray_command(app: &AppHandle, id: &str) {
    let has_window = !app.webview_windows().is_empty();
    if let Some(rest) = id.strip_prefix("tray:open:") {
        // "<store>:<id>"; Database sessions' ids start "@db/" already
        let session = rest.split_once(':').map(|(_, id)| id).unwrap_or(rest);
        deliver(app, Some(cli::Request::Session { id: session.to_string() }));
        return;
    }
    match id {
        "tray:quit" => app.exit(0),
        // a new window starts with a Local Shell
        "tray:newLocalShell" | "tray:openSession" | "tray:newWindow" if !has_window => deliver(app, None),
        "tray:newWindow" => {
            let _ = new_window(app, None, None, None);
        }
        "tray:newLocalShell" | "tray:openSession" => {
            deliver(app, None);
            let front = FRONT_WINDOW.lock().unwrap().clone().unwrap_or_else(|| "main".into());
            let _ = app.emit_to(front.as_str(), "menu", id.trim_start_matches("tray:"));
        }
        _ => deliver(app, None),
    }
}

/// Something to open from outside: kept for the window in front, which is told and brought forward; with no window
/// open (SuperXTerminal kept in the menu bar), a new one, which takes it as it starts
fn deliver(app: &AppHandle, request: Option<cli::Request>) {
    if app.webview_windows().is_empty() {
        if let Some(request) = request {
            PENDING.lock().unwrap().push(request);
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = main_window(&handle);
        });
        return;
    }
    let front = FRONT_WINDOW.lock().unwrap().clone().filter(|label| app.get_webview_window(label).is_some()).unwrap_or_else(|| "main".into());
    if let Some(request) = request {
        PENDING.lock().unwrap().push(request);
        let _ = app.emit_to(front.as_str(), "open-request", ());
    }
    if let Some(window) = app.get_webview_window(&front) {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// What's waiting to open, taken once
#[tauri::command]
fn requests_take() -> Vec<cli::Request> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

/// Settings > General: SuperXTerminal opens ssh:// and telnet:// links (Terminal does until then), as LaunchServices
/// records it for the user
#[tauri::command]
fn links_make_default(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSString;
        #[link(name = "CoreServices", kind = "framework")]
        unsafe extern "C" {
            /// LaunchServices' call to make an app the default for a URL scheme; 0 when it worked
            fn LSSetDefaultHandlerForURLScheme(scheme: *const std::ffi::c_void, bundle: *const std::ffi::c_void) -> i32;
        }
        let bundle = NSString::from_str(&app.config().identifier);
        for scheme in ["ssh", "telnet"] {
            let scheme = NSString::from_str(scheme);
            // NSString is toll-free bridged to the CFString LaunchServices takes
            let status = unsafe {
                LSSetDefaultHandlerForURLScheme(&*scheme as *const NSString as *const _, &*bundle as *const NSString as *const _)
            };
            if status != 0 {
                return Err(format!("macOS refused (error {status}). The app may need to be in Applications first."));
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Only on the Mac.".into())
    }
}

/// What each new window starts with, by its label; taken once by `window_start`
static WINDOW_OPENS: Mutex<Option<std::collections::HashMap<String, WindowStart>>> = Mutex::new(None);
/// The window last brought to the front, which the menu bar's commands go to
static FRONT_WINDOW: Mutex<Option<String>> = Mutex::new(None);
/// The number of the next window ("window-2" ...)
static NEXT_WINDOW: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(2);

/// File > New Window, and Connect in new window (`open` the session, "store:id"), as SuperXPuTTY's new instance: another
/// window with its own Sessions tree and tabs
///
/// Async, so it runs off the main thread. On Windows Tauri runs a plain command on the main thread, inside WebView2's
/// handling of the page's call, and `run_on_main_thread` from there makes the window at once, in that handler: WebView2
/// never finished the new web view, which stayed blank (about:blank, the window's grey), and the page that asked never
/// heard back. Move to New Window, New Window and Connect in new window all did this on Windows; the Mac didn't mind.
#[tauri::command]
async fn window_new(app: AppHandle, open: Option<String>, adopt: Option<serde_json::Value>, layout: Option<String>) -> Result<(), String> {
    new_window(&app, open, adopt, layout)
}

/// Make another window, starting with `open`, `adopt` or `layout` (see [`WindowStart`]), on the main thread. Never
/// from a plain (sync) command: see [`window_new`].
fn new_window(app: &AppHandle, open: Option<String>, adopt: Option<serde_json::Value>, layout: Option<String>) -> Result<(), String> {
    let label = format!("window-{}", NEXT_WINDOW.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    if open.is_some() || adopt.is_some() || layout.is_some() {
        WINDOW_OPENS.lock().unwrap().get_or_insert_with(Default::default).insert(label.clone(), WindowStart { open, adopt, layout });
    }
    let handle = app.clone();
    // windows are made on the main thread (the web view's settings need it)
    app.run_on_main_thread(move || {
        // down and to the right of the window in front, as a Mac app cascades new windows
        let front_label = FRONT_WINDOW.lock().unwrap().clone().unwrap_or_else(|| "main".into());
        let front = handle.get_webview_window(&front_label).or_else(|| handle.get_webview_window("main"));
        let at = front.and_then(|w| Some(w.outer_position().ok()?.to_logical::<f64>(w.scale_factor().ok()?))).map(|p| (p.x + 28.0, p.y + 28.0));
        let _ = window(&handle, &label, at);
        // in front at once, so the menu's next command (Cmd+T) goes to it
        *FRONT_WINDOW.lock().unwrap() = Some(label.clone());
        if let Some(new) = handle.get_webview_window(&label) {
            let _ = new.set_focus();
        }
    })
    .map_err(|e| e.to_string())
}

/// What a window starts with: a session to open or a tab to take over; None for a Local Shell
#[tauri::command]
fn window_start(window: tauri::WebviewWindow) -> Option<WindowStart> {
    WINDOW_OPENS.lock().unwrap().as_mut().and_then(|opens| opens.remove(window.label()))
}

/// The main window, made here rather than in tauri.conf.json so its web view can have Writing Tools off: macOS put
/// its "Write with Siri" badge by the terminal's cursor, and offered to rewrite what's typed into a session
fn main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    window(app, "main", None)
}

/// A window's own light or dark for the app's theme (Settings > General > Theme); None follows the system. On
/// Windows it's what Windows draws the window's menu bar and title bar in, which otherwise follow the system whatever
/// the app's theme; on the Mac the menu bar is the system's, so windows are left to it.
#[cfg(windows)]
fn window_theme(theme: &str) -> Option<tauri::Theme> {
    match theme {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    }
}

/// Windows: the menus that drop down from the menu bar (and right-click menus) in the app's theme. They aren't the
/// window's: Windows draws every popup menu in the app's one preferred mode, which Tauri leaves following the system,
/// so a Light app had dark menus on a dark system. uxtheme's undocumented SetPreferredAppMode (ordinal 135, Windows 10
/// 1903 and later) forces it, and FlushMenuThemes (136) has menus already made redrawn in it.
#[cfg(windows)]
fn apply_menu_theme(theme: &str) {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    // PreferredAppMode: AllowDark (follow the system), ForceDark, ForceLight
    let mode: i32 = match theme {
        "light" => 3,
        "dark" => 2,
        _ => 1,
    };
    let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
    unsafe {
        let uxtheme = LoadLibraryW(name.as_ptr());
        if uxtheme.is_null() {
            return;
        }
        if let Some(set_preferred_app_mode) = GetProcAddress(uxtheme, 135 as *const u8) {
            let set_preferred_app_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_preferred_app_mode);
            set_preferred_app_mode(mode);
        }
        if let Some(flush_menu_themes) = GetProcAddress(uxtheme, 136 as *const u8) {
            let flush_menu_themes: unsafe extern "system" fn() = std::mem::transmute(flush_menu_themes);
            flush_menu_themes();
        }
    }
}

/// Windows: every window's menu bar, menus and title bar in the app's theme (on the Mac nothing changes)
fn apply_window_theme(app: &AppHandle, theme: &str) {
    #[cfg(windows)]
    {
        apply_menu_theme(theme);
        for window in app.webview_windows().values() {
            let _ = window.set_theme(window_theme(theme));
        }
    }
    #[cfg(not(windows))]
    let _ = (app, theme);
}

/// A window's colour before its page has drawn: the page's own window colour (app.css's --bg-window), light only when
/// Settings > General > Theme is Light. Following the system starts dark too, so a dark system never sees a white
/// flash; a light one sees dark for a moment instead, as the page follows the system once it's loaded.
fn loading_colour(theme: &str) -> tauri::window::Color {
    if theme == "light" { tauri::window::Color(0xf5, 0xf5, 0xf5, 0xff) } else { tauri::window::Color(0x2d, 0x2d, 0x30, 0xff) }
}

/// Whether a window opens where it was last left: the main window only, as the one SuperXTerminal starts with; New
/// Window's windows each start down and to the right of the one in front, as a Mac app cascades them
fn remembers_place(label: &str) -> bool {
    label == "main" && !demo::on()
}

/// What's remembered of the main window (tauri-plugin-window-state, in the app's config folder as .window-state.json):
/// its position and size, and whether it was maximized or full screen. Not whether it was shown, since it's always
/// shown at the start, nor its frame, which is always the system's.
fn remembered_state() -> tauri_plugin_window_state::StateFlags {
    use tauri_plugin_window_state::StateFlags;
    StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN
}

/// A SuperXTerminal window: the first ("main"), or another made by New Window ("window-2", ...) at `at` (logical)
fn window(app: &tauri::AppHandle, label: &str, at: Option<(f64, f64)>) -> tauri::Result<()> {
    let builder = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::default())
        .title("SuperXTerminal")
        .inner_size(1400.0, 900.0)
        .min_inner_size(800.0, 500.0)
        .disable_drag_drop_handler();
    let builder = match at {
        Some((x, y)) => builder.position(x, y),
        None => builder,
    };
    #[cfg(target_os = "macos")]
    let builder = builder.with_webview_configuration(writing_tools_off());
    let theme = app.try_state::<AppState>().map(|s| s.settings.lock().unwrap().theme.clone()).unwrap_or_default();
    // the window's own colour while the page loads, which was white whatever the theme, a flash in a dark room
    let builder = builder.background_color(loading_colour(&theme));
    // Windows: its menu bar, menus and title bar in the app's theme from the start
    #[cfg(windows)]
    let builder = {
        apply_menu_theme(&theme);
        builder.theme(window_theme(&theme))
    };
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
/// Run as ssh's askpass for Connect external (`askpass.rs`) instead of as the app; true when it did
pub fn askpass_helper() -> bool {
    askpass::helper()
}

/// Run as one of demo mode's stand-in devices (`demo_device.rs`) instead of as the app; true when it did
pub fn demo_device() -> bool {
    demo_device::run()
}

/// Start the app: load the settings, open the main window, and handle the command line SuperXPuTTY takes
pub fn run() {
    // demo mode: a fresh folder of made-up sessions in place of the user's own (demo.rs)
    if demo::on() {
        if let Err(e) = demo::prepare() {
            applog::warn(format!("The demo's folder couldn't be made: {e}"));
        }
    }
    let user_folder = sessions::user_folder();
    let settings = AppSettings::load(&user_folder);
    onepassword::set_enabled(settings.one_password);
    let profiles = Profiles::load(&user_folder);
    applog::info(format!("SuperXTerminal {} started (user folder {})", env!("CARGO_PKG_VERSION"), user_folder.display()));
    // started with something to open (SuperXPuTTY's command line)
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(request) = cli::from_args(&args) {
        PENDING.lock().unwrap().push(request);
    }
    // No raw device events (Windows; the setting does nothing elsewhere): the window toolkit (tao) otherwise registers
    // the app for raw keyboard and mouse input, to report device events nothing here listens to, and Windows doesn't
    // call a low-level keyboard hook of a program registered so. A Remote Desktop tab's hook for the Windows key
    // (rdp_win_keys.rs) then went in without an error and never heard a key: Win+R stayed on this computer.
    let builder = tauri::Builder::default().device_event_filter(tauri::DeviceEventFilter::Always);
    // one SuperXTerminal: started again (from a script, a terminal), the first one opens what it was given. Not for a
    // demo, which would otherwise only bring a SuperXTerminal already running to the front
    let builder = if demo::on() {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| deliver(app, cli::from_args(argv.get(1..).unwrap_or(&[])))))
    };
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // the main window opens where it was last, its size, maximized or full screen as it was left
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(remembered_state())
                .with_filter(remembers_place)
                .build(),
        )
        .menu({
            let settings = settings.clone();
            move |app| menu(app, &settings)
        })
        // menu items the UI handles arrive there as a "menu" event with the item's id
        .on_menu_event(|app, event| {
            if event.id().0.starts_with("tray:") {
                tray_command(app, event.id().0.as_str());
                return;
            }
            if event.id().0 == "thirdPartyNotices" {
                open_notices(app);
                return;
            }
            // to the window in front only: with several windows, Cmd+T opens one shell, in that window
            let front = FRONT_WINDOW.lock().unwrap().clone().filter(|label| app.get_webview_window(label).is_some());
            let _ = app.emit_to(front.as_deref().unwrap_or("main"), "menu", event.id().0.as_str());
        })
        // a window's tabs go with it: their shells and connections end
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(true) => *FRONT_WINDOW.lock().unwrap() = Some(window.label().to_string()),
            tauri::WindowEvent::Destroyed => {
                applog::info(format!("Window {} closed; its sessions end", window.label()));
                // kept now, not only when the app quits: it may stay in the menu bar or tray, and be ended there
                if remembers_place(window.label()) {
                    use tauri_plugin_window_state::AppHandleExt;
                    let _ = window.app_handle().save_window_state(remembered_state());
                }
                pty::close_window(&window.state::<pty::Ptys>(), window.label());
                rdp::close_window(&window.state::<rdp::Rdps>(), window.label());
                sftp::close_window(&window.state::<sftp::Sftps>(), window.label())
            }
            _ => {}
        })
        .setup(|app| {
            console::allow_ctrl_c();
            logs::sweep_temp();
            // pins left by an older MSI show a blank file once their icon has gone (pins.rs). Not for a demo, which
            // may be a development build: its pins are the installed app's
            #[cfg(windows)]
            if !demo::on() {
                pins::repair();
            }
            #[cfg(target_os = "macos")]
            no_siri_badge();
            main_window(app.handle())?;
            tray::refresh(app.handle());
            command_tap::install(app.handle());
            Ok(())
        })
        .manage(pty::Ptys::default())
        .manage(rdp::Rdps::default())
        .manage(sftp::Sftps::default())
        .manage(logs::Logs::default())
        .manage(rhai_scripts::RhaiRuns::default())
        .manage(server_load::ServerLoads::default())
        .manage(AppState {
            user_folder,
            settings: Mutex::new(settings),
            sessions: Mutex::new(Sessions::default()),
            profiles: Mutex::new(profiles),
            database: Mutex::new(DbState::default()),
            db_server: Mutex::new(Vec::new()),
            files_seen: Mutex::new(file_watch::Seen::nothing()),
            files_read: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            sessions_load,
            sessions_shown,
            open_link,
            settings_get,
            settings_set_folder,
            folder_writable,
            folder_exists,
            settings_save,
            icons_custom,
            highlighting_load,
            session_get,
            session_password_has,
            session_password_set,
            session_password_from_pw,
            session_save,
            session_delete,
            session_move,
            folder_move,
            items_move,
            items_delete,
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
            serial_devices_list,
            sessions_files_changed,
            session_external,
            serial_device_set,
            pty::pty_special,
            overrides_get,
            sessions_bulk_edit,
            sessions_table_load,
            sessions_table_save,
            sessions_table_export_csv,
            sessions_table_read_csv,
            session_passwords_set,
            sessions_export_csv,
            sessions_csv_template,
            onepassword_check,
            ui_themes_load,
            ui_themes_save,
            window_new,
            layouts_list,
            applog_since,
            applog_write,
            applog_clear,
            applog_reveal,
            sessions_reveal,
            sessions_backups,
            sessions_restore,
            command_history_load,
            command_history_add,
            requests_take,
            links_make_default,
            shortcuts_keys,
            shortcuts_reserved,
            layout_read,
            layout_save,
            layout_delete,
            layout_rename,
            window_start,
            sessions_import_plan,
            sessions_import_apply,
            ssh_config_path,
            sessions_import_source,
            ssh_algorithms_available,
            sessions_export_xml,
            sessions_edit_externally,
            overrides_set,
            override_presets_get,
            profile_key_age,
            variables_usage,
            profiles_missing,
            variables_built_in,
            override_presets_set,
            pty::pty_serial_get,
            pty::pty_serial_set,
            log_open,
            log_write,
            log_close,
            log_read,
            log_chunk,
            highlighting_user_folder,
            open_path,
            buttonbars_load,
            buttonbars_save,
            buttonbars_export,
            buttonbars_import,
            scripts_list,
            script_chosen,
            script_copy_in,
            session_script,
            quick_connect_script,
            script_load,
            script_save,
            script_delete,
            text_file_read,
            text_file_write,
            script_output_save,
            rhai_scripts::rhai_run,
            rhai_scripts::rhai_reply,
            rhai_scripts::rhai_stop,
            rhai_scripts::rhai_check,
            server_load::server_load_watch,
            server_load::server_load_stop,
            dropped_file_save,
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
            profiles_export,
            profiles_read_file,
            profile_templates,
            profiles_read_putty,
            profile_template_share,
            profiles_save,
            profiles_delete,
            clipboard::clipboard_text,
            pty::pty_open,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_close,
            sftp::sftp_open,
            sftp::sftp_close,
            sftp::sftp_answer,
            sftp::sftp_list,
            sftp::sftp_mkdir,
            sftp::sftp_rename,
            sftp::sftp_delete,
            sftp::sftp_cancel,
            sftp::sftp_transfer,
            sftp::local_list,
            sftp::local_mkdir,
            sftp::local_rename,
            sftp::local_reveal,
            rdp::rdp_open,
            rdp::rdp_key,
            rdp::rdp_keys,
            rdp::rdp_grab_keys,
            rdp::rdp_text,
            rdp::rdp_sync,
            rdp::rdp_mouse,
            rdp::rdp_resize,
            rdp::rdp_close,
            rdp::rdp_clipboard_announce,
            pty::pty_attach
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match &event {
                // a link macOS hands over (superxterminal://, ssh://, telnet://)
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Opened { urls } => {
                    for url in urls {
                        deliver(app, cli::from_url(url.as_str()));
                    }
                }
                // the last window closed: with the menu bar icon, SuperXTerminal keeps running there
                tauri::RunEvent::ExitRequested { api, code: None, .. } => {
                    if app.state::<AppState>().settings.lock().unwrap().show_in_menu_bar {
                        api.prevent_exit();
                    }
                }
                // the Dock icon clicked with no window open: a new one
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { has_visible_windows: false, .. } => deliver(app, None),
                _ => {}
            }
        });
}

#[cfg(test)]
mod tests {
    /// What [`super::open_command`] would run: the program and its arguments
    #[cfg(not(windows))]
    fn opening(path: &str, how: super::Opening) -> Vec<String> {
        let command = super::open_command(std::path::Path::new(path), how);
        std::iter::once(command.get_program()).chain(command.get_args()).map(|a| a.to_string_lossy().into_owned()).collect()
    }

    /// The Linux build opened files with the Mac's `open`, which Linux hasn't got, so Help > Third-Party Notices,
    /// Edit Sessions Externally, Open log and showing a file in its folder did nothing there. Not on Windows, which
    /// opens files its own way (cmd's start, Explorer's /select,).
    #[cfg(not(windows))]
    #[test]
    fn files_open_with_the_systems_own_opener() {
        use super::Opening::*;
        let file = "/home/me/SuperXTerminal/Sessions.xml";
        if cfg!(target_os = "macos") {
            assert_eq!(opening(file, InItsApp), ["open", file]);
            assert_eq!(opening(file, AsText), ["open", "-t", file]);
            assert_eq!(opening(file, InItsFolder), ["open", "-R", file]);
        } else {
            assert_eq!(opening(file, InItsApp), ["xdg-open", file]);
            assert_eq!(opening(file, AsText), ["xdg-open", file]);
            // xdg-open can't select a file, so its folder is opened
            assert_eq!(opening(file, InItsFolder), ["xdg-open", "/home/me/SuperXTerminal"]);
            assert_eq!(opening("Sessions.xml", InItsFolder), ["xdg-open", "Sessions.xml"]);
        }
    }

    /// An app's state with these settings, the user's folder a temporary one
    fn state_with(settings: crate::settings::AppSettings) -> super::AppState {
        use std::sync::Mutex;
        super::AppState {
            user_folder: std::env::temp_dir().join("sxt-state-test"),
            settings: Mutex::new(settings),
            sessions: Mutex::new(Default::default()),
            profiles: Mutex::new(Default::default()),
            database: Mutex::new(Default::default()),
            db_server: Mutex::new(Vec::new()),
            files_seen: Mutex::new(crate::file_watch::Seen::nothing()),
            files_read: Mutex::new(None),
        }
    }

    #[test]
    fn whether_private_sessions_are_off_is_read_without_hanging() {
        // private_off once took the settings' lock twice in one statement, and the app hung as it read the sessions:
        // run it (and store_file, which asks it) on a thread of its own, and fail rather than hang
        let settings = crate::settings::AppSettings {
            settings_folder: Some(std::env::temp_dir()),
            use_sessions_xml: true,
            use_private_sessions: false,
            ..Default::default()
        };
        let state = std::sync::Arc::new(state_with(settings));
        let (done, finished) = std::sync::mpsc::channel();
        let running = std::sync::Arc::clone(&state);
        std::thread::spawn(move || {
            let off = super::private_off(&running);
            let refused = super::store_file(&running, crate::sessions::Store::Private).is_err();
            let _ = done.send((off, refused));
        });
        let (off, refused) = finished.recv_timeout(std::time::Duration::from_secs(2)).expect("private_off hung: a lock taken twice");
        assert!(off);
        assert!(refused, "Private sessions turned off: nothing is written there");
    }

    #[test]
    fn private_sessions_are_off_only_while_saved_sessions_are_in_use() {
        assert!(super::private_off_when(false, true));
        // no Saved sessions (no settings folder, or Use Sessions.xml off): Private stay on, whatever the setting
        assert!(!super::private_off_when(false, false));
        assert!(!super::private_off_when(true, true));
    }

    #[test]
    fn show_in_the_file_manager_selects_the_sessions_file_or_opens_its_folder_saying_why_before_there_is_one() {
        let folder = std::env::temp_dir().join("sxt-reveal-test");
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("Sessions.xml");
        // none yet: the folder, saying why the file isn't in it
        let (shown, note) = super::file_or_folder(&file);
        assert_eq!(shown, folder);
        assert!(note.unwrap().contains("There's no Sessions.xml yet"));
        std::fs::write(&file, "<ArrayOfSessionData />").unwrap();
        assert_eq!(super::file_or_folder(&file), (file.clone(), None));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_dropped_file_is_saved_under_its_own_name_in_a_folder_of_its_own() {
        use std::path::Path;
        let base = Path::new("/tmp/SuperXTerminal-dropped");
        assert_eq!(super::dropped_file_path(base, "Sessions.xml", "1-2"), base.join("1-2").join("Sessions.xml"));
        // a name with folders, or .., stays inside
        assert_eq!(super::dropped_file_path(base, "../../etc/passwd", "1-2"), base.join("1-2").join("passwd"));
        assert_eq!(super::dropped_file_path(base, "..", "1-2"), base.join("1-2").join("dropped"));
    }

    #[test]
    fn a_session_with_no_script_runs_its_profiles_found_as_a_launch_finds_the_profile() {
        use crate::sessions::{FolderSetting, Sessions, Store, StoredSession};
        let profiles: crate::profiles::Profiles = serde_json::from_str(
            r#"{"profiles":[{"name":"Default Settings"},{"name":"Linux","script":"linux.spsl"},{"name":"Cisco","script":"cisco.spsl"}]}"#,
        )
        .unwrap();
        let mut sessions = Sessions::default();
        let folder = FolderSetting { path: "Lab".into(), putty_profile: "Cisco".into(), ..FolderSetting::default() };
        sessions.folders.entry(Store::Saved).or_default().insert("lab".into(), folder);
        let at = |attributes: &[(&str, &str)]| {
            let mut all = vec![("SessionId".to_string(), "Lab/web".to_string()), ("Host".to_string(), "192.168.0.10".to_string())];
            all.extend(attributes.iter().map(|(n, v)| (n.to_string(), v.to_string())));
            StoredSession { attributes: all }
        };
        let spec = |session: &StoredSession| super::script_spec(&sessions, &profiles, Store::Saved, session);
        // just a name and a host: its profile's script
        assert_eq!(spec(&at(&[("PuttySession", "Linux")])), Some(("linux.spsl".into(), Some("Linux".into()))));
        // no profile of its own: the folder's profile's
        assert_eq!(spec(&at(&[("PuttySession", "Default Settings")])), Some(("cisco.spsl".into(), Some("Cisco".into()))));
        // its own script wins
        assert_eq!(spec(&at(&[("PuttySession", "Linux"), ("SPSLFileName", "own.spsl")])), Some(("own.spsl".into(), None)));
    }

    #[test]
    fn a_quick_connects_typed_fields_are_its_scripts_session_values() {
        let quick = crate::launch::QuickConnect {
            protocol: "SSH".into(),
            host: "admin@192.168.0.50".into(),
            username: String::new(),
            password: "never shown".into(),
            profile: "Jump Host".into(),
        };
        let values = super::quick_values(&quick.session().unwrap());
        let get = |k: &str| values.get(k).map(String::as_str).unwrap_or("?");
        assert_eq!(
            [get("host_address"), get("host_port"), get("session_name"), get("session_username"), get("session_profile"), get("session_protocol")],
            ["192.168.0.50", "22", "192.168.0.50", "admin", "Jump Host", "SSH"]
        );
        // the password typed stays out of the script's values
        assert!(!values.values().any(|v| v.contains("never shown")));
        let typed_port = crate::launch::QuickConnect { host: "sw1:2222".into(), username: "me".into(), ..quick };
        let values = super::quick_values(&typed_port.session().unwrap());
        assert_eq!((values["host_address"].as_str(), values["host_port"].as_str(), values["session_username"].as_str()), ("sw1", "2222", "me"));
    }

    #[test]
    fn a_scripts_save_goes_in_the_output_folder_unless_its_path_says_otherwise() {
        use std::path::Path;
        let (home, output) = (Path::new("/home/me"), Path::new("/home/me/Documents/SuperXTerminal/output"));
        assert_eq!(super::script_output_path("sw1-run.txt", home, output), output.join("sw1-run.txt"));
        assert_eq!(super::script_output_path("lab/sw1.txt", home, output), output.join("lab/sw1.txt"));
        assert_eq!(super::script_output_path("~/Desktop/run.txt", home, output), home.join("Desktop/run.txt"));
        // Windows' own ways of writing the home folder too, as a key file's
        assert_eq!(super::script_output_path("%USERPROFILE%\\Desktop\\run.txt", home, output), home.join("Desktop\\run.txt"));
        assert_eq!(super::script_output_path("~\\Desktop\\run.txt", home, output), home.join("Desktop\\run.txt"));
        let full = if cfg!(windows) { "C:\\backups\\run.txt" } else { "/tmp/backups/run.txt" };
        assert_eq!(super::script_output_path(full, home, output), Path::new(full));
    }

    use super::{loading_colour, remembered_state, remembers_place, view_checks, window_changes, WindowChanges};
    use crate::settings::AppSettings;
    use tauri_plugin_window_state::StateFlags;

    #[test]
    fn a_window_loads_dark_unless_the_theme_is_light() {
        use tauri::window::Color;
        assert_eq!(loading_colour("light"), Color(0xf5, 0xf5, 0xf5, 0xff));
        assert_eq!(loading_colour("dark"), Color(0x2d, 0x2d, 0x30, 0xff));
        assert_eq!(loading_colour("system"), Color(0x2d, 0x2d, 0x30, 0xff));
    }

    #[test]
    fn the_view_menu_ticks_what_is_shown_and_follows_each_change() {
        let mut settings = AppSettings::default();
        settings.show_sessions_pane = true;
        settings.show_send_commands = false;
        settings.show_button_bar = true;
        settings.show_status_bar = true;
        settings.sessions_auto_hide = false;
        settings.show_session_detail = true;
        settings.show_log_viewer = false;
        settings.always_on_top = true;
        assert_eq!(
            view_checks(&settings),
            [
                ("toggleSessionsPane", true),
                ("toggleSendCommands", false),
                ("toggleButtonBar", true),
                ("toggleStatusBar", true),
                ("toggleSessionsAutoHide", false),
                ("toggleSessionDetail", true),
                ("toggleLogViewer", false),
                ("toggleAlwaysOnTop", true),
                ("toggleGridView", false),
            ]
        );
        // each shown or hidden makes the menu again, so its tick follows
        let mut log = settings.clone();
        log.show_log_viewer = true;
        assert_eq!(window_changes(&settings, &log), WindowChanges { menu: true, theme: false });
        let mut top = settings.clone();
        top.always_on_top = false;
        assert_eq!(window_changes(&settings, &top), WindowChanges { menu: true, theme: false });
        let mut grid = settings.clone();
        grid.grid_view = true;
        assert_eq!(window_changes(&settings, &grid), WindowChanges { menu: true, theme: false });
    }

    #[test]
    fn a_terminal_preference_saved_leaves_the_menu_bar_and_theme_alone() {
        // a checkbox in a terminal tab's menu saves the settings; setting the menu bar again shifted the whole window
        let before = AppSettings::default();
        let mut after = before.clone();
        after.terminal_prefs.insert("copyOnSelect".into(), "false".into());
        assert_eq!(window_changes(&before, &after), WindowChanges { menu: false, theme: false });
        let mut keys = before.clone();
        keys.shortcuts.insert("newTab".into(), "Ctrl+T".into());
        assert_eq!(window_changes(&before, &keys), WindowChanges { menu: true, theme: false });
        let mut overrides = before.clone();
        overrides.use_local_overrides = !before.use_local_overrides;
        assert_eq!(window_changes(&before, &overrides), WindowChanges { menu: true, theme: false });
        let mut theme = before.clone();
        theme.theme = "light".into();
        assert_eq!(window_changes(&before, &theme), WindowChanges { menu: false, theme: true });
    }

    #[test]
    fn only_the_main_window_opens_where_it_was_left() {
        assert!(remembers_place("main"));
        // New Window's windows cascade from the one in front instead
        assert!(!remembers_place("window-2"));
    }

    #[test]
    fn the_main_windows_place_size_maximized_and_full_screen_are_remembered_but_not_whether_it_was_shown() {
        let state = remembered_state();
        assert!(state.contains(StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN));
        assert!(!state.intersects(StateFlags::VISIBLE | StateFlags::DECORATIONS));
    }

    #[test]
    fn a_window_the_page_asks_for_is_made_by_an_async_command_so_windows_draws_it() {
        // Move to New Window (and New Window, Connect in new window) opened a grey window on Windows that never loaded,
        // and the window that asked stopped hearing back: a plain command runs on the main thread inside WebView2's
        // handling of the page's call, and a web view made there never finishes. Nothing here can make a WebView2
        // window (the app's windows aren't tested whole: docs/TESTING.md), so this reads the commands themselves: each
        // that makes a window must be async, which Tauri runs off the main thread.
        // as one kind of line ending, whichever Git checked out
        let source = include_str!("lib.rs").replace("\r\n", "\n");
        // the attribute in two halves, so this test's own text isn't taken for a command
        let commands = source.split(concat!("#[tauri", "::command]")).skip(1);
        let mut checked = 0;
        for command in commands {
            let signature = command.trim_start().lines().next().unwrap_or("");
            // the command's body: up to the first line that closes an item
            let body = command.split("\n}\n").next().unwrap_or("");
            let makes_window = ["new_window(", "main_window(", "WebviewWindowBuilder"].iter().any(|call| body.contains(call));
            if makes_window {
                checked += 1;
                assert!(signature.contains("async fn"), "a command that makes a window isn't async, so it hangs on Windows: {signature}");
            }
        }
        assert!(checked > 0, "found no command that makes a window: has window_new moved?");
    }

    /// A session or profile shared between the Mac and Windows names its login script relative to the settings folder;
    /// one typed on Windows, with backslashes, is found on the Mac as well, where \ would be a letter in the name
    #[test]
    fn a_relative_login_script_is_found_with_either_systems_slashes() {
        let dir = std::env::temp_dir().join(format!("sxt-read-script-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts").join("login.spsl"), "SENDLINE x").unwrap();
        for spec in ["scripts/login.spsl", r"scripts\login.spsl"] {
            assert!(matches!(super::read_script(spec, None, &dir), super::SessionScript::Text { ref text, .. } if text == "SENDLINE x"), "{spec}");
        }
        // where a copy of its own would go, for the tab's warning
        let looked = dir.join("scripts/other.spsl").display().to_string();
        assert!(matches!(super::read_script("scripts/other.spsl", None, &dir), super::SessionScript::Missing { looked: Some(ref l), .. } if *l == looked));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// What Move to / Copy to work on: the app's state with a settings folder (Saved sessions) and a user's folder
    /// (Private sessions) of the test's own, both empty, and the folder holding the two, to remove at the end
    fn transfer_state(name: &str) -> (super::AppState, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("sxt-transfer-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (team, me) = (dir.join("team"), dir.join("me"));
        std::fs::create_dir_all(&team).unwrap();
        std::fs::create_dir_all(&me).unwrap();
        let settings = crate::settings::AppSettings { settings_folder: Some(team), ..Default::default() };
        (super::AppState { user_folder: me, ..state_with(settings) }, dir)
    }

    /// Save a new session `id` ("Lab/sw1") in a store's file, as Edit Session does
    fn add_session(state: &super::AppState, store: crate::sessions::Store, id: &str) {
        let (folder, name) = id.rsplit_once('/').unwrap_or(("", id));
        let fields = crate::editing::SessionFields { name: name.into(), folder: folder.into(), host: format!("{name}.example"), ..Default::default() };
        crate::editing::save(&super::store_file(state, store).unwrap(), None, &fields, &super::other_ids(state, store)).unwrap();
        read_files(state);
    }

    /// Read the session files into the state again, as the tree does after each change, keeping the Database's
    fn read_files(state: &super::AppState) {
        let folder = state.settings.lock().unwrap().settings_folder.clone();
        let mut problems = Vec::new();
        let read = crate::sessions::Sessions::load(folder.as_deref(), &state.user_folder, &mut problems);
        assert!(problems.is_empty(), "{problems:?}");
        let mut sessions = state.sessions.lock().unwrap();
        sessions.saved = read.saved;
        sessions.private = read.private;
    }

    /// The ids in a store, as the tree shows them once the files are read again
    fn ids_in(state: &super::AppState, store: crate::sessions::Store) -> Vec<String> {
        read_files(state);
        let sessions = state.sessions.lock().unwrap();
        let list = if store == crate::sessions::Store::Saved { &sessions.saved } else { &sessions.private };
        list.iter().map(|s| s.id().to_string()).collect()
    }

    /// Move to: a session is taken out of where it was only once it's in the other list. One whose name is taken
    /// there didn't arrive, so it stays, rather than being deleted from the one place it was
    #[test]
    fn a_move_takes_out_only_the_sessions_that_arrived_and_their_favorite_and_password_go_with_them() {
        use crate::sessions::Store::{Private, Saved};
        let (state, dir) = transfer_state("move");
        for id in ["Lab/sw1", "Lab/sw2", "Lab/sw3"] {
            add_session(&state, Saved, id);
        }
        add_session(&state, Private, "Core/sw2");
        crate::shortcuts::edit(&state.user_folder, |s| s.favorites = vec!["Lab/sw1".into(), "Lab/sw2".into()]).unwrap();
        crate::session_passwords::set(&state.user_folder, "Lab/sw1", "hunter2").unwrap();

        // dragged onto Private sessions' Core folder, where there's already an sw2
        let ids = vec!["Lab/sw1".to_string(), "Lab/sw2".to_string()];
        let folders = Some(vec!["Core".to_string(), "Core".to_string()]);
        let report = tauri::async_runtime::block_on(super::transfer(&state, Saved, ids, Private, true, folders, false)).unwrap();

        assert_eq!(report.done, 1);
        assert_eq!(report.problems, ["Core/sw2: there's already a session with that name there."]);
        assert_eq!(ids_in(&state, Private), ["Core/sw1", "Core/sw2"]);
        assert_eq!(ids_in(&state, Saved), ["Lab/sw2", "Lab/sw3"], "sw2 didn't arrive, so it's still where it was");
        // the one that moved is still a Favorite and still has its password, under its new place
        assert_eq!(crate::shortcuts::load(&state.user_folder).favorites, ["Core/sw1", "Lab/sw2"]);
        assert!(crate::session_passwords::has(&state.user_folder, "Core/sw1"));
        assert!(!crate::session_passwords::has(&state.user_folder, "Lab/sw1"));
        crate::session_passwords::set(&state.user_folder, "Core/sw1", "").unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Copy to: the session stays where it was; a name taken there is left out, or comes as name-1 once the user
    /// agreed to that (SuperXPuTTY's MakeUniqueSessionId). Saved and Private sessions share their names, so one taken
    /// in either counts
    #[test]
    fn a_copy_leaves_the_session_where_it_was_and_a_name_taken_comes_with_a_number_only_when_agreed() {
        use crate::sessions::Store::{Private, Saved};
        let (state, dir) = transfer_state("copy");
        add_session(&state, Saved, "Lab/sw1");
        add_session(&state, Private, "Lab/sw2");
        add_session(&state, Private, "Lab/sw2-1");
        let copy = |ids: &[&str], rename_taken: bool| {
            let ids = ids.iter().map(|id| id.to_string()).collect();
            tauri::async_runtime::block_on(super::transfer(&state, Private, ids, Saved, false, None, rename_taken)).unwrap()
        };

        // Lab/sw2 is already in Private sessions itself, which is as good as taken in Saved
        let report = copy(&["Lab/sw2"], false);
        assert_eq!((report.done, report.problems.len()), (0, 1));
        assert_eq!(ids_in(&state, Saved), ["Lab/sw1"]);

        let report = copy(&["Lab/sw2"], true);
        assert_eq!((report.done, report.problems.len()), (1, 0));
        // sw2-1 is taken too, in Private sessions: the copy is sw2-2, by name as well as id
        assert_eq!(ids_in(&state, Saved), ["Lab/sw1", "Lab/sw2-2"]);
        assert_eq!(state.sessions.lock().unwrap().find(Saved, "Lab/sw2-2").unwrap().get("SessionName"), "sw2-2");
        assert_eq!(state.sessions.lock().unwrap().find(Saved, "Lab/sw2-2").unwrap().get("Host"), "sw2.example");
        assert_eq!(ids_in(&state, Private), ["Lab/sw2", "Lab/sw2-1"], "a copy takes nothing out");

        // within one list there's nothing to move, and a copy needs a folder to go to (the tree's Paste)
        assert!(tauri::async_runtime::block_on(super::transfer(&state, Saved, vec!["Lab/sw1".into()], Saved, false, None, false)).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Out of the Database: what arrives in a file is a plain session, with none of the table's fields (its row's id,
    /// version and locks would mean nothing there, and SuperXPuTTY's PlainCopy leaves them too). A move then deletes
    /// the rows that arrived, but never a row locked in the table, which stays and is said to. The table is out of
    /// reach here, so the deletion waits in Pending changes
    #[test]
    fn a_move_out_of_the_database_leaves_the_tables_fields_behind_and_a_locked_row_in_the_table() {
        use crate::sessions::Store::{Database, Private};
        let (state, dir) = transfer_state("database");
        let row = |name: &str| crate::db::Row { folder: "Lab".into(), name: name.into(), host: format!("{name}.example"), protocol: "SSH".into(), port: 22, ..Default::default() };
        let free = crate::db::session_of(&row("sw1"), "row-1", "7");
        let mut locked = crate::db::session_of(&row("sw2"), "row-2", "7");
        locked.set("DatabaseLocked", "true");
        locked.set("DatabaseManagedBy", "netbox");
        state.sessions.lock().unwrap().database = vec![free.clone(), locked.clone()];
        *state.db_server.lock().unwrap() = vec![free, locked];
        state.settings.lock().unwrap().database.enabled = true;
        state.database.lock().unwrap().error = "The database can't be reached.".into();

        let ids = vec!["@db/Lab/sw1".to_string(), "@db/Lab/sw2".to_string()];
        let report = tauri::async_runtime::block_on(super::transfer(&state, Database, ids, Private, true, None, false)).unwrap();

        assert_eq!(report.done, 2);
        assert_eq!(report.problems, ["@db/Lab/sw2: locked in the database, so it stays there."]);
        assert_eq!(ids_in(&state, Private), ["Lab/sw1", "Lab/sw2"]);
        let file = std::fs::read_to_string(state.user_folder.join("PrivateSessions.xml")).unwrap();
        assert!(!file.contains("Database") && !file.contains("@db/") && !file.contains("row-1"), "{file}");
        assert!(file.contains("Host=\"sw1.example\""), "{file}");
        // only the row that isn't locked is to be deleted from the table
        let source = state.settings.lock().unwrap().database.source();
        let waiting = crate::outbox::changes(&state.user_folder, &source);
        assert_eq!(waiting.len(), 1);
        assert_eq!((waiting[0].kind, waiting[0].row_id.as_str(), waiting[0].from.as_str()), (crate::outbox::Kind::Delete, "row-1", "Lab/sw1"));
        // and the tree shows the table without it, the locked one still there
        let shown: Vec<String> = state.sessions.lock().unwrap().database.iter().map(|s| s.id().to_string()).collect();
        assert_eq!(shown, ["@db/Lab/sw2"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
