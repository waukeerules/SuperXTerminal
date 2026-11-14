//! Database sessions: the connection's settings (Settings > Database), reading the table or its cache, changing
//! its sessions and folders, and the changes that wait to be sent or were refused (Pending changes). The table
//! itself is `crate::db`, and the queue of changes `crate::outbox`; what the tree's root shows is [`DbState`].

use std::path::PathBuf;

use tauri::State;

use crate::commands::session_editing::with_copied_password;
use crate::commands::sessions::{follow_folder, follow_id, forget_folder, forget_id};
use crate::{applog, db, demo, editing, folders, outbox, sessions, AppState};

/// How Database sessions stand, for the tree's root
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DbState {
    pub(crate) enabled: bool,
    /// The last read's change token; "" when only the cache has been read
    #[serde(skip)]
    pub(crate) token: String,
    pub(crate) writable: bool,
    /// Why the last read failed, if it did (the cached sessions are shown meanwhile)
    pub(crate) error: String,
    /// The table's name, "schema.table", for the root's tooltip
    pub(crate) source: String,
    /// The optional columns the table has (color, jump_host, highlight_rules), which writes include
    #[serde(skip)]
    pub(crate) optional_columns: Vec<String>,
    /// How many sessions the last read (or the cache) had
    pub(crate) count: usize,
    /// When the sessions shown were read, "HH:MM", for "offline, cached 09:41"
    pub(crate) synced_at: String,
    /// Changes waiting to be sent, and changes the table refused (Pending changes)
    pub(crate) pending: usize,
    pub(crate) failed: usize,
}

/// Database sessions' settings for the window, and whether a password is stored
#[tauri::command]
pub(crate) fn db_settings_get(state: State<'_, AppState>) -> (db::DbSettings, bool) {
    (state.settings.lock().unwrap().database.clone(), db::has_password())
}

