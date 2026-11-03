//! SuperXTerminal's Rust core: the commands the UI (`src/`) calls through Tauri's `invoke`.

mod algorithms;
mod applog;
mod askpass;
mod buttonbars;
mod cli;
mod command_tap;
mod console;
mod db;
mod editing;
mod external;
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
mod rdp;
mod rdp_clipboard;
mod rdp_options;
mod scripts;
mod secrets;
mod outbox;
mod overrides;
mod sessions;
mod shortcuts;
mod streams;
mod tray;
mod variables;
mod settings;
mod sftp;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
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
    files_seen: Mutex<u64>,
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
fn sessions_load(app: AppHandle, state: State<'_, AppState>) -> SessionsLoaded {
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    let saved_off = !state.settings.lock().unwrap().use_sessions_xml;
    *state.files_seen.lock().unwrap() = files_fingerprint(&state);
    let mut problems = Vec::new();
    let mut loaded = Sessions::load(saved_folder(&state).as_deref(), &state.user_folder, &mut problems);
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
        saved_off,
    };
    let counts = (loaded.saved.len(), loaded.private.len(), loaded.database.len());
    *state.sessions.lock().unwrap() = loaded;
    applog::info(format!("Read the sessions: {} saved, {} private, {} from the database", counts.0, counts.1, counts.2));
    for problem in &result.problems {
        applog::warn(format!("Sessions: {problem}"));
    }
    tray::refresh(&app);
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
    if store == sessions::Store::Saved && !state.settings.lock().unwrap().use_sessions_xml {
        return Err("Saved sessions are turned off (Settings > General > Use Sessions.xml).".into());
    }
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    sessions::file_of(store, settings_folder.as_deref(), &state.user_folder).ok_or_else(|| "Choose the settings folder first.".into())
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

/// Favorites and Recent follow a session renamed or moved
fn follow_id(state: &AppState, old: &str, new: &str) {
    if old != new {
        shortcuts::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
        overrides::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
    }
}

/// Favorites, Recent and local overrides forget a deleted session
fn forget_id(state: &AppState, gone: &str) {
    shortcuts::forget(&state.user_folder, |s| s == gone);
    overrides::forget(&state.user_folder, |s| s == gone);
}

/// Favorites and Recent follow the sessions below a renamed or moved folder ("Lab" or "@db/Lab"), any spelling of it
fn follow_folder(state: &AppState, from: &str, to: &str) {
    let rename = |id: &str| folders::under(id, from).filter(|rest| !rest.is_empty()).map(|rest| format!("{to}{rest}"));
    shortcuts::follow(&state.user_folder, rename);
    overrides::follow(&state.user_folder, rename);
}

