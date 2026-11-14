//! Move to and Copy to between Saved, Private and Database sessions, and Edit Several Sessions' one change made
//! to many. Both work across the stores, so they send a Database session's part to the table (`database.rs`) and
//! write the files' part with `crate::editing`, and say what went and what didn't in a [`TransferReport`].

use tauri::State;

use crate::commands::database::{db_session, submit};
use crate::commands::sessions::{follow_id, other_ids, store_file};
use crate::{db, editing, outbox, sessions, AppState};

/// What a Move to / Copy to did
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TransferReport {
    done: usize,
    problems: Vec<String>,
}

/// Copy (or move) sessions to another root, each to the same folder path there or to the folder given in `folders`, as
/// SuperXPuTTY's Move to / Copy to and drags; copies within one root too when `folders` are given (the tree's Paste).
/// A copy out of the table leaves its database fields behind (SuperXPuTTY's PlainCopy); a move deletes only what
/// arrived, and never a locked row.
#[tauri::command]
pub(crate) async fn sessions_transfer(
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
pub(crate) async fn transfer(
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
pub(crate) struct BulkChange {
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
pub(crate) async fn sessions_bulk_edit(state: State<'_, AppState>, items: Vec<(sessions::Store, String)>, change: BulkChange) -> Result<TransferReport, String> {
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

#[cfg(test)]
mod tests {
    use crate::tests::state_with;

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
