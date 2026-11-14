//! File > Edit Sessions as Table: every session, Saved, Private and Database, as rows to change many at once, and
//! the table's own CSV export and import. The rows' fields are `crate::editing`'s; the CSV is `crate::importing`'s.

use std::path::PathBuf;

use tauri::State;

use crate::commands::sessions::{follow_id, other_ids, store_file};
use crate::{editing, importing, session_passwords, sessions, AppState};

/// One session for File > Edit Sessions as Table…: where it is, every field Edit Session has (a `-pw` masked), and,
/// for a Database session, what the table locks against changes
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TableRow {
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
pub(crate) fn sessions_table_load(state: State<'_, AppState>) -> Vec<TableRow> {
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
pub(crate) fn sessions_table_save(state: State<'_, AppState>, store: sessions::Store, edits: Vec<(Option<String>, editing::SessionFields)>) -> Result<Vec<String>, String> {
    let path = store_file(&state, store)?;
    let ids = editing::save_many(&path, &edits, &other_ids(&state, store))?;
    for ((old, _), new) in edits.iter().zip(&ids) {
        if let Some(old) = old {
            follow_id(&state, old, new);
        }
    }
    Ok(ids)
}

/// One row for [`sessions_table_export_csv`]: its store now, where it was read (its store and id; None for a new row),
/// and its fields now
pub(crate) type TableCsvRow = (sessions::Store, Option<(sessions::Store, String)>, editing::SessionFields);

/// Edit Sessions as Table…'s Export CSV…: the rows shown, as they are in the table, saved or not, in Export Sessions to
/// CSV's form (so Import Sessions > CSV reads it back). Each `(store now, where it was read: its store and id, fields
/// now)`, None for a new row; a session read keeps what the table doesn't show (its paths, RDP options...) and its
/// -pw, which the CSV writes as ********. A row moved to another store but not saved is written in its new one.
/// Nothing is saved. Returns how many were written and how many had a -pw masked.
#[tauri::command]
pub(crate) fn sessions_table_export_csv(state: State<'_, AppState>, rows: Vec<TableCsvRow>, path: PathBuf) -> Result<(usize, usize), String> {
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
pub(crate) fn sessions_table_read_csv(path: PathBuf) -> Result<importing::CsvSessions, String> {
    importing::read_csv_file(&path)
}
