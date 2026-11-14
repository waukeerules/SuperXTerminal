//! File > Import Sessions and Export Sessions: what an import would add before it's applied, applying it, where
//! each kind of import usually is on this computer, the CSV export and its template. The formats themselves
//! (PuTTY, SuperPuTTY, SecureCRT, ssh config, CSV...) are read in `crate::importing`.

use std::path::PathBuf;

use tauri::State;

use crate::commands::sessions::{other_ids, saved_folder, store_file};
use crate::{importing, sessions, AppState};

/// Export sessions to a CSV file, as SuperXPuTTY's Export sessions to CSV: the ones given (a folder's, the
/// selection's), or every session. Returns how many were written and how many had a -pw password masked.
#[tauri::command]
pub(crate) fn sessions_export_csv(state: State<'_, AppState>, items: Option<Vec<(sessions::Store, String)>>, path: PathBuf) -> Result<(usize, usize), String> {
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

/// Save a CSV template for File > Import Sessions: the columns, with example rows to copy (importing::template)
#[tauri::command]
pub(crate) fn sessions_csv_template(path: PathBuf) -> Result<(), String> {
    importing::write_template(&path)
}

/// The plan for importing sessions (`kind` "csv", "mobaxterm", "rdp" or "ssh"), worked out without changing anything
pub(crate) fn import_plan(state: &AppState, kind: &str, paths: &[PathBuf], store: sessions::Store) -> Result<importing::Plan, String> {
    let all = state.sessions.lock().unwrap();
    let use_saved = saved_folder(state).is_some();
    let existing = importing::Existing { saved: &all.saved, private: &all.private, use_saved };
    let store = if use_saved { store } else { sessions::Store::Private };
    importing::plan(kind, paths, store, &existing)
}

/// What an import would do, for the window to show before anything changes
#[tauri::command]
pub(crate) fn sessions_import_plan(state: State<'_, AppState>, kind: String, paths: Vec<PathBuf>, store: sessions::Store) -> Result<importing::Summary, String> {
    Ok(import_plan(&state, &kind, &paths, store)?.summary())
}

/// Import sessions, as planned again from the same files; returns how many were added and updated. The caller reloads.
#[tauri::command]
pub(crate) fn sessions_import_apply(state: State<'_, AppState>, kind: String, paths: Vec<PathBuf>, store: sessions::Store) -> Result<(usize, usize), String> {
    let plan = import_plan(&state, &kind, &paths, store)?;
    if let Some(error) = plan.error {
        return Err(error);
    }
    importing::apply(&plan, |store| store_file(&state, store), |store| other_ids(&state, store))
}

/// Where an import that reads no chosen file reads (ssh config, PuTTY's and KiTTY's saved sessions, Remote Desktop's
/// recent computers), for the import window; None for imports that ask for a file
#[tauri::command]
pub(crate) fn sessions_import_source(kind: String) -> Option<String> {
    importing::source_of_import(&kind)
}

/// ~/.ssh/config, for Import from ssh config
#[tauri::command]
pub(crate) fn ssh_config_path() -> String {
    importing::ssh_config_path().display().to_string()
}
