//! Changing Saved and Private sessions and their folders: Edit Session's fields and save, a session's own stored
//! password, and moving, renaming and deleting sessions and folders in the Sessions tree. The changes themselves
//! are `crate::editing` and `crate::folders`; Database sessions are changed in `database.rs`.

use tauri::State;

use crate::commands::sessions::{follow_folder, follow_id, forget_folder, forget_id, other_ids, store_file};
use crate::{editing, folders, overrides, session_passwords, sessions, shortcuts, AppState};

/// One session's fields for Edit Session (a `-pw` password masked)
#[tauri::command]
pub(crate) fn session_get(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<editing::SessionFields, String> {
    let sessions = state.sessions.lock().unwrap();
    let session = sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?;
    Ok(editing::SessionFields::of(session))
}

/// Save a session from Edit Session (`original_id` None for a new one); returns its id. The caller reloads the tree.
/// `copy_of` is the session a new one was filled in from (Duplicate as…, Save session as…), whose -pw it keeps.
#[tauri::command]
pub(crate) fn session_save(
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
pub(crate) fn with_copied_password(state: &AppState, mut fields: editing::SessionFields, copy_of: Option<&(sessions::Store, String)>) -> editing::SessionFields {
    if let Some((store, id)) = copy_of && let Some(source) = state.sessions.lock().unwrap().find(*store, id) {
            fields.extra_args = editing::unmask_password(&fields.extra_args, source.get("ExtraArgs"));
        }
    fields
}

/// Whether a session has a password of its own (Edit Session's Password), without reading the Keychain
#[tauri::command]
pub(crate) fn session_password_has(state: State<'_, AppState>, id: String) -> bool {
    session_passwords::has(&state.user_folder, &id)
}

/// Store a session's own password in the Keychain, or forget it when `password` is empty
#[tauri::command]
pub(crate) fn session_password_set(state: State<'_, AppState>, id: String, password: String) -> Result<(), String> {
    session_passwords::set(&state.user_folder, &id, &password)
}

/// Edit sessions… and Edit Sessions as Table…: one password stored as each session's own, in the Keychain on this
/// computer only, or forgotten from each when `password` is empty. Returns how many
#[tauri::command]
pub(crate) fn session_passwords_set(state: State<'_, AppState>, ids: Vec<String>, password: String) -> Result<usize, String> {
    session_passwords::set_many(&state.user_folder, &ids, &password)
}

/// Edit Session's Move to Password: the session's stored -pw becomes its own password in the Keychain. The -pw stays
/// in its extra arguments until the window saves them without it.
#[tauri::command]
pub(crate) fn session_password_from_pw(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<(), String> {
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
pub(crate) fn session_delete(state: State<'_, AppState>, store: sessions::Store, id: String) -> Result<(), String> {
    let path = store_file(&state, store)?;
    editing::delete(&path, &id)?;
    forget_id(&state, &id);
    Ok(())
}

/// Rename or move a folder (every session below goes with it, and its settings)
#[tauri::command]
pub(crate) fn folder_move(state: State<'_, AppState>, store: sessions::Store, from: String, to: String) -> Result<(), String> {
    folders::check_name(to.rsplit('/').next().unwrap_or(&to))?;
    let path = store_file(&state, store)?;
    folders::move_folder(&path, &from, &to, &other_ids(&state, store))?;
    follow_folder(&state, &from, &to);
    Ok(())
}

/// Delete a folder and every session in it
#[tauri::command]
pub(crate) fn folder_delete(state: State<'_, AppState>, store: sessions::Store, path: String) -> Result<(), String> {
    let file = store_file(&state, store)?;
    folders::delete_folder(&file, &path)?;
    forget_folder(&state, &path);
    Ok(())
}

/// Save a folder's settings (colour and defaults)
#[tauri::command]
pub(crate) fn folder_settings_save(state: State<'_, AppState>, store: sessions::Store, fields: folders::FolderFields) -> Result<(), String> {
    let file = store_file(&state, store)?;
    folders::save_settings(&file, &fields)
}

/// Move a session into another folder of its store; returns its new id
#[tauri::command]
pub(crate) fn session_move(state: State<'_, AppState>, store: sessions::Store, id: String, folder: String, name: Option<String>) -> Result<String, String> {
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
pub(crate) fn items_move(state: State<'_, AppState>, store: sessions::Store, folders: Vec<String>, sessions: Vec<String>, to: String) -> Result<folders::Moved, String> {
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
pub(crate) fn items_delete(state: State<'_, AppState>, store: sessions::Store, folders: Vec<String>, sessions: Vec<String>) -> Result<Vec<String>, String> {
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