/// Favorites and Recent forget the sessions below a deleted folder
fn forget_folder(state: &AppState, path: &str) {
    let gone = |id: &str| folders::under(id, path).is_some_and(|rest| !rest.is_empty());
    shortcuts::forget(&state.user_folder, gone);
    overrides::forget(&state.user_folder, gone);
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

/// Open a file or folder in the app the system uses for it: `open` on the Mac, Explorer's "start" on Windows
fn open_in_app(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // start's first quoted argument is a window title, hence the empty one
        std::process::Command::new("cmd").args(["/c", "start", ""]).arg(path).creation_flags(CREATE_NO_WINDOW).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("open").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
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
/// saved (`sessions_files_changed`). Returns what to say in the status bar.
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
    // -t: the default text editor, whatever app .xml files open in
    #[cfg(not(windows))]
    std::process::Command::new("open").arg("-t").arg(&path).spawn().map_err(|e| e.to_string())?;
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
#[tauri::command]
fn session_external(state: State<'_, AppState>, store: sessions::Store, id: String, how: String) -> Result<String, String> {
    let profiles = state.profiles.lock().unwrap().clone();
    let values = variables::values(&state.user_folder);
    let serial_devices = state.settings.lock().unwrap().serial_devices.clone();
    let sessions = state.sessions.lock().unwrap();
    let session = &overrides::applied(&state.user_folder, sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?);
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

/// The session files' contents, hashed: Sessions.xml and Folders.xml in the settings folder, the private ones in the
/// user's folder, as SuperXPuTTY's SessionsFileWatcher compares (a file rewritten unchanged doesn't count)
fn files_fingerprint(state: &AppState) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut files = vec![state.user_folder.join("PrivateSessions.xml"), state.user_folder.join("PrivateFolders.xml")];
    if let Some(folder) = saved_folder(state) {
        files.push(folder.join("Sessions.xml"));
        files.push(folder.join("Folders.xml"));
    }
    for file in files {
        std::fs::read(&file).unwrap_or_default().hash(&mut hasher);
    }
    hasher.finish()
}

/// Whether a session file changed since it was last read (a teammate saving to the shared folder, SuperXPuTTY, an
/// editor); the tree then reads them again
#[tauri::command]
fn sessions_files_changed(state: State<'_, AppState>) -> bool {
    files_fingerprint(&state) != *state.files_seen.lock().unwrap()
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
fn settings_save(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    let mut current = state.settings.lock().unwrap();
    let folder = current.settings_folder.clone();
    *current = AppSettings { settings_folder: folder, ..settings };
    current.icon_size = match current.icon_size {
        16 | 20 | 24 | 32 => current.icon_size,
        _ => 24,
    };
    current.tree_font_size = current.tree_font_size.clamp(10, 24);
    current.shortcuts = keys::cleaned(&current.shortcuts);
    current.save(&state.user_folder)?;
    // the menu bar's keys follow Settings > Shortcuts
    if let Ok(bar) = menu(&app, &current.shortcuts) {
        let _ = app.set_menu(bar);
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
        let submitted = submit(&state, changes).await?;
        problems.extend(submitted.problems);
        sent.into_iter().enumerate().filter(|(i, _)| !submitted.refused.contains(i)).map(|(_, s)| s).collect()
    } else {
        let path = store_file(&state, to)?;
        let others = other_ids(&state, to);
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
        problems.extend(submit(&state, deletes).await?.problems);
    } else if delete_source {
        for (session, _) in &arrived {
            if let Err(e) = store_file(&state, from).and_then(|path| editing::delete(&path, session.id())) {
                problems.push(e);
            }
        }
    }
    if delete_source {
        // Favorites and Recent follow sessions moved to the other root
        for (session, path) in arrived.iter().filter(|(s, _)| s.get("DatabaseLocked") != "true") {
            let new = if to == Store::Database { format!("@db/{path}") } else { path.clone() };
            follow_id(&state, session.id(), &new);
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
/// Mac's text editor. The app has the file among its resources; a development build reads the project's.
fn open_notices<R: Runtime>(app: &AppHandle<R>) {
    let bundled = app.path().resource_dir().map(|d| d.join("THIRD-PARTY-NOTICES.txt")).ok().filter(|p| p.exists());
    let path = bundled.unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../THIRD-PARTY-NOTICES.txt"));
    if let Err(e) = std::process::Command::new("open").arg("-t").arg(&path).status() {
        applog::warn(format!("Couldn't open {}: {e}", path.display()));
    }
}

/// The menu bar: the app menu (with Settings..., Cmd+,), File (New Local Shell, Cmd+T), Edit (which WebKit needs for
/// Cmd+C/V), Tools (Environment Variables...) and Window.
/// SuperXPuTTY's File, View and Tools menus join it as their features arrive.
fn menu<R: Runtime>(app: &AppHandle<R>, shortcuts: &BTreeMap<String, String>) -> tauri::Result<Menu<R>> {
    // an action's item, with its key now (keys.rs: the user's, else the Mac default)
    let action = |id: &str| MenuItem::with_id(app, id, keys::label(id), true, keys::key(shortcuts, id).as_deref());
    let plain = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>);
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
                    &plain("import:csv", "From CSV File…")?,
                    &plain("import:ssh", "From ssh config…")?,
                ],
            )?,
            &plain("exportXml", "Export Sessions…")?,
            &plain("exportCsv", "Export Sessions to CSV…")?,
            &PredefinedMenuItem::separator(app)?,
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
        ],
    )?;
    // shown or hidden, as SuperXPuTTY's View > Send Commands and View > Button Bar
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &action("toggleSessionsPane")?,
            &action("toggleSendCommands")?,
            &action("toggleButtonBar")?,
            &action("toggleStatusBar")?,
            &PredefinedMenuItem::separator(app)?,
            &action("toggleSessionsAutoHide")?,
            &action("toggleLayouts")?,
            &action("toggleSessionDetail")?,
            &action("toggleLogViewer")?,
            &PredefinedMenuItem::separator(app)?,
            &action("toggleAlwaysOnTop")?,
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
    std::process::Command::new("open").arg("-R").arg(applog::file()).spawn().map(|_| ()).map_err(|e| e.to_string())
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
            let _ = window_new(app.clone(), None, None, None);
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
#[tauri::command]
fn window_new(app: AppHandle, open: Option<String>, adopt: Option<serde_json::Value>, layout: Option<String>) -> Result<(), String> {
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

pub fn run() {
    let user_folder = sessions::user_folder();
    let settings = AppSettings::load(&user_folder);
    let profiles = Profiles::load(&user_folder);
    applog::info(format!("SuperXTerminal {} started (user folder {})", env!("CARGO_PKG_VERSION"), user_folder.display()));
    // started with something to open (SuperXPuTTY's command line)
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(request) = cli::from_args(&args) {
        PENDING.lock().unwrap().push(request);
    }
    tauri::Builder::default()
        // one SuperXTerminal: started again (from a script, a terminal), the first one opens what it was given
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| deliver(app, cli::from_args(argv.get(1..).unwrap_or(&[])))))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .menu({
            let shortcuts = settings.shortcuts.clone();
            move |app| menu(app, &shortcuts)
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
                pty::close_window(&window.state::<pty::Ptys>(), window.label());
                rdp::close_window(&window.state::<rdp::Rdps>(), window.label());
                sftp::close_window(&window.state::<sftp::Sftps>(), window.label())
            }
            _ => {}
        })
        .setup(|app| {
            console::allow_ctrl_c();
            logs::sweep_temp();
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
        .manage(AppState {
            user_folder,
            settings: Mutex::new(settings),
            sessions: Mutex::new(Sessions::default()),
            profiles: Mutex::new(profiles),
            database: Mutex::new(DbState::default()),
            db_server: Mutex::new(Vec::new()),
            files_seen: Mutex::new(0),
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
            serial_devices_list,
            sessions_files_changed,
            session_external,
            serial_device_set,
            pty::pty_special,
            overrides_get,
            sessions_bulk_edit,
            sessions_export_csv,
            window_new,
            layouts_list,
            applog_since,
            applog_write,
            applog_clear,
            applog_reveal,
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
