//! Reading the sessions for the Sessions tree, and what the other session commands share: which file a store's
//! sessions are in, whether Saved or Private sessions are in use, noticing a session file changed elsewhere, and
//! having Favorites, Recent, local overrides and stored passwords follow a session that's renamed, moved or deleted.
//! The files' format and the sessions themselves are `crate::sessions`; changing them is `session_editing.rs`.

use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::sessions::{self, SessionsLoaded};
use crate::{applog, file_watch, folders, overrides, remount, saved_cache, session_passwords, shortcuts, tray, AppState};

/// What the last read of the session files said besides the sessions themselves, kept so they can be shown again
/// without reading the files ([`sessions_shown`])
#[derive(Clone)]
pub(crate) struct FilesRead {
    /// The files that couldn't be read, and why
    problems: Vec<String>,
    /// The settings folder couldn't be reached: Saved sessions are the copy kept on this computer ([`saved_cache`])
    saved_offline: bool,
    /// When that copy was read; "" when not offline, or there's none
    saved_cached_at: String,
}

/// Read (or read again) the session and folder files, and give the UI what it shows
#[tauri::command]
pub(crate) fn sessions_load(app: AppHandle, state: State<'_, AppState>) -> SessionsLoaded {
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
    if cfg!(target_os = "macos") && !saved_offline && let Some(folder) = saved_folder(&state) {
            remount::remember(&state.user_folder, &folder);
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
pub(crate) fn sessions_shown(app: AppHandle, state: State<'_, AppState>) -> SessionsLoaded {
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
pub(crate) fn shown(state: &AppState, read: FilesRead) -> SessionsLoaded {
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

/// The file a store's sessions are in, or an error when there's no settings folder yet
pub(crate) fn store_file(state: &AppState, store: sessions::Store) -> Result<PathBuf, String> {
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
pub(crate) const SAVED_OFFLINE: &str = "The settings folder can't be reached, so Saved sessions can't be changed now. They're shown as last read, and can be changed once it's back.";

/// Whether Private sessions are turned off: Use PrivateSessions.xml unticked, and Saved sessions in use (without them,
/// Private sessions stay on whatever the setting says, so sessions always have somewhere to be, as when the settings
/// folder has gone since it was turned off)
pub(crate) fn private_off(state: &AppState) -> bool {
    // read, and the lock let go, before saved_folder takes it: the settings' Mutex can't be taken twice, and taking it
    // in one statement with saved_folder hung the app as it first read the sessions
    let use_private = state.settings.lock().unwrap().use_private_sessions;
    private_off_when(use_private, saved_folder(state).is_some())
}

/// `private_off`'s rule: off only when it's turned off and Saved sessions are in use
pub(crate) fn private_off_when(use_private: bool, saved_in_use: bool) -> bool {
    !use_private && saved_in_use
}

/// The settings folder when Saved sessions are in use (Sessions.xml and Folders.xml are read from it); None when
/// there's none yet or they're turned off, as SuperXPuTTY's UseSessionsXml
pub(crate) fn saved_folder(state: &AppState) -> Option<PathBuf> {
    let settings = state.settings.lock().unwrap();
    settings.settings_folder.clone().filter(|_| settings.use_sessions_xml)
}

/// The session files, in the order they're known by ([`file_watch`]): the private two in the user's folder, then
/// Sessions.xml and Folders.xml in the settings folder when Saved sessions are in use
pub(crate) fn session_files(state: &AppState) -> Vec<PathBuf> {
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
pub(crate) fn sessions_files_changed(state: State<'_, AppState>) -> bool {
    // the Mac: a settings folder on a network share that went with the network isn't put back by macOS when the
    // network returns. If its server answers again, the share is connected again here (once each time it comes
    // back), and the check below then finds the folder there. It may wait on macOS asking for the share's password,
    // which is one more reason this isn't on the window's thread
    if cfg!(target_os = "macos") && let Some(folder) = saved_folder(&state).filter(|folder| !saved_cache::reachable(folder)) {
            remount::reconnect(&state.user_folder, &folder);
        }
    let files = session_files(&state);
    state.files_seen.lock().unwrap().changed(&files, std::time::Instant::now(), file_watch::read_or_nothing)
}

/// The session ids in the store other than `store`, which a change in `store` must not clash with
pub(crate) fn other_ids(state: &AppState, store: sessions::Store) -> Vec<String> {
    let sessions = state.sessions.lock().unwrap();
    let other = if store == sessions::Store::Saved { &sessions.private } else { &sessions.saved };
    other.iter().map(|s| s.id().to_string()).collect()
}

/// Favorites, Recent, local overrides and the session's own password follow a session renamed or moved
pub(crate) fn follow_id(state: &AppState, old: &str, new: &str) {
    if old != new {
        shortcuts::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
        overrides::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
        session_passwords::follow(&state.user_folder, |id| (id == old).then(|| new.to_string()));
    }
}

/// Favorites, Recent, local overrides and the session's own password forget a deleted session
pub(crate) fn forget_id(state: &AppState, gone: &str) {
    shortcuts::forget(&state.user_folder, |s| s == gone);
    overrides::forget(&state.user_folder, |s| s == gone);
    session_passwords::forget(&state.user_folder, |s| s == gone);
}

/// Favorites and Recent follow the sessions below a renamed or moved folder ("Lab" or "@db/Lab"), any spelling of it
pub(crate) fn follow_folder(state: &AppState, from: &str, to: &str) {
    let rename = |id: &str| folders::under(id, from).filter(|rest| !rest.is_empty()).map(|rest| format!("{to}{rest}"));
    shortcuts::follow(&state.user_folder, rename);
    overrides::follow(&state.user_folder, rename);
    session_passwords::follow(&state.user_folder, rename);
}

/// Favorites and Recent forget the sessions below a deleted folder
pub(crate) fn forget_folder(state: &AppState, path: &str) {
    let gone = |id: &str| folders::under(id, path).is_some_and(|rest| !rest.is_empty());
    shortcuts::forget(&state.user_folder, gone);
    overrides::forget(&state.user_folder, gone);
    session_passwords::forget(&state.user_folder, gone);
}

#[cfg(test)]
mod tests {
    use crate::tests::state_with;

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
}