/// Save Database sessions' settings; `password` None keeps the stored one, "" forgets it
#[tauri::command]
pub(crate) fn db_settings_save(state: State<'_, AppState>, database: db::DbSettings, password: Option<String>) -> Result<(), String> {
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
pub(crate) async fn db_test(state: State<'_, AppState>, database: db::DbSettings, password: Option<String>) -> Result<db::TestResult, String> {
    let user_folder = state.user_folder.clone();
    db::test(&database, &user_folder, password.as_deref()).await
}

/// Read the table again (or, when it can't be reached, keep the cache) and say how it went
#[tauri::command]
pub(crate) async fn db_refresh(state: State<'_, AppState>) -> Result<DbState, String> {
    refresh_database(&state).await
}

/// Read the table again, keeping the cache when it can't be reached; used by Refresh and after every change
pub(crate) async fn refresh_database(state: &AppState) -> Result<DbState, String> {
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
pub(crate) fn show_database(state: &AppState, settings: &db::DbSettings, db_state: &mut DbState) {
    let changes = outbox::changes(&state.user_folder, &settings.source());
    (db_state.pending, db_state.failed) = outbox::count(&changes);
    let server = state.db_server.lock().unwrap();
    let shown = if changes.is_empty() { server.clone() } else { outbox::apply(&server, &changes) };
    state.sessions.lock().unwrap().database = shown;
}

/// Show this connection's cache, as SuperXPuTTY does at start before the table is read; nothing if it's another's
pub(crate) fn show_cache(state: &AppState, settings: &db::DbSettings, db_state: &mut DbState) {
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
pub(crate) async fn apply_change(
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
pub(crate) async fn flush(state: &AppState, settings: &db::DbSettings) -> Result<(), String> {
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
pub(crate) struct Submitted {
    /// Saved or queued
    pub(crate) done: usize,
    /// Why each refused one was refused, e.g. "core-sw1: Host is locked in the database."
    pub(crate) problems: Vec<String>,
    /// The positions of the refused ones, so a move removes only what arrived
    pub(crate) refused: std::collections::HashSet<usize>,
}

/// Make changes from the tree, as SuperXPuTTY's SubmitBatch: online with nothing queued, each is written at once;
/// offline, or behind queued changes (to keep their order), or once the table drops mid-way, they're queued and shown
/// at once. One refused change doesn't stop the others. Then the table is read again (or, offline, the queue shown).
pub(crate) async fn submit(state: &AppState, changes: Vec<outbox::PendingChange>) -> Result<Submitted, String> {
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
pub(crate) async fn submit_one(state: &AppState, change: outbox::PendingChange) -> Result<(), String> {
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
pub(crate) fn db_cached(state: State<'_, AppState>) -> DbState {
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
pub(crate) fn db_write_context(state: &AppState) -> (db::DbSettings, PathBuf, Vec<String>) {
    let settings = state.settings.lock().unwrap().database.clone();
    let optional = state.database.lock().unwrap().optional_columns.clone();
    (settings, state.user_folder.clone(), optional)
}

/// A Database session as last read, refusing one that's locked whole
pub(crate) fn db_session(state: &AppState, id: &str) -> Result<sessions::StoredSession, String> {
    let sessions = state.sessions.lock().unwrap();
    let session = sessions.find(sessions::Store::Database, id).ok_or("The session is no longer in the database view; Refresh.")?.clone();
    if session.get("DatabaseLocked") == "true" {
        return Err(format!("\"{}\" is locked in the database.", session.get("SessionName")));
    }
    Ok(session)
}

/// Save a Database session from Edit Session (`original_id` None for a new one), then read the table back
#[tauri::command]
pub(crate) async fn db_session_save(
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
pub(crate) async fn db_session_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
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
pub(crate) async fn db_session_move(state: State<'_, AppState>, id: String, folder: String, name: Option<String>) -> Result<String, String> {
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
pub(crate) async fn db_folder_move(state: State<'_, AppState>, from: String, to: String) -> Result<(), String> {
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
pub(crate) async fn db_folder_delete(state: State<'_, AppState>, path: String) -> Result<(), String> {
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
pub(crate) async fn db_folder_settings_save(state: State<'_, AppState>, fields: folders::FolderFields) -> Result<(), String> {
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

/// How Database sessions stand now, without reading anything (after a change from the tree)
#[tauri::command]
pub(crate) fn db_status(state: State<'_, AppState>) -> DbState {
    state.database.lock().unwrap().clone()
}

/// One queued change, for the Pending changes window
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PendingView {
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
pub(crate) fn db_pending_list(state: State<'_, AppState>) -> Vec<PendingView> {
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
pub(crate) async fn db_pending_retry(state: State<'_, AppState>, ids: Vec<String>, overwrite: bool) -> Result<DbState, String> {
    let source = state.settings.lock().unwrap().database.source();
    for id in &ids {
        outbox::set_error(&state.user_folder, &source, id, None, overwrite)?;
    }
    refresh_database(&state).await
}

/// Drop the chosen changes; the sessions go back to what the table has
#[tauri::command]
pub(crate) fn db_pending_discard(state: State<'_, AppState>, ids: Vec<String>) -> Result<DbState, String> {
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
pub(crate) async fn db_create_schema(state: State<'_, AppState>, database: db::DbSettings, password: Option<String>) -> Result<String, String> {
    if let Some(password) = password.filter(|p| !p.is_empty()) {
        db::set_password(&password)?;
    }
    let user_folder = state.user_folder.clone();
    db::create_or_upgrade_schema(&database, &user_folder).await
}

/// Whether the table changed since the last read (one small query); the UI refreshes when it did
#[tauri::command]
pub(crate) async fn db_check(state: State<'_, AppState>) -> Result<bool, String> {
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
pub(crate) fn db_export(database: db::DbSettings, path: String) -> Result<(), String> {
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
pub(crate) fn db_import(state: State<'_, AppState>, path: String) -> Result<db::DbSettings, String> {
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
