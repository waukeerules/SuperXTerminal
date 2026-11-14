//! The session files as files: Edit Sessions Externally, Export to XML, showing a store's file in its folder, and
//! its backups (listing them and putting one back). Reading the files for the tree is `sessions.rs`.

use std::path::PathBuf;

use tauri::State;

use crate::commands::sessions::store_file;
use crate::{applog, opening, sessions, AppState};

/// File > Edit Sessions Externally, as SuperXPuTTY's: the sessions file (Sessions.xml, or PrivateSessions.xml when
/// Saved sessions are turned off) in Notepad++ on Windows when it's installed, else the system's app for it; the
/// Mac's default text editor there. The file is made first if there isn't one yet. The tree reads it again when it's
/// saved (`sessions_files_changed`). On Linux, the app .xml files open in. Returns what to say in the status bar.
#[tauri::command]
pub(crate) fn sessions_edit_externally(state: State<'_, AppState>) -> Result<String, String> {
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
        if let Some(editor) = opening::notepad_plus_plus() {
            std::process::Command::new(editor).arg(&path).spawn().map_err(|e| format!("Couldn't start Notepad++: {e}"))?;
            return Ok(format!("Editing {} in Notepad++; the sessions are read again when it's saved", path.display()));
        }
        opening::open_in_app(&path)?;
    }
    #[cfg(not(windows))]
    opening::open_command(&path, opening::Opening::AsText).spawn().map_err(|e| e.to_string())?;
    Ok(format!("Editing {}; the sessions are read again when it's saved", path.display()))
}

/// File > Export Sessions…, as SuperXPuTTY's: every session (Saved, Private and Database) in one Sessions.xml, which
/// SuperXPuTTY and File > Import Sessions > From SuperPuTTY XML File read. Database sessions go as plain sessions, in
/// their folders, without the table's id, version or locks. Returns how many were written.
#[tauri::command]
pub(crate) fn sessions_export_xml(state: State<'_, AppState>, path: PathBuf) -> Result<usize, String> {
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

/// Settings > General's Show buttons beside Use Sessions.xml and Use PrivateSessions.xml: the file in Finder or
/// File Explorer, whether or not it's in use. Sessions.xml is in the settings folder, PrivateSessions.xml in this
/// user's own folder (on this computer); Database sessions have no file. Returns what to tell the user when the file
/// isn't there yet and its folder is shown instead, so an empty-looking folder isn't a puzzle
#[tauri::command]
pub(crate) fn sessions_reveal(state: State<'_, AppState>, store: sessions::Store) -> Result<Option<String>, String> {
    let (shown, note) = file_or_folder(&sessions_file_of(&state, store)?);
    opening::reveal(&shown)?;
    Ok(note)
}

/// Sessions.xml's or PrivateSessions.xml's file, for Restore from Backup: Database sessions have none, and Sessions.xml
/// none until a settings folder is chosen
pub(crate) fn sessions_file_of(state: &AppState, store: sessions::Store) -> Result<PathBuf, String> {
    let settings_folder = state.settings.lock().unwrap().settings_folder.clone();
    sessions::file_of(store, settings_folder.as_deref(), &state.user_folder).ok_or_else(|| match store {
        sessions::Store::Database => "Database sessions aren't kept in a file".to_string(),
        _ => "No settings folder has been chosen yet".to_string(),
    })
}

/// Settings > General's Restore from Backup: the backups of Sessions.xml or PrivateSessions.xml, newest first
#[tauri::command]
pub(crate) fn sessions_backups(state: State<'_, AppState>, store: sessions::Store) -> Result<Vec<sessions::Backup>, String> {
    Ok(sessions::backups(&sessions_file_of(&state, store)?))
}

/// Put one of Sessions.xml's or PrivateSessions.xml's backups back (the file now is backed up first). The Sessions
/// tree reads it again, as for any change on disk (`sessions_files_changed`)
#[tauri::command]
pub(crate) fn sessions_restore(state: State<'_, AppState>, store: sessions::Store, name: String) -> Result<(), String> {
    let file = sessions_file_of(&state, store)?;
    sessions::restore(&file, &name)?;
    applog::info(format!("Restored {} from {name}", file.display()));
    Ok(())
}

/// What to show in Finder or File Explorer for a sessions file: the file, or its folder when it isn't there yet (no
/// session has been saved to it), since revealing a missing file opens Explorer on Documents instead and Finder on
/// nothing. The folder comes with a note saying the file will be made there
pub(crate) fn file_or_folder(file: &std::path::Path) -> (PathBuf, Option<String>) {
    if file.exists() {
        return (file.to_path_buf(), None);
    }
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let what = if name == "Sessions.xml" { "Saved" } else { "Private" };
    let note = format!("There's no {name} yet: it's made in this folder when the first {what} session is saved.");
    (file.parent().map(std::path::Path::to_path_buf).unwrap_or_default(), Some(note))
}

#[cfg(test)]
mod tests {
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
}
